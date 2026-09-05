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

   The manifest goes into the **last** commit before the tag. `v0.117.1`
   regenerated it in the prep commit, then one more commit moved a tracked
   file, and all four build legs refused the tag. Step 4 now asks this
   question too (`next-tag-project.sh --check` counts a stale manifest as
   a claim without proof), so a tag cut after a red step 4 is the only way
   to meet the guard again.

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
   the container `latest` tag are cross-version mutable pointers. GitHub
   publication explicitly uses `make_latest=legacy` for stable releases and
   `make_latest=false` for prereleases, so delayed recovery of an older stable
   cannot force it to Latest. The workflow
   is a **visibility barrier, not a cross-registry transaction**: npm and GHCR
   writes are irreversible and may already be public while the GitHub Release
   remains a draft. A failure therefore converges forward under the same
   immutable coordinate; it never rolls a registry back or overwrites a
   divergent identity. GitHub retains
   only one pending train and may replace it with a newer one, so never queue
   more than one train behind the active run. Replay refuses to replace an
   occupied release asset with different bytes, so timestamped or otherwise
   non-reproducible rebuilds stop rather than silently refresh public bytes. A
   missing asset is filled only after the occupied set compares byte-for-byte.
   A stable replay always converges Homebrew and `latest`, including recovery
   after either post-public job failed. Already-correct pointers no-op;
   prereleases never run these jobs. Before either possible write, the workflow
   proves this release is the newest public stable SemVer, so an old-tag replay
   fails rather than downgrading a floating pointer.
   There is an unavoidable short post-public window before the downstream
   Homebrew commit lands, because the formula cannot safely point at a draft.
   The replay
   helper comes from the exact workflow commit, so a historical tag does not
   need to contain future release tooling. SLSA provenance is created only by
   the original tag-push context and exposed as a verified run artifact; the
   isolated asset-convergence writer then attaches it with the other seven
   exact assets, so a later run can recover it. Draft release reads require
   push access even in a public repository. The existing draft preparer alone
   reads the initial OCI marker and, on manual replay, downloads exactly one
   existing statement by immutable asset ID. It supplies the marker decision
   as job outputs and the unverified statement as a run artifact, without
   executing a Docker or SLSA verifier. A statement download is not proof.
   The event selector accepts only the selected provenance lane's success and
   the other lane's intentional skip. Asset convergence explicitly rejoins that
   result: GitHub's transitive skip cannot suppress it, but any failed,
   cancelled or skipped direct prerequisite still refuses the writer. Stable
   pointers likewise require successful finalization before their rejoin.
   Manual replay refuses branch-context regeneration. Every push and replay
   provenance lane, plus the read-only
   final proof, runs the pinned `slsa-verifier` against the four native subjects,
   repository, and exact source tag before proceeding. `workflow_dispatch`
   cannot generate missing
   tag-context provenance: if the statement was never staged, rerun the
   original tag-push run while that run and its artifacts are retained, provided
   its immutable workflow can complete. A defect in that historical workflow
   cannot be repaired by rerunning it; fix and test the ceremony on main and
   use a new version rather than moving the old tag. The
   exact GHCR digest is durably recorded in a single release-body marker only
   after both digest-addressed Linux container binaries, copied from stopped
   containers without executing image content, hash identically to their
   matching native tarballs. The proof job has read-only contents/packages.
   The OCI index must contain exactly the two Linux runnable images plus one
   BuildKit attestation descriptor bound to each image digest. Those
   `unknown/unknown` metadata entries are not runnable platforms. Unknown
   entries, missing or duplicate subjects, and duplicate platform or manifest
   digests refuse; provenance generation stays enabled. This index census
   proves structure and binding, not the authenticity of attestation contents.
   The marker job has contents-write only and performs no Docker operation.
   Only then may `image:<version>` be created.
   If the marker survives but that tag is absent, replay heals it from the
   exact `image@digest`; if the marker is absent, an occupied version tag is
   never adopted as authority. Release publication then crosses three disjoint
   authorities. The asset-convergence job has contents-write only, downloads the
   exact run artifacts, and stages/verifies the eight GitHub assets with
   workflow-SHA first-party tooling; it receives no SLSA, npm, Docker, package,
   or deploy-key authority. Asset census and upload use the immutable release
   ID, occupied bytes download by asset ID, and every write is preceded by a
   fresh release-ID/tag/SHA check and followed by convergence revalidation. A
   move visible to the pre-write check refuses with zero uploads; a move inside
   the unavoidable read/POST gap still cannot redirect the ID-scoped upload and
   is refused by the post-write read. The final proof has
   contents/attestations/packages
   read only and verifies the checksum manifest, native attestations, tag-bound
   SLSA, npm SRI, the marker owner's digest, OCI identity, and stopped-container payload
   bytes. Finally, the contents/discussions writer downloads the exact artifacts
   again and independently compares all eight current GitHub assets byte-for-byte,
   checks the checksum manifest, and re-reads release identity, state, and marker
   immediately before either accepting an already-public replay or PATCHing the
   draft. It trusts only the read-only proof's digest for immutable external
   registries and invokes no SLSA, npm, or Docker verifier. The tap deploy key is
   reduced to a boolean readiness result and unset before the step that receives
   the step-local GitHub token.
   OCI labels are identity metadata, not proof of binary bytes.
   Repository administrators can still mutate release metadata between separate
   GitHub API calls. Neither asset upload nor the release PATCH supports the
   conditional precondition this workflow would need to combine its identity
   read with the write. That minimal admin API TOCTOU is residual authority: the
   ID-scoped upload cannot resolve a different tag, and post-operation reads
   refuse visible drift, but the workflow cannot lock administrators out. This
   is distinct from cross-registry atomicity, which this visibility barrier does
   not claim.
   This barrier is future-only:
   **v0.116.2 is not retroactively atomic**, and its already-public registry
   history is not rewritten to pretend otherwise.

   Before the first future stable train, configure both `NPM_TOKEN` (granular
   automation token) and the repository-scoped `TAP_DEPLOY_KEY`. Missing npm
   authority blocks an absent package version; missing tap authority blocks a
   stable draft before visibility. Identical occupied npm bytes need no token.

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
| `ghcr.io/supernovae-st/nika:<ver>` | immutable multi-arch image, bit-identical to the tarballs |
| `ghcr.io/supernovae-st/nika:latest` | stable-only floating pointer, moved after finalization |
| Homebrew formula bump | `supernovae-st/homebrew-tap` (deploy-key scoped) |
| `supernovae-st-nika-check-wasm-<ver>.tgz` (+ `.sha256`) | the npm tarball, byte-identical to what `npm publish` ships — attested like the binaries |
| `@supernovae-st/nika-check-wasm` on npm | immutable browser checker; first publication requests npm provenance, while replay independently proves exact SRI (absence requires `NPM_TOKEN`) |

The release body starts with the curated **What / Install / Verify /
Provenance** front page rendered by `scripts/release/render-notes.sh`, with
GitHub's generated PR list appended. The workflow then appends one hidden GHCR
digest marker while preserving that body and refuses a malformed, duplicate,
or changed marker on replay.

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

Three independent chains cover the native artifacts: the checksum manifest,
GitHub's signed attestation, and the SLSA generator's intoto statement. npm's
registry provenance is requested during the original publish, but this release
barrier's replay proof for npm is the registry's exact sha512 SRI. Any claimed
chain failing is a stop-the-line event.

## The record

Every release is also a claim on the machine-verified timeline ·
[nika.sh/timeline](https://nika.sh/timeline) renders the spec's
`timeline/timeline.yaml`, and CI re-proves the provable claims (GitHub ·
crates.io) on every push and weekly. A release that isn't in the record
isn't released; a record that can't be re-proven isn't a record.
