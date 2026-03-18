# Nika LSP — Definitive Plan (Source of Truth)

> Version: 2.0 | Date: 2026-03-18 (updated after v0.30.6 analysis)
> Research: 30+ Opus agents, 6 rounds, ~600K chars of analysis
> Baseline: v0.30.6 (5,219 tests passing, display module, DAG v3, model_intel.rs)
> Status: **APPROVED + CALIBRATED** — decisions finalized with user confirmation

## Confirmed Decisions (from brainstorm)

| Decision | Answer | Impact |
|----------|--------|--------|
| model_intel.rs | **Commit now** (A), move to nika-lsp-core later | PR 0.5 |
| tower-lsp upgrade | **In PR 3** (B), not separate PR 0 | Simplifies |
| Order | **WorldDatabase first** (A), then error recovery | PR 1 → PR 2 |
| Timeline v0.31 | **12-15 weeks OK** | Realistic |
| Standalone merge | **Direct in PR 1** (merge unique files immediately) | Less PRs |
| REPL mode | **Full Jupyter notebook** (Level C) | PR 15 |
| DAG visual | **Mode 2 bidirectional** (drag → YAML, YAML → graph) | PR 12 |
| Notebook | **VS Code Notebook API native** (A+) | PR 15 |

## v0.30.6 Baseline (what we START with)

| Component | Status | LOC | Tests |
|-----------|--------|-----|-------|
| display.rs (DAG v3 ASCII, verb icons, box headers) | **NEW, committed** | 805 | Used in 5,219 suite |
| model_intel.rs (model catalog, capabilities, pricing) | **NEW, NOT committed** | 1,413 | 31 |
| 7 LSP handlers (completion, hover, def, actions, semantic, symbols) | Stable | 6,286 | 179 |
| LSP infrastructure (ast_index, doc_store, conversion, capabilities) | Stable | 2,032 | ~30 |
| Standalone nika-lsp (65% duplicate) | To merge | 4,019 | ~73 |
| Full test suite | **5,219 passing** | — | 5,219 |
| Unique standalone files to merge: node_context, mcp_discovery, template_validation | 3 files | 1,210 | ~28 |

### Key Leverageable Assets

1. **`display.rs`** (805 LOC) — DAG rendering with verb icons, box-drawing, status badges. Can be reused for LSP inlay hints and code lens visual style.
2. **`model_intel.rs`** (1,413 LOC) — Complete model catalog. Ready for hover, code actions, cost radar.
3. **`tree-sitter-yaml 0.7`** — Already a dep (TUI uses it). Just needs LSP bridge.
4. **`cost.rs`** — Pricing for 7 providers, 50+ models. Wired to ProviderResponded events.
5. **Two-Phase IR** — Raw → Analyzed pipeline stable. Phase 2 emits NIKA-140-151 diagnostics.
6. **`depends_on:` only** — Nuclear purge of `flow:` complete. No migration logic needed.

---

## Vision

**"The rust-analyzer of AI workflow engines"** — but with features no LSP has ever had, because no other tool is simultaneously a language, a runtime, an AI orchestrator, and an MCP integration platform.

---

## Architecture (Final)

### 3-Crate Design

```
nika-lsp-core/          # Protocol-agnostic intelligence (NO lsp-types dep!)
  ├── Own types: Diagnostic, TextRange, Severity, AnalysisSnapshot
  ├── WorldDatabase (generation-based, NOT salsa)
  ├── Handlers: pure fn(db, file, pos) -> Response
  ├── tree-sitter-yaml for error recovery (STRUCTURE-ONLY)
  └── Consumed by: LSP, TUI, CLI (all 3)

nika (main binary)
  ├── src/lsp/ → thin tower-lsp-server 0.23 shim → delegates to nika-lsp-core
  ├── src/tui/ → embeds nika-lsp-core directly (zero IPC)
  └── nika check → WorldDatabase one-shot

nika-lsp (standalone binary)
  └── ~30 lines: tower-lsp wiring + stdio
```

