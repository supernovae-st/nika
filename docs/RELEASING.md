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
4. creates the GitHub release with those tarballs + a `SHA256SUMS` file,
5. **bumps the Homebrew tap** formula (version + the 4 sha256s) — *if* the
   `TAP_DEPLOY_KEY` secret is set (see §3); otherwise it logs a notice and you
   bump the formula by hand (§2).

Re-run a tag's build without re-tagging via the **workflow_dispatch** input.
Runs for the same tag never overlap. GitHub retains one pending replay and may
replace it with a newer one; an already-published asset is downloaded and
compared, and a replay refuses to replace it when the bytes differ.
The workflow reads replay tooling from its own exact commit, not from the
historical tag. Existing SLSA provenance is preserved rather than regenerated.

No CI release pipeline existed before this — a tag did nothing. `scripts/release.sh`
(monorepo) still only tags + pushes; the binaries come from the workflow.

---

## 2. Homebrew formula — by hand (when the tap token isn't set)

The release already published the tarballs + checksums. From the tap clone:

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

With it set, step §1 closes the loop end-to-end (no manual formula edit).

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
