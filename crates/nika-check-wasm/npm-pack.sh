#!/usr/bin/env bash
# npm-pack.sh — the npm artifact, projected from the tree (never authored).
#
# Everything npm sees is derived here at pack time: package.json comes from
# `cargo metadata`, LICENSE from the workspace root, README from this crate.
# There is no checked-in manifest to drift from the crate — the projection IS
# the source (docs/plans/2026-07-28-check-wasm-npm-distribution.md §3).
#
# Version honesty: a release-versioned package only ever leaves a tagged
# tree. An untagged run packs an explicit `-dev.g<sha>` prerelease that
# cannot impersonate a release — the lineage lie (an npm "0.106.0" built
# from a moved main) is unrepresentable, not just forbidden.
set -euo pipefail
cd "$(dirname "$0")"

# distribution is stricter than a local build: build-wasm.sh tolerates a
# missing binder/optimizer (a dev box may stop at the .wasm) — a package
# may not, so the tools are preconditions here
command -v npm >/dev/null || {
  echo "npm missing"
  exit 2
}
command -v wasm-opt >/dev/null || {
  echo "wasm-opt (binaryen) missing — the shipped artifact is always optimized"
  exit 2
}
command -v wasm-bindgen >/dev/null || {
  echo "wasm-bindgen-cli missing — cargo install wasm-bindgen-cli (match Cargo.lock)"
  exit 2
}

# the binder must match the lock — drifted glue is glue the lock never
# tested (build-wasm.sh states the law; this script enforces it)
lock_bindgen="$(grep -A1 'name = "wasm-bindgen"$' ../../Cargo.lock | sed -n 's/version = "\(.*\)"/\1/p')"
have_bindgen="$(wasm-bindgen --version | awk '{print $2}')"
if [ "$lock_bindgen" != "$have_bindgen" ]; then
  echo "wasm-bindgen ${have_bindgen} ≠ Cargo.lock ${lock_bindgen} — install the lock's version"
  exit 2
fi

# the tag must BE this version's tag — `--exact-match` alone accepts any
# tag on HEAD (a stray `wip` would mint a release-versioned tarball on a
# dev box, one `npm publish` away from permanent)
crate_version="$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys;print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="nika-check-wasm"))')"
suffix=""
if [ "$(git describe --tags --exact-match 2>/dev/null || true)" != "v${crate_version}" ]; then
  suffix="-dev.g$(git rev-parse --short=12 HEAD)"
  echo "HEAD is not v${crate_version} — packing a ${suffix} prerelease (a release version only ever leaves its own tag)"
fi

bash ./build-wasm.sh

rm -rf dist-npm
mkdir dist-npm
cp pkg/nika_check_wasm.js pkg/nika_check_wasm.d.ts pkg/nika_check_wasm_bg.wasm dist-npm/
if [ -f pkg/nika_check_wasm_bg.wasm.d.ts ]; then
  cp pkg/nika_check_wasm_bg.wasm.d.ts dist-npm/
fi
cp README.md dist-npm/README.md
cp ../../LICENSE dist-npm/LICENSE

NIKA_NPM_SUFFIX="$suffix" python3 - <<'PY'
import json, os, subprocess

meta = json.loads(subprocess.run(
    ["cargo", "metadata", "--no-deps", "--format-version", "1"],
    capture_output=True, text=True, check=True).stdout)
pkg = next(p for p in meta["packages"] if p["name"] == "nika-check-wasm")

# the Cargo description is a paragraph; npm gets its first sentence
# (the first ". " lands after "row shape" — the file citations with
# dots all live in later sentences)
description = pkg["description"].split(". ")[0]
license_id = pkg["license"]
if not license_id:
    raise SystemExit("crate has no license field — refusing to project")

manifest = {
    "name": "@supernovae-st/nika-check-wasm",
    "version": pkg["version"] + os.environ.get("NIKA_NPM_SUFFIX", ""),
    "description": description,
    "license": license_id,
    "type": "module",
    "main": "nika_check_wasm.js",
    "types": "nika_check_wasm.d.ts",
    "exports": {
        ".": {
            "types": "./nika_check_wasm.d.ts",
            "default": "./nika_check_wasm.js",
        },
        "./nika_check_wasm_bg.wasm": "./nika_check_wasm_bg.wasm",
    },
    "sideEffects": False,
    # the floor the README's Node example actually needs (import.meta.resolve,
    # WebAssembly, ESM) — a hint for npm's UI, not an enforcement
    "engines": {"node": ">=18"},
    "repository": {
        "type": "git",
        # VERBATIM from the workspace manifest — npm provenance validates
        # this field against the publishing repo (its #1 documented failure
        # mode), so it carries zero decoration: no git+ prefix, no .git
        # suffix, nothing that could disagree with the OIDC claim
        "url": pkg["repository"],
        "directory": "crates/nika-check-wasm",
    },
    # deliberately NOT the workspace homepage: the package's home is the
    # page where the artifact runs, not the studio site
    "homepage": "https://nika.sh/play",
    "keywords": ["nika", "workflow", "checker", "wasm", "yaml", "static-analysis"],
    "publishConfig": {"access": "public", "provenance": True},
}
with open("dist-npm/package.json", "w") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")
print(f"package.json projected · {manifest['name']}@{manifest['version']}")
PY

(cd dist-npm && npm pack --silent)
# glob, not ls (SC2012) — rm -rf + mkdir above guarantees exactly one match.
# The sidecar records the BARE name: gh release upload attaches files flat,
# so a path-bearing sidecar could never verify on the consumer's disk.
tarballs=(dist-npm/*.tgz)
name="$(basename "${tarballs[0]}")"
(cd dist-npm && { shasum -a 256 "$name" || sha256sum "$name"; } >"${name}.sha256")
echo "packed · dist-npm/${name}"
cat "dist-npm/${name}.sha256"
