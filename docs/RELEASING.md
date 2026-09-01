<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# Releasing nika

How a `nika` version reaches users — binaries, Homebrew, the editor extension.
Everything **binary-side is automated** off a git tag; the marketplace/npm steps
are one command each (they need a token only you hold).

> The public binary is **`nika`**. The engine builds `nika-cli` (the operator
> surface · the L5 `nika` composition root is reserved for later); the release
> renames the seed to its public name. The Homebrew formula does
> `bin.install "nika"` and tests `nika --version`.

---

## 0. Stable and next — a version names ONE behavior family

Two trains exist at any moment, and every surface says which one it serves:

| train | identity | who reads it |
|---|---|---|
| **stable** | the newest tag (`git tag --sort=-v:refname \| head -1`) · immutable once published | brew · the Registry · nika-action · starters · docs/site "stable" · anyone who installed |
| **next** | `main` at `<next>.0-dev` (a real semver prerelease) → `<next>.0-rc.N` → `<next>.0` | contributors · the spec/pack/VS Code integration train · docs/site "next" |

`main` never identifies as the published version once its behavior has moved
past it: the day the tree diverges materially (a language change · a new
authority · a graph format · a refusal that shipped as an accept), open the
next train —

```bash
bash scripts/release/wave-sweep.sh 0.109.0-dev --dev   # every carrier · no changelog fold
```

`nika --version`, the trace's `engine_version`, every path-dep pin and the
kit trio then read `0.109.0-dev`; the Dockerfile teaching comment stays on
the newest PUBLISHED version (it downloads a release tarball). CI's
`version-uniform` ratchet proves the sweep is whole on every push. The rc
sweep (`wave-sweep.sh 0.109.0-rc.1`) folds the changelog heading; the tag
`v0.109.0-rc.1` must spell the workspace version exactly (release.yml refuses
otherwise) and its assets carry the same string. Stable consumers move only
when a tag they can install exists — never because `main` moved.

---

## 1. Cut a binary release (fully automated)

```bash
# from the engine repo, on main, fully green:
git tag v0.90.0            # vMAJOR.MINOR.PATCH — must match the workspace version
git push origin v0.90.0
```

The tag is the one publication coordinate shared by GitHub, npm, Homebrew and
OCI. Strict SemVer prereleases such as `v1.0.0-rc.1` are accepted; build
metadata such as `v1.0.0+build` is refused because SemVer ignores it for
precedence. The workflow's first job checks this before any build or registry
write, preventing a partially published train.

That tag fires **`.github/workflows/release.yml`**, which:

1. builds `nika` for **macOS arm64/x64** and **Linux arm64/x64** (release · `--locked`),
2. **gates the upload through `scripts/ci/funnel-e2e.sh`** against the staged
   binary — the stranger's first path played end to end; if the funnel fails,
   nothing uploads. Its needles judge the CURRENT binary, never a remembered
   one: after any render-wording change, re-derive them by running the funnel
   locally (`bash scripts/ci/funnel-e2e.sh target/release/nika`) BEFORE
   pushing the tag (the v0.107.0 lesson — a wording fix removed `FLOOR`, and
   the hidden `guard` verb is asked of the verb itself, not the `--help`
   listing),
3. packages each as `nika-<platform>-<version>.tar.gz` (+ a `.sha256` sidecar),
4. holds the GitHub release as a draft while npm converges and GHCR builds a
   digest, proves both Linux payloads against the native tarballs, durably
   records that digest, then converges the immutable version coordinate,
5. verifies the exact eight-asset allowlist, checksums, five source-bound
   GitHub attestations, the generic SLSA signature/source/four subjects, npm
   SRI, and the two-platform OCI digest + labels, then one finalizer makes the
   release public,
6. moves stable-only Homebrew and GHCR `latest` pointers after finalization.

Replay a tag without re-tagging via the **workflow_dispatch** input.
Always dispatch the current workflow from `main`; the input, not the workflow
ref, names the immutable tag to rebuild:

```sh
gh workflow run release.yml --repo supernovae-st/nika --ref main \
  -f tag=v0.116.2
```

Never select the historical tag as the workflow ref. GitHub would execute the
workflow YAML stored in that tag, which can predate the immutable uploader and
concurrency guards. Already-published tags cannot be retrofitted, so the
operator command and the current workflow's ref guard are both part of the
replay boundary.

All live and replay release trains share one global publication lane because
Homebrew and the container `latest` tag are cross-version mutable pointers.
This is a **visibility barrier, not a cross-registry transaction**. npm and
GHCR writes are irreversible and may be public while GitHub remains a draft;
recovery is forward convergence under the same immutable tag, never rollback.
GitHub retains only one pending train and may replace it with a newer one, so
never queue more than one train behind the active run. An already-published
asset is downloaded and compared, and a replay refuses to replace it when the
bytes differ. A timestamped or otherwise non-reproducible rebuild therefore
stops; missing assets are filled only when the occupied set still compares
byte-for-byte. The workflow reads replay tooling from its own exact commit, not
from the historical tag. The tag-push lane cryptographically verifies generic
SLSA provenance and stages it on the draft immediately. A manual replay can
only preserve and re-verify an existing statement: `workflow_dispatch` cannot
regenerate missing tag-context SLSA because the workflow branch is not an
honest provenance identity for the historical tag. If the statement is
missing, rerun the original tag-push run while that run and its artifacts are
retained. Every stable replay converges Homebrew and GHCR `latest`, so a failure in
either post-public job can be repaired after the release is already public.
Already-correct pointers no-op, and both possible writes first prove this is the
newest public stable SemVer; an old-tag replay refuses instead of downgrading.
Prereleases never move them. GitHub publication passes `make_latest=legacy`
for stable releases and `make_latest=false` for prereleases, preventing delayed
older-stable recovery from forcing Latest. Missing mandatory credentials keep the draft
closed. Homebrew necessarily has a short
post-public update window because its formula cannot safely point at draft
assets. This protection is future-only; **v0.116.2 is not retroactively
atomic**, and no workflow can rewrite its already-public history into one.

