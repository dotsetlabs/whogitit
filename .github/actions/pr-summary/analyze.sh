#!/bin/bash
set -e

BASE_SHA="$1"
HEAD_SHA="$2"

# Find whogitit binary
WHOGITIT=$(which whogitit 2>/dev/null || echo "./target/release/whogitit")

if [ ! -x "$WHOGITIT" ]; then
  echo "Error: whogitit not found"
  echo "has_data=false" >> "$GITHUB_OUTPUT"
  exit 0
fi

# Use the summary command to analyze the commit range
if SUMMARY=$("$WHOGITIT" summary --base "$BASE_SHA" --head "$HEAD_SHA" --format json 2>/dev/null); then
  # Parse JSON output
  COMMITS_ANALYZED=$(echo "$SUMMARY" | jq -r '.commits_analyzed // 0')
  COMMITS_WITH_AI=$(echo "$SUMMARY" | jq -r '.commits_with_ai // 0')
  TOTAL_AI=$(echo "$SUMMARY" | jq -r '.lines.ai // 0')
  TOTAL_AI_MODIFIED=$(echo "$SUMMARY" | jq -r '.lines.ai_modified // 0')
  TOTAL_HUMAN=$(echo "$SUMMARY" | jq -r '.lines.human // 0')
  TOTAL_ORIGINAL=$(echo "$SUMMARY" | jq -r '.lines.original // 0')
  AI_PERCENT=$(echo "$SUMMARY" | jq -r '.ai_percentage // 0')

  if [ "$COMMITS_WITH_AI" -eq 0 ] || [ -z "$COMMITS_WITH_AI" ]; then
    echo "No commits with AI attribution found"
    echo "has_data=false" >> "$GITHUB_OUTPUT"
    exit 0
  fi

  # Output results
  echo "has_data=true" >> "$GITHUB_OUTPUT"
  echo "total_ai=$TOTAL_AI" >> "$GITHUB_OUTPUT"
  echo "total_ai_modified=$TOTAL_AI_MODIFIED" >> "$GITHUB_OUTPUT"
  echo "total_human=$TOTAL_HUMAN" >> "$GITHUB_OUTPUT"
  echo "total_original=$TOTAL_ORIGINAL" >> "$GITHUB_OUTPUT"
  echo "commits_with_ai=$COMMITS_WITH_AI" >> "$GITHUB_OUTPUT"
  echo "commit_count=$COMMITS_ANALYZED" >> "$GITHUB_OUTPUT"
  echo "ai_percent=$AI_PERCENT" >> "$GITHUB_OUTPUT"

  # Generate per-commit details for the table
  COMMIT_DETAILS=""
  COMMITS=$(git rev-list --reverse "$BASE_SHA".."$HEAD_SHA" 2>/dev/null || echo "")

  for COMMIT in $COMMITS; do
    SHORT=$(echo "$COMMIT" | cut -c1-7)

    if ATTR=$("$WHOGITIT" show "$COMMIT" --format json 2>/dev/null); then
      if echo "$ATTR" | jq -e '.files' > /dev/null 2>&1; then
        AI=$(echo "$ATTR" | jq -r '[.files[].summary.ai_lines] | add // 0')
        AI_MOD=$(echo "$ATTR" | jq -r '[.files[].summary.ai_modified_lines] | add // 0')
        HUMAN=$(echo "$ATTR" | jq -r '[.files[].summary.human_lines] | add // 0')
        FILES=$(echo "$ATTR" | jq -r '.files | length')

        if [ "${AI:-0}" != "0" ] || [ "${AI_MOD:-0}" != "0" ]; then
          MSG=$(git log -1 --format=%s "$COMMIT" 2>/dev/null | head -c 50)
          COMMIT_DETAILS="${COMMIT_DETAILS}| \`${SHORT}\` | ${MSG} | ${AI:-0} | ${AI_MOD:-0} | ${HUMAN:-0} | ${FILES:-0} |
"
        fi
      fi
    fi
  done

  echo "$COMMIT_DETAILS" > /tmp/commit_details.txt

  echo "Analysis complete:"
  echo "  Commits analyzed: $COMMITS_ANALYZED"
  echo "  Commits with AI: $COMMITS_WITH_AI"
  echo "  Total AI lines: $TOTAL_AI"
  echo "  AI-modified lines: $TOTAL_AI_MODIFIED"
  echo "  Human lines: $TOTAL_HUMAN"
  echo "  AI percentage: $AI_PERCENT%"