### Key Invariants

1. **`nika-lsp-core` does NOT depend on `lsp-types`** — own Diagnostic/TextRange/Severity
2. **tree-sitter bridge is STRUCTURE-ONLY** — never trust ts-yaml value types (YAML 1.1 vs 1.2)
3. **Inter-file deps**: `include_dependencies: DashMap<Url, Vec<Url>>` + `workspace_revision: AtomicU64`
4. **Handlers are pure functions** — no state, no async, testable without transport
5. **Error codes**: always `NikaError` with `NIKA-XXX`, never `anyhow`

---

## Roadmap: 20 PRs across 4 versions

### v0.31 "Already 11/10" — 12-15 weeks, 7 PRs

| PR | Scope | Weeks | Tests | Checkpoint |
|----|-------|-------|-------|------------|
| **PR 0.5** | Commit model_intel.rs + fix Cargo.toml comment (v0.22→0.20) | 0.5 | 31 new (5,250 total) | `cargo test --lib && cargo clippy` |
| **PR 1** | `nika-lsp-core` crate: WorldDatabase, LineIndex, PositionIndex, AnalysisSnapshot, DagGraph. **Merge standalone unique files** (node_context→context.rs, mcp_discovery→mcp_tools.rs, template_validation→template.rs). proptest + criterion. | 3 | 80+ unit, proptest | `cargo test -p nika-lsp-core --lib` |
| **PR 2** | Error recovery: tree-sitter-yaml bridge (STRUCTURE-ONLY). 50+ broken YAML fixtures with insta snapshots. | 2 | 50+ snapshots | All fixtures produce PartialWorkflow |
| **PR 3a** | Handler migration: wire nika-lsp-core handlers into both entry points. Old code stays. tower-lsp upgrade to 0.23 happens HERE. | 2.5 | 250+ (merged) | Feature parity verified |
| **PR 3b** | Delete old handlers (19 files). Only after PR 3a proven 1 week. | 0.5 | Same 250+ pass | `git diff --stat` shows net LOC decrease |
| **PR 4** | Inlay hints (3 ON: deps, timeout, binding) + Code Lens (Run/Validate, Workspace Trust gated) + wire model_intel into hover/code_action | 2 | 40+ new | Visible in VS Code |
| **PR 5** | Rename + References + Folding + Document Links + Highlight + Call Hierarchy | 1.5 | 30+ new | `nika.check` equivalent in editor |

**v0.31 exit criteria**: 0% duplication, error recovery works, <50ms completions, inlay hints visible, code lens clickable, rename across file. 400+ LSP tests. 5,400+ total tests.

**v0.31 delivers**: Zero duplication, error recovery on broken YAML, cached <50ms completions, inlay hints, code lens, rename, references. Already better than any YAML workflow LSP.

### v0.32 "Intelligence" — 7 weeks, 5 PRs

