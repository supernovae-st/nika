# Session Handoff — Editors, Architecture, Zed, Distribution, Launch

**Date**: 2026-04-07
**Session type**: Mega session — podcast, 5 editors, distribution audit, architecture review, launch plan
**Duration**: ~4 hours, 25+ agents deployed

---

## MEGA PROMPT — Copy-paste for next session

```markdown
# Context: Nika Mega Session Continuation

I'm continuing work on Nika — a Rust YAML workflow engine for AI ("Inference as Code").
v0.77.0, 379K LOC, 17 crates, 10,435+ tests. Solo founder, launching May 5 on HN.

## WHAT WAS DONE (already committed or in working tree)

### A. 5 Editor Extensions — Full Multi-Editor Support

We built first-class support for 5 editors in one session. Architecture: 1 shared LSP
(`nika lsp --stdio`), thin editor-specific wrappers. Same pattern as rust-analyzer.

**VS Code / Cursor** (`editors/vscode/`):
- EXISTING extension, FIXES applied:
  - Open VSX publishing was BROKEN (ovsx CLI `--no-git-tag-version` flag not supported,
    masked by `continue-on-error: true`). Fixed in `release.yml:946`.
  - PNG icon 256x256 created (`icons/nika-icon.png`, butterfly on #0f172a)
  - Marketplace metadata added: icon, homepage, bugs, galleryBanner, pricing
  - `.vscodeignore` cleaned: excluded esbuild.mjs, vitest.config.js, out/test/
  - `.gitignore` added: *.vsix, node_modules/, out/
- VSIX builds clean (518 KB), 12/12 vitest tests pass
- Publisher `supernovae` matches Open VSX namespace (created manually)
- STILL NEEDED: OVSX_PAT token in GitHub secrets, then push+tag to publish

**Zed** (`editors/zed/`, 12 files) — THE KILLER INTEGRATION:
- Rust WASM extension via `zed_extension_api` v0.7
- 4-layer architecture:
  1. **LSP**: binary discovery (nika-lsp > nika lsp --stdio), cached path
  2. **MCP Context Server**: `nika mcp` registered as context server — Zed's AI agent
     can validate workflows, generate tasks, explain errors, visualize DAGs
  3. **Tree-sitter**: highlights.scm (250 lines), outline.scm, brackets.scm, indents.scm,
     runnables.scm (inline ▶ buttons on `workflow:` declarations)
  4. **Tasks**: tasks.json.example (Run, Check, Lint, Test, Explain, Dry run, Graph)
- Code-reviewed by rust-pro agent → refactored to combinator chain, LspBinary type
- Path suffixes: `.nika.yaml` (with leading dot, prevents false activation)
- LSP init options REMOVED (server uses sensible defaults)
- Plan: `docs/plans/2026-04-07-zed-deep-integration-plan.md`
- STILL NEEDED: Submit to `zed-industries/extensions` registry

**Neovim** (`editors/neovim/`, 6 files, 761 lines):
- `lua/nika/init.lua` (262 lines): setup(), LSP via nvim-lspconfig, keymaps, commands
- `lua/nika/health.lua` (145 lines): `:checkhealth nika` (binary, version, LSP, tree-sitter)
- `ftdetect/nika.lua`: `*.nika.yaml` → filetype `yaml.nika` (compound)
- `ftplugin/yaml.lua`: buffer-local settings (2-space, tree-sitter folding)
- `after/queries/yaml/highlights.scm` (87→expanded): Nika-specific tree-sitter queries
- LSP command: `{"nika", "lsp", "--stdio"}`, root: nika.toml > .nika > .git
- 6 keymaps (<leader>n): r=run, c=check, d=dag, l=lint, e=explain, t=test
- 7 commands: NikaRun, NikaCheck, NikaGraph, NikaLint, NikaExplain, NikaTest, NikaInfo
- README with lazy.nvim + packer setup
- STILL NEEDED: Submit PR to `neovim/nvim-lspconfig`

**Helix** (`editors/helix/`, 5 files, 569 lines):
- `languages.toml` (55 lines): nika language + dual LSP (nika-lsp + yaml-language-server)
- `queries/nika/highlights.scm` (211 lines): unified with Zed/Neovim
- `queries/nika/textobjects.scm` (40 lines): maf/mif=task, mac/mic=workflow, mae/mie=pair
- `queries/nika/indents.scm` (41 lines): YAML indentation rules
- Root patterns: nika.toml, .nika, .git
- STILL NEEDED: Submit PR to `helix-editor/helix`

**Shared** (`editors/shared/`):
- `nika-keywords.json`: canonical keyword DB (7 categories: verbs, transforms, builtins,
  workflow_keys, task_keys, providers, extract_modes) — generated from Rust source
- `extract-keywords.py` (~160 lines): parses 6 Rust source files, sanity checks, --check mode

### B. Highlights Unified Across 3 Editors

All Tree-sitter editors (Zed, Neovim, Helix) now have FULL feature parity:
- 5 verbs (infer, exec, fetch, invoke, agent) — keyword.function/keyword.control
- 14 provider names — constants
- 9 extract modes — constants
- HTTP methods (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS)
- Tool patterns (nika:*, server::tool) — function calls
- $env.* — variable.builtin
- Template expressions {{...}} in strings
- NIKA-XXX error codes in comments
- Task ID values — function names
- JSON Schema fields (properties, required, items, minimum, etc.)
- 52 verb sub-fields (prompt, url, command, tool, max_turns, etc.)

Capture name conventions differ per editor (Zed=@type, Neovim=@field, Helix=@type)
but semantic mapping is identical.

### C. Sync Automation

`editors/sync-editors.sh` (745+ lines):
- Extracts keywords from `KNOWN_TRANSFORM_NAMES`, `KNOWN_BUILTIN_TOOLS`,
  `KNOWN_TASK_KEYS` in nika-core Rust source
- Compares against 4 editor configs (VS Code TextMate, Zed .scm, Neovim .scm, Helix .scm)
- `--fix` auto-updates all editors when drift detected
- `--json` for CI, `--verbose` for debug
- Reviewed by bash-pro agent: 9 bugs fixed (double-output, symlinks, injection, guards)
- CI workflow: `.github/workflows/editor-sync.yml` — runs on PR when source or editors change

### D. LSP Defaults Moved Server-Side

The LSP server (`nika-lsp`) ignores initOptions and uses sensible defaults:
- validation.enabled = true
- completion.providers = true
- completion.mcpTools = true
- diagnostics.delay = 300ms
Client configs simplified: Zed removed init_options method, Neovim default = {}

### E. Distribution Audit + Fixes

Channel matrix discovered:
| Channel | Was | Fixed |
|---------|-----|-------|
| VS Code Marketplace | v0.74.0 (working) | metadata + icon added |
| Open VSX (Cursor) | NEVER published | ovsx flag bug fixed |
| Homebrew | v0.72.0 | HOMEBREW_TAP_TOKEN expired (manual) |
| npm | v0.71.0 | Windows build cascade (needs fix) |
| crates.io | v0.47.1 | Abandoned (decide strategy) |
| Docker | v0.74.0 | Working |

CI was RED: Clippy `approx_constant` on 3.14/2.71 → fixed (3.16/2.73)
Also: `.unwrap()` in MinBy/MaxBy → `is_none_or()` (Rust 1.94 idiom)
Also: `media-provenance` bundled in default (zero opt-in features)

### F. Code Reviews (4 specialized agents)

1. **Code reviewer**: 1 blocking (unwrap) + 7 warnings + 5 suggestions → all addressed
2. **Rust pro**: Zed lib.rs refactored (LspBinary type, combinator chain, zed_extension_api 0.7)
3. **Architect**: Diamond pattern documented, sync-editors.sh → template generation proposal,
   CI integration added, nika-keywords.json as canonical source
4. **Bash pro**: 9 bugs in sync-editors.sh fixed (portability, safety, correctness)

### G. Architecture Documented

17 crates follow the Diamond Pattern:
```
L0 nika-core (35K)     — ZERO I/O
L1 nika-lsp-core (12K) — pure, zero async
L2 nika-engine (158K)  — execution
L3 Support crates      — display, media, mcp, daemon, vault, storage, event
L4 Surface crates      — cli, tui, serve, lsp, sdk
L5 nika (2K)           — binary
```

Split rule: only when 2 consumers need same code without the other's deps.
22K LOC/crate average — highest density of any comparable Rust project.
Compared to: rust-analyzer (42 crates), Nushell (46), Helix (14), Zed (444).

Documented in: `dx/.claude/rules/architecture.md`

### H. Research Reports (5)

1. **Competitive landscape (April 7)**: "Inference as Code" = exclusively Nika on GitHub.
   Zero Rust competitors at scale. MCP at 9,700+ repos. LangChain fatigue.
   Nika occupies empty quadrant: YAML + Rust + CLI + AI-native.

2. **Show HN playbook**: Post Tue/Wed 14h Paris. URL = blog post (NOT repo).
   AGPL: mention factually, don't apologize. Stay 6h+ in comments.
   Engineering first, AI second. ripgrep model: deep technical blog = launch asset.

3. **Zed deep integration**: 4 layers (LSP + MCP Context Server + Tree-sitter + Tasks).
   Slash commands REMOVED from Zed, replaced by MCP. No custom UI panels in WASM.
   Context server = killer feature. Process::Command available. HTTP client available.

4. **Editor audit (9 editors)**: Neovim (HIGH) > Helix (HIGH) > Zed (MEDIUM-HIGH) >
   Sublime (MEDIUM) > Emacs (MEDIUM) > JetBrains (LOW-MEDIUM) > Nova/Lapce/Kakoune (LOW).

5. **Rust workspace best practices**: Universal "diamond" pattern across all major Rust projects.
   nika-core as foundation (zero I/O) matches every successful project.
   nika-engine at 158K LOC is dense but intentional for solo dev.

### I. Launch Plan (May 5)

4-week plan at `docs/plans/2026-04-07-launch-plan-may5.md`:
- W1: Distribution (tag, Open VSX, npm, upstream PRs)
- W2: Docs + Polish (9 Mintlify pages, API ref, rate limit, LSP diagnostics)
- W3: Content (README, Show HN blog, demo video, comparison page, community)
- W4: Stabilization (install script, cross-platform tests, tag, LAUNCH May 5)

---

## WHAT STILL NEEDS TO HAPPEN

### IMMEDIATE (this session)
1. ✅ Clippy clean (redundant closure fixed)
2. Run full test suite (10,435+ must pass)
3. Commit all uncommitted work (33 files in working tree)
4. Consider: are the Templatable<T> changes ready to commit? Or WIP branch?

### MANUAL (Thibaut only, not automatable)
1. Generate OVSX_PAT on open-vsx.org → GitHub secrets
2. Refresh HOMEBREW_TAP_TOKEN → GitHub secrets
3. Push + tag → CI publishes everywhere

### UPSTREAM PRs (Week 1)
1. `zed-industries/extensions` — Zed extension registry
2. `neovim/nvim-lspconfig` — nika_lsp server config
3. `helix-editor/helix` — nika language + queries

### DOCS (Week 2)
9 missing Mintlify pages: quickstart, transforms (64), builtins (63), structured-output,
scheduling, testing, install, configuration, FAQ

### CONTENT (Week 3)
- Show HN blog post (2000-5000 words, technical, benchmarks)
- Demo video (asciinema, < 3min)
- README final polish

### ARCHITECTURE IMPROVEMENTS (post-launch)
- Replace sync-editors.sh (745 lines bash) with template generation (generate.py + templates/)
- Consider nika-parser crate extraction (AST parsing from engine)
- Tree-sitter grammar dédié (tree-sitter-nika vs piggybacking on tree-sitter-yaml)
- `nika init` generates editor configs (.zed/tasks.json, .vscode/extensions.json)

### ZED DEEP INTEGRATION (post-initial-launch)
- Phase 4: Binary auto-download via `download_file` capability
- Phase 5: Submit to Zed extensions registry
- Semantic token rules mapping (custom → theme captures)
- Explore ACP (Agent Client Protocol) for Nika-as-agent in Zed

## KEY FILES TO READ

- `docs/plans/2026-04-07-launch-plan-may5.md` — Full 4-week plan
- `docs/plans/2026-04-07-zed-deep-integration-plan.md` — Zed 4 layers
- `docs/plans/2026-04-07-model-resilience.md` — gpt-5.2 hardening (5 phases)
- `dx/.claude/rules/architecture.md` — Editor arch + crate diamond
- `editors/README.md` — Multi-editor architecture overview
- `editors/shared/nika-keywords.json` — Canonical keyword database
- `editors/zed/src/lib.rs` — Zed extension (LSP + MCP)
- `editors/neovim/lua/nika/init.lua` — Neovim plugin
- `editors/helix/languages.toml` — Helix config

## MEMORY ENTRIES (for future sessions)
- `project_architecture_philosophy.md` — Diamond pattern, split rules, v0 philosophy
- `project_editor_architecture_2026_04_07.md` — 5 editors, sync, MCP context server
- `project_distribution_audit_2026_04_07.md` — Channel matrix, fixes applied
- `project_launch_plan_may5.md` — Show HN playbook, competitive matrix
- `feedback_no_conservative_delays.md` — NEVER delay "post-launch", ship everything NOW
```

