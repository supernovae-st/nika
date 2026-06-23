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

## 1. Cut a binary release (fully automated)

```bash
# from the engine repo, on main, fully green:
git tag v0.90.0            # vMAJOR.MINOR.PATCH — must match the workspace version
git push origin v0.90.0
```

That tag fires **`.github/workflows/release.yml`**, which:

1. builds `nika` for **macOS arm64/x64** and **Linux arm64/x64** (release · `--locked`),
2. packages each as `nika-<platform>-<version>.tar.gz` (+ a `.sha256` sidecar),
3. creates the GitHub release with those tarballs + a `SHA256SUMS` file,
4. **bumps the Homebrew tap** formula (version + the 4 sha256s) — *if* the
   `HOMEBREW_TAP_TOKEN` secret is set (see §3); otherwise it logs a notice and you
   bump the formula by hand (§2).

Re-run a tag's build without re-tagging via the **workflow_dispatch** input.

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

## 3. One-time secret for the auto-formula-bump

Create a fine-grained PAT with **contents:write** on `supernovae-st/homebrew-nika`,
then add it to the engine repo:

```bash
gh secret set HOMEBREW_TAP_TOKEN --repo supernovae-st/nika --body "<pat>"
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

The vsix is independent of the binary release (it shells out to `nika lsp` from
the user's PATH), so it can ship on its own cadence.

---

## 5. TypeScript SDK — npm

```bash
cd nika/02-engineering/repos/client-sdk
npm publish                          # needs `npm login` (or NPM_TOKEN)
```

---

## Release checklist

- [ ] workspace version bumped (`Cargo.toml`), `CHANGELOG.md` has the section
- [ ] `scripts/refresh-status.sh` block + `ROADMAP.md` block in sync (vector 23)
- [ ] `git tag vX.Y.Z && git push origin vX.Y.Z` → release workflow green
- [ ] Homebrew formula bumped (auto via §3, or §2 by hand) · `brew install` smoke
- [ ] `nika mcp` smoke (`initialize` + `tools/list`) · no stale `mcp serve --stdio` docs/config
- [ ] `nika init` creates `.vscode/settings.json`, `AGENTS.md`, `.cursor/rules/nika.mdc`
- [ ] `nika wire cursor` migrates stale MCP config and preserves other servers
- [ ] `nika doctor` reports editor/agent readiness without printing secrets
- [ ] `install.sh` asset names match `release.yml` (`nika-macos-arm64-X.Y.Z.tar.gz`, etc.)
- [ ] extension `vsce publish` + `ovsx publish` (if shipping the editor side)
- [ ] `npm publish` the SDK (if shipping it)