The exact GHCR digest is stored as a hidden marker in the GitHub release body,
not as a ninth asset. Before persistence and again before finalization, the
workflow pulls each Linux platform by exact digest, creates a stopped container,
copies out `/usr/local/bin/nika` without executing image content, and compares
its sha256 with the matching extracted native tarball; label checks alone do
not prove payload bytes. A durable marker authorizes healing a
missing immutable version tag from the exact `image@digest`. Without a marker,
the workflow never adopts an occupied version coordinate. Release bodies and
release fields can still be changed manually by a repository administrator.
The residual authority is an admin-writer TOCTOU between the workflow's
repeated reads: drift observed by a read is refused, but the workflow cannot
lock out an administrator between checks. That is separate from cross-registry
atomicity, which this visibility barrier does not claim.

No CI release pipeline existed before this — a tag did nothing. `scripts/release.sh`
(monorepo) still only tags + pushes; the binaries come from the workflow.

---

## 2. Homebrew formula — repair after a post-public automation failure

The finalizer published the tarballs + checksums, but the downstream formula
write can still fail in the unavoidable post-public window. From the tap clone:

```bash
gh release download v0.90.0 --repo supernovae-st/nika --dir /tmp/rel
scripts/release/update-formula.sh \
  ../homebrew/Formula/nika.rb 0.90.0 /tmp/rel
# review the diff (version + 4 sha256), then commit + push the tap
```

`update-formula.sh` rewrites only the `version` line and the four `sha256` lines
(each matched to its `url` by platform); the `url`s carry `#{version}` and don't
change. `brew install supernovae-st/tap/nika` then pulls the new version.

---

## 3. One-time deploy key for the auto-formula-bump

Create an SSH deploy key, add its public key to
`supernovae-st/homebrew-tap` with write access, then add the private key to
the engine repo:

```bash
gh secret set TAP_DEPLOY_KEY --repo supernovae-st/nika < /path/to/private-key
```

It is mandatory for a new stable train: without it the visibility finalizer
keeps the GitHub release in draft. Once public, the formula update follows the
finalizer; if that downstream write fails, repair it forward with §2.

The npm package also needs a granular automation token when the selected
version is absent:

```bash
gh secret set NPM_TOKEN --repo supernovae-st/nika
```

An identical occupied npm version needs no credential. An absent version with
no token, an unknown registry lookup, or a divergent SRI keeps the draft closed.
The first publish invokes npm's `--provenance`; recovery proves the registry SRI
and does not claim an independent cryptographic re-verification of npm's
provenance envelope.

---

## 4. Editor extension — VS Code Marketplace + OpenVSX (Cursor)

The extension lives in the monorepo at `nika/02-engineering/repos/vscode`
(version-synced to the engine). Build + publish:

```bash
cd nika/02-engineering/repos/vscode
npm ci && npm run compile
vsce package                         # → nika-lang-<version>.vsix

# VS Code Marketplace (publisher: supernovae) — needs a PAT from dev.azure.com:
vsce publish                         # or: vsce publish -p "$VSCE_PAT"

# OpenVSX (this is what Cursor / VSCodium / Windsurf pull) — token from open-vsx.org:
npm i -g ovsx
ovsx publish nika-lang-<version>.vsix -p "$OVSX_PAT"
```

The VSIX publishes through its own operator-gated train (it shells out to
`nika lsp` from the user's PATH), but its package version stays on the engine's
major.minor wave. `scripts/ci/ecosystem-coherence.py` checks the repo package,
latest GitHub release, VS Marketplace, and OpenVSX independently so one stale
publication surface cannot hide behind another.

---

## 5. TypeScript SDK — npm

```bash
cd nika/02-engineering/repos/client-sdk
npm publish                          # needs `npm login` (or NPM_TOKEN)
```

---

## Release checklist

- [ ] workspace version bumped (`Cargo.toml` · via `wave-sweep.sh`), `CHANGELOG.md` has the section · `scripts/ci/check-version-uniform.sh` OK
- [ ] pushed tag matches the Cargo workspace version exactly (`release.yml` enforces it)
- [ ] `scripts/refresh-status.sh` block + `ROADMAP.md` block in sync (vector 23)
- [ ] `git tag vX.Y.Z && git push origin vX.Y.Z` → release workflow green
- [ ] Homebrew formula bumped (auto via §3, or §2 by hand) · `brew install` smoke
- [ ] `nika run --help` from the release asset contains every documented run flag
      (`--json`, `--output`, `--no-progress`, `--quiet`, `--dry-run`)
- [ ] `nika mcp` smoke (`initialize` + `tools/list`) · no stale `mcp serve --stdio` docs/config
- [ ] `nika mcp --transport http` smoke (curl `initialize` on loopback · foreign origin → 403) — the funnel's mcp-http leg plays this against the tarball automatically
- [ ] `nika init` creates `.vscode/settings.json`, `AGENTS.md`, `.cursor/rules/nika.mdc`
- [ ] `nika wire cursor` migrates stale MCP config and preserves other servers
- [ ] `nika doctor` reports editor/agent readiness without printing secrets
- [ ] `install.sh` asset names match `release.yml` (`nika-macos-arm64-X.Y.Z.tar.gz`, etc.)
- [ ] extension `vsce publish` + `ovsx publish` (if shipping the editor side)
- [ ] `npm publish` the SDK (if shipping it)
