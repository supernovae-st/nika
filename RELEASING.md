# Releasing Nika

The release train is one tag push; everything after is CI. This file is the
ceremony: what a releaser does, what the machine does, and what a user can
prove afterwards. The lineage law applies throughout: a published release is
a historical record: **never rewrite a live release body retroactively**
without an explicit operator decision.

## The ceremony (human side)

1. **Preview the changelog, then run the one release sweep.** Each change
   already described itself in its own file under `changelog.d/`
   (`changelog.d/README.md` is the contract). Read the assembled body first;
   then let the sweep own the fold and every version carrier as one act:

   ```sh
   bash scripts/release/changelog-assemble.sh --check
   bash scripts/release/changelog-assemble.sh             # preview it
   bash scripts/release/wave-sweep.sh <NEXT>
   ```

   `wave-sweep.sh` delegates the engine section to
   `changelog-assemble.sh --fold`, so the two mechanisms cannot create two
   headings. The assembler splices the body in as `## [<NEXT>]` with its
   compare link, restores the `[Unreleased]` stub, and **`git rm`s** the
   fragments it consumed — the deletions land STAGED, so `git add
   CHANGELOG.md` beside them and the fold is one commit. (Plain `rm` left them
   unstaged, and since this file stages by explicit path and never `git add
   -A`, the release would have shipped the assembled section *and* the
   fragments it was made from; the next fold would then emit each one twice.)
   Hand-curated
   narrative (a BREAKING window, an era note) may still be edited into the
   section afterwards: the section, not the release page, is where curation
   lives. `CHANGELOG.md` stays the single source the release body renders
   from.

   **Why fragments.** `CHANGELOG.md` was a shared append target, so two
   branches that shared no source file still collided there — measured
   2026-08-24 on four security pull requests (#1162 #1163 #1164 #1165):
   `git merge-tree` reported one conflict each, always this file, and zero
   overlap across the nine crate files they touched. Same shape as the
   `estate.yaml` collision of 2026-08-20, same fix: stop sharing the target.
   `scripts/ci/check-changelog-fragments.sh` refuses a bullet written back
   into `[Unreleased]` (pre-push and in CI).

   `git-cliff` stays the fallback for a tag nobody curated —
   `changelog-cliff.yml` still runs on tag push and its idempotence guard
   skips whenever the fold already landed a `## [<NEXT>]` section:

   ```sh
   git-cliff --config cliff.toml v<PREV>..HEAD --tag v<NEXT> --strip all
   ```

2. **Review every surface the sweep moved.** The workspace manifest is the
   authority and the release workflow refuses a tag that disagrees with it
   (first gate), but the complete carrier set is larger:

   ```sh
   # the authority
   Cargo.toml                     version = "<NEXT>"
   # the internal pins
   crates/*/Cargo.toml            { path = "../nika-x", version = "<NEXT>" }
   # what an editor SHOWS the user
   .agents/plugins/nika/{plugin.json,.*-plugin/plugin.json}
   # the install example in the image header
   Dockerfile                     v=<NEXT>
   # all three lock families follow the workspace
   Cargo.lock
   fuzz/Cargo.lock
   crates/nika-acp/Cargo.lock
   ```

   Then `bash scripts/refresh-status.sh --write` · it regenerates the block
   AND writes it into `ROADMAP.md` and `.claude/CLAUDE.md` (the loop closed
   2026-08-14 · before that the script only printed and two releases carried
   a stale HEAD). Vector 23 refuses a tag while those blocks name the old
   version; **vector 47** refuses one while any surface above disagrees.
   Run `bash scripts/hygiene/check-all.sh` before tagging: both fire
   there, and both were earned by a release that shipped without them.

3. **Regenerate the estate manifest.**

   ```sh
   python3 scripts/estate.py --write && git add estate.yaml
   ```

   The manifest is a WHOLE-TREE projection, so requiring it per-commit made
   every pair of concurrent branches collide on a file neither had edited —
   four pull requests, four conflicts, 2026-08-20, and `git merge-tree`
   named the projection as the only overlap. It is therefore no longer a
   commit-time refusal: the pre-commit hook and the `estate` CI job block
   on COVERAGE (a path no rule classifies) and merely report FRESHNESS.

   Freshness is owned HERE instead, where it is the deliverable rather
   than a formality: `release.yml` refuses a tag whose tree does not match
   its manifest, because that tree is the one whose binaries a user will
   verify. Forget this step and the release stops at the guard — which is
   the same shape as the version-match guard above, and for the same
   reason.

4. **Project the next tag (the anti-08-08 question).** Before the tag
   exists, ask what it would actually contain and what it would claim
   without proof. The command reads `release.yml`, `wiring.yaml`, the
   CHANGELOG `[Unreleased]` section and the CI job keys. If it had
   existed on 2026-08-08 it would have said: the CHANGELOG promises the
   harness · `release.yml` does not build it.

   ```sh
   bash scripts/ci/next-tag-project.sh --check
   ```

   `--check` exits 1 on UNPROVEN claims. Do not tag while it is red.
   A `✅` in `[Unreleased]` without a release build line and a named CI
   job is a `🟡`.

5. **Tag and push.**

   The tag is the shared GitHub/npm/Homebrew/OCI coordinate. It accepts strict
   SemVer core plus prerelease identifiers (`v1.0.0-rc.1`), but not build
   metadata (`+build`): SemVer ignores metadata for precedence while the four
   registries must agree on one spelling. The workflow validates that
   coordinate in its first job, before builds and before any publication write.

   ```sh
   git tag v<NEXT> && git push origin v<NEXT>
   ```

   Nothing else. `workflow_dispatch` can replay an existing tag. Dispatch it
   from the current workflow on `main`, while the `tag` input identifies the
   immutable source tag:

   ```sh
   gh workflow run release.yml --repo supernovae-st/nika --ref main \
     -f tag=v<NEXT>
   ```

   Never select the historical tag as the workflow ref: that executes the
   workflow YAML stored in the tag and can predate the immutable uploader and
   concurrency guards. This cannot be retrofitted into already-published tags;
   the operator command and the current workflow's ref guard are both part of
   the replay boundary. All live and
   replay release trains share one global publication lane because Homebrew and
   the container `latest` tag are cross-version mutable pointers. GitHub retains
   only one pending train and may replace it with a newer one, so never queue
   more than one train behind the active run. Replay refuses to replace an
   occupied release asset with different bytes, so timestamped or otherwise
   non-reproducible rebuilds stop rather than silently refresh public bytes. A
   missing asset is filled only after the occupied set compares byte-for-byte.
   Know that re-dispatching an old tag still re-points `latest` (docker + release
   ordering). The replay
   helper comes from the exact workflow commit, so a historical tag does not
   need to contain future release tooling. Existing SLSA provenance is
   preserved byte-for-byte and a missing statement follows the same guarded
   replay boundary. SLSA provenance is created only by the original tag-push
   context. Manual replay requires exactly one existing statement asset and
   refuses branch-context regeneration; `slsa-verifier` remains the separate
   cryptographic and source-identity judge.

   The portable Agent Plugins mirror is downstream of this immutable tag.
   After the release assets are green, its release-heal lane runs
   `python3 scripts/resync-mirror.py --ref v<NEXT>` in `nika-plugins` and
   opens a reviewable PR. Never copy `nika-plugins@main` back into `.agents/`:
   that reverses ownership and can downgrade a prepared engine release.

6. **Close the release record.** Add the engine release entry to the pinned
   `nika-spec/timeline/timeline.yaml`, let its timeline CI re-prove the tag and
   publication claims, and verify it renders at [nika.sh/timeline](https://nika.sh/timeline)
   before declaring the release complete. The binary train may finish before
   this cross-repository record update, but the release contract does not.

## What the machine publishes (per tag)

| Asset | Proof it carries |
|---|---|
| `nika-{macos,linux}-{arm64,x64}-<ver>.tar.gz` | the four platform binaries |
| `SHA256SUMS` | checksum manifest (proof 1) |
| GitHub native attestation | `gh attestation verify` (proof 2) |
| `multiple.intoto.jsonl` | SLSA provenance asset, offline-verifiable (proof 3) |
| `ghcr.io/supernovae-st/nika:{<ver>,latest}` | multi-arch image, bit-identical to the tarballs |
| Homebrew formula bump | `supernovae-st/homebrew-tap` (deploy-key scoped) |
| `supernovae-st-nika-check-wasm-<ver>.tgz` (+ `.sha256`) | the npm tarball, byte-identical to what `npm publish` ships — attested like the binaries |
| `@supernovae-st/nika-check-wasm` on npm | the browser checker, published with npm provenance (token present; loud-skipped otherwise, the tarball stays publish-ready) |

The release body is rendered by `scripts/release/render-notes.sh`: the
curated **What / Install / Verify / Provenance** front page from the
changelog section: with GitHub's generated PR list appended below it.

## What a user can prove

```sh
sha256sum -c SHA256SUMS --ignore-missing            # macOS: shasum -a 256 -c
gh attestation verify nika-<platform>-<ver>.tar.gz --repo supernovae-st/nika
slsa-verifier verify-artifact nika-<platform>-<ver>.tar.gz \
  --provenance-path multiple.intoto.jsonl \
  --source-uri github.com/supernovae-st/nika --source-tag v<ver>
gh attestation verify supernovae-st-nika-check-wasm-<ver>.tgz --repo supernovae-st/nika
npm audit signatures                                # in a project depending on the package
```

Three independent chains: the checksum manifest, GitHub's signed
attestation, and the SLSA generator's intoto statement. Any one of them
failing is a stop-the-line event.

## The record

Every release is also a claim on the machine-verified timeline ·
[nika.sh/timeline](https://nika.sh/timeline) renders the spec's
`timeline/timeline.yaml`, and CI re-proves the provable claims (GitHub ·
crates.io) on every push and weekly. A release that isn't in the record
isn't released; a record that can't be re-proven isn't a record.
