#!/bin/bash
# whogitit capture hook for Claude Code
# This script captures file changes for AI attribution tracking
# It handles Edit, Write, and Bash tools with proper before/after tracking
#
# For Edit/Write: tracks single file changes
# For Bash: snapshots all dirty files before command, detects changes after

set -o pipefail
umask 077

# Enable debug logging when WHOGITIT_HOOK_DEBUG is set
DEBUG_ENABLED="${WHOGITIT_HOOK_DEBUG:-}"
DEBUG_LOG=""
ERROR_LOG=""

log_debug() {
    if [[ -n "$DEBUG_ENABLED" && -n "$DEBUG_LOG" ]]; then
        echo "$(date '+%Y-%m-%d %H:%M:%S') [$HOOK_PHASE] $1" >> "$DEBUG_LOG"
    fi
}

log_error() {
    if [[ -n "$DEBUG_ENABLED" && -n "$ERROR_LOG" ]]; then
        echo "$(date '+%Y-%m-%d %H:%M:%S') [$HOOK_PHASE] ERROR: $1" >> "$ERROR_LOG"
    fi
    log_debug "ERROR: $1"
}

# Read the hook input from stdin
INPUT=$(cat)
if [[ -z "$INPUT" ]]; then
    log_error "Empty input from stdin"
    exit 0
fi

# Determine hook phase (pre or post)
HOOK_PHASE="${WHOGITIT_HOOK_PHASE:-post}"

# Extract tool name with fallback
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // .tool // ""' 2>/dev/null)
if [[ $? -ne 0 ]]; then
    log_error "Failed to parse JSON input"
    exit 0
fi

log_debug "Tool: $TOOL_NAME"

# Check if whogitit is available (do this early)
WHOGITIT_BIN="${WHOGITIT_BIN:-$HOME/.cargo/bin/whogitit}"
if [[ ! -x "$WHOGITIT_BIN" ]]; then
    # Try to find it in PATH
    if command -v whogitit &> /dev/null; then
        WHOGITIT_BIN=$(command -v whogitit)
    else
        log_error "whogitit binary not found"
        exit 0
    fi
fi

# Get repository root
REPO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null)
if [[ -z "$REPO_ROOT" ]]; then
    log_debug "Not in a git repository, skipping"
    exit 0
fi

# State directory for tracking pre-edit content (repo-local for safety)
STATE_DIR="$REPO_ROOT/.whogitit/state"
BASH_STATE_DIR="$STATE_DIR/bash"
mkdir -p "$STATE_DIR" "$BASH_STATE_DIR" 2>/dev/null || {
    echo "whogitit: Failed to create state directory: $STATE_DIR" >&2
    exit 0
}
chmod 700 "$STATE_DIR" "$BASH_STATE_DIR" 2>/dev/null || true

DEBUG_LOG="$STATE_DIR/hook-debug.log"
ERROR_LOG="$STATE_DIR/hook-errors.log"

# Clean up stale state files (older than 1 hour)
find "$STATE_DIR" -type f -mmin +60 -delete 2>/dev/null

log_debug "Hook started"

# ============================================================================
# UTILITY FUNCTIONS
# ============================================================================

# Hash a string for state file naming
hash_string() {
    if command -v md5 &> /dev/null; then
        echo -n "$1" | md5
    elif command -v md5sum &> /dev/null; then
        echo -n "$1" | md5sum | cut -d' ' -f1
    else
        echo -n "$1" | cksum | cut -d' ' -f1
    fi
}

# Get transcript path from input
get_transcript_path() {
    echo "$INPUT" | jq -r '.transcript_path // ""' 2>/dev/null
}

