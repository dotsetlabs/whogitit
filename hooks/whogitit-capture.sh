#!/bin/bash
# whogitit capture hook for Claude Code
# This script captures file changes for AI attribution tracking
# It reads the conversation transcript to extract actual user prompts

set -o pipefail

# Enable debug logging
DEBUG_LOG="/tmp/whogitit-hook-debug.log"
ERROR_LOG="/tmp/whogitit-hook-errors.log"

log_debug() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') [$HOOK_PHASE] $1" >> "$DEBUG_LOG"
}

log_error() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') [$HOOK_PHASE] ERROR: $1" >> "$ERROR_LOG"
    log_debug "ERROR: $1"
}

# State directory for tracking pre-edit content
STATE_DIR="${TMPDIR:-/tmp}/whogitit-state"
mkdir -p "$STATE_DIR" 2>/dev/null || {
    log_error "Failed to create state directory: $STATE_DIR"
    exit 0
}

# Clean up stale state files (older than 1 hour)
find "$STATE_DIR" -type f -mmin +60 -delete 2>/dev/null

# Read the hook input from stdin
INPUT=$(cat)
if [[ -z "$INPUT" ]]; then
    log_error "Empty input from stdin"
    exit 0
fi

# Determine hook phase (pre or post)
HOOK_PHASE="${WHOGITIT_HOOK_PHASE:-post}"

log_debug "Hook started"

# Extract tool name with fallback
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // .tool // ""' 2>/dev/null)
if [[ $? -ne 0 ]]; then
    log_error "Failed to parse JSON input"
    exit 0
fi

log_debug "Tool: $TOOL_NAME"

# Only process Edit and Write tools
if [[ "$TOOL_NAME" != "Edit" && "$TOOL_NAME" != "Write" ]]; then
    log_debug "Skipping non-Edit/Write tool"
    exit 0
fi

# Extract file path with multiple fallbacks
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // .file_path // ""' 2>/dev/null)

log_debug "File path from input: $FILE_PATH"

if [[ -z "$FILE_PATH" || "$FILE_PATH" == "null" ]]; then
    log_error "Empty or null file path in input"
    exit 0
fi

