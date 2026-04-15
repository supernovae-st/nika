#!/usr/bin/env bash
# Vector 3: src LOC vs MEMORY.md (< 2% drift = yellow, > 2% = red).
set -u
MEMORY="$HOME/.claude/projects/-Users-thibaut-dev-supernovae-hq/memory/MEMORY.md"

# shellcheck disable=SC2038  # .rs filenames are ASCII-only, no need for -print0
actual="$(find crates -path '*/src/*.rs' -not -path '*/target/*' 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')"
recorded="$(grep -oE 'Total engine src LOC: ~?[0-9.,]+k?' "$MEMORY" 2>/dev/null | head -1 | grep -oE '[0-9.,]+k?' | head -1)"
# Handle "~13.5k" → 13500
if [[ "$recorded" == *k ]]; then
  recorded=$(awk -v n="${recorded%k}" 'BEGIN{print int(n * 1000)}')
fi

if [ -z "$recorded" ] || [ -z "$actual" ]; then
  echo "cannot determine LOC"
  exit 1
fi

drift_pct=$(awk -v a="$actual" -v r="$recorded" 'BEGIN{if(r==0){print 100}else{d=(a-r)/r*100;if(d<0)d=-d;printf "%.1f",d}}')
drift_cmp=$(awk -v d="$drift_pct" 'BEGIN{if(d>5){print "red"}else if(d>1){print "yellow"}else{print "green"}}')

case "$drift_cmp" in
  green)
    echo "OK ($actual LOC)"
    exit 0
    ;;
  yellow)
    echo "drift ${drift_pct}%: MEMORY=$recorded actual=$actual"
    exit 1
    ;;
  red)
    echo "drift ${drift_pct}%: MEMORY=$recorded actual=$actual"
    exit 2
    ;;
esac
