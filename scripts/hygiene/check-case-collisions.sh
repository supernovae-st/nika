#!/usr/bin/env bash
# Vector 29: Case-insensitive filename collisions in tracked files.
#
# macOS HFS+ (default) and Windows NTFS (default) are case-insensitive but
# case-preserving. Git *can* track two files that differ only in case
# (e.g. `Foo.rs` + `foo.rs`), but when that repo lands on a case-insensitive
# filesystem, one of the two will silently overwrite the other at checkout
# and a bug chain starts.
#
# This check finds any pair of tracked paths that lowercase to the same
# string within the same parent directory.
#
# Exit codes:
#   0 -- GREEN (no collisions)
#   2 -- RED (at least one collision)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# List every tracked path, lowercase it, look for duplicates.
# We group by lowercased path; >1 hit in the same group = collision.
collisions=$(git ls-files \
  | awk '{ orig=$0; lc=tolower($0); print lc "\t" orig }' \
  | sort \
  | awk -F'\t' '
      { count[$1]++; paths[$1] = paths[$1] ? paths[$1] "\n      " $2 : $2 }
      END {
        for (k in count) {
          if (count[k] > 1) {
            printf "    %s (%d paths):\n      %s\n", k, count[k], paths[k]
          }
        }
      }
    ')

if [ -n "$collisions" ]; then
  echo "RED: case-insensitive filename collision(s) detected:"
  echo "$collisions"
  echo ""
  echo "Hint: rename one of the clashing paths. On macOS/Windows default"
  echo "filesystems, only one of the two files survives checkout — the"
  echo "repo appears fine on Linux CI but breaks for half the devs."
  exit 2
fi

echo "OK: no case-insensitive filename collisions (git ls-files clean)"
exit 0
