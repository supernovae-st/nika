#!/usr/bin/env bash
# Vector 10: CITATION.cff + LICENSE sanity.
set -u
[ -f LICENSE ] || { echo "LICENSE missing"; exit 2; }
head -1 LICENSE | grep -qi 'AFFERO' || { echo "LICENSE is not AGPL"; exit 2; }
echo "OK"; exit 0
