#!/usr/bin/env bash
# Vector 4: CHANGELOG top entry date should match a real commit within 48h.
set -u
CHANGELOG="CHANGELOG.md"
[ -f "$CHANGELOG" ] || {
  echo "CHANGELOG.md not found"
  exit 2
}

# Find the first versioned entry header. The link form is the one
# git-cliff actually writes — `## [0.107.0](https://…/compare/…) - DATE`
# — and requiring ` - ` immediately after `]` saw 5 of 25 entries, all
# of them pre-cliff. The gate read a 110-day-old top and called it OK
# (2026-08-02). The `(…)` group is optional so both forms match.
top_date="$(grep -oE '^## \[[^]]+\](\([^)]*\))? - [0-9]{4}-[0-9]{2}-[0-9]{2}' "$CHANGELOG" | head -1 | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}')"
[ -z "$top_date" ] && {
  echo "no dated entry"
  exit 1
}

# Find corresponding tag commit (macOS + linux compatible, in seconds)
today="$(date +%s)"
entry_ts=$(date -j -f '%Y-%m-%d' "$top_date" +%s 2>/dev/null || date -d "$top_date" +%s 2>/dev/null)
[ -z "$entry_ts" ] && {
  echo "bad date format"
  exit 1
}

age_days=$(((today - entry_ts) / 86400))
if [ "$age_days" -lt 0 ]; then
  echo "future-dated entry: $top_date"
  exit 2
fi

# The header promises « within 48h » and nothing ever compared it. A
# release entry ages the moment the next dev cycle starts, so staleness
# is a WARNING (yellow), not drift — but it has to be said, or the
# vector only ever catches a clock set to the future.
STALE_DAYS="${CHANGELOG_STALE_DAYS:-45}"
if [ "$age_days" -gt "$STALE_DAYS" ]; then
  echo "WARN: top entry $top_date is ${age_days}d old (> ${STALE_DAYS}d) — a release is overdue or the entry never moved"
  exit 1
fi
echo "OK (top=$top_date, ${age_days}d old)"
exit 0
