#!/usr/bin/env bash
# Regression proof for the release-wave composition and carrier contract.
set -euo pipefail

# A test that runs git must not let the CALLER'S git environment redirect it.
# `git -C` scopes the directory, never the index: an inherited GIT_INDEX_FILE
# (or GIT_DIR / GIT_WORK_TREE) points this test's own fixture commits at the
# repository it was invoked from — measured as a `fixture` commit riding a
# real branch while the test printed PASS (issue 1237). Hooks export these.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX GIT_COMMON_DIR \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE \
  GIT_QUARANTINE_PATH

# Snapshot the repository the test was invoked from, if any, so the exit trap
# can prove the run never touched it. The tree may legitimately be dirty
# before the run; the contract is equality, not emptiness.
CALLER_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
CALLER_HEAD=""
CALLER_STATUS=""
if [ -n "$CALLER_ROOT" ]; then
  CALLER_HEAD="$(git -C "$CALLER_ROOT" rev-parse HEAD)"
  CALLER_STATUS="$(git -C "$CALLER_ROOT" status --porcelain)"
fi

verify_caller_untouched() {
  [ -n "$CALLER_ROOT" ] || return 0
  if [ "$(git -C "$CALLER_ROOT" rev-parse HEAD)" != "$CALLER_HEAD" ]; then
    printf 'release-tooling.test: FAIL · the test moved HEAD in %s\n' \
      "$CALLER_ROOT" >&2
    exit 1
  fi
  if [ "$(git -C "$CALLER_ROOT" status --porcelain)" != "$CALLER_STATUS" ]; then
    printf 'release-tooling.test: FAIL · the test wrote into %s\n' \
      "$CALLER_ROOT" >&2
    exit 1
  fi
}

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
TEST_ROOT="$(mktemp -d)"
FIXTURE="$TEST_ROOT/clean"
DIRTY_FIXTURE="$TEST_ROOT/dirty"
ROLLBACK_FIXTURE="$TEST_ROOT/rollback"
trap 'rm -rf "$TEST_ROOT"; verify_caller_untouched' EXIT

fail() {
  printf 'release-tooling.test: %s\n' "$1" >&2
  exit 1
}

mkdir -p \
  "$FIXTURE/.agents/plugins/nika/.claude-plugin" \
  "$FIXTURE/.agents/plugins/nika/.codex-plugin" \
  "$FIXTURE/.agents/plugins/nika/.cursor-plugin" \
  "$FIXTURE/.claude" \
  "$FIXTURE/changelog.d" \
  "$FIXTURE/crates/nika-acp/src" \
  "$FIXTURE/crates/nika-core/src" \
  "$FIXTURE/fuzz/src"

cat >"$FIXTURE/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/nika-core"]
exclude = ["crates/nika-acp", "fuzz"]
resolver = "2"

[workspace.package]
version = "0.114.0"
EOF
cat >"$FIXTURE/crates/nika-core/Cargo.toml" <<'EOF'
[package]
name = "nika-core"
version.workspace = true
edition = "2021"
EOF
printf '' >"$FIXTURE/crates/nika-core/src/lib.rs"
cat >"$FIXTURE/crates/nika-acp/Cargo.toml" <<'EOF'
[package]
name = "nika-acp"
version = "0.114.0"
edition = "2021"

[dependencies]
nika-core = { path = "../nika-core" }
EOF
printf '' >"$FIXTURE/crates/nika-acp/src/lib.rs"
cat >"$FIXTURE/fuzz/Cargo.toml" <<'EOF'
[package]
name = "nika-fuzz"
version = "0.0.0"
edition = "2021"

[dependencies]
nika-core = { path = "../crates/nika-core" }
EOF
printf '' >"$FIXTURE/fuzz/src/lib.rs"

cargo generate-lockfile -q --manifest-path "$FIXTURE/Cargo.toml"
cargo generate-lockfile -q --manifest-path "$FIXTURE/fuzz/Cargo.toml"
cargo generate-lockfile -q --manifest-path "$FIXTURE/crates/nika-acp/Cargo.toml"

cat >"$FIXTURE/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

One file per change under [`changelog.d/`](changelog.d/).