| PR | Scope | Weeks |
|----|-------|-------|
| **PR 6** | Elm-style errors + expanded coverage (32% → 60%+). 20+ new code actions. | 2 |
| **PR 7** | Template intelligence: scope lens (what can I access here?), transform chain type-checking, `{{with.data.` completions from upstream output.schema | 2 |
| **PR 8** | Prompt quality linter (world's first!) + Cost radar inlay hints (uses cost.rs + model_intel.rs) + provider switcher code action | 1.5 |
| **PR 9** | DAG bottleneck detection (critical path at edit time) + Smart scaffold code action ("scaffold this task" based on DAG context) | 1 |
| **PR 10** | Workflow recipes system: 12 built-in recipes, `src/recipe/` module, `nika recipe` CLI, LSP code actions, pattern detector | 0.5 |

**v0.32 delivers**: 10 novel features with zero prior art (prompt linter, cost radar, DAG bottleneck, smart scaffold, template scope lens, transform type-checker, binding cycle detector, MCP tool signatures, provider switcher, workflow recipes).

### v0.33 "Magic" — 8 weeks, 5 PRs

| PR | Scope | Weeks |
|----|-------|-------|
| **PR 11** | Live MCP tool discovery: async schema fetch, cached in WorldDatabase, param completions, tool hover docs | 2 |
| **PR 12** | DAG visualization: VS Code webview with React Flow v12 + ELK.js. Mode 2 bidirectional (drag → YAML, YAML → graph). Custom nodes per verb, animated edges, critical path glow. | 3 |
| **PR 13** | Time-travel debugging: enriched trace v2 (PromptSnapshot, RawResponse), timeline model, replay engine, VS Code panel with step-through, diff mode for A/B. | 2 |
| **PR 14** | Learning mode: ghost text walkthroughs, progressive hover docs (3 tiers), "What's next?" code action, first-run detection, VS Code walkthrough page | 0.5 |
| **PR 15** | REPL/Notebook mode: VS Code Notebook API (cells = tasks, ▶ Run per cell, output blocks, scratch pad, mock mode, staleness detection) | 0.5 |

**v0.33 delivers**: Visual DAG editor, time-travel debugging, full notebook experience, learning mode. This is the "Figma for AI workflows" moment.

### v0.34 "Ecosystem" — ongoing

| PR | Scope |
|----|-------|
| **PR 16** | Gradual type system: `types:` block, `returns:` annotation, Phase 2.5 type checker, NIKA-155-159, cross-file types |
| **PR 17** | Prompt diff + A/B testing: semantic diff, `# @experiment:` annotations, prompt templates library (chain-of-thought, few-shot wrapping) |
| **PR 18** | Formatting (opinionated) + workspace diagnostics + workflow linter (best practices) |
| **PR 19** | TUI Studio refactoring → IDE layout: File Tree, Context Inspector (4 modes), DAG panel, trace timeline, keybindings (F5 Run, F6 Validate) |
| **PR 20** | Multimodal feature flags: trait interfaces for media artifacts (image preview on hover, format compatibility). Connect when media pipeline is ready. |

---

## 20 Novel Features Catalog

| # | Feature | PR | Prior Art | Effort | Impact |
|---|---------|-----|-----------|--------|--------|
| 1 | **Data Flow Type Propagation** | PR 7 | Zero | Medium | Catches deep property typos in templates |
| 2 | **Template Scope Lens** | PR 7 | Zero | Medium | "What can I access here?" completions |
| 3 | **Transform Chain Type-Checker** | PR 7 | Zero | Low-Med | Only valid transforms suggested after `\|` |
| 4 | **Binding Cycle Detector** | PR 6 | Zero | Low | Catches implicit deadlocks at edit time |
| 5 | **Prompt Quality Linter** | PR 8 | Zero | Medium | World's first in-editor prompt linter |
| 6 | **Cost Radar** | PR 8 | Infracost (IaC) | Medium | Per-task `[~$0.03]` and workflow `[$0.45]` |
| 7 | **DAG Bottleneck Detection** | PR 9 | Zero | Medium | Critical path analysis at edit time |
| 8 | **Smart Scaffold** | PR 9 | Zero | Med-High | DAG-aware task generation |
| 9 | **MCP Tool Signatures** | PR 11 | Zero | High | IntelliSense for MCP tools |
| 10 | **Provider Switcher** | PR 8 | Zero | Medium | "Switch to gpt-4o-mini: save 90%" |
| 11 | **Workflow Recipes** | PR 10 | npm init | Low | 12 parameterized workflow templates |
| 12 | **DAG Visual Editor** | PR 12 | Node-RED | High | Bidirectional YAML ↔ graph editing |
| 13 | **Time-Travel Debugging** | PR 13 | Redux DevTools | High | Step through workflow execution |
| 14 | **Notebook Mode** | PR 15 | Jupyter | High | Full notebook with cells = tasks |
| 15 | **Learning Mode** | PR 14 | Zero | Medium | Ghost text + progressive hover |
| 16 | **Gradual Types** | PR 16 | TypeScript | High | Optional type annotations for YAML |
| 17 | **Prompt Diff** | PR 17 | Zero | Medium | Semantic diff of prompt changes |
| 18 | **A/B Testing** | PR 17 | Zero | High | `# @experiment:` variant comparison |
| 19 | **Workflow Diff Preview** | PR 13 | Zero | High | Semantic DAG diffing |
| 20 | **Model Intelligence** | PR 4 | Zero | Done! | 1,413 LOC, 31 tests (model_intel.rs) |

---

## UX Philosophy

### Trust Equation

```
Trust = (Accuracy x Relevance) / (Frequency x Intrusiveness)
```

### 3 Diagnostic Levels

| Level | Default | Shows |
|-------|---------|-------|
| `essential` | **YES** (new users) | Errors only |
| `recommended` | After opt-in | + Warnings |
| `comprehensive` | Power users | + Info/hints/best practices |

### Inlay Hint Defaults

| Hint | Default | Rationale |
|------|---------|-----------|
| Dependency chain | **ON** | Invisible without scrolling |
| Timeout clarification | **ON** | `30` is ambiguous |
| Binding source | **ON** | Helps beginners |
| Verb badge | OFF | Already visible as YAML key |
| Template preview | OFF | Dynamic, can mislead |
| Provider | OFF | Debug-only |
| Cost estimate | OFF | Needs accuracy validation |

### Accessibility

- Severity prefix in ALL messages: `[ERROR] NIKA-020: ...`
- Tooltips on all inlay hints
- 3-sentence diagnostic: what, why, how to fix
- Test against high-contrast themes

### Security

- Code Lens "Run" gated by VS Code Workspace Trust
- Untrusted workspaces: "Validate" only, never "Run"
- exec: tasks require confirmation in playground mode
- Per-session cost cap in REPL mode

---

## Visual Stack (DAG + Notebook)

### DAG: React Flow v12 + ELK.js

```
editors/vscode/src/dag/
  DagWebviewProvider.ts      # VS Code webview lifecycle
  DagPanel.tsx               # React Flow canvas
  components/
    nodes/                   # 5 verb nodes + ForEachGroup
    edges/                   # DependsOn (solid) + DataFlow (dashed animated)
    layout/elkLayout.ts      # ELK.js Sugiyama DAG layout
    sync/syncEngine.ts       # Bidirectional YAML ↔ graph
```

**Verb colors**: purple=infer, green=exec, blue=fetch, yellow=invoke, red=agent

**Bidirectional sync**: Drag node → generates YAML. Edit YAML → graph updates. Reconciliation engine handles conflicts.

### Notebook: VS Code Notebook API

```
editors/vscode/src/notebook/
  NikaNotebookSerializer.ts    # .nika.yaml ↔ notebook cells
  NikaNotebookController.ts    # Kernel: ▶ Run sends nika/playground/run
  NikaOutputRenderer.ts        # Markdown, JSON tree, image preview
  NikaScratchPad.ts            # Free-form prompt testing panel
```

**Cells = Tasks**. Each task is a cell. Run button per cell. Output block below with formatted result. Scratch pad for testing prompts without creating tasks.

---

## TUI Studio IDE Layout

```
┌──────────────┬─────────────────────┬──────────────────┐
│ File Tree    │  YAML Editor        │  Context Panel   │
│              │  (nika-lsp-core)    │  (cursor-aware)  │
│ workflows/   │                     │                  │
│ ├ blog.nika  │  Completions        │  On task: deps,  │
│ ├ seo.nika   │  Inline diagnostics │   output, cost,  │
│ └ test.nika  │  Inlay hints        │   last run stats │
│              │  Line numbers       │                  │
│              │                     │  On verb: docs,  │
│              │                     │   examples, fields│
│              │                     │                  │
│              │                     │  On binding:     │
│              │                     │   source, type,  │
│              │                     │   used-in list   │
├──────────────┴─────────────────────┤                  │
│  DAG View (interactive ASCII)      │  On template:    │
│  [research]→[write]→[publish]      │   resolves-to,   │
│  Click = jump, color = verb        │   available       │
├────────────────────────────────────┤   transforms     │
│  Bottom: Diagnostics | Trace | Out │                  │
├────────────────────────────────────┴──────────────────┤
│  [F1 Help] [F5 Run] [F6 Check] [F7 Trace] $0.45 12t │
└───────────────────────────────────────────────────────┘
```

4 inspector modes based on cursor position: Task, Verb, Binding, Template. Each shows contextually relevant information from `AnalysisSnapshot`.

---

## Code Review Methodology (per PR)

### Pre-PR Checklist
- [ ] Read existing related code thoroughly
- [ ] Write handler test signatures FIRST (TDD)
- [ ] Property-based tests for position-dependent features (proptest)
- [ ] Check if a skill applies (rust-core, rust-async, rust-security)

### During PR Checklist
- [ ] `cargo test --lib` passes (no keychain!)
- [ ] `cargo clippy -- -D warnings` zero warnings
- [ ] No new dependencies without justification in PR description
- [ ] Error codes follow NIKA-XXX convention
- [ ] NikaError, not anyhow
- [ ] 1 FIX = 1 COMMIT with co-author lines
- [ ] AST: always Raw → Analyzed → Lower, never skip phases

### Post-PR Review (launch `spn-rust:rust-pro` agent)
- [ ] Architecture conformance (3-crate boundaries respected)
- [ ] Performance check (criterion benchmarks for new hot paths)
- [ ] Security review (exec: tasks, MCP connections, file paths)
- [ ] Accessibility check (severity prefixes, tooltips, contrast)
- [ ] Test coverage (new code tested, snapshots where applicable)
- [ ] No regressions in existing 5400+ tests

### Code Review Agent Prompt Template
```
Review this PR against the Nika LSP definitive plan:
1. Does it respect the 3-crate architecture?
2. Are handlers pure functions with no state?
3. Does it use NikaError with NIKA-XXX codes?
4. Are there proptest/criterion tests for hot paths?
5. Does it handle broken YAML gracefully (error recovery)?
6. Is the UX consistent with the trust equation?
```

---

## Testing Strategy

### Test Pyramid

```
                /\
               /  \      Integration: 20+ (real LSP protocol)
              /────\
             /      \    Snapshot: 100+ (insta YAML per handler)
            /────────\
           /          \  Property: 30+ (proptest for positions)
          /────────────\
         /              \ Benchmark: 10+ (criterion for latency)
        /────────────────\
       /                  \ Unit: 300+ (pure function tests)
      /____________________\
```

### Fixture Corpus
```
fixtures/broken/              # 50+ files for error recovery
  missing-colon.nika.yaml
  incomplete-task.nika.yaml
  duplicate-verb.nika.yaml
  mixed-indentation.nika.yaml
  truncated-file.nika.yaml
  unicode-keys.nika.yaml
  empty.nika.yaml
  comments-only.nika.yaml
  multi-document.nika.yaml
  ... (40 more)
```

### Performance Targets

| Metric | Target | How to measure |
|--------|--------|----------------|
| Completion latency | < 50ms | criterion bench with 100-task fixture |
| Hover latency | < 30ms | criterion bench |
| Diagnostic latency | < 200ms | debounced background, wall-clock |
| Position conversion | O(log n) | criterion, proptest roundtrip |
| Node lookup | O(log n) | criterion with PositionIndex |
| Startup | < 1s | wall-clock to first diagnostic |
| Memory (100 tasks) | < 10MB | RSS measurement |
| Broken YAML completions | 100% | fixture corpus coverage |

---

## Error Code Coverage Map

### v0.31 Target: 40%+

| Range | Category | v0.31 | v0.32 | v0.33 |
|-------|----------|------|------|------|
| 000-009 | Workflow | YES | YES | YES |
| 010-019 | Schema | YES | YES | YES |
| 020-029 | DAG | YES | YES | YES |
| 030-039 | Provider | Partial | YES (model_intel) | YES |
| 040-049 | Template | NO | YES (PR 7) | YES |
| 050-059 | Task/path | YES | YES | YES |
| 060-069 | Output | NO | Partial | YES |
| 070-089 | With+DAG validation | YES | YES | YES |
| 090-099 | JSONPath/IO | NO | Partial | Partial |
| 100-109 | MCP | NO | NO | YES (PR 11) |
| 110-119 | Agent | NO | Partial | YES |
| 140-151 | AST analysis | YES | YES | YES |
| 155-159 | Type checking | NO | NO | YES (PR 16) |
| 160-164 | Parse errors | YES | YES | YES |
| 280-289 | Artifacts | NO | Partial | YES |
| 290-294 | Recipes | NO | YES (PR 10) | YES |
| 300-309 | Structured output | NO | Partial | YES |
| 350-359 | Trace replay | NO | NO | YES (PR 13) |

---

## VS Code Extension Roadmap (parallel to Rust LSP)

### v0.31 Extension
- [ ] Fix TextMate grammar: multiline strings (`|`, `>`), `edges:`, `skills:`
- [ ] 7 verb-specific snippets
- [ ] Status bar item (LSP running/error)
- [ ] 3 commands: nika.run, nika.check, nika.restartServer
- [ ] Keybindings: Ctrl+Shift+R (run), Ctrl+Shift+V (validate)
- [ ] Configuration: inlay hints, code lens, diagnostic level
- [ ] Icon files (nika-light.svg, nika-dark.svg)

### v0.32 Extension
- [ ] Provider switcher code action integration
- [ ] Cost radar display settings
- [ ] Recipe QuickPick command

### v0.33 Extension
- [ ] React Flow DAG webview (full bidirectional Mode 2)
- [ ] Time-travel replay panel + keybindings (Ctrl+Right/Left to step)
- [ ] Notebook serializer + controller + output renderers
- [ ] Scratch pad panel
- [ ] Learning mode walkthrough page

---

## Multimodal Feature Flags (v0.34, prep now)

Interfaces to define NOW (in nika-lsp-core) for later connection:

```rust
/// Trait for artifact preview providers.
/// Implement when media pipeline is ready.
pub trait ArtifactPreviewProvider: Send + Sync {
    fn can_preview(&self, mime_type: &str) -> bool;
    fn preview(&self, path: &Path) -> Result<ArtifactPreview>;
}

pub enum ArtifactPreview {
    ImageThumbnail { data: Vec<u8>, width: u32, height: u32 },
    AudioWaveform { samples: Vec<f32>, duration_ms: u64 },
    TextPreview { content: String, truncated: bool },
    JsonTree { value: serde_json::Value },
}
```

---

## Grand Summary

| Metric | Today (6.2/10) | v0.31 | v0.32 | v0.33 |
|--------|----------------|------|------|------|
| Error recovery | 0% | 100% | 100% | 100% |
| Error code coverage | 32% | 40%+ | 60%+ | 80%+ |
| LSP features | 7/17 | 14/17 | 17/17 | 17/17 + 20 novel |
| Code duplication | 65% | 0% | 0% | 0% |
| Completion latency | ~200ms | <50ms | <50ms | <50ms |
| Novel features | 0 | 1 (model_intel) | 10 | 20 |
| Visual DAG | None | None | None | Full bidirectional |
| Notebook mode | None | None | None | Full Jupyter-style |
| Tests | 179 | 400+ | 500+ | 600+ |
