# Mega Handoff — Session 2026-04-07

## Copy-Paste Mega Prompt for Next Session

```
I'm continuing a massive session on Nika (Rust workflow engine, "Inference as Code").
Here's everything that was done and what needs to happen next.

## COMPLETED THIS SESSION

### 1. Podcast (DONE)
- `docs/podcast/nika-72h-v2-2026-04-07.mp3` — 12min28s, FR, OpenAI TTS (onyx) + ambient
- Covers: 385 commits, 10 releases (v0.65→v0.75), scheduling, keys, IDE, providers, architecture
- Generator: `docs/podcast/generate-72h-v2.py`

### 2. Distribution Audit + Fixes (DONE)
- **CI was RED** — Clippy `approx_constant` on 3.14/2.71 → changed to 3.16/2.73
- **Clippy `.unwrap()` in MinBy/MaxBy** → replaced with `is_none_or()` (Rust 1.94 idiom)
- **Open VSX NEVER published** (Cursor invisible) — `--no-git-tag-version` flag bug in ovsx CLI, masked by `continue-on-error: true`. FIXED in `release.yml:946`
- **VSIX icon missing** → created `editors/vscode/icons/nika-icon.png` (256x256 butterfly on #0f172a)
- **Marketplace metadata** → added icon, homepage, bugs, galleryBanner, pricing to package.json
- **`.vscodeignore`** → excluded dev files (esbuild.mjs, vitest.config.js, out/test/)
- **`.gitignore` vscode** → added *.vsix, node_modules/, out/
- **`media-provenance` bundled** → zero opt-in, everything in default features
- **CHANGELOG v0.75.0** → updated with runner decomposition, zero opt-in, LSP events

### Channel Matrix (needs manual action):
| Channel | Published | Code | Action Needed |
|---------|-----------|------|---------------|
| VS Code Marketplace | v0.74.0 | v0.77.0 | Push + tag |
| Open VSX (Cursor) | NEVER | v0.77.0 | OVSX_PAT token in GitHub secrets |
| Homebrew | v0.72.0 | v0.77.0 | HOMEBREW_TAP_TOKEN refresh |
| npm | v0.71.0 | v0.77.0 | Fix Windows build cascade |
| crates.io | v0.47.1 | v0.77.0 | Decide strategy |
| Docker | v0.74.0 | v0.77.0 | Auto on next release |

### 3. Editor Extensions (DONE — 5 editors)

**Zed** (`editors/zed/`, 12 files):
- Rust WASM extension via `zed_extension_api` v0.7
- 4 layers: LSP + MCP Context Server + Tree-sitter + Tasks
- `lib.rs`: binary discovery (nika-lsp > nika), MCP via `nika mcp`
- `runnables.scm`: inline ▶ buttons on `workflow:` declarations
- `tasks.json.example`: Run, Check, Lint, Test, Explain, Dry run, Graph
- Code-reviewed by rust-pro agent → refactored to combinator chain
- Path suffixes fixed: `.nika.yaml` (not `nika.yaml`)
- MCP Context Server = KILLER FEATURE (AI agent calls Nika tools)

**Neovim** (`editors/neovim/`, 6 files, 761 lines):
- Lua plugin: `require("nika").setup({ lsp = true, keymaps = true })`
- Filetype: `yaml.nika` (compound, preserves YAML base)
- LSP: `{"nika", "lsp", "--stdio"}`, root: nika.toml > .nika > .git
- 6 keymaps (<leader>n prefix): run, check, dag, lint, explain, test
- 7 commands: NikaRun, NikaCheck, NikaGraph, NikaLint, NikaExplain, NikaTest, NikaInfo
- Health check: `:checkhealth nika` (binary, version, LSP, tree-sitter)
- lazy.nvim + packer setup in README

**Helix** (`editors/helix/`, 5 files, 569 lines):
- `languages.toml`: nika language + dual LSP (nika-lsp + yaml-language-server)
- Tree-sitter queries: highlights, textobjects (maf/mif = task, mae/mie = workflow), indents
- Root patterns: nika.toml, .nika, .git

**Shared** (`editors/shared/`):
- `nika-keywords.json` — canonical keyword database generated from Rust source
- `extract-keywords.py` — extracts verbs, transforms, builtins, keys, providers, extract modes
- 7 categories, sanity checks, `--check` mode for CI

**Sync Automation** (`editors/sync-editors.sh`, 745+ lines):
- Extracts keywords from nika-core Rust source
- Compares against VS Code (TextMate), Zed (.scm), Neovim (.scm), Helix (.scm)
- `--fix` auto-updates all editors
- `--json` for CI, `--verbose` for debug
- Reviewed by bash-pro: 9 bugs fixed (double-output, symlinks, injection, guards, portability)

**CI** (`.github/workflows/editor-sync.yml`):
- Runs on PR when nika-core or editors/ change
- Uses sync-editors.sh to detect drift

### 4. Highlights Unified (DONE)
All 3 Tree-sitter editors (Zed, Neovim, Helix) now have feature parity:
- 5 verbs as keyword.function/keyword.control
- Provider names (14 known providers) as constants
- Extract modes (9) highlighted
- HTTP methods (GET, POST, etc.)
- Tool names (nika:*, server::tool) as function calls
- $env.* as variable.builtin
- Template expressions {{...}} in strings
- NIKA-XXX error codes in comments
- Task ID values as function names
- JSON Schema fields (properties, required, items...)
- 52 verb sub-fields

### 5. LSP Defaults Server-Side (DONE)
- LSP server ignores initOptions, uses sensible defaults unconditionally
- Zed lib.rs: removed language_server_initialization_options method
- Neovim init.lua: lsp_settings default changed to {} (empty)
- VS Code already sent zero init options (was already correct)

### 6. Architecture Documented (DONE)
- `dx/.claude/rules/architecture.md` — updated with editor architecture + crate diamond pattern
- Memory: `project_architecture_philosophy.md` — 17 crates, split rules, v0 philosophy, industry comparison

### 7. Research Reports (DONE)
- **Competitive landscape**: LangChain fatigue, zero Rust competitors, "Inference as Code" exclusive
- **Show HN playbook**: Timing (Tue/Wed 14h Paris), blog post URL, AGPL framing, 6h+ in comments
- **Zed deep integration**: 4 layers, MCP context server, runnables, WASM capabilities/limits
- **Editor audit**: 9 editors analyzed, prioritized (Neovim > Helix > Zed > Sublime > Emacs > JetBrains)
- **Rust workspace best practices**: Compared to rust-analyzer (42 crates), Nushell (46), Helix (14)

### 8. Plans Written (DONE)
- `docs/plans/2026-04-07-launch-plan-may5.md` — 4 weeks day-by-day, file paths, verification
- `docs/plans/2026-04-07-zed-deep-integration-plan.md` — 4 layers, MCP, runnables, tasks
- Memory: `project_launch_plan_may5.md` — Show HN playbook, competitive matrix, risks

---

## WHAT NEEDS TO HAPPEN NEXT

### IMMEDIATE (before next tag)
1. Fix clippy lint in `infer.rs:845` (redundant closure → remove wrapper)
2. Commit all editor extensions + fixes
3. Run `./editors/sync-editors.sh` to verify no drift
4. Run full `cargo test --workspace --lib` (should be 10,435+)
5. Run `cargo clippy --workspace -- -D warnings` (must be clean)

### MANUAL (Thibaut only)
1. Generate OVSX_PAT on open-vsx.org → add to GitHub secrets
2. Refresh HOMEBREW_TAP_TOKEN → GitHub secrets
3. Push + tag v0.77.1 (or whatever next version)
4. Verify Open VSX publish → search "nika" in Cursor

### WEEK 1 PRIORITIES (Distribution)
1. Submit Zed extension to `zed-industries/extensions` registry
2. Submit nvim-lspconfig PR to `neovim/nvim-lspconfig`
3. Submit Helix upstream PR to `helix-editor/helix`
4. Fix npm publish cascade (Windows build artifact issue)
5. Decide crates.io strategy (publish or drop)

### WEEK 2 PRIORITIES (Docs)
1. 9 missing Mintlify pages (quickstart, transforms, builtins, scheduling, testing, install, config, FAQ)
2. API reference for nika serve endpoints
3. QA 115 showcases + 44 course exercises
4. Per-IP rate limiting on nika serve
5. LSP diagnostic improvements (code_description URLs)

### WEEK 3 PRIORITIES (Content)
1. README final polish
2. Show HN blog post (2000-5000 words, technical, with benchmarks)
3. Demo video (asciinema, < 3 min)
4. Comparison page vs LangChain/CrewAI/n8n
5. GitHub Discussions + SECURITY.md

### WEEK 4 (Launch)
1. Install script hardening (test all platforms)
2. Full test suite + cross-platform smoke tests
3. Tag launch version
4. May 5: Show HN (Tue/Wed 14h Paris, blog URL, 6h+ comments)

---

## KEY ARCHITECTURE DECISIONS TO PRESERVE

- 1 LSP binary, N thin editor wrappers (rust-analyzer pattern)
- MCP Context Server in Zed = AI agent uses Nika tools directly
- Keywords sync from Rust source → JSON → 4 editors (automated)
- nika-core = ZERO I/O (pure types, parsing, catalogs)
- nika-lsp-core depends on nika-core ONLY (not engine) → 15s compile
- Diamond pattern: L0 core → L1 lsp-core → L2 engine → L3 support → L4 surfaces → L5 binary
- Split rule: only when 2 consumers need same code without the other's deps

## FILES CREATED/MODIFIED THIS SESSION

### New files:
- editors/zed/ (12 files)
- editors/neovim/ (6 files)
- editors/helix/ (5 files)
- editors/shared/nika-keywords.json
- editors/shared/extract-keywords.py
- editors/sync-editors.sh
- editors/README.md
- editors/vscode/icons/nika-icon.svg
- editors/vscode/icons/nika-icon.png
- .github/workflows/editor-sync.yml
- docs/plans/2026-04-07-launch-plan-may5.md
- docs/plans/2026-04-07-zed-deep-integration-plan.md
- docs/podcast/nika-72h-v2-2026-04-07.mp3
- docs/podcast/generate-72h-v2.py
- docs/podcast/generate-72h-fr.py
- docs/podcast-script-72h.md

### Modified files:
- tools/nika-core/src/binding/transform.rs (clippy fix: 3.14→3.16, unwrap→is_none_or)
- tools/nika/Cargo.toml (media-provenance in default)
- tools/nika-engine/Cargo.toml (media-provenance in default)
- tools/nika/CHANGELOG.md (v0.75.0 entry)
- .github/workflows/release.yml (ovsx fix line 946)
- editors/vscode/package.json (icon, metadata, version)
- editors/vscode/.vscodeignore (dev files excluded)
- editors/vscode/.gitignore (*.vsix, node_modules, out)
- dx/.claude/rules/architecture.md (editor arch + crate diamond)
```

---

## Session Stats

| Metric | Value |
|--------|-------|
| Agents deployed | 25+ |
| Files created | 30+ |
| Files modified | 15+ |
| Research reports | 5 (competitive, Show HN, Zed, editors, Rust workspaces) |
| Plans written | 2 (launch May 5, Zed deep integration) |
| Code reviews | 4 (code-reviewer, rust-pro, architect, bash-pro) |
| Bugs found + fixed | 15+ (clippy, ovsx, unwrap, path_suffixes, env cache, shell injection...) |
| Podcast generated | 1 (12min28s FR) |
| Memory entries | 4 new + 1 updated |