# Make absolute path
if [[ ! "$FILE_PATH" = /* ]]; then
    FILE_PATH="$(pwd)/$FILE_PATH"
fi

# Normalize path (resolve symlinks, remove ..)
FILE_PATH=$(realpath "$FILE_PATH" 2>/dev/null || echo "$FILE_PATH")

log_debug "Absolute path: $FILE_PATH"

# Hash the file path for state file name (use md5 on macOS, md5sum on Linux)
if command -v md5 &> /dev/null; then
    STATE_HASH=$(echo -n "$FILE_PATH" | md5)
elif command -v md5sum &> /dev/null; then
    STATE_HASH=$(echo -n "$FILE_PATH" | md5sum | cut -d' ' -f1)
else
    # Fallback to simple hash
    STATE_HASH=$(echo -n "$FILE_PATH" | cksum | cut -d' ' -f1)
fi
STATE_FILE="$STATE_DIR/$STATE_HASH"

log_debug "State file: $STATE_FILE"

if [[ "$HOOK_PHASE" == "pre" ]]; then
    # PRE-TOOL: Save current file content before modification
    if [[ -f "$FILE_PATH" ]]; then
        if cp "$FILE_PATH" "$STATE_FILE" 2>/dev/null; then
            LINES=$(wc -l < "$STATE_FILE" 2>/dev/null | tr -d ' ')
            log_debug "Saved pre-edit state: $LINES lines"
        else
            log_error "Failed to copy file to state: $FILE_PATH"
        fi
    else
        rm -f "$STATE_FILE" 2>/dev/null
        log_debug "File doesn't exist, removed state"
    fi
    exit 0
fi

# POST-TOOL: Capture the change

# Get old content from state file
OLD_CONTENT=""
if [[ -f "$STATE_FILE" ]]; then
    OLD_CONTENT=$(cat "$STATE_FILE" 2>/dev/null)
    if [[ $? -ne 0 ]]; then
        log_error "Failed to read state file"
        OLD_CONTENT=""
    else
        OLD_LINES=$(echo "$OLD_CONTENT" | wc -l | tr -d ' ')
        log_debug "Read old content: $OLD_LINES lines"
    fi
    rm -f "$STATE_FILE" 2>/dev/null
else
    log_debug "WARNING: No state file found (file may be new)"
fi

# Get new content from actual file
if [[ ! -f "$FILE_PATH" ]]; then
    log_error "File doesn't exist for post-hook: $FILE_PATH"
    exit 0
fi

NEW_CONTENT=$(cat "$FILE_PATH" 2>/dev/null)
if [[ $? -ne 0 ]]; then
    log_error "Failed to read file: $FILE_PATH"
    exit 0
fi
NEW_LINES=$(echo "$NEW_CONTENT" | wc -l | tr -d ' ')

log_debug "Read new content: $NEW_LINES lines"

# Skip if no actual change
if [[ "$OLD_CONTENT" == "$NEW_CONTENT" ]]; then
    log_debug "No change detected, skipping"
    exit 0
fi

# Get the prompt from the conversation transcript
# Claude Code provides transcript_path pointing to the conversation JSON
TRANSCRIPT_PATH=$(echo "$INPUT" | jq -r '.transcript_path // ""' 2>/dev/null)

PROMPT=""
if [[ -n "$TRANSCRIPT_PATH" && -f "$TRANSCRIPT_PATH" ]]; then
    # Extract the most recent user message from the transcript
    # The transcript is a JSONL file (one JSON object per line)
    # User messages have type="user" and no toolUseResult (those are tool responses)
    # The actual prompt is in .message.content (can be string or array)
    PROMPT=$(jq -s '
        [.[] | select(.type == "user" and .toolUseResult == null)] |
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
    ' "$TRANSCRIPT_PATH" 2>/dev/null | head -c 2000)
    log_debug "Extracted prompt from transcript: ${PROMPT:0:100}..."
fi

# Fallback to tool input description or default
if [[ -z "$PROMPT" || "$PROMPT" == "null" ]]; then
    PROMPT=$(echo "$INPUT" | jq -r '.tool_input.description // ""' 2>/dev/null)
fi

if [[ -z "$PROMPT" || "$PROMPT" == "null" ]]; then
    PROMPT="AI-assisted code change"
fi

log_debug "Prompt: ${PROMPT:0:100}..."

# Check if whogitit is available
WHOGITIT_BIN="${WHOGITIT_BIN:-$HOME/.cargo/bin/whogitit}"
if [[ ! -x "$WHOGITIT_BIN" ]]; then
    log_error "whogitit binary not found at: $WHOGITIT_BIN"
    exit 0
fi

# Build and send to whogitit
capture_result=""
if [[ -z "$OLD_CONTENT" ]]; then
    log_debug "Sending as NEW file"
    capture_result=$(jq -n \
        --arg tool "$TOOL_NAME" \
        --arg file_path "$FILE_PATH" \
        --arg prompt "$PROMPT" \
        --arg new_content "$NEW_CONTENT" \
        '{
            tool: $tool,
            file_path: $file_path,
            prompt: $prompt,
            old_content: null,
            new_content: $new_content
        }' 2>/dev/null | "$WHOGITIT_BIN" capture --stdin 2>&1)
else
    log_debug "Sending as MODIFIED file"
    capture_result=$(jq -n \
        --arg tool "$TOOL_NAME" \
        --arg file_path "$FILE_PATH" \
        --arg prompt "$PROMPT" \
        --arg old_content "$OLD_CONTENT" \
        --arg new_content "$NEW_CONTENT" \
        '{
            tool: $tool,
            file_path: $file_path,
            prompt: $prompt,
            old_content: $old_content,
            new_content: $new_content
        }' 2>/dev/null | "$WHOGITIT_BIN" capture --stdin 2>&1)
fi

capture_exit=$?
if [[ $capture_exit -ne 0 ]]; then
    log_error "whogitit capture failed (exit $capture_exit): $capture_result"
else
    if [[ -n "$capture_result" ]]; then
        log_debug "capture output: $capture_result"
    fi
fi

log_debug "Hook completed"
