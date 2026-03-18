# 31 — Nika LSP: World-Class UX Vision

> Research date: 2026-03-18
> Sources: Perplexity (rust-analyzer, Terraform, Prisma, GraphQL, GitHub Actions, Red Hat YAML LSP), deep code audit of 25+ LSP source files, schema/error analysis
> Goal: Make the Nika LSP the best workflow-DSL editing experience in existence

---

## Table of Contents

1. [Current State Audit](#1-current-state-audit)
2. [What Makes the Best LSPs Magic](#2-what-makes-the-best-lsps-magic)
3. [The 30 Ideas — Ranked by Impact](#3-the-30-ideas--ranked-by-impact)
4. [UX Sceptical Questions](#4-ux-sceptical-questions)
5. [Architecture Constraints](#5-architecture-constraints)
6. [Implementation Roadmap](#6-implementation-roadmap)

---

## 1. Current State Audit

### What We Have (Solid Foundation)

| Feature | Quality | Notes |
|---------|---------|-------|
| Diagnostics (Phase 1+2) | Good | 150+ error codes, spans, NIKA-XXX |
| Completions | Good | Verbs, task IDs, bindings, templates, providers, schemas |
| Hover | Good | Rich markdown docs for verbs/fields/tasks |
| Go-to-Definition | Good | Task refs, templates, includes |
| Code Actions | Good | Quick fixes, fuzzy task matching, field insertion |
| Semantic Tokens | Good | Keywords, verbs, variables, templates |
| Document Symbols | Good | Hierarchical outline |
| Incremental Sync | OK | INCREMENTAL mode, but no incremental re-parse |

### What We're Missing (Critical Gaps)

| Feature | Impact | Exists in... |
|---------|--------|-------------|
| **Inlay Hints** | HIGH | rust-analyzer, TypeScript |
| **Code Lens** | HIGH | rust-analyzer (Run test), gopls (gc_details) |
| **Rename/Refactor** | HIGH | All modern LSPs |
| **Document Links** | MEDIUM | Red Hat YAML (clickable URLs/paths) |
| **Folding Ranges** | MEDIUM | All YAML LSPs |
| **References** | MEDIUM | rust-analyzer, gopls |
| **Formatting** | MEDIUM | yaml-language-server |
| **Workspace Diagnostics** | MEDIUM | LSP 3.17 pull model |
| **Document Highlight** | LOW | rust-analyzer (highlight all refs) |
| **Selection Range** | LOW | Smart expand/shrink selection |
| **Call Hierarchy** | LOW | Shows task dependency chain |

### Architecture Observations

1. **Two LSP implementations** — `src/lsp/` (tower-lsp) AND `tools/nika-lsp/` (standalone) diverge in features. Need to consolidate or clearly separate responsibilities.
2. **No incremental re-parsing** — Full re-parse on every change. OK for small workflows, could lag on large ones (100+ tasks).
3. **UTF-16 conversion bug** — `conversion.rs` uses `ch.len_utf8()` instead of `ch.len_utf16()` for multi-byte chars (emoji/CJK). Will cause offset errors.
4. **Static MCP discovery** — `mcp_discovery.rs` has hardcoded tool defs. No live MCP connection.
5. **No error recovery parser** — Parse failure = no completions. Unlike rust-analyzer's rowan (always produces a tree).

---

## 2. What Makes the Best LSPs Magic

### rust-analyzer — The Gold Standard

What makes developers love it:

1. **Rowan CST (Concrete Syntax Tree)** — Parsing NEVER fails. Broken code produces error nodes, not parse failures. Every feature works on broken code. This is the #1 UX differentiator.
2. **Salsa incremental computation** — Only recomputes what changed. Sub-millisecond responses on edits.
3. **Inlay hints that teach** — Type hints, parameter names, lifetime elisions, binding modes. Users learn the language by reading hints.
4. **Context-aware highlighting** — Hover on `|` → highlights closure captures. Hover on `return` → highlights all exit points. Hover on `async` → highlights yield points. **This is pure magic.**
5. **Assists that understand intent** — "Extract function", "Fill match arms", "Convert to if-let". Not just fixes, but refactoring intelligence.
6. **Related tests discovery** — Put cursor on function → find all tests that call it.

### GitHub Actions LSP — Best YAML Workflow LSP

1. **Expression parser** — Custom parser for `${{ }}` expressions with completions for contexts, functions, payloads.
2. **Action refs intelligence** — `uses: actions/checkout@v4` → auto-fetch inputs/outputs from GitHub for completions.
3. **Event payload awareness** — Completions change based on `on: pull_request` vs `on: push`.

### Terraform LSP — Best IaC LSP

1. **Provider schema auto-fetch** — After `terraform init`, completions include all provider resources/attributes.
2. **Required fields auto-fill** — Selecting a resource auto-inserts all required fields with placeholders.
3. **Module treeview** — Widget showing module dependency tree.

### Prisma LSP — Best Schema DSL LSP

1. **Relation-aware completions** — Suggests `@relation` fields based on model structure.
2. **Migration preview** — Shows what migration would be generated.
3. **Formatting** — Opinionated auto-format on save.

### Elm Compiler — Best Error Philosophy

1. **Errors that teach** — Every error message explains WHY, suggests a fix, links to docs.
2. **Progressive disclosure** — One error at a time. Fix it, see the next one.
3. **"Did you mean?"** — Fuzzy matching for misspelled identifiers.

---

## 3. The 30 Ideas — Ranked by Impact

### Tier 1: Game Changers (must-have for "best LSP")

#### 1. Error-Recovery Parser
**Inspiration:** rust-analyzer's rowan CST

Currently, a parse error means no AST, which means no completions/hover/navigation on broken code. Users writing a new workflow get nothing until the YAML is syntactically valid.

**The fix:** Make the parser produce a partial AST with error nodes. Every LSP feature should work on broken code. This is the single highest-impact improvement.

```yaml
tasks:
  research:
    infer:
      model: claude-sonnet-4-5-20250514
      prompt: |
        # Cursor is here, user is typing
        # Currently: NO completions because YAML is incomplete
        # With error recovery: full completions available
```

**Effort:** HIGH (requires parser rework) | **Impact:** CRITICAL

---

#### 2. Inlay Hints
**Inspiration:** rust-analyzer type hints, parameter name hints

Show invisible-but-important information inline:

```yaml
tasks:
  research:                          # ← [depends: none] [verb: infer]
    infer:
      model: claude-sonnet-4-5-20250514            # ← [cost: ~$3/1K tokens]
      timeout: 30                    # ← [= 30 seconds]
    with:
      topic: $user_input             # ← [from: context.inputs.topic]

  write_article:                     # ← [depends: research] [verb: infer]
    depends_on: [research]
    infer:
      prompt: "Write about {{with.data}}"  # ← [resolved: research.output]
    with:
      data: $research                # ← [type: string, ~2KB]
```

**Types of inlay hints for Nika:**
- **Dependency chain**: `[depends: none]`, `[depends: research, fetch_data]`
- **Verb badge**: `[verb: infer]`, `[verb: exec]`, `[verb: fetch]`
- **Timeout duration**: `[= 30 seconds]`, `[= 5 minutes]`
- **Binding resolution**: `[from: context.inputs.topic]`, `[from: task.output]`
- **Template preview**: Show what `{{with.alias}}` resolves to
- **Provider/cost hint**: `[anthropic, ~$3/1K]`, `[openai, ~$1.5/1K]`
- **Task count**: `[12 tasks]`, `[3 parallel, 9 sequential]` at workflow level

**Effort:** MEDIUM | **Impact:** HIGH

---

#### 3. Code Lens — "Run Workflow" / "Run Task" / "Validate"
**Inspiration:** rust-analyzer "Run test", gopls "Run benchmark"

```yaml
schema: nika/workflow@0.12          # ▶ Run Workflow | ✓ Validate | 📊 DAG
workflow:
  name: content-pipeline

tasks:
  research:                         # ▶ Run from here | 🔗 2 dependents
    infer:
      prompt: "Research {{with.topic}}"
```

**Code lens types:**
- `▶ Run Workflow` — executes `nika run` on the file
- `▶ Run from here` — executes with `--task research` flag
- `✓ Validate` — runs `nika check`
- `📊 Show DAG` — opens dependency graph visualization
- `🔗 N dependents` — how many tasks depend on this one
- `⏱ Last run: 2.3s` — execution time from last run (if trace exists)

**Effort:** MEDIUM | **Impact:** HIGH

---

#### 4. Live MCP Tool Discovery
**Inspiration:** Terraform provider schema auto-fetch

Currently MCP tools are hardcoded in `mcp_discovery.rs`. Instead:

1. Parse the workflow's `mcp:` section
2. Connect to declared MCP servers (or read cached schemas)
3. Provide live completions for available tools and their parameters

```yaml
mcp:
  servers:
    novanet:
      command: novanet
      args: [mcp]

tasks:
  search:
    invoke:
      server: novanet
      tool: novanet_search    # ← completions from live server!
      params:
        query: "QR codes"     # ← param completions from tool schema!
```

**Effort:** HIGH (needs async MCP client in LSP) | **Impact:** HIGH

---

#### 5. Workflow-Aware Rename/Refactor
**Inspiration:** rust-analyzer rename, TypeScript organize imports

Rename a task ID → updates ALL references:
- `depends_on: [old_name]` → `depends_on: [new_name]`
- `with: { data: $old_name }` → `with: { data: $new_name }`
- Template refs: `{{with.old_name}}` handling
- Cross-file: if included via `include:`

**Effort:** MEDIUM | **Impact:** HIGH

---

### Tier 2: Delight Features (wow factor)

#### 6. DAG Visualization via Code Lens
**Inspiration:** Terraform module treeview

Click "📊 Show DAG" code lens → opens a side panel showing the dependency graph as a visual diagram. Tasks are nodes, `depends_on` are edges. Color-coded by verb type:
- 🟣 `infer:` (AI) | 🟢 `exec:` (shell) | 🔵 `fetch:` (HTTP) | 🟡 `invoke:` (MCP) | 🔴 `agent:` (loop)

Interactive: click a node → jump to task definition.

**Effort:** HIGH (needs VS Code webview) | **Impact:** HIGH

---

#### 7. Context-Aware Semantic Highlighting
**Inspiration:** rust-analyzer's closure capture/exit point highlighting

- **Data flow highlighting**: Click on a `with:` binding → highlight all tasks that consume it
- **Dependency chain highlighting**: Click on `depends_on` → highlight the full upstream chain
- **Template variable highlighting**: Click on `{{with.alias}}` → highlight where `alias` is defined
- **Critical path highlighting**: Highlight the longest execution path through the DAG

**Effort:** MEDIUM | **Impact:** MEDIUM-HIGH

---

#### 8. Smart Scaffolding / Template Snippets
**Inspiration:** Terraform required fields auto-fill, JetBrains live templates

When the user types a verb, auto-scaffold the entire task structure:

```yaml
# User types: "infer" and presses Tab
tasks:
  ${1:task_name}:
    infer:
      model: ${2|claude-sonnet-4-5-20250514,claude-opus-4-0-20250514,gpt-4o|}
      prompt: |
        ${3:Your prompt here}
    ${0}
```

**Verb-specific scaffolds:**
- `infer:` → model + prompt + optional system/temperature
- `exec:` → command + optional args/env/cwd
- `fetch:` → url + method + optional headers/body
- `invoke:` → server + tool + params
- `agent:` → model + goal + tools + optional max_turns

**Also:** Full workflow scaffold from scratch with `nika` snippet.

**Effort:** LOW | **Impact:** MEDIUM

---

#### 9. Diagnostic Messages That Teach (Elm/Rust Philosophy)
**Inspiration:** Elm compiler, Rust compiler

Current diagnostics are functional but terse. Transform them into learning experiences:

```
Before:
  error[NIKA-050]: Unknown task 'reserach' in depends_on

After:
  error[NIKA-050]: Unknown task reference

    ┌─ workflow.nika.yaml:12:18
    │
 12 │     depends_on: [reserach]
    │                  ^^^^^^^^ task 'reserach' not found
    │
    = did you mean 'research'?
    = available tasks: research, write_article, publish
    = help: depends_on references must match an existing task ID
            https://nika.dev/docs/depends-on
```

**Key principles:**
- Show the "what" (primary label) AND the "why" (secondary labels)
- Always suggest fixes ("did you mean?")
- Link to documentation
- One error at a time for beginners (progressive disclosure option)

**Effort:** MEDIUM | **Impact:** MEDIUM-HIGH

---

#### 10. Document Links (Clickable Paths/URLs)
**Inspiration:** Red Hat yaml-language-server

Make paths and URLs clickable:

```yaml
include:
  - ./lib/common-tasks.nika.yaml    # ← Ctrl+Click → opens file
  - pkg://supernovae/seo-tools      # ← Ctrl+Click → opens registry page

tasks:
  fetch_data:
    fetch:
      url: https://api.example.com/data  # ← Ctrl+Click → opens URL
```

**Effort:** LOW | **Impact:** MEDIUM

---

#### 11. Folding Ranges (Smart Collapse)
**Inspiration:** All modern LSPs

```yaml
tasks:                              # ← [collapse all tasks]
  research:                         # ← [collapse task]
    infer:                          # ← [collapse verb block]
      model: claude-sonnet-4-5-20250514
      prompt: |                     # ← [collapse multiline string]
        Long prompt text
        spanning many lines...
    with:                           # ← [collapse bindings]
      topic: $context_input
```

**Effort:** LOW | **Impact:** MEDIUM

---

#### 12. Find All References
**Inspiration:** rust-analyzer, TypeScript

`Find References` on a task ID shows everywhere it's used:
- `depends_on: [task_id]`
- `with: { alias: $task_id }`
- Template: `{{with.alias}}` (when alias points to task)
- `include:` references in other files

**Effort:** MEDIUM | **Impact:** MEDIUM

---

### Tier 3: Polish & Advanced (differentiation)

#### 13. Workspace-Wide Diagnostics
**Inspiration:** LSP 3.17 diagnostic pull model

Currently only the open file gets diagnostics. Workspace diagnostics scan ALL `.nika.yaml` files:
- Broken `include:` references (file doesn't exist)
- Package version conflicts
- MCP server config inconsistencies
- Unused tasks (defined but never referenced)

**Effort:** MEDIUM | **Impact:** MEDIUM

---

#### 14. Template Expression Language Intelligence
**Inspiration:** GitHub Actions `${{ }}` expression parser

Full intelligence for `{{...}}` template expressions:
- Completions: `{{with.` → list all defined aliases
- Validation: `{{with.nonexistent}}` → error
- Hover: `{{with.data}}` → shows source task and type
- Transform awareness: `{{with.data | upper}}` → validate transform exists

**Effort:** MEDIUM | **Impact:** MEDIUM

---

#### 15. Provider Intelligence
**Inspiration:** Terraform provider auto-detection

- Auto-detect available providers from env vars (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`)
- Show provider status in inlay hints: `[key: ✓]` or `[key: ✗ missing]`
- Model completions per provider: `anthropic` → `claude-sonnet-4-5-20250514`, `claude-opus-4-0-20250514`, etc.
- Cost estimates on hover: "$0.003/1K input, $0.015/1K output"

**Effort:** MEDIUM | **Impact:** MEDIUM

---

#### 16. YAML Formatting
**Inspiration:** Prisma LSP auto-format, prettier

Opinionated Nika YAML formatting:
- Consistent indentation (2 spaces)
- Canonical key ordering: `schema` → `workflow` → `context` → `mcp` → `tasks`
- Task key ordering: `depends_on` → verb → `with` → `output`
- Multiline string formatting
- Comment preservation

**Effort:** MEDIUM | **Impact:** MEDIUM

---

#### 17. Execution Trace Integration
**Inspiration:** gopls gc_details, Temporal workflow visualization

If a trace file exists from a previous `nika run`:
- Show execution time per task as inlay hint: `[2.3s]`
- Color tasks by status: green (success), red (failed), grey (skipped)
- Show output preview on hover
- Code lens: "⏱ Last: 2.3s | ✓ Success" or "✗ Failed: timeout"

**Effort:** HIGH | **Impact:** MEDIUM

---

#### 18. Multi-File Intelligence (Include Resolution)
**Inspiration:** TypeScript project references

Full intelligence across `include:` boundaries:
- Completions from included files' task IDs
- Go-to-definition jumps into included files
- Diagnostics for broken includes
- Workspace-wide rename across included files

**Effort:** HIGH | **Impact:** MEDIUM

---

#### 19. JSON Schema Output Validation Preview
For tasks with `output.schema:`, show a preview of what the schema expects:

```yaml
output:
  schema:                           # ← Hover: "expects { title: string, body: string }"
    type: object
    properties:
      title: { type: string }
      body: { type: string }
```

**Effort:** LOW | **Impact:** LOW-MEDIUM

---

#### 20. "Explain Error" Code Action
**Inspiration:** Rust `--explain E0384`

Code action on any NIKA-XXX error → opens detailed explanation:
- What the error means
- Common causes
- How to fix it
- Example of correct code

Could open in a hover panel, side panel, or external doc.

**Effort:** LOW | **Impact:** MEDIUM

---

### Tier 4: Moonshots (revolutionary but complex)

#### 21. AI-Assisted Prompt Writing
**Inspiration:** Cursor/Copilot inline completions

When writing `prompt:` blocks, provide AI-assisted completions:
- Suggest prompt improvements
- Auto-complete based on task name and bindings
- "Improve this prompt" code action
- Template variable insertion suggestions

**Effort:** VERY HIGH | **Impact:** HIGH (but controversial)

---

#### 22. Live Workflow Preview Panel
**Inspiration:** Markdown preview, Mermaid preview

VS Code side panel showing:
- DAG visualization (auto-updating)
- Data flow diagram
- Task card view with verb icons
- Estimated execution timeline

Auto-updates as user types.

**Effort:** VERY HIGH (VS Code webview extension) | **Impact:** HIGH

---

#### 23. Workflow Debugger Integration
**Inspiration:** DAP (Debug Adapter Protocol)

Set breakpoints on tasks. Step through workflow execution:
- Pause before/after each task
- Inspect bindings at each step
- Watch template resolution
- Modify and re-run from breakpoint

**Effort:** VERY HIGH | **Impact:** HIGH

---

#### 24. Semantic Diff for Workflows
When viewing git diffs, understand structural changes:
- "Task 'research' was renamed to 'deep_research'"
- "New dependency: write → research"
- "Provider changed from openai to anthropic"
- "Timeout increased from 30s to 60s"

**Effort:** HIGH | **Impact:** MEDIUM

---

#### 25. Package Registry Integration
**Inspiration:** npm/cargo LSP integrations

```yaml
include:
  - pkg://supernovae/seo-tools@^1.0  # ← completions from registry!
```

- Search packages inline
- Version completions
- Hover: package description, README preview
- Code action: "Update to latest version"

**Effort:** HIGH | **Impact:** MEDIUM (depends on registry maturity)

---

#### 26. Cost Estimation Code Lens
Estimate workflow execution cost based on:
- Model pricing (per provider)
- Estimated token counts (from prompt length)
- Number of tasks × iterations

```yaml
schema: nika/workflow@0.12          # 💰 Estimated cost: $0.12-$0.45
```

**Effort:** MEDIUM | **Impact:** LOW-MEDIUM

---

#### 27. Workflow Linter (Beyond Schema)
Style/best-practice rules beyond schema validation:
- "Task has no description — consider adding one"
- "Long prompt (>2000 chars) — consider extracting to file"
- "Sequential tasks with no data dependency — consider parallelizing"
- "No timeout set — will use default (300s)"
- "Using deprecated schema version"

**Effort:** MEDIUM | **Impact:** MEDIUM

---

#### 28. MCP Tool Documentation on Hover
When hovering over `invoke:` tool names, show:
- Tool description
- Input schema (parameters)
- Output schema
- Example usage
- Link to MCP server docs

**Effort:** MEDIUM (needs MCP schema cache) | **Impact:** MEDIUM

---

#### 29. Smart Task Reordering
Code action: "Optimize task order" — reorder tasks based on dependencies for maximum parallelism. Show before/after DAG comparison.

**Effort:** MEDIUM | **Impact:** LOW

---

#### 30. Workflow Testing Integration
Code lens on test workflows: "Run Gate Tests" → execute the full gate test suite from the editor, showing results inline.

**Effort:** HIGH | **Impact:** LOW-MEDIUM

---

## 4. UX Sceptical Questions

### The Hard Questions We Must Answer

**Q1: Two LSP implementations — why?**
`src/lsp/` (tower-lsp, feature-gated in main binary) AND `tools/nika-lsp/` (standalone crate) do overlapping work. Users will be confused about which to use. **Decision needed:** Consolidate into one, or clearly separate (e.g., nika-lsp = lightweight, nika lsp = full-featured with runtime integration).

**Q2: How do we handle the "cold start" problem?**
User opens a `.nika.yaml` file for the first time. No MCP servers running, no context, no trace history. What's the experience? Must be good even with zero runtime context.

**Q3: Inlay hints — too noisy?**
rust-analyzer users frequently toggle hints off because they're distracting. We need sensible defaults: ON for dependency chain and timeout, OFF for cost estimates and binding resolution. All configurable.

**Q4: Code Lens "Run" — security implications?**
Clicking "Run Workflow" could execute arbitrary shell commands (`exec:` verb). Need user confirmation or a sandbox mode. Compare: rust-analyzer's "Run test" is safe because tests are sandboxed.

**Q5: Live MCP discovery — performance?**
Connecting to MCP servers from the LSP adds latency and complexity. What if the server is down? Need: async background connection, cached schemas, graceful degradation.

**Q6: Error recovery parser — is it worth the rewrite?**
The current parser uses `serde_yaml`. Switching to a hand-written error-recovery parser is a massive effort. Alternative: use `yaml-rust2` for a CST-like approach, or wrap serde_yaml with fallback heuristics.

**Q7: Formatting — opinionated or configurable?**
Prisma chose opinionated (one true format). Red Hat YAML chose configurable. For a DSL like Nika, opinionated is probably better — reduces bikeshedding. But must handle edge cases (comments, multiline strings).

**Q8: How do we measure LSP UX quality?**
- Time from keystroke to completion popup
- Percentage of "useful" completions (signal vs noise)
- Error recovery rate (% of broken files that still get completions)
- User telemetry (opt-in) on which features are used

**Q9: What about Neovim/Helix users?**
Code Lens and DAG visualization are VS Code-specific (webview). We need graceful degradation: Neovim gets all LSP standard features but not custom webviews. Document this clearly.

**Q10: Should the LSP be the "single pane of glass" for Nika?**
Or should the TUI remain the primary runtime interface? The LSP could become so feature-rich that it replaces the need for `nika ui`. Is that desirable?

---

## 5. Architecture Constraints

### Must-Address Before Adding Features

1. **UTF-16 conversion bug** — Fix `conversion.rs` before any new position-dependent feature. Emoji in task names or prompts will break offset calculation.

2. **Consolidate LSP implementations** — Pick one as the canonical LSP. Suggested: keep `src/lsp/` as the canonical (integrated with nika binary, single install), deprecate standalone `nika-lsp` or make it a thin wrapper.

3. **Error recovery in parser** — This unblocks 80% of the UX improvements. Without it, completions/hover/navigation on incomplete files remain broken.

4. **Incremental re-parsing** — For large workflows (100+ tasks), full re-parse on every keystroke will cause lag. Consider: debounce + incremental (re-parse only changed YAML block).

5. **Async MCP client** — The LSP runs in a tokio runtime already. Adding async MCP connections is natural but needs careful lifecycle management (server starts/stops/crashes).

---

## 6. Implementation Roadmap

### Phase 1: Foundation (Weeks 1-3)
Priority: fix bugs, add low-hanging fruit

- [ ] Fix UTF-16 conversion bug in `conversion.rs`
- [ ] Add folding ranges (LOW effort, MEDIUM impact)
- [ ] Add document links (LOW effort, MEDIUM impact)
- [ ] Add smart snippets/scaffolding per verb (LOW effort, MEDIUM impact)
- [ ] Improve error messages (Elm/Rust philosophy)
- [ ] Add "Explain Error" code action

### Phase 2: Core UX (Weeks 4-8)
Priority: the features that make users say "wow"

- [ ] Implement inlay hints (dependency, verb, timeout, binding resolution)
- [ ] Implement Code Lens (Run Workflow, Run Task, Validate, dependents count)
- [ ] Implement Rename/Refactor (task IDs across all references)
- [ ] Implement Find All References
- [ ] Implement template expression completions (`{{with.` awareness)
- [ ] Add provider intelligence (model completions, key status)

### Phase 3: Intelligence (Weeks 9-14)
Priority: deep semantic features

- [ ] Error recovery parser (partial AST with error nodes)
- [ ] Live MCP tool discovery (async, cached)
- [ ] Multi-file intelligence (include resolution)
- [ ] Workspace-wide diagnostics
- [ ] Context-aware semantic highlighting (data flow, dep chain)
- [ ] YAML formatting (opinionated)

### Phase 4: Magic (Weeks 15+)
Priority: moonshot differentiators

- [ ] DAG visualization panel (VS Code webview)
- [ ] Execution trace integration (inlay hints from past runs)
- [ ] Workflow linter (best practices beyond schema)
- [ ] Package registry integration
- [ ] Cost estimation code lens
- [ ] Workflow debugger (DAP integration)

---

## Summary: The 5 Most Impactful Changes

If we could only do 5 things, these would transform the Nika editing experience:

1. **Error recovery parser** — Everything works on broken code. This is table stakes for a great LSP.
2. **Inlay hints** — Make invisible workflow structure visible. Dependency chains, binding resolution, timeout values.
3. **Code Lens (Run/Validate)** — One-click workflow execution from the editor. Immediate feedback loop.
4. **Live MCP discovery** — Real completions from real servers. No more guessing tool names and parameters.
5. **Elm-style error messages** — Errors that teach, not just report. "Did you mean?" everywhere.

These 5 features would put the Nika LSP ahead of every YAML workflow LSP in existence, including GitHub Actions.