# Extract prompt from transcript
extract_prompt_from_transcript() {
    local transcript_path="$1"
    local fallback="$2"

    if [[ -n "$transcript_path" && -f "$transcript_path" ]]; then
        local prompt
        prompt=$(jq -rs '
            [.[] | select(.type == "user" and .toolUseResult == null and .isCompactSummary != true)] |
            last |
            if .message.content then
                if (.message.content | type) == "string" then
                    .message.content
                elif (.message.content | type) == "array" then
                    [.message.content[] | select(.type == "text") | .text] | join(" ")
                else
                    ""
                end
            else
                ""
            end
        ' "$transcript_path" 2>/dev/null | head -c 2000)

        if [[ -n "$prompt" && "$prompt" != "null" && "$prompt" != '""' ]]; then
            echo "$prompt"
            return
        fi
    fi

    echo "$fallback"
}

# Extract context (plan mode, subagent) from transcript
extract_context_from_transcript() {
    local transcript_path="$1"

    local plan_mode="false"
    local is_subagent="false"
    local agent_depth="0"

    if [[ -n "$transcript_path" && -f "$transcript_path" ]]; then
        plan_mode=$(jq -s '
            ([.[] | select(.planMode != null)] | last | .planMode) //
            (
                [.[] | select(.tool_name == "EnterPlanMode" or .tool_name == "ExitPlanMode")] |
                if length > 0 then
                    last | .tool_name == "EnterPlanMode"
                else
                    false
                end
            )
        ' "$transcript_path" 2>/dev/null)

        local subagent_info
        subagent_info=$(jq -s '
            [.[] | select(.tool_name == "Task")] | length as $task_count |
            ([.[] | select(.agentId != null)] | length > 0) as $has_agent_id |
            {
                is_subagent: ($has_agent_id or $task_count > 0),
                agent_depth: (if $task_count > 0 then 1 else 0 end)
            }
        ' "$transcript_path" 2>/dev/null)

        is_subagent=$(echo "$subagent_info" | jq -r '.is_subagent // false')
        agent_depth=$(echo "$subagent_info" | jq -r '.agent_depth // 0')
    fi

    # Return as JSON
    echo "{\"plan_mode\": $plan_mode, \"is_subagent\": $is_subagent, \"agent_depth\": $agent_depth}"
}

# Send a file change to whogitit capture
send_to_whogitit() {
    local tool="$1"
    local file_path="$2"
    local prompt="$3"
    local old_content="$4"
    local old_content_present="$5"
    local new_content="$6"
    local context_json="$7"

    local plan_mode is_subagent agent_depth
    plan_mode=$(echo "$context_json" | jq -r '.plan_mode // false')
    is_subagent=$(echo "$context_json" | jq -r '.is_subagent // false')
    agent_depth=$(echo "$context_json" | jq -r '.agent_depth // 0')

    local capture_result
    if [[ "$old_content_present" != "1" ]]; then
        log_debug "Sending $file_path as NEW file"
        capture_result=$(jq -n \
            --arg tool "$tool" \
            --arg file_path "$file_path" \
            --arg prompt "$prompt" \
            --arg new_content "$new_content" \
            --argjson old_content_present false \
            --argjson plan_mode "$plan_mode" \
            --argjson is_subagent "$is_subagent" \
            --argjson agent_depth "$agent_depth" \
            '{
                tool: $tool,
                file_path: $file_path,
                prompt: $prompt,
                old_content: null,
                old_content_present: $old_content_present,
                new_content: $new_content,
                context: {
                    plan_mode: $plan_mode,
                    is_subagent: $is_subagent,
                    agent_depth: $agent_depth
                }
            }' 2>/dev/null | "$WHOGITIT_BIN" capture --stdin 2>&1)
    else
        log_debug "Sending $file_path as MODIFIED file"
        capture_result=$(jq -n \
            --arg tool "$tool" \
            --arg file_path "$file_path" \
            --arg prompt "$prompt" \
            --arg old_content "$old_content" \
            --arg new_content "$new_content" \
            --argjson old_content_present true \
            --argjson plan_mode "$plan_mode" \
            --argjson is_subagent "$is_subagent" \
            --argjson agent_depth "$agent_depth" \
            '{
                tool: $tool,
                file_path: $file_path,
                prompt: $prompt,
                old_content: $old_content,
                old_content_present: $old_content_present,
                new_content: $new_content,
                context: {
                    plan_mode: $plan_mode,
                    is_subagent: $is_subagent,
                    agent_depth: $agent_depth
                }
            }' 2>/dev/null | "$WHOGITIT_BIN" capture --stdin 2>&1)
    fi

    local capture_exit=$?
    if [[ $capture_exit -ne 0 ]]; then
        log_error "whogitit capture failed for $file_path (exit $capture_exit): $capture_result"
        return 1
    else
        if [[ -n "$capture_result" ]]; then
            log_debug "capture output for $file_path: $capture_result"
        fi
    fi
    return 0
}

# ============================================================================
# EDIT/WRITE TOOL HANDLING (single file)
# ============================================================================

handle_edit_write_pre() {
    local file_path="$1"
    local state_file="$2"

    if [[ -f "$file_path" ]]; then
        if cp "$file_path" "$state_file" 2>/dev/null; then
            local lines
            lines=$(wc -l < "$state_file" 2>/dev/null | tr -d ' ')
            log_debug "Saved pre-edit state for $file_path: $lines lines"
        else
            log_error "Failed to copy file to state: $file_path"
        fi
    else
        rm -f "$state_file" 2>/dev/null
        log_debug "File doesn't exist, removed state: $file_path"
    fi
}

handle_edit_write_post() {
    local file_path="$1"
    local state_file="$2"
    local tool_name="$3"

    # Get old content from state file
    local old_content=""
    local old_content_present="0"
    if [[ -f "$state_file" ]]; then
        old_content=$(cat "$state_file" 2>/dev/null)
        old_content_present="1"
        rm -f "$state_file" 2>/dev/null
    fi

    # Get new content from actual file
    if [[ ! -f "$file_path" ]]; then
        log_error "File doesn't exist for post-hook: $file_path"
        return 1
    fi

    local new_content
    new_content=$(cat "$file_path" 2>/dev/null)
    if [[ $? -ne 0 ]]; then
        log_error "Failed to read file: $file_path"
        return 1
    fi

    # Skip if no actual change
    if [[ "$old_content" == "$new_content" ]]; then
        log_debug "No change detected for $file_path, skipping"
        return 0
    fi

    # Get prompt and context
    local transcript_path
    transcript_path=$(get_transcript_path)

    local prompt
    prompt=$(extract_prompt_from_transcript "$transcript_path" "AI-assisted code change")

    # Try tool input description as fallback
    if [[ -z "$prompt" || "$prompt" == "null" || "$prompt" == '""' ]]; then
        prompt=$(echo "$INPUT" | jq -r '.tool_input.description // "AI-assisted code change"' 2>/dev/null)
    fi

    local context_json
    context_json=$(extract_context_from_transcript "$transcript_path")

    # Send to whogitit
    send_to_whogitit "$tool_name" "$file_path" "$prompt" "$old_content" "$old_content_present" "$new_content" "$context_json"
}

# ============================================================================
# BASH TOOL HANDLING (multiple files via git status tracking)
# ============================================================================

# Get unique ID for this Bash command invocation
get_bash_invocation_id() {
    # Use tool_use_id if available, otherwise generate from timestamp
    local tool_use_id
    tool_use_id=$(echo "$INPUT" | jq -r '.tool_use_id // .id // ""' 2>/dev/null)
    if [[ -n "$tool_use_id" && "$tool_use_id" != "null" ]]; then
        echo "$tool_use_id"
    else
        echo "bash_$(date +%s%N)"
    fi
}

# Get list of dirty files (modified, staged, or untracked) in the repo
get_dirty_files() {
    cd "$REPO_ROOT" || return

    # Build a deduplicated list of modified/staged/untracked paths.
    # Using name-only plumbing avoids word-splitting issues with spaces.
    {
        git diff --name-only --diff-filter=ACMR 2>/dev/null
        git diff --cached --name-only --diff-filter=ACMR 2>/dev/null
        git ls-files --others --exclude-standard 2>/dev/null
    } | awk 'NF && !seen[$0]++'
}

# Snapshot all dirty files before Bash command
handle_bash_pre() {
    local bash_id="$1"
    local bash_state_subdir="$BASH_STATE_DIR/$bash_id"

    mkdir -p "$bash_state_subdir" 2>/dev/null

    # Save list of dirty files and their content
    local file_count=0
    local manifest_file="$bash_state_subdir/manifest.txt"

    # Clear manifest
    > "$manifest_file"

    while IFS= read -r rel_path; do
        if [[ -z "$rel_path" ]]; then
            continue
        fi

        local abs_path="$REPO_ROOT/$rel_path"

        # Skip if not a regular file
        if [[ ! -f "$abs_path" ]]; then
            continue
        fi

        # Skip binary files (check if file contains null bytes)
        if file "$abs_path" 2>/dev/null | grep -q "binary\|executable\|image\|archive"; then
            log_debug "Skipping binary file: $rel_path"
            continue
        fi

        # Hash the path for state file name
        local path_hash
        path_hash=$(hash_string "$rel_path")
        local state_file="$bash_state_subdir/$path_hash"

        # Save current content
        if cp "$abs_path" "$state_file" 2>/dev/null; then
            echo "$rel_path" >> "$manifest_file"
            file_count=$((file_count + 1))
        fi
    done < <(get_dirty_files)

    # Also record files that don't exist yet (will be created by command)
    # We'll detect these in post by comparing git status

    log_debug "Bash pre-hook: saved state for $file_count dirty files (id: $bash_id)"
}

# Detect changes after Bash command and capture them
handle_bash_post() {
    local bash_id="$1"
    local bash_command="$2"
    local bash_description="$3"
    local bash_state_subdir="$BASH_STATE_DIR/$bash_id"

    if [[ ! -d "$bash_state_subdir" ]]; then
        log_debug "No pre-Bash state found for id: $bash_id"
        return 0
    fi

    local manifest_file="$bash_state_subdir/manifest.txt"

    # Get prompt and context from transcript
    local transcript_path
    transcript_path=$(get_transcript_path)

    # Use bash description as prompt, with command as fallback
    local prompt
    if [[ -n "$bash_description" && "$bash_description" != "null" ]]; then
        prompt="[Bash] $bash_description"
    elif [[ -n "$bash_command" ]]; then
        # Truncate long commands
        local cmd_preview="${bash_command:0:200}"
        if [[ ${#bash_command} -gt 200 ]]; then
            cmd_preview="$cmd_preview..."
        fi
        prompt="[Bash] $cmd_preview"
    else
        prompt="[Bash] AI-executed shell command"
    fi

    local context_json
    context_json=$(extract_context_from_transcript "$transcript_path")

    local changed_count=0
    local created_count=0

    # Check files that were in pre-state manifest
    if [[ -f "$manifest_file" ]]; then
        while IFS= read -r rel_path; do
            if [[ -z "$rel_path" ]]; then
                continue
            fi

            local abs_path="$REPO_ROOT/$rel_path"
            local path_hash
            path_hash=$(hash_string "$rel_path")
            local state_file="$bash_state_subdir/$path_hash"

            # Get old content
            local old_content=""
            local old_content_present="0"
            if [[ -f "$state_file" ]]; then
                old_content=$(cat "$state_file" 2>/dev/null)
                old_content_present="1"
            fi

            # Check if file still exists
            if [[ ! -f "$abs_path" ]]; then
                # File was deleted - we don't track deletions
                log_debug "File deleted by Bash: $rel_path"
                continue
            fi

            # Get new content
            local new_content
            new_content=$(cat "$abs_path" 2>/dev/null)

            # Check if changed
            if [[ "$old_content" != "$new_content" ]]; then
                log_debug "File modified by Bash: $rel_path"
                send_to_whogitit "Bash" "$abs_path" "$prompt" "$old_content" "$old_content_present" "$new_content" "$context_json"
                changed_count=$((changed_count + 1))
            fi
        done < "$manifest_file"
    fi

    # Check for newly created files (in git status but not in manifest)
    while IFS= read -r rel_path; do
        if [[ -z "$rel_path" ]]; then
            continue
        fi

        # Skip if was in manifest (already handled above)
        if [[ -f "$manifest_file" ]] && grep -qxF "$rel_path" "$manifest_file" 2>/dev/null; then
            continue
        fi

        local abs_path="$REPO_ROOT/$rel_path"

        # Skip if not a regular file
        if [[ ! -f "$abs_path" ]]; then
            continue
        fi

        # Skip binary files
        if file "$abs_path" 2>/dev/null | grep -q "binary\|executable\|image\|archive"; then
            log_debug "Skipping new binary file: $rel_path"
            continue
        fi

        # This is a new file created by the Bash command
        local new_content
        new_content=$(cat "$abs_path" 2>/dev/null)

        if [[ -n "$new_content" ]]; then
            log_debug "File created by Bash: $rel_path"
            send_to_whogitit "Bash" "$abs_path" "$prompt" "" "0" "$new_content" "$context_json"
            created_count=$((created_count + 1))
        fi
    done < <(get_dirty_files)

    # Clean up state
    rm -rf "$bash_state_subdir" 2>/dev/null

    log_debug "Bash post-hook: $changed_count files modified, $created_count files created (id: $bash_id)"
}

# ============================================================================
# MAIN ROUTING LOGIC
# ============================================================================

case "$TOOL_NAME" in
    Edit|Write)
        # Extract file path
        FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // .file_path // ""' 2>/dev/null)

        if [[ -z "$FILE_PATH" || "$FILE_PATH" == "null" ]]; then
            log_error "Empty or null file path for $TOOL_NAME"
            exit 0
        fi

        # Make absolute path
        if [[ ! "$FILE_PATH" = /* ]]; then
            FILE_PATH="$(pwd)/$FILE_PATH"
        fi
        FILE_PATH=$(realpath "$FILE_PATH" 2>/dev/null || echo "$FILE_PATH")

        # Create state file path
        STATE_HASH=$(hash_string "$FILE_PATH")
        STATE_FILE="$STATE_DIR/$STATE_HASH"

        if [[ "$HOOK_PHASE" == "pre" ]]; then
            handle_edit_write_pre "$FILE_PATH" "$STATE_FILE"
        else
            handle_edit_write_post "$FILE_PATH" "$STATE_FILE" "$TOOL_NAME"
        fi
        ;;

    Bash)
        # Get Bash command details
        BASH_COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // ""' 2>/dev/null)
        BASH_DESCRIPTION=$(echo "$INPUT" | jq -r '.tool_input.description // ""' 2>/dev/null)
        BASH_ID=$(get_bash_invocation_id)

        log_debug "Bash command (id: $BASH_ID): ${BASH_COMMAND:0:100}..."

        if [[ "$HOOK_PHASE" == "pre" ]]; then
            handle_bash_pre "$BASH_ID"
        else
            handle_bash_post "$BASH_ID" "$BASH_COMMAND" "$BASH_DESCRIPTION"
        fi
        ;;

    *)
        # Skip other tools (Task, Read, Glob, Grep, etc.)
        log_debug "Skipping tool: $TOOL_NAME"
        exit 0
        ;;
esac

log_debug "Hook completed"