## [0.114.0](https://github.com/supernovae-st/nika/compare/v0.113.0..v0.114.0) - 2026-08-23
EOF
printf '%s\n' '- **Release composition.** One fragment becomes one release bullet.' \
  >"$FIXTURE/changelog.d/1.changed.md"
printf '%s\n' '# Changelog fragments' >"$FIXTURE/changelog.d/README.md"
cat >"$FIXTURE/.agents/plugins/nika/CHANGELOG.md" <<'EOF'
# Nika kit changelog

## 0.114.0 — 2026-08-23
EOF
for manifest in \
  "$FIXTURE/.agents/plugins/nika/plugin.json" \
  "$FIXTURE/.agents/plugins/nika/.claude-plugin/plugin.json" \
  "$FIXTURE/.agents/plugins/nika/.codex-plugin/plugin.json" \
  "$FIXTURE/.agents/plugins/nika/.cursor-plugin/plugin.json"; do
  printf '%s\n' '{"version": "0.114.0"}' >"$manifest"
done
printf '%s\n' 'RUN curl https://example.invalid/install.sh | sh -s -- v=0.114.0' \
  >"$FIXTURE/Dockerfile"
# Backticks are the literal Markdown carrier.
# shellcheck disable=SC2016
printf '%s\n' '| workspace | v0.114.0 |' '| branch | `chore/release-0.114.0` |' \
  >"$FIXTURE/ROADMAP.md"
cp "$FIXTURE/ROADMAP.md" "$FIXTURE/.claude/CLAUDE.md"

git -C "$FIXTURE" init -q
git -C "$FIXTURE" add changelog.d/1.changed.md changelog.d/README.md
git -C "$FIXTURE" -c user.name=fixture -c user.email=fixture@example.invalid \
  commit -q -m fixture

cp -R "$FIXTURE" "$DIRTY_FIXTURE"
cp -R "$FIXTURE" "$ROLLBACK_FIXTURE"

tree_digest() {
  local repo="$1"
  find "$repo" -path "$repo/.git" -prune -o -type f -print \
    | LC_ALL=C sort \
    | while IFS= read -r path; do shasum -a 256 "$path"; done \
    | shasum -a 256 | awk '{print $1}'
}

engine_headings() {
  grep -c '^## \[0\.115\.0\]' "$1/CHANGELOG.md" || true
}

kit_headings() {
  grep -c '^## 0\.115\.0 — ' "$1/.agents/plugins/nika/CHANGELOG.md" || true
}

# Dry -> apply -> immediate retry is one idempotent operation. The dry pass
# must be byte-inert, and neither apply nor retry may duplicate either heading.
before_dry="$(tree_digest "$FIXTURE")"
CARGO_NET_OFFLINE=true SPN_WAVE_REPO="$FIXTURE" \
  bash "$ROOT/scripts/release/wave-sweep.sh" 0.115.0 --dry >/dev/null
[ "$(tree_digest "$FIXTURE")" = "$before_dry" ] \
  || fail 'a dry sweep changed the fixture tree'
[ "$(engine_headings "$FIXTURE")" -eq 0 ] \
  || fail 'a dry sweep created an engine heading'
[ "$(kit_headings "$FIXTURE")" -eq 0 ] \
  || fail 'a dry sweep created a kit heading'

CARGO_NET_OFFLINE=true SPN_WAVE_REPO="$FIXTURE" \
  bash "$ROOT/scripts/release/wave-sweep.sh" 0.115.0 >/dev/null

[ "$(engine_headings "$FIXTURE")" -eq 1 ] \
  || fail 'a release sweep must create exactly one engine release heading'
[ "$(kit_headings "$FIXTURE")" -eq 1 ] \
  || fail 'a release sweep must create exactly one kit release heading'
[ ! -e "$FIXTURE/changelog.d/1.changed.md" ] \
  || fail 'a release sweep must consume the folded fragments'
git -C "$FIXTURE" diff --cached --name-status -- changelog.d/1.changed.md \
  | grep -q '^D' || fail 'a release sweep must stage each consumed fragment deletion'

CARGO_NET_OFFLINE=true SPN_WAVE_REPO="$FIXTURE" \
  bash "$ROOT/scripts/release/wave-sweep.sh" 0.115.0 >/dev/null