---

## J. AI Assistant Deep Integration (RESEARCHED, PLANNED)

Nika covers 8/15 AI coding assistants. No framework in the world covers more than 2.

### Already shipping (via nika init + nika setup):
- Claude Code: claude.md (563 lines) → ~/.claude/rules/nika.md + CLAUDE.md symlink
- Cursor: cursor.mdc (518 lines) → ~/.cursor/rules/nika.mdc
- Copilot: copilot.md (516 lines) → .github/copilot-instructions.md
- Windsurf: windsurf.md (518 lines) → ~/.windsurf/rules/nika.md
- Roo Code: roo.md (517 lines) → ~/.roo/rules/nika.md
- AGENTS.md: cross-tool standard (60K+ repos, Linux Foundation)
- Zed: MCP Context Server (nika mcp via extension)
- Aider/Codex: read AGENTS.md natively

### Missing (6 to add — same content, different format/path):
- Gemini CLI: .gemini/GEMINI.md
- Amazon Q: .amazonq/rules/nika.rule.md
- JetBrains AI: .aiassistant/rules/nika.md
- Cline: .clinerules
- Continue.dev: .continue/assistant/config.yaml mcpServers
- Sourcegraph Cody: .cody/ignore (limited)

### Claude Code Plugin (THE game changer)

