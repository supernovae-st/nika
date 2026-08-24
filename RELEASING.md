# Releasing Nika

The release train is one tag push; everything after is CI. This file is the
ceremony: what a releaser does, what the machine does, and what a user can
prove afterwards. The lineage law applies throughout: a published release is
a historical record: **never rewrite a live release body retroactively**
without an explicit operator decision.

## The ceremony (human side)

1. **Fill the changelog.** The `[Unreleased]` section becomes the version
   section: generate it, don't hand-type it:

   ```sh
   git-cliff --config cliff.toml v<PREV>..HEAD --tag v<NEXT> --strip all
   ```

   Splice the output under `## [Unreleased]` as `## [<NEXT>]`, newest first.
   Hand-curated narrative (a BREAKING window, an era note) may replace the
   generated body: the section, not the release page, is where curation
   lives. `CHANGELOG.md` is the single source the release body renders from.

2. **Bump every surface that spells the version.** The workspace manifest
   is the authority and the release workflow refuses a tag that disagrees
   with it (first gate), but four other places carry the number and two
   releases in a row forgot them (2026-08-02):

   ```sh
   # the authority
   Cargo.toml                     version = "<NEXT>"
   # the internal pins · 212 of them across 50 manifests
   crates/*/Cargo.toml            { path = "../nika-x", version = "<NEXT>" }
   # what an editor SHOWS the user
   .agents/plugins/nika/.{claude,codex,cursor}-plugin/plugin.json
   # the install example in the image header
   Dockerfile                     v=<NEXT>
   # follows the workspace, and is easy to mistake for noise
   fuzz/Cargo.lock
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

   ```sh
   git tag v<NEXT> && git push origin v<NEXT>
   ```

   Nothing else. `workflow_dispatch` can rebuild an existing tag; know that
   re-dispatching an old tag re-points `latest` (docker + release ordering).

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