[ "$(engine_headings "$FIXTURE")" -eq 1 ] \
  || fail 'an immediate retry duplicated the engine heading'
[ "$(kit_headings "$FIXTURE")" -eq 1 ] \
  || fail 'an immediate retry duplicated the kit heading'
if SPN_CHANGELOG_REPO="$FIXTURE" \
  bash "$ROOT/scripts/release/changelog-assemble.sh" --fold 0.115.0 \
  >"$FIXTURE/direct-retry.out" 2>&1; then
  fail 'the fragment assembler accepted an existing exact release heading'
fi
grep -q 'already exists' "$FIXTURE/direct-retry.out" \
  || fail 'the fragment assembler did not name the duplicate-heading refusal'
[ "$(engine_headings "$FIXTURE")" -eq 1 ] \
  || fail 'a refused direct fold changed the engine heading count'

# A valid local edit belongs to the bullet that is folded. Consuming that
# tracked fragment must stage its deletion without refusing after the heading
# has already been written.
printf '%s\n' '  locally refined before the release fold.' \
  >>"$DIRTY_FIXTURE/changelog.d/1.changed.md"
CARGO_NET_OFFLINE=true SPN_WAVE_REPO="$DIRTY_FIXTURE" \
  bash "$ROOT/scripts/release/wave-sweep.sh" 0.115.0 >/dev/null
[ "$(engine_headings "$DIRTY_FIXTURE")" -eq 1 ] \
  || fail 'a locally refined fragment did not produce exactly one engine heading'
[ "$(kit_headings "$DIRTY_FIXTURE")" -eq 1 ] \
  || fail 'a locally refined fragment did not produce exactly one kit heading'
grep -q 'locally refined before the release fold' "$DIRTY_FIXTURE/CHANGELOG.md" \
  || fail 'the fold lost the valid local fragment refinement'
git -C "$DIRTY_FIXTURE" diff --cached --name-status -- changelog.d/1.changed.md \
  | grep -q '^D' || fail 'the locally refined fragment deletion was not staged'

# An injected failure immediately after the changelog rewrite must restore the
# changelog, every fragment byte, and the index. A normal retry then completes
# once, without a self-compare link or a duplicate engine/kit heading.
printf '%s\n' '  locally refined before the injected failure.' \
  >>"$ROLLBACK_FIXTURE/changelog.d/1.changed.md"