else
  # Fallback: analyze commits individually
  echo "Summary command failed, falling back to individual commit analysis..."

  COMMITS=$(git rev-list --reverse "$BASE_SHA".."$HEAD_SHA" 2>/dev/null || echo "")

  if [ -z "$COMMITS" ]; then
    echo "No commits to analyze"
    echo "has_data=false" >> "$GITHUB_OUTPUT"
    exit 0
  fi

  TOTAL_AI=0
  TOTAL_AI_MODIFIED=0
  TOTAL_HUMAN=0
  TOTAL_ORIGINAL=0
  COMMITS_WITH_AI=0
  COMMIT_COUNT=0
  COMMIT_DETAILS=""

  for COMMIT in $COMMITS; do
    COMMIT_COUNT=$((COMMIT_COUNT + 1))
    SHORT=$(echo "$COMMIT" | cut -c1-7)

    if ATTR=$("$WHOGITIT" show "$COMMIT" --format json 2>/dev/null); then
      if echo "$ATTR" | jq -e '.files' > /dev/null 2>&1; then
        AI=$(echo "$ATTR" | jq -r '[.files[].summary.ai_lines] | add // 0')
        AI_MOD=$(echo "$ATTR" | jq -r '[.files[].summary.ai_modified_lines] | add // 0')
        HUMAN=$(echo "$ATTR" | jq -r '[.files[].summary.human_lines] | add // 0')
        ORIGINAL=$(echo "$ATTR" | jq -r '[.files[].summary.original_lines] | add // 0')
        FILES=$(echo "$ATTR" | jq -r '.files | length')

        AI=${AI:-0}
        AI_MOD=${AI_MOD:-0}
        HUMAN=${HUMAN:-0}
        ORIGINAL=${ORIGINAL:-0}

        if [ "$AI" != "0" ] || [ "$AI_MOD" != "0" ]; then
          COMMITS_WITH_AI=$((COMMITS_WITH_AI + 1))
          TOTAL_AI=$((TOTAL_AI + AI))
          TOTAL_AI_MODIFIED=$((TOTAL_AI_MODIFIED + AI_MOD))
          TOTAL_HUMAN=$((TOTAL_HUMAN + HUMAN))
          TOTAL_ORIGINAL=$((TOTAL_ORIGINAL + ORIGINAL))

          MSG=$(git log -1 --format=%s "$COMMIT" 2>/dev/null | head -c 50)
          COMMIT_DETAILS="${COMMIT_DETAILS}| \`${SHORT}\` | ${MSG} | ${AI} | ${AI_MOD} | ${HUMAN} | ${FILES:-0} |
"
        fi
      fi
    fi
  done

  # Changed lines = AI + AI_modified + Human (NOT including original/unchanged)
  CHANGED_LINES=$((TOTAL_AI + TOTAL_AI_MODIFIED + TOTAL_HUMAN))

  if [ "$COMMITS_WITH_AI" -eq 0 ]; then
    echo "No commits with AI attribution found"
    echo "has_data=false" >> "$GITHUB_OUTPUT"
    exit 0
  fi

  # AI percentage is of CHANGED lines only
  if [ "$CHANGED_LINES" -gt 0 ]; then
    AI_PERCENT=$(echo "scale=1; ($TOTAL_AI + $TOTAL_AI_MODIFIED) * 100 / $CHANGED_LINES" | bc)
  else
    AI_PERCENT="0"
  fi

  echo "has_data=true" >> "$GITHUB_OUTPUT"
  echo "total_ai=$TOTAL_AI" >> "$GITHUB_OUTPUT"
  echo "total_ai_modified=$TOTAL_AI_MODIFIED" >> "$GITHUB_OUTPUT"
  echo "total_human=$TOTAL_HUMAN" >> "$GITHUB_OUTPUT"
  echo "total_original=$TOTAL_ORIGINAL" >> "$GITHUB_OUTPUT"
  echo "commits_with_ai=$COMMITS_WITH_AI" >> "$GITHUB_OUTPUT"
  echo "commit_count=$COMMIT_COUNT" >> "$GITHUB_OUTPUT"
  echo "ai_percent=$AI_PERCENT" >> "$GITHUB_OUTPUT"

  echo "$COMMIT_DETAILS" > /tmp/commit_details.txt

  echo "Analysis complete (fallback):"
  echo "  Commits analyzed: $COMMIT_COUNT"
  echo "  Commits with AI: $COMMITS_WITH_AI"
  echo "  Total AI lines: $TOTAL_AI"
  echo "  AI percentage: $AI_PERCENT%"
fi
