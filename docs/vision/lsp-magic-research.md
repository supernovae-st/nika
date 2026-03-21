# Research Report: What Makes an LSP Feel Magical

**Date**: 2026-03-21
**Scope**: Cross-ecosystem analysis of the best developer experiences achievable through LSP and editor tooling
**Goal**: Identify features that would make Nika's `.nika.yaml` editing feel like "just hit Tab and everything writes itself"

---

## Summary

The best developer experiences share three traits: **anticipation** (predicting what you want before you ask), **zero-friction execution** (one keystroke does the right thing), and **ambient intelligence** (contextual information appears exactly when needed without cluttering). The gold standard implementations are Cursor Tab (AI-powered multi-line predictions), rust-analyzer (semantic code actions + postfix completions), and Emmet (abbreviation expansion). A Nika LSP that combines schema-aware YAML expansion, inline cost/token hints, DAG preview CodeLens, and AI-powered workflow generation would be unprecedented for any YAML DSL.

---

## 1. Cursor Tab / Ghost Text Predictions

### How It Works

Cursor Tab uses a custom **Fusion model** -- a Sparse Mixture of Experts (MoE) architecture trained on **edit sequences** (git diffs), not fill-in-the-middle (FIM). This is the key architectural difference from Copilot.

**Pipeline:**
1. Every keystroke/cursor move triggers a cloud request with full context (current file, open files, recent edits, linter errors)
2. The Fusion model processes this "pre-filled token hungry" input and predicts the **next action**, not just the next line
3. Ghost text appears inline -- can span multiple lines, jump to other locations, or propose diffs
4. Press Tab to accept; cursor jumps to the next logical edit location (sometimes 18 lines away, sometimes another file)

**Speculative decoding**: The model feeds existing code back as a strong prior. It "agrees" with most of it until it reaches a change point, then generates only the delta. This makes multi-line predictions near-instant.

**Key insight**: Cursor Tab is trained via **online RL (policy gradients)** on accept/reject signals from real developer sessions. It learns to suggest only high-confidence, "zero-entropy" edits -- the changes you were obviously going to make anyway.

### What Makes It Feel Magical

| Aspect | Regular Completion | Cursor Tab |
|--------|-------------------|------------|
| Scope | Next few tokens/lines | Multi-line edits, file jumps, cross-file propagation |
| Context | Local code only | Full codebase + recent edits + linter errors + intent signals |
| UX | Static dropdown menus | Streaming ghost text with inline diffs |
| Speed | Waits for full generation | Speculative decoding + KV caching = sub-200ms |
| Training | FIM on code corpus | Edit sequences from real dev sessions (RL-optimized) |

### Nika Application

A "Cursor Tab for workflows" could predict:
- After writing `infer:`, suggest the full prompt/model/params block based on task name and context
- After adding a task, suggest `depends_on:` connections based on data flow analysis
- After writing one `content:` part, predict remaining parts from the workflow pattern
- Ghost-text preview of `with:` bindings resolved with actual values