rollback_changelog="$(shasum -a 256 "$ROLLBACK_FIXTURE/CHANGELOG.md" | awk '{print $1}')"
rollback_index="$(git -C "$ROLLBACK_FIXTURE" rev-parse --git-path index)"
case "$rollback_index" in
  /*) ;;
  *) rollback_index="$ROLLBACK_FIXTURE/$rollback_index" ;;
esac
rollback_index_digest="$(shasum -a 256 "$rollback_index" | awk '{print $1}')"
if CARGO_NET_OFFLINE=true SPN_CHANGELOG_FAIL_AFTER_WRITE=1 \
  SPN_WAVE_REPO="$ROLLBACK_FIXTURE" \
  bash "$ROOT/scripts/release/wave-sweep.sh" 0.115.0 >/dev/null 2>&1; then
  fail 'the injected post-rewrite failure unexpectedly succeeded'
fi
[ "$(shasum -a 256 "$ROLLBACK_FIXTURE/CHANGELOG.md" | awk '{print $1}')" = \
  "$rollback_changelog" ] || fail 'a failed fold did not restore CHANGELOG.md'
[ "$(shasum -a 256 "$rollback_index" | awk '{print $1}')" = \
  "$rollback_index_digest" ] || fail 'a failed fold did not restore the git index'
[ "$(engine_headings "$ROLLBACK_FIXTURE")" -eq 0 ] \
  || fail 'a failed fold left an engine heading behind'
[ "$(kit_headings "$ROLLBACK_FIXTURE")" -eq 0 ] \
  || fail 'a failed fold left a kit heading behind'
grep -q 'locally refined before the injected failure' \
  "$ROLLBACK_FIXTURE/changelog.d/1.changed.md" \
  || fail 'a failed fold did not restore the locally modified fragment'
if git -C "$ROLLBACK_FIXTURE" diff --cached --name-status -- changelog.d/1.changed.md \
  | grep -q '^D'; then
  fail 'a failed fold left the fragment deletion staged'
fi

CARGO_NET_OFFLINE=true SPN_WAVE_REPO="$ROLLBACK_FIXTURE" \
  bash "$ROOT/scripts/release/wave-sweep.sh" 0.115.0 >/dev/null
[ "$(engine_headings "$ROLLBACK_FIXTURE")" -eq 1 ] \
  || fail 'retry after rollback did not create exactly one engine heading'
[ "$(kit_headings "$ROLLBACK_FIXTURE")" -eq 1 ] \
  || fail 'retry after rollback did not create exactly one kit heading'
if grep -q 'compare/v0\.115\.0\.\.v0\.115\.0' "$ROLLBACK_FIXTURE/CHANGELOG.md"; then
  fail 'retry after rollback created a self-compare release link'
fi

for lock in Cargo.lock fuzz/Cargo.lock crates/nika-acp/Cargo.lock; do
  awk '
    /^name = "nika-core"$/ { found=1; next }
    found && /^version = / { gsub(/"/, "", $3); exit($3 == "0.115.0" ? 0 : 1) }
    END { if (!found) exit 1 }
  ' "$FIXTURE/$lock" || fail "$lock did not move nika-core to 0.115.0"
done

uniform="$(SPN_VERSION_REPO="$FIXTURE" bash "$ROOT/scripts/ci/check-version-uniform.sh")"
printf '%s\n' "$uniform" | grep -q 'locks 3/3' \
  || fail 'the uniformity verdict must disclose all three checked lock families'

perl -0pi -e 's/(name = "nika-core"\nversion = ")0\.115\.0/${1}0.114.0/' \
  "$FIXTURE/crates/nika-acp/Cargo.lock"
if SPN_VERSION_REPO="$FIXTURE" bash "$ROOT/scripts/ci/check-version-uniform.sh" \
  >"$FIXTURE/uniform.out" 2>&1; then
  fail 'the uniformity gate accepted a stale excluded-crate lock'
fi
grep -q 'crates/nika-acp/Cargo.lock disagrees' "$FIXTURE/uniform.out" \
  || fail 'the uniformity failure did not name the stale excluded-crate lock'

grep -q 'crates/nika-acp/Cargo.lock' "$ROOT/RELEASING.md" \
  || fail 'the canonical carrier list omits crates/nika-acp/Cargo.lock'
grep -q 'before declaring the release complete' "$ROOT/RELEASING.md" \
  || fail 'the ceremony does not say when the timeline record closes the release'
concurrency_block="$(sed -n '/^concurrency:/,/^env:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$concurrency_block" | grep -Fqx '  group: nika-release-train' \
  || fail 'live and replay release trains do not share one publication lane'
if printf '%s\n' "$concurrency_block" \
  | grep -Eq 'github\.event\.inputs\.tag|github\.ref_name'; then
  fail 'release concurrency is still partitioned by tag'
fi
printf '%s\n' "$concurrency_block" | grep -Fqx '  cancel-in-progress: false' \
  || fail 'the release workflow may cancel a train after an irreversible write'
coordinate_job="$(sed -n '/^  coordinate:/,/^  build:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$coordinate_job" | grep -q 'WORKFLOW_REF:.*github.ref' \
  || fail 'manual replay does not inspect the selected workflow ref'
printf '%s\n' "$coordinate_job" | grep -q 'refs/heads/main' \
  || fail 'manual replay does not require the current main workflow guards'
printf '%s\n' "$coordinate_job" | grep -q 'TAG:.*github.event.inputs.tag.*github.ref_name' \
  || fail 'the first release gate does not select the publication tag'
printf '%s\n' "$coordinate_job" | grep -q 'scripts/release/check-release-tag.sh' \
  || fail 'the first release gate bypasses the canonical coordinate validator'
printf '%s\n' "$coordinate_job" | grep -q 'refs/tags/.*\^{}' \
  || fail 'the first release gate does not peel the selected tag'
printf '%s\n' "$coordinate_job" | grep -q 'github.sha' \
  || fail 'tag push does not bind the peeled commit to github.sha'
# The GitHub context is matched literally.
# shellcheck disable=SC2016
printf '%s\n' "$coordinate_job" | grep -q 'ref: \${{ github.workflow_sha }}' \
  || fail 'the first release gate does not use the workflow commit validator'
printf '%s\n' "$coordinate_job" | grep -q 'persist-credentials: false' \
  || fail 'the read-only release-coordinate checkout persists credentials'
build_job="$(sed -n '/^  build:/,/^  release-draft:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$build_job" | grep -q '^    needs: coordinate' \
  || fail 'release builds can bypass the replay workflow-ref guard'
# The GitHub context is matched literally.
# shellcheck disable=SC2016
printf '%s\n' "$build_job" | grep -q 'ref: \${{ needs.coordinate.outputs.sha }}' \
  || fail 'release builders do not consume the frozen commit sha'
for tag in v1.0.0 v1.0.0-rc.1 v0.80.0-alpha.1; do
  bash "$ROOT/scripts/release/check-release-tag.sh" "$tag" \
    || fail "canonical publication coordinate $tag was refused"
done
for tag in latest v1.0.0-rc.01 v1.0.0+build v01.0.0; do
  if bash "$ROOT/scripts/release/check-release-tag.sh" "$tag" \
    >"$TEST_ROOT/tag-check.out" 2>&1; then
    fail "non-canonical publication coordinate $tag passed the first gate"
  fi
  grep -Fq "invalid canonical semver tag: $tag" "$TEST_ROOT/tag-check.out" \
    || fail "the first gate did not explain the rejected coordinate $tag"
done
grep -q -- '--ref main' "$ROOT/RELEASING.md" \
  || fail 'the canonical replay ceremony does not select current guards'
grep -q -- '--ref main' "$ROOT/docs/RELEASING.md" \
  || fail 'the public replay guide does not select current guards'
for label in \
  org.opencontainers.image.revision \
  org.opencontainers.image.version \
  org.opencontainers.image.source; do
  grep -q "$label" "$ROOT/.github/workflows/release.yml" \
    || fail "the release image omits OCI label $label"
done
if grep -q -- '--clobber' "$ROOT/.github/workflows/release.yml"; then
  fail 'the release workflow can overwrite bytes under an occupied asset name'
fi
release_job="$(sed -n '/^  release-draft:/,/^  provenance:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$release_job" | grep -q 'prepare-draft-release.sh' \
  || fail 'the GitHub release is not prepared through the 404-only draft guard'
if grep -q 'target_commitish' "$ROOT/scripts/release/prepare-draft-release.sh"; then
  fail 'existing release identity still trusts target_commitish'
fi
printf '%s\n' "$release_job" | grep -q "if: github.event_name == 'push'" \
  || fail 'native attestations can be regenerated by manual replay'
if printf '%s\n' "$release_job" | grep -q 'discussion-category'; then
  fail 'the draft preparer starts a discussion before finalization'
fi
npm_publish_job="$(sed -n '/^  npm-wasm-publish:/,/^  docker:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$npm_publish_job" | grep -q 'actions/checkout@' \
  || fail 'the isolated npm publish job cannot access the immutable asset helper'
# The GitHub context is matched literally.
# shellcheck disable=SC2016
printf '%s\n' "$npm_publish_job" | grep -q 'ref: \${{ github.workflow_sha }}' \
  || fail 'a historical replay reads its helper from the old release tree'
printf '%s\n' "$npm_publish_job" | grep -q 'npm-publish-immutable.sh' \
  || fail 'the npm publish job bypasses exact SRI convergence'
printf '%s\n' "$npm_publish_job" | grep -q 'persist-credentials: false' \
  || fail 'the elevated npm publish checkout persists its write credential'
printf '%s\n' "$npm_publish_job" | grep -q "if: github.event_name == 'push'" \
  || fail 'the npm tarball attestation can run on manual replay'
provenance_job="$(sed -n '/^  provenance:/,/^  provenance-publish:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$provenance_job" | grep -q 'upload-assets: false' \
  || fail 'the upstream SLSA uploader can delete and replace occupied provenance'
printf '%s\n' "$provenance_job" | grep -q '^      contents: read' \
  || fail 'the SLSA generator lacks read-only repository identity access'
if printf '%s\n' "$provenance_job" | grep -q '^      contents: write'; then
  fail 'the SLSA generator retains unnecessary release write authority'
fi
printf '%s\n' "$provenance_job" | grep -q "if: github.event_name == 'push'" \
  || fail 'manual replay can generate branch-context provenance for an old tag'
provenance_publish_job="$(sed -n '/^  provenance-publish:/,/^  provenance-replay-check:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$provenance_publish_job" | grep -q 'verify-slsa-provenance.sh' \
  || fail 'push provenance is not cryptographically source/subject verified'
printf '%s\n' "$provenance_publish_job" | grep -q 'upload-assets-immutable.sh' \
  || fail 'push provenance is not immutably staged on the draft immediately'
provenance_replay_job="$(sed -n '/^  provenance-replay-check:/,/^  provenance-result:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$provenance_replay_job" | grep -q 'verify-slsa-provenance.sh' \
  || fail 'prior-run provenance verification trusts non-empty bytes only'
[ "$(grep -c 'slsa-framework/slsa-verifier/actions/installer@ea584f4502babc6f60d9bc799dbbb13c1caa9ee6' \
  "$ROOT/.github/workflows/release.yml")" -eq 3 ] \
  || fail 'the official SLSA verifier installer SHA is not pinned in all three judges'
provenance_result_job="$(sed -n '/^  provenance-result:/,/^  bump-formula:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$provenance_result_job" | grep -q 'if: always()' \
  || fail 'event-specific provenance results can be hidden by skipped needs'
printf '%s\n' "$provenance_result_job" | grep -q 'PUSH_RESULT' \
  || fail 'push provenance result is not selected explicitly'
printf '%s\n' "$provenance_result_job" | grep -q 'REPLAY_RESULT' \
  || fail 'replay provenance result is not selected explicitly'
finalizer="$(sed -n '/^  finalize:/,/^  move-latest:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$finalizer" | grep -q 'if: always()' \
  || fail 'finalizer can be skipped by a failed prerequisite'
printf '%s\n' "$finalizer" | grep -q 'release-assets-barrier.sh' \
  || fail 'finalizer bypasses the exact eight-asset allowlist'
printf '%s\n' "$finalizer" | grep -q 'verify-release-attestations.sh' \
  || fail 'finalizer omits five source-bound attestations'
printf '%s\n' "$finalizer" | grep -q 'read-release-state.sh' \
  || fail 'finalizer does not re-read release ID/tag/draft/prerelease state'
printf '%s\n' "$finalizer" | grep -q 'release-digest-marker.sh' \
  || fail 'finalizer does not re-read the persisted GHCR digest'
printf '%s\n' "$finalizer" | grep -q 'finalize-release.sh' \
  || fail 'finalizer bypasses the idempotent release-ID transition helper'
printf '%s\n' "$finalizer" | grep -q 'transitioned:' \
  || fail 'finalizer does not expose whether this run published the draft'
if printf '%s\n' "$finalizer" | grep -q 'uses: docker/'; then
  fail 'a third-party Docker action receives finalizer release-write authority'
fi
printf '%s\n' "$finalizer" | grep -q '^      packages: read' \
  || fail 'self-contained finalizer lacks read-only GHCR authority'
printf '%s\n' "$finalizer" | grep -q 'docker login ghcr.io' \
  || fail 'self-contained finalizer does not authenticate the first-party Docker CLI'
# Shell variables are matched literally in the workflow source.
# shellcheck disable=SC2016
printf '%s\n' "$finalizer" | grep -q '"\$ARTIFACTS" "\$IMAGE"' \
  || fail 'workflow does not pass artifacts and image into the finalizer contract'
finalizer_helper="$(cat "$ROOT/scripts/release/finalize-release.sh")"
for proof in \
  release-assets-barrier.sh \
  verify-release-attestations.sh \
  verify-slsa-provenance.sh \
  npm-publish-immutable.sh \
  release-digest-marker.sh \
  oci-coordinate-immutable.sh \
  verify-oci-payload.sh; do
  printf '%s\n' "$finalizer_helper" | grep -q "$proof" \
    || fail "finalizer helper does not independently rerun ${proof}"
done
bump_job="$(sed -n '/^  bump-formula:/,/^  npm-wasm-pack:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$bump_job" | grep -q '^    needs: finalize' \
  || fail 'Homebrew does not depend on finalization'
if printf '%s\n' "$bump_job" | grep -q 'needs.finalize.outputs.transitioned'; then
  fail 'post-public Homebrew recovery is still gated on this run finalizing'
fi
printf '%s\n' "$bump_job" | grep -q "!contains(needs.finalize.outputs.version, '-')" \
  || fail 'stable public replay cannot converge Homebrew'
printf '%s\n' "$bump_job" | grep -q 'assert-newest-public-stable.sh' \
  || fail 'Homebrew pointer can downgrade to an old public stable'
latest_job="$(sed -n '/^  move-latest:/,$p' "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$latest_job" | grep -q '^    needs: finalize' \
  || fail 'GHCR latest does not depend on finalization'
if printf '%s\n' "$latest_job" | grep -q 'needs.finalize.outputs.transitioned'; then
  fail 'post-public latest recovery is still gated on this run finalizing'
fi
printf '%s\n' "$latest_job" | grep -q "!contains(needs.finalize.outputs.version, '-')" \
  || fail 'stable public replay cannot converge GHCR latest'
if printf '%s\n' "$latest_job" | grep -q "github.event_name == 'push'"; then
  fail 'GHCR latest still rejects a manual run that actually finalized'
fi
printf '%s\n' "$latest_job" | grep -q 'assert-newest-public-stable.sh' \
  || fail 'GHCR latest can downgrade to an old public stable'
printf '%s\n' "$latest_job" | grep -q 'converge-oci-pointer.sh' \
  || fail 'GHCR latest replay is not idempotent by digest'
printf '%s\n' "$latest_job" | grep -q 'GH_TOKEN:.*github.token' \
  || fail 'GHCR latest first-party gh calls lack GH_TOKEN'
docker_job="$(sed -n '/^  docker:/,/^  oci-proof:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$docker_job" | grep -q 'release-digest-marker.sh' \
  || fail 'the digest builder cannot read an already-durable marker'
printf '%s\n' "$docker_job" | grep -q '^      contents: read' \
  || fail 'the digest builder lacks contents read authority'
printf '%s\n' "$docker_job" | grep -q '^      packages: write' \
  || fail 'the digest builder lacks package write authority'
if printf '%s\n' "$docker_job" | grep -q '^      contents: write'; then
  fail 'the digest builder retains release-body write authority'
fi
if printf '%s\n' "$docker_job" | grep -q 'discover '; then
  fail 'marker absence still adopts a pre-existing version coordinate'
fi
printf '%s\n' "$docker_job" | grep -q 'verify-oci-payload.sh' \
  || fail 'the candidate digest is not payload-bound before persistence'
proof_job="$(sed -n '/^  oci-proof:/,/^  oci-marker:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$proof_job" | grep -q '^    needs: \[release-draft, docker\]' \
  || fail 'pre-marker OCI proof does not depend on digest construction'
printf '%s\n' "$proof_job" | grep -q '^      contents: read' \
  || fail 'pre-marker OCI proof lacks read-only contents authority'
printf '%s\n' "$proof_job" | grep -q '^      packages: read' \
  || fail 'pre-marker OCI proof lacks read-only package authority'
if printf '%s\n' "$proof_job" | grep -q '^      contents: write\|^      packages: write'; then
  fail 'pre-marker OCI proof retains write authority'
fi
printf '%s\n' "$proof_job" | grep -q 'verify-oci-payload.sh' \
  || fail 'pre-marker OCI proof does not compare stopped-container bytes'
marker_job="$(sed -n '/^  oci-marker:/,/^  oci-version:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$marker_job" | grep -q '^    needs: \[release-draft, oci-proof\]' \
  || fail 'digest marker persistence does not depend on read-only payload proof'
printf '%s\n' "$marker_job" | grep -q '^      contents: write' \
  || fail 'digest marker job lacks release-body write authority'
if printf '%s\n' "$marker_job" | grep -q '^      packages:'; then
  fail 'digest marker job retains package authority'
fi
if printf '%s\n' "$marker_job" | grep -q 'uses: docker/'; then
  fail 'digest marker job exposes release-write authority to a Docker action'
fi
if printf '%s\n' "$marker_job" | grep -q 'verify-oci-payload.sh\|oci-coordinate-immutable.sh'; then
  fail 'digest marker job performs registry or payload operations'
fi
printf '%s\n' "$marker_job" | grep -q 'read-release-state.sh' \
  || fail 'digest marker job does not re-read release identity before staging'
version_job="$(sed -n '/^  oci-version:/,/^  oci-final-proof:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$version_job" | grep -q '^    needs: \[release-draft, oci-marker\]' \
  || fail 'immutable OCI version convergence can run before marker durability'
printf '%s\n' "$version_job" | grep -q 'oci-coordinate-immutable.sh' \
  || fail 'post-marker OCI version does not converge by exact digest'
final_proof_job="$(sed -n '/^  oci-final-proof:/,/^  finalize:/p' \
  "$ROOT/.github/workflows/release.yml")"
printf '%s\n' "$final_proof_job" | grep -q '^    needs: \[release-draft, oci-version\]' \
  || fail 'pre-finalization OCI proof does not depend on post-marker convergence'
printf '%s\n' "$final_proof_job" | grep -q 'verify-oci-payload.sh' \
  || fail 'pre-finalization OCI proof does not compare payload bytes'
printf '%s\n' "$final_proof_job" | grep -q '^      contents: read' \
  || fail 'pre-finalization OCI proof lacks read-only contents authority'
printf '%s\n' "$final_proof_job" | grep -q '^      packages: read' \
  || fail 'pre-finalization OCI proof lacks read-only package authority'
if printf '%s\n' "$final_proof_job" | grep -q '^      contents: write\|^      packages: write'; then
  fail 'pre-finalization OCI proof retains write authority'
fi
printf '%s\n' "$finalizer" | grep -q '^      - oci-final-proof' \
  || fail 'finalizer does not depend on the post-marker payload result'
grep -q 'docker pull' "$ROOT/scripts/release/verify-oci-payload.sh" \
  || fail 'payload proof does not pull the exact digest'
grep -q 'docker create' "$ROOT/scripts/release/verify-oci-payload.sh" \
  || fail 'payload proof does not create stopped containers'
grep -q 'docker cp' "$ROOT/scripts/release/verify-oci-payload.sh" \
  || fail 'payload proof does not copy bytes from stopped containers'
if grep -q 'docker run' "$ROOT/scripts/release/verify-oci-payload.sh"; then
  fail 'payload proof executes image content'
fi
grep -q 'make_latest=legacy' "$ROOT/scripts/release/finalize-release.sh" \
  || fail 'stable publication does not select GitHub legacy Latest policy'
grep -q 'make_latest=false' "$ROOT/scripts/release/finalize-release.sh" \
  || fail 'prerelease publication does not explicitly refuse Latest'
grep -q 'original tag-push run' "$ROOT/RELEASING.md" \
  || fail 'canonical replay docs do not explain missing-provenance recovery'
grep -q 'original tag-push run' "$ROOT/docs/RELEASING.md" \
  || fail 'public replay docs do not explain missing-provenance recovery'
bash "$ROOT/scripts/release/tests/immutable-assets.test.sh" >/dev/null \
  || fail 'the immutable asset replay regression failed'
bash "$ROOT/scripts/release/tests/publication-barrier.test.sh" >/dev/null \
  || fail 'the cross-registry publication barrier regression failed'
bash "$ROOT/scripts/release/tests/finalize-release.test.sh" >/dev/null \
  || fail 'the self-contained finalizer barrier regression failed'
grep -q 'TAP_DEPLOY_KEY' "$ROOT/docs/RELEASING.md" \
  || fail 'the operator guide does not name the release workflow deploy key'
if grep -q 'HOMEBREW_TAP_TOKEN' "$ROOT/docs/RELEASING.md"; then
  fail 'the operator guide still names the retired Homebrew PAT secret'
fi

echo 'release-tooling.test: PASS'
