#!/usr/bin/env bash
# Vector 12: any src/*.rs file exceeding 1500 LOC cap.
set -uo pipefail
max=1500
violations=""
while IFS= read -r line; do
  loc="${line%% *}"
  file="${line#* }"
  if [ "$loc" -gt "$max" ]; then
    violations="${violations}${file}:${loc} "
  fi
done < <(find tools -path '*/src/*.rs' -not -path '*/target/*' 2>/dev/null \
         | xargs wc -l 2>/dev/null \
         | grep -v 'total$' \
         | awk '{print $1, $2}')

if [ -n "$violations" ]; then
  echo "files over 1500 LOC: ${violations}"; exit 2
fi
echo "OK (no file >${max} LOC)"; exit 0