**Sources**: [neon.com/blog/tab-coding-cursor](https://neon.com/blog/tab-coding-cursor), [cursor.com/blog/tab-rl](https://cursor.com/blog/tab-rl), [coplay.dev/blog/a-brief-history-of-cursors-tab-completion](https://coplay.dev/blog/a-brief-history-of-cursors-tab-completion)

---

## 2. rust-analyzer: The Gold Standard LSP

### Why It's the Gold Standard

rust-analyzer demonstrates what happens when an LSP deeply understands a language's semantics. Every feature feels like the editor "knows Rust."

### Auto-Import on Completion

Type `HashMap` anywhere and the completion not only inserts the type but also adds `use std::collections::HashMap;` at the top of the file. No separate import step. Configurable via `rust-analyzer.completion.autoimport.enable`.

**Nika equivalent**: Type `gpt-4o` in a model field and auto-insert the `provider: openai` field if missing. Type an MCP tool name and auto-add the `mcp:` server config block.

### Inlay Hints

Inline ghost annotations that show:
- **Type annotations**: `let x = foo()` shows `: String` after `x`
- **Parameter names**: `foo(42, true)` shows `count:` and `verbose:` before args
- **Chaining hints**: `iter().filter().map()` shows intermediate types at each step
- **Lifetime elision**: Shows implicit lifetime parameters
- **Closure return types**: Shows inferred return types for closures

All toggleable per-category. Non-editable, non-intrusive.

**Nika equivalent**:
- Show `~$0.03` cost estimate after `model: gpt-4o`
- Show `~2,400 tokens` after long prompt blocks
- Show resolved `{{with.alias}}` values inline
- Show `-> text/plain` output type after task blocks
- Show `~3.2s` estimated execution time

### Postfix Completions

Transform expressions by typing a dot-suffix:
- `value.if` -> `if value { }`
- `value.match` -> `match value { }`
- `value.dbg` -> `dbg!(value)`
- `value.ok` -> `Ok(value)`
- `value.some` -> `Some(value)`
- `value.let` -> `let var = value;`
- `value.box` -> `Box::new(value)`
- `value.ref` -> `&value`

**Nika equivalent**: Postfix-style expansions for YAML:
- `openai.infer` -> expands to full `infer:` block with `provider: openai`
- `https://api.example.com.fetch` -> expands to full `fetch:` block
- `tool_name.invoke` -> expands to full `invoke:` block
- `gpt-4o.agent` -> expands to full `agent:` block with model config

### Smart Code Actions (Fill Match Arms, Add Missing Members)

- **Fill match arms**: Write `match expr { }` and a code action fills all enum variants
- **Add missing trait members**: Implement a trait and get stubs for all required methods
- **Extract function**: Select code, extract into a function with correct parameters and return type
- **Inline variable**: Replace variable with its expression everywhere

**Nika equivalent**:
- "Add missing required fields" -- adds all required schema fields for a verb block
- "Fill content parts" -- scaffold text/image/audio parts for a multimodal task
- "Extract task" -- select inline config and extract into a named task with depends_on wiring
- "Add error handler" -- wrap task with on_error/retry config

**Sources**: [rust-analyzer.github.io/manual.html](https://rust-analyzer.github.io/manual.html), [rust-analyzer.github.io/book/configuration.html](https://rust-analyzer.github.io/book/configuration.html)

---

## 3. Emmet: Abbreviation Expansion

### How It Works

Emmet parses a CSS-like selector syntax through a pipeline:
1. **Lexer** breaks `div>ul>li*3` into tokens: elements, hierarchy operators, multipliers
2. **Parser** builds an AST: `div` contains `ul` which contains 3x `li`
3. **Snippet matcher** resolves elements against a snippet registry
4. **Variable resolver** fills TextMate-style placeholders (`${1}`, `${2}`)
5. **Formatter** outputs indented markup

**Power features:**
- `ul>li.item$*5` -> 5 list items with classes `item1` through `item5`
- `div.container>header+main+footer` -> full page structure
- Wrap with abbreviation: select text, wrap in any structure
- `lorem10` -> 10 words of filler text

Reports of **65% keystroke reduction** and **41% fewer syntax errors**.

### Nika Equivalent: Workflow Abbreviations

A "Nikamet" abbreviation system:

```
infer:gpt4o>summarize        ->  Full infer task with gpt-4o model, summarize prompt scaffold
fetch:get>json               ->  Full fetch task with GET method, JSON response handling
pipe:3                       ->  Pipeline of 3 sequential tasks with depends_on wiring
fan:5                        ->  5 parallel tasks (no depends_on between them)
agent:claude>researcher      ->  Full agent task with Claude model, researcher persona
```

**Implementation**: Parse abbreviation string -> build YAML AST -> expand with schema-aware defaults -> output formatted YAML with tabstops.

**Sources**: [github.com/emmetio/expand-abbreviation](https://github.com/emmetio/expand-abbreviation), [docs.emmet.io](https://docs.emmet.io/actions/expand-abbreviation/)

---

## 4. Snippet Engines

### Power Hierarchy

| Engine | Tabstops | Choices | Regex Transforms | Mirrors | Conditionals | Best For |
|--------|----------|---------|------------------|---------|--------------|----------|
| TextMate | Basic | No | No | Basic | No | Simple expansions |
| LSP Snippets | Nested | Yes `${1\|a,b,c\|}` | Yes | Yes | Via transforms | Cross-editor standard |
| UltiSnips | Advanced | Yes | Python-powered | Sync fields | Python if/else | Vim power users |
| LuaSnip | Full nested | choiceNodes | Lua + regex | Dynamic mirrors | Lua conditionals | Neovim, most flexible |
| VS Code | LSP syntax | Yes | Yes | Yes | Basic | Largest user base |

### Most Powerful LSP Snippet Features

**Nested tabstops with choices:**
```
${1|infer,exec,fetch,invoke,agent|}:
  ${2:task_name}:
    ${3:config}
```

**Regex transforms (mirror + transform):**
```
# Type task name, auto-generate snake_case ID
${1:My Task Name} -> ${1/(.*)/${1:/downcase}/}_task
```

**Variables:**
- `$TM_FILENAME` -- current file name
- `$CLIPBOARD` -- clipboard contents
- `$CURRENT_YEAR` -- date variables

### Nika Application

Rich snippet templates for every verb:

```yaml
# Trigger: "infer" + Tab
infer:
  ${1:task_name}:
    model: ${2|gpt-4o,claude-sonnet-4-20250514,gemini-2.0-flash|}
    prompt: |
      ${3:Your prompt here}
    ${4:max_tokens: ${5:1000}}
```

```yaml
# Trigger: "agent" + Tab
agent:
  ${1:agent_name}:
    model: ${2|claude-sonnet-4-20250514,gpt-4o|}
    persona: |
      ${3:You are a helpful assistant.}
    tools:
      - ${4:tool_name}
    max_turns: ${5:10}
```

---

## 5. CodeLens

### Best Implementations

| LSP | CodeLens | Action |
|-----|----------|--------|
| gopls (Go) | `run test` / `debug test` above test functions | Executes `go test` with filter |
| rust-analyzer | `Run` / `Debug` above `fn main` and `#[test]` | Cargo run/test |
| Java JDT LS | `Run` / `Debug` above `main()` | JVM launch |
| TypeScript | `5 references` above symbols | Navigate to references |
| GitLens | `Last changed 2 days ago by alice` | Git blame inline |
| Deno LSP | Test status with module specifier | Test runner integration |

### Technical Mechanism

1. Client sends `textDocument/codeLens` request
2. Server returns `CodeLens[]` with `range`, `command`, and `title`
3. Client renders clickable text above the target line
4. Click executes the `command` (e.g., `editor.action.runTest`)
5. Optional `codeLens/resolve` for lazy computation

### Nika CodeLens Ideas

```yaml
# [Run Task] [Preview Output] [Estimate: ~$0.03, ~2.4s]
infer:
  summarize:
    model: gpt-4o
    prompt: |
      Summarize this document.

# [Run Pipeline] [Show DAG] [Total: ~$0.12, ~8.7s]
workflow:
  name: research-pipeline
```

Specific CodeLens for Nika:
- **`Run Task`** -- execute single task, show output in panel
- **`Run from Here`** -- execute DAG from this task forward
- **`Show DAG`** -- render task dependency graph (ASCII in terminal, Mermaid in webview)
- **`Estimate Cost`** -- show token count * model pricing
- **`Preview Prompt`** -- render prompt with template variables resolved
- **`Show Schema`** -- display JSON schema for this verb block
- **`N references`** -- count of `depends_on` references to this task

---

## 6. Inlay Hints

### Best Implementations

**rust-analyzer** (most configurable):
- Type annotations: `let x` shows `: Vec<String>` after the binding
- Parameter names: `foo(42)` shows `count:` before 42
- Chaining: `iter().filter().map()` shows return type at each step
- Lifetime elision: shows implicit `'a` parameters
- Closure returns: shows `-> bool` after closure

**TypeScript**: Parameter names, return types, variable types. Granular control via `includeInlayParameterNameHints`, `includeInlayVariableTypeHints`.

**Technical mechanism**: `textDocument/inlayHint` request returns `InlayHint[]` with:
- `position` -- where to insert the ghost text
- `label` -- the displayed text
- `kind` -- Type, Parameter, or custom
- `tooltip` -- hover details (via `inlayHint/resolve`)
- `paddingLeft/Right` -- spacing

### Nika Inlay Hint Ideas

```yaml
infer:
  summarize:                          # -> text/plain
    model: gpt-4o                     # ~$2.50/1M in, ~$10/1M out
    prompt: |                         # ~847 tokens
      Summarize {{with.document}}     # = "The quick brown fox..."
    max_tokens: 500                   # ~$0.005 est.

fetch:
  get_data:                           # -> application/json
    url: "{{with.api_url}}"           # = "https://api.example.com/data"
    method: GET                       # 200 OK (cached)

agent:
  researcher:                         # ~$0.15-0.45 est. (5-10 turns)
    model: claude-sonnet-4-20250514
    max_turns: 10                     # avg 6.2 turns (historical)
```

Categories:
- **Cost estimates**: `~$0.03` next to model lines based on prompt length * pricing
- **Token counts**: `~847 tokens` next to prompt blocks
- **Resolved templates**: Show actual values of `{{with.x}}` expressions
- **Output types**: `-> text/plain`, `-> application/json` after task names
- **Historical stats**: Average execution time from past runs
- **Dependency info**: `depends on: [task_a, task_b]` / `depended by: [task_c]`

---

## 7. Live Preview

### Existing Implementations

- **CSS LSP**: Live style preview as you type
- **Markdown**: Side-by-side rendered preview (VS Code built-in)
- **Mermaid**: Diagram preview extensions
- **CWL (Common Workflow Language)**: Workflow graph preview via Rabix/Benten LSP
- **Shaders**: GLSL/HLSL preview in webview panels

### Technical Integration

LSP itself does not have a native webview protocol. Previews are implemented via:
1. **Editor webview panels** (VS Code `createWebviewPanel`)
2. **Virtual documents** (read-only rendered content)
3. **Custom LSP notifications** triggering client-side rendering
4. **Tree View Protocol** for structured previews

### Nika Live Preview Ideas

**DAG Visualization** (highest impact):
- Side panel showing task dependency graph
- Updates live as you edit the workflow
- Highlights current task, shows execution order
- Color-codes by verb type (infer=blue, exec=green, fetch=orange, invoke=purple, agent=red)

**Prompt Preview**:
- Side panel showing rendered prompt with template variables resolved
- Highlights `{{with.x}}` in context
- Shows estimated token count with a visual bar

**Cost Dashboard**:
- Running total of estimated workflow cost
- Per-task breakdown
- Model pricing table
- "What-if" sliders for max_tokens

**TUI Integration** (unique to Nika):
- Since Nika already has a TUI, the LSP could send data to the TUI for rich preview
- DAG view in TUI's Studio tab (view 1/s)
- Cost estimates in TUI's Control tab (view 3/x)

---

## 8. AI-Assisted LSP

### Architecture Patterns

| Tool | Architecture | Integration | Latency | Context Window |
|------|-------------|-------------|---------|----------------|
| Codeium | VS Code extension + cloud API | Completion provider alongside LSP | ~883ms | Moderate |
| Supermaven | Extension + custom "Babble" model | Layers on LSP completions | <100ms | 1M tokens |
| Continue.dev | Open-source extension + any model | Completion provider + chat | Varies | Varies |
| Cursor Tab | Fork of VS Code + Fusion MoE model | Deep editor integration | <200ms | Full codebase |

**Key insight**: None of these run as LSP servers. They are **editor extensions** that register as completion providers alongside the LSP. The LSP handles schema/syntax; the AI extension handles intent/prediction.

### Supermaven's Speed Secret

Supermaven achieves sub-100ms latency through:
1. **Custom neural architecture ("Babble")** -- more efficient than standard Transformers for long context
2. **Pre-indexing** -- indexes entire repo on startup (10-20s), then serves from cache
3. **Optimized GPU serving** -- battle-tested infrastructure
4. Benchmarks: Supermaven 250ms vs Codeium 883ms vs Copilot 783ms

### Nika AI-Assisted LSP

A Nika LSP could provide AI-powered suggestions via:

1. **Schema-aware AI completions**: After typing `infer:`, AI suggests a complete task block based on:
   - The workflow name and description
   - Other tasks in the workflow
   - Common patterns from a corpus of `.nika.yaml` files

2. **Prompt generation**: Type a task name like `summarize_article` and AI generates:
   ```yaml
   infer:
     summarize_article:
       model: gpt-4o-mini
       prompt: |
         Summarize the following article concisely.
         Focus on key points and conclusions.

         Article: {{with.article}}
       max_tokens: 500
   ```

3. **Workflow synthesis**: Describe intent in a comment, AI generates the full workflow:
   ```yaml
   # Research a topic, summarize findings, generate report
   # -> AI expands to 3-task pipeline with depends_on wiring
   ```

4. **Error fix suggestions**: When a diagnostic is raised, AI suggests the fix with context awareness

---

## 9. The Most Magical Developer Experiences

### Top 10 DX Innovations of All Time

1. **IntelliSense / Autocomplete** (1990s, Visual Studio) -- context-aware suggestions changed everything
2. **Git integration in editors** (2010s) -- version control without context switches
3. **AI code completion** (2022+, Copilot/Cursor) -- predictive coding feels psychic
4. **Semantic refactoring** (JetBrains, 2000s) -- safe, project-wide renames/restructures
5. **Extension marketplaces** (VS Code, 2015) -- infinite customization
6. **Multi-cursor editing** (Sublime Text, 2012) -- edit N lines simultaneously
7. **Integrated debugging** (IntelliJ/Xcode) -- breakpoints without leaving the editor
8. **Live templates / snippets** (JetBrains/VS Code) -- instant boilerplate
9. **AI agents / Composer** (Cursor, 2024+) -- multi-file changes from natural language
10. **Codebase-aware chat** (Cursor/Claude Code, 2025+) -- instant project understanding

### What Makes Developers Say "Wow"

The common thread is **eliminated friction between intent and result**:
- "I was going to type that" -> Tab (Cursor)
- "I need all match arms" -> Code action fills them (rust-analyzer)
- "I want a div with 5 list items" -> `div>ul>li*5` Tab (Emmet)
- "Show me who changed this" -> Inline annotation (GitLens)
- "Run just this test" -> Click the CodeLens (gopls/rust-analyzer)

The magic formula: **Anticipation + One Keystroke + Correct Result**

### Next-Gen Editor Innovations (2025-2026)

- **Zed**: Edit predictions via open-source Zeta model, agent panel with multiple AI threads, 120fps rendering, real-time multiplayer with voice
- **Windsurf**: Cascade mode for autonomous multi-step workflows, in-editor web app previews, Netlify deploys
- **Cursor**: Fusion model for edit-sequence prediction, speculative decoding for instant suggestions

**Sources**: [zed.dev](https://zed.dev), [octavehq.com/post/windsurf-vs-cursor-vs-zed](https://www.octavehq.com/post/windsurf-vs-cursor-vs-zed-which-ai-ide-in-2026)

---

## 10. YAML-Specific Magic

### Current Pain Points

1. **Indentation hell** -- one wrong space breaks everything
2. **No structural awareness** -- editors treat YAML as plain text
3. **Template blindness** -- `{{variables}}` are opaque strings
4. **Schema ignorance** -- no idea what fields are valid without docs
5. **Copy-paste friction** -- JSON doesn't paste as YAML
6. **No live feedback** -- have to run to find errors

### What Would Make YAML Editing Magical

**Auto-indentation that actually works:**
- Indent continuation lines correctly based on YAML context (mapping vs sequence vs scalar)
- Smart Enter: after `key:` at end of line, next line indented +2 with cursor positioned
- Re-indent on paste (adjust entire pasted block to match context indentation level)

**Smart paste:**
- Paste JSON, auto-convert to YAML with correct indentation
- Paste a URL, auto-expand to `fetch:` block
- Paste a prompt, auto-wrap in `|` block scalar with correct indentation

**Inline schema validation with fix suggestions:**
- Red squiggle on unknown fields with "Did you mean `model`?" quick fix
- Warning on deprecated fields with auto-migration code action
- "Add missing required field `prompt`" quick fix that inserts with tabstop

**Template expansion with live preview:**
- Hover over `{{with.x}}` to see resolved value
- Inlay hint showing the actual value inline
- Autocomplete for available template variables
- Error when referencing undefined variables

**Nika-specific magic features:**

| Feature | Trigger | Result |
|---------|---------|--------|
| Verb expansion | Type `infer:` + Tab | Full task scaffold with model/prompt/params |
| Model picker | Type `model:` + space | Dropdown of all known models with pricing info |
| Provider auto-config | Select a model | Auto-adds `provider:` if missing |
| depends_on completion | Type `depends_on:` | Dropdown of all task IDs in the workflow |
| Template autocomplete | Type `{{` | All available `with:` bindings and task outputs |
| Cost estimate | Hover on `model:` line | Tooltip with pricing per 1K tokens |
| DAG validation | Save file | Cycle detection, unreachable task warnings |
| Workflow scaffold | `nika:workflow` + Tab | Complete workflow skeleton with metadata |

### The "yaml-language-server" Baseline

Red Hat's yaml-language-server (the leading YAML LSP) provides:
- Schema-aware validation and completion
- Hover documentation from schemas
- Auto-completion for enum values
- Basic auto-indentation

**What it lacks** (and Nika's LSP should provide):
- No template variable resolution
- No cross-field validation (e.g., model requires matching provider)
- No live preview of any kind
- No cost/performance estimates
- No DAG-aware completions
- No smart paste
- No abbreviation expansion
- No postfix completions
- No CodeLens actions

---

## Synthesis: The Nika LSP Magic Roadmap

### Tier 1: "Just Works" (foundation -- many already exist in nika-lsp-core)

Already implemented or partially implemented in `tools/nika-lsp-core/src/handlers/`:
- [x] Schema-aware completions (`completion.rs`)
- [x] Hover documentation (`hover.rs`)
- [x] Go to definition (`definition.rs`)
- [x] Code actions (`code_action.rs`)
- [x] Semantic tokens (`semantic_tokens.rs`)
- [x] Document symbols (`symbols.rs`)
- [ ] Diagnostics with quick fixes
- [ ] Auto-indentation on Enter/paste
- [ ] Format on save

### Tier 2: "Smart" (differentiators)

- [ ] **Postfix completions**: `openai.infer` -> full infer block
- [ ] **Template variable resolution**: Autocomplete `{{with.` with available bindings
- [ ] **Cross-field validation**: model/provider consistency checks
- [ ] **depends_on completions**: Suggest task IDs, detect cycles
- [ ] **Rich snippets**: Verb-specific scaffolds with choices and tabstops
- [ ] **Auto-import**: Select model -> auto-add provider config
- [ ] **Smart paste**: JSON -> YAML, URL -> fetch block, prompt -> block scalar

### Tier 3: "Magical" (delight)

- [ ] **Inlay hints**: Cost estimates, token counts, resolved templates, output types
- [ ] **CodeLens**: Run Task, Show DAG, Estimate Cost, Preview Prompt
- [ ] **DAG visualization**: Live-updating dependency graph (TUI or webview)
- [ ] **Abbreviation expansion**: `infer:gpt4o>summarize` -> full task block
- [ ] **Fill missing fields**: One code action adds all required fields
- [ ] **AI-powered completions**: Generate full task blocks from intent

### Tier 4: "Superpower" (moat)

- [ ] **AI workflow synthesis**: Comment-to-workflow generation
- [ ] **Live cost dashboard**: Running total with per-task breakdown
- [ ] **Historical execution stats**: Average time/cost from past runs as inlay hints
- [ ] **Prompt preview panel**: Rendered prompt with resolved variables
- [ ] **Edit predictions**: Cursor-Tab-style next-action prediction for YAML workflows
- [ ] **Cross-file awareness**: Understand shared `with:` bindings across workflow includes

---

## Confidence Level

**High** -- All features described are implemented in production LSPs across major ecosystems. The technical mechanisms (CodeLens, inlay hints, semantic tokens, completion providers) are well-documented in the LSP specification. The Nika-specific applications are feasible given the existing `nika-lsp-core` architecture with its cursor context analysis, catalog system, and pure-function handler design.

## Methodology

- Tools used: Perplexity AI search (10 queries)
- Sources analyzed: 60+ URLs across documentation, blog posts, forums, and official specifications
- Ecosystems covered: Rust, TypeScript, Go, Java, Kotlin, Python, HTML/CSS, YAML
- Editors covered: VS Code, Cursor, Zed, Windsurf, Neovim, JetBrains, Sublime Text

## Sources

1. [neon.com - Tab Coding with Cursor](https://neon.com/blog/tab-coding-cursor) -- Cursor Tab deep dive
2. [cursor.com - Tab RL](https://cursor.com/blog/tab-rl) -- Reinforcement learning for tab completions
3. [coplay.dev - History of Cursor Tab](https://coplay.dev/blog/a-brief-history-of-cursors-tab-completion) -- Fusion model architecture
4. [rust-analyzer Manual](https://rust-analyzer.github.io/manual.html) -- Feature reference
5. [rust-analyzer Configuration](https://rust-analyzer.github.io/book/configuration.html) -- All settings
6. [Emmet Expand Abbreviation](https://github.com/emmetio/expand-abbreviation) -- Parser/expander source
7. [Emmet Documentation](https://docs.emmet.io/actions/expand-abbreviation/) -- Official docs
8. [LSP Specification](https://microsoft.github.io/language-server-protocol/) -- Protocol reference
9. [VS Code LSP Guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide) -- Implementation guide
10. [Supermaven Introduction](https://supermaven.com/blog/introducing-supermaven) -- Babble model, latency
11. [Zed Editor](https://zed.dev) -- Next-gen editor features
12. [langserver.org](https://langserver.org) -- LSP implementations catalog
13. [pygls Inlay Hints](https://pygls.readthedocs.io/en/latest/servers/examples/inlay-hints.html) -- Python LSP inlay hint implementation
14. [VS Code Semantic Highlight Guide](https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide) -- Semantic tokens reference
15. [Red Hat yaml-language-server](https://github.com/redhat-developer/yaml-language-server) -- Leading YAML LSP

## Further Research Suggestions

- **Benchmark Cursor Tab's Fusion model** against open-source alternatives (Zed's Zeta, Codestral) for YAML-specific completion quality
- **Prototype Emmet-style abbreviation parser** for `.nika.yaml` syntax (`infer:gpt4o>summarize` expansion)
- **Measure real-world token counts** for common Nika workflow patterns to calibrate cost estimate accuracy
- **Survey Nika users** on which Tier 2/3 features would have highest impact on their workflow
- **Evaluate tree-sitter YAML grammar** quality for semantic token extraction vs custom YAML parser
- **Research MCP-aware completions** -- how to suggest MCP tool names and parameters from connected servers
