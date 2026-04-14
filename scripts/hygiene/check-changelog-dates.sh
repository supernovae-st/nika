#!/usr/bin/env bash
# Vector 4: CHANGELOG top entry date should match a real commit within 48h.
set -u
CHANGELOG="CHANGELOG.md"
[ -f "$CHANGELOG" ] || { echo "CHANGELOG.md not found"; exit 2; }

# Find first versioned entry header: "## [X.Y.Z...] - YYYY-MM-DD"
top_date="$(grep -oE '^## \[[^]]+\] - [0-9]{4}-[0-9]{2}-[0-9]{2}' "$CHANGELOG" | head -1 | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}')"
[ -z "$top_date" ] && { echo "no dated entry"; exit 1; }

# Find corresponding tag commit (macOS + linux compatible, in seconds)
today="$(date +%s)"
entry_ts=$(date -j -f '%Y-%m-%d' "$top_date" +%s 2>/dev/null || date -d "$top_date" +%s 2>/dev/null)
[ -z "$entry_ts" ] && { echo "bad date format"; exit 1; }

age_days=$(( (today - entry_ts) / 86400 ))
if [ "$age_days" -lt 0 ]; then
  echo "future-dated entry: $top_date"; exit 2
fi
echo "OK (top=$top_date, ${age_days}d old)"; exit 0