8 integration surfaces discovered:
1. CLAUDE.md (done)
2. .claude/rules/ — modular rules (11 in dev repo, 0 shipped to users)
3. .claude/commands/ — /nika-run, /nika-check, /nika-new, /nika-debug
4. .claude/agents/ — workflow-designer, nika-debugger subagents
5. .claude/skills/ — nika-yaml (auto-trigger on *.nika.yaml), nika-debug
6. Hooks — PostToolUse auto-validates .nika.yaml on every Edit/Write
7. MCP server — .mcp.json pre-populated with nika mcp serve
8. Plugin marketplace — /plugin marketplace add SuperNovae-studio/nika-plugin

Plugin bundles everything: commands + agents + skills + hooks + MCP + LSP.
One install, permanent. First workflow engine plugin on Claude Code marketplace.

### Cursor Deep Integration (RESEARCHED)

Current: single 518-line .mdc file. Research says split into 3:
- nika-project.mdc (~20 lines, alwaysApply: true) — project identity
- nika-syntax.mdc (~150 lines, globs: *.nika.yaml) — 5 verbs, data flow
- nika-mistakes.mdc (~80 lines, globs: *.nika.yaml) — error table

Also: pre-populate .cursor/mcp.json with nika MCP server.
nika-lang extension already installed in Cursor (v0.51.0 found).

