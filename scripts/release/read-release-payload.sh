#!/usr/bin/env bash
# Download original payload bytes with the existing draft owner's authority.
# No signature claim here: the read-only selector verifies tag attestations.
set -euo pipefail
if [ "$#" -ne 5 ]; then
  echo "usage: $0 <owner/repo> <release-id> <tag> <sha> <new-output-dir>" >&2
  exit 64
fi
repo="$1"
release_id="$2"
tag="$3"
sha="$4"
output="$5"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[ ! -e "$output" ] && [ ! -L "$output" ] || {
  echo 'release payload: output already exists' >&2
  exit 73
}
scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
mkdir "$scratch/payload"
read_inventory() {
  bash "$here/read-release-state.sh" "$repo" "$release_id" "$tag" "$sha" >/dev/null
  gh api "repos/${repo}/releases/${release_id}/assets" --paginate \
    --jq '.[] | [.id, .name] | @tsv' >"$scratch/raw"
  python3 "$here/release-payload.py" inventory "$tag" "$scratch/raw"
}
read_inventory >"$scratch/before"
while IFS=$'\t' read -r asset_id name; do
  [ "$name" != multiple.intoto.jsonl ] || continue
  gh api "repos/${repo}/releases/assets/${asset_id}" \
    -H 'Accept: application/octet-stream' >"$scratch/payload/$name"
done <"$scratch/before"
python3 "$here/release-payload.py" validate "$tag" "$scratch/payload"
read_inventory >"$scratch/after"
cmp -s "$scratch/before" "$scratch/after" || {
  echo 'release payload: asset identity changed during download' >&2
  exit 73
}
bash "$here/read-release-state.sh" "$repo" "$release_id" "$tag" "$sha" >/dev/null
python3 - "$scratch/payload" "$output" <<'PY'
import shutil
import sys

shutil.copytree(sys.argv[1], sys.argv[2])
PY