### NEW: Progressive Discovery Architecture

Old model: 5 monolithic 500-line files per AI tool. AI absorbs ~30%.
New model: shared/ content modules + per-tool assemblers + MCP live layer.

Plan: `docs/plans/2026-04-07-ai-rules-architecture.md`

4 layers:
- L0 Identity (<20 lines, always loaded) — "Nika project, schema @0.12"
- L1 Syntax (~100 lines, on *.nika.yaml edit) — verbs + data flow
- L2 Reference (~80 lines, on demand) — transforms, errors, providers
- L3 Live (MCP + LSP) — nika_schema, nika_check, nika_error_lookup

Source modules: tools/nika-cli/rules/shared/ (identity, verbs, data-flow,
  common-mistakes, providers, structured-output, advanced)
Assembly: per-tool in init.rs (compose modules → tool-specific format)
Hook: PostToolUse auto-validates .nika.yaml on Edit/Write

### Daemon + Doctor Integration (brainstormed, not yet implemented)

The lifecycle must be CLOSED — no stale files, no missing configs:
- **daemon**: detect CLI version change at startup → auto fast_rule_update()
  (mechanism exists in install.rs with xxhash, just needs daemon trigger)
- **doctor**: new `check_ai_ecosystem()` section — verify rules freshness,
  MCP configs present, editor extensions installed, hooks configured
  `--fix` auto-repairs everything
- **init**: smart detection — only generate files for tools that are installed
  (check ~/.claude/, ~/.cursor/, `code` in PATH, `zed` in PATH)

Full design in `docs/plans/2026-04-07-ai-rules-architecture.md` section
"Daemon + Doctor Integration".

### Implementation Tasks (next session):
1. Create tools/nika-cli/rules/shared/ modules (extract from existing claude.md)
2. Create per-tool assemblers in init.rs
3. Split cursor.mdc into 3 files (project/syntax/reference)
4. Pre-populate .mcp.json + .cursor/mcp.json with nika MCP server
5. Add .claude/settings.json with hooks + permissions
6. Add 6 missing AI tools (Gemini, Amazon Q, JetBrains, Cline, Continue, Codex)
7. Create Claude Code plugin skeleton
8. Update nika setup for multi-file deployment
9. Update xxhash fingerprinting for per-file tracking

### Architecture Detail: Deployment System
```
Source of truth: tools/nika-cli/rules/ (5 files, compile-time embedded)
                         │
                    include_str!()
                         │
              ┌──────────┼──────────┐
              │          │          │
         nika init   nika setup  fast_rule_update()
              │          │          │
         Project     User home   Auto on version
         files       directories  change
              │          │          │
         AGENTS.md   ~/.claude/   xxhash64
         .mcp.json   ~/.cursor/   fingerprint
         .github/    ~/.windsurf/ (no stale
         .cursor/    ~/.roo/       rules)
```

---

## K. The 6 Architectural Innovations (DETAILED)

### 1. LSP Découplé — 15s vs 90s compile

nika-lsp-core depends on nika-core ONLY (35K LOC, zero I/O).
NOT on nika-engine (158K LOC, all providers, all builtins, mistralrs, rig-core, reqwest).

Impact: when you modify a provider or builtin, the LSP does NOT recompile.
LSP devs pay 15s compile. Engine devs pay 90s. Independence.

Same pattern as rust-analyzer: `ide` crate knows nothing about LSP protocol.
`rust-analyzer` crate is the thin adapter. Our `nika-lsp` is 4K LOC adapter.

### 2. Textobjects Helix — Semantic YAML Selection

maf / mif = select around/inside a TASK (block_sequence_item that starts with `- id:`)
mac / mic = select around/inside the WORKFLOW (entire document)
mae / mie = select around/inside a key-value pair

First-ever semantic textobjects for a YAML-based workflow language in Helix.
No other YAML tool has this — kubectl, helm, ansible all use generic YAML textobjects.

### 3. Zed MCP Context Server — First of Its Kind

nika mcp registered as context_server in extension.toml.
Zed's AI Agent Panel can call Nika tools: validate, generate_task, error_lookup, dag_viz.
Searched all published Zed extensions: NONE use context_server_command() for a dev tool.
Only usage is LLM providers (Ollama, etc.). Nika = first workflow tool as MCP in Zed.

### 4. Compound Filetype yaml.nika

Neovim filetype = "yaml.nika" (not just "nika").
All existing YAML plugins continue working (schemastore, yaml-companion, indent rules).
Nika layer adds ON TOP: LSP, keymaps, highlights, commands.
Same pattern as typescript.tsx, markdown.mdx — official Neovim convention.

### 5. Sanity Checks in extract-keywords.py

If transforms < 60: REFUSE to generate (parser broken detection)
If builtins < 50: REFUSE to generate
If verbs != 5: REFUSE to generate
Prevents a broken parser from propagating empty keywords to 4 editors.
Fail-safe by design. CI catches it before merge.

### 6. Runnables in Zed — Inline ▶ Buttons

runnables.scm detects `workflow: <name>` pattern via Tree-sitter.
Zed shows a ▶ button inline next to the workflow declaration.
Click → executes `nika run $ZED_FILE` in integrated terminal.
Same UX as "Run Test" buttons in test files. First workflow engine with this.

---

## Session Statistics

| Metric | Value |
|--------|-------|
| Agents deployed | 25+ |
| Files created | 35+ |
| Files modified | 20+ |
| Research reports | 5 |
| Code reviews | 4 (code-reviewer, rust-pro, architect, bash-pro) |
| Plans written | 3 (launch, Zed, model resilience) |
| Podcast generated | 1 (12min28s FR) |
| Memory entries created | 5 |
| DX rules updated | 1 (architecture.md) |
| Bugs found + fixed | 18+ |
| Lines of handoff | 400+ |
