# LSP Gold Standard Research for Nika

> Research date: 2026-03-18
> Purpose: Inform the design of nika-lsp with patterns from the best LSPs in the industry

---

## Table of Contents

1. [rust-analyzer: The Gold Standard](#1-rust-analyzer-the-gold-standard)
2. [DSL-Specific LSP Innovations](#2-dsl-specific-lsp-innovations)
3. [AI-Enhanced Developer Experiences](#3-ai-enhanced-developer-experiences)
4. [Error UX Best Practices](#4-error-ux-best-practices)
5. [YAML Workflow Engine LSPs](#5-yaml-workflow-engine-lsps)
6. [Patterns Applicable to Nika LSP](#6-patterns-applicable-to-nika-lsp)

---

## 1. rust-analyzer: The Gold Standard

### 1.1 Architecture: Why It's Fast

rust-analyzer's performance comes from three architectural pillars:

**Rowan: Concrete Syntax Trees (CSTs)**
- Inspired by Roslyn (C#) and Swift's libsyntax
- **Green tree**: Immutable, offset-free, purely functional. Uses arena allocation for cheap structural sharing. No parent pointers. Enables efficient subtree extraction/replacement for refactors
- **Red tree**: On-demand, lazily created during traversal. Adds parent pointers, absolute offsets, and error data
- **Lossless**: Preserves ALL tokens including whitespace, comments, and error nodes. The tree is a perfect round-trip representation of source text
- **Parsing never fails**: Always produces a tree + `Vec<Error>`, never `Result<T, Error>`. Invalid syntax becomes error nodes with associated messages, allowing traversal and analysis of malformed code

**Salsa: Incremental Computation Framework**
- Models the compiler as a database of cached queries with fine-grained dependency tracking
- Each query records which revision it was computed from
- On file edits, only affected queries are invalidated and recomputed
- Example: changing a function body does NOT invalidate the `ItemTree` (which only tracks signatures), so downstream type checks of callers are not recomputed
- Key data structures:
  - `ItemTree`: Condenses one `SyntaxTree` into a modification-stable structure (ignores body changes)
  - `DefMap`: Module tree and scopes for crates
- Multi-threaded analysis with per-thread query storages using fine-grained locking
- Result: sub-100ms response times on large codebases (including rust-lang/rust itself)

**Three-Phase Pipeline**
- Parse -> Name Resolution -> Type Inference
- Each phase is a salsa query layer
- Changes in phase N only propagate to phase N+1 if the output actually changed

### 1.2 Inlay Hints Strategy

rust-analyzer provides six types of inlay hints, each revealing implicit language semantics:

| Hint Type | What It Shows | UX Impact |
|-----------|--------------|-----------|
| **Type hints** | Inferred types on `let` bindings | See types without hovering |
| **Parameter name hints** | Argument names in function calls | `foo(bar: 1, baz: 2)` clarity |
| **Chaining hints** | Types between method chain dots | Understand intermediate types in `.iter().map().filter()` |
| **Closure return type hints** | Inferred return types for closures | Demystify closure signatures |
| **Lifetime elision hints** | Implicit lifetimes in generics | Make borrow checker transparent |
| **Binding mode hints** | `ref`/`mut` in patterns | Show what pattern matching does behind the scenes |

**Configuration principle**: Every hint type is individually toggleable. The `skip_trivial` option for lifetime hints avoids clutter on obvious cases. This granularity lets beginners see everything and experts pare down to just what they need.

**Applicable pattern for Nika**: Inlay hints for inferred values -- show resolved template values, inferred providers, expanded `with:` bindings inline.

### 1.3 Code Actions and Assists

rust-analyzer organizes assists into semantic categories:

**Generation assists** (create new code):
- `generate_impl` -- create `impl` block from trait
- `generate_derive` -- add derive attribute
- `generate_from_impl`, `generate_into_impl`, `generate_try_from_impl`
- Fill match arms -- exhaustive pattern matching

**Refactoring assists** (restructure existing code):
- Extract function, extract variable, extract struct
- Inline function, inline variable
- Move item to module/file
- Reorder fields, parameters, match arms

**Conversion assists** (change form):
- Convert `if` to `match` and vice versa
- Convert `Result` to `?`
- Convert `match` to `if let`
- Replace `unwrap` with `match`

**Diagnostic quick-fixes** (fix errors):
- Add missing imports (auto-import on completion)
- Add missing fields, match arms
- Remove unused imports
- Fix borrow checker issues (add `mut`, `ref`, wrap)

**Navigation-as-action**:
- Related tests discovery: find all tests that call a function
- Expand macro recursively: see generated code
- Highlight exit points / yield points / closure captures contextually

### 1.4 "Magic" Features That Surprise Developers

These features make rust-analyzer feel like it truly understands code:

1. **Contextual highlighting on hover**:
   - Hover `|` on a closure: highlights all captured variables
   - Hover `return` / `?` / `->`: highlights all exit points in the function
   - Hover `async` / `await`: highlights all yield points
   - Hover `break` / `loop`: highlights all related breakpoints

2. **Auto-import on completion**: Type a symbol name, select the completion, and the `use` statement is automatically inserted at the top of the file

3. **Recursive macro expansion**: See the full expanded output of nested macro calls, formatted and syntax-highlighted

4. **Structural search and replace**: Query code patterns semantically (understanding the AST, not just text) and refactor across the project

5. **On-typing assists**: As you type, the LSP proactively restructures code (e.g., auto-close brackets with correct nesting for Rust's syntax)

### 1.5 Error Recovery Philosophy

- The parser ALWAYS produces a tree, even for completely broken input
- Error nodes are first-class citizens in the CST
- Analysis continues past errors, providing as many diagnostics and features as possible
- The `ItemTree` layer abstracts away function bodies entirely, so broken function bodies don't affect module-level analysis
- This means completions, go-to-definition, and hover work even in files with many errors

---

## 2. DSL-Specific LSP Innovations

### 2.1 Prisma LSP

**What it does well for schema DSLs:**
- **Multi-file schema support**: `PrismaSchema` class aggregates schemas across multiple `.prisma` files
- **Relation-aware completions**: Suggests related models when defining `@relation` fields
- **Schema-level diagnostics**: Validates not just syntax but semantic correctness (e.g., one-to-many relationships must have matching fields on both sides)
- **Go-to-definition**: Jump between models, enums, and type references across files
- **Intelligent auto-completion**: Context-aware suggestions for model fields, types, attributes, and directives

**Applicable pattern for Nika**: Multi-file support (include/skill resolution), cross-reference validation between tasks.

### 2.2 GraphQL LSP

**Architecture:**
- Process model: IDE launches isolated GraphQL servers as child processes, one per project configuration
- Each server caches schema artifacts, fragment definitions, and queries
- Communication via JSON-RPC over stdio or IPC

**Key features:**
- Schema validation: Parse queries against cached schemas for type mismatches
- Context-aware completions: Suggest fields, arguments, and types based on schema
- Fragment suggestions: Resolve and suggest fragment spreads
- Type information on hover: Show resolved types for fields and variables
- Multi-project support: Separate servers per `.graphqlrc.yml` configuration

**Applicable pattern for Nika**: Expression-aware completions inside `${{ }}` templates, schema-driven validation layered on top of YAML parsing.

### 2.3 Terraform LSP (terraform-ls)

**IaC-specific features:**
- **Provider schema completions**: After `terraform init`, loads provider schemas for context-aware autocomplete of resource types, attributes, and blocks
- **Pre-fill required fields**: When completing a resource block, auto-fills all required attributes (alphabetically sorted)
- **Module reference navigation**: Go-to-definition and find-references for module calls across the project
- **terraform validate integration**: Runs validation commands and publishes diagnostics directly in editor
- **Semantic tokens**: Type-aware syntax highlighting distinguishing primitives, templates, objects, maps
- **Reference counts**: Shows how many times a block/attribute is referenced (code lens)
- **Treeview widgets**: Visual trees for providers, modules, and their relationships

**Architecture:**
- Lexer/tokenizer -> AST -> provider schema integration for semantic analysis
- Workspace indexing on init (scans all `.tf` files, supports symlinks)
- Sequential processing of document change events

**Applicable pattern for Nika**: Provider schema-driven completions (Nika knows which MCP tools are available), reference counting for task IDs, workspace-wide indexing of `.nika.yaml` files.

### 2.4 dbt Power User

**SQL workflow-specific features:**
- **Model lineage visualization**: DAG with column-level dependency details rendered in the editor
- **Auto-completion for references**: `ref()` function suggests model names with column details
- **Jinja-to-SQL compilation**: Show compiled SQL from Jinja templates inline
- **Query execution and preview**: Run full or partial SQL with results displayed in editor
- **AI-powered documentation generation**: Generate and validate docs for models

**Applicable pattern for Nika**: DAG visualization as a code lens or side panel, template compilation preview (show resolved `{{with.alias}}` values), execution from the editor.

---

## 3. AI-Enhanced Developer Experiences

### 3.1 How AI Tools Enhance LSP

| Tool | Architecture | Key Enhancement |
|------|-------------|----------------|
| **GitHub Copilot** | "Suggest-first" -- predicts from millions of repos | Inline suggestions as you type, boilerplate generation |
| **Cursor** | Deep semantic indexing of the repository | Multi-file diff generation from natural language, chat with codebase awareness |
| **Sourcegraph Cody** | "Search-first" -- code graph engine | Cross-repo context (82% usable code vs. 68% for Copilot), precise multi-repo suggestions |

### 3.2 UX Patterns That Create "Understanding"

1. **Contextual chat**: Cmd+L opens chat pre-loaded with current file context; Cmd+Enter pulls relevant snippets from elsewhere in the codebase
2. **Clickable navigation**: File names in chat/suggestions are clickable -- jump directly to location
3. **Multi-file agency**: Describe changes in natural language -> receive a multi-file diff for review
4. **Code graph queries**: "How does data flow from X to Y?" with precise cross-repo answers
5. **AI code review**: Scan git diffs for issues, suggest improvements with project context

### 3.3 The "Magic" Threshold

The key insight: developers feel "understood" when the tool:
- **Anticipates** what they need before they ask (auto-import, smart defaults)
- **Explains** in terms of their domain, not the tool's internals
- **Navigates** the same mental model as the developer (clickable references, cross-file awareness)
- **Remembers** context across actions (not stateless -- knows what you've been working on)
- **Prevents** mistakes proactively rather than reporting them after the fact

---

## 4. Error UX Best Practices

### 4.1 The Elm Philosophy

Elm treats the compiler as a teaching assistant, not a gatekeeper. Core principles:

1. **Show code exactly as written** (no pretty-printing) to avoid mental mapping
2. **Use first-person language** ("I" / "we") to personify the compiler conversationally
3. **Provide specific fix suggestions** over jargon
4. **Hide type-checker internals** -- present mistakes in plain English
5. **Assume beginners** -- be verbose and helpful by default
6. **Pinpoint the smallest relevant span** for IDE integration

**Concrete Elm error format:**
```
-- CYCLIC DEFINITION -------------------------------------------- file.elm

The `x` value is defined directly in terms of itself, causing an infinite loop.

2| x = x + 1
   ^

Are you trying to mutate a variable? Elm does not have mutation, so when I
see `x` defined in terms of `x`, I treat it as a recursive definition...
```

**What makes this great:**
- The header names the error category in human terms ("CYCLIC DEFINITION" not "E0391")
- Shows the exact code with a pointer
- Asks what the developer might have been trying to do
- Teaches a language concept (immutability) as part of the error
- Suggests an alternative approach

**Another example -- name clash:**
```
-- NAME CLASH --------------------------------------------------- file.elm

This file defines multiple `Heading` type constructors. One here:

19| = Heading (Heading char)
     ^^^^^^^

And another one here:

23| type alias Heading char =
              ^^^^^^^

How can I know which one you want? Rename one of them!
```

The error shows BOTH locations, explains the ambiguity from the compiler's perspective, and gives a direct actionable fix.

### 4.2 The Rust Compiler Philosophy

Rust's errors use structured formatting with labeled spans:

- **Primary label** (red, `^^^`): Describes the "what" -- what went wrong
- **Secondary label** (blue, `---`): Explains the "why" -- context for the error
- **Help line**: Suggests concrete fixes
- **Note line**: Provides additional context or links to documentation

**Concrete rustc error format:**
```
error[E0502]: cannot borrow `foo` as mutable because it is also borrowed as immutable
  --> src/main.rs:6:5
   |
4  |     let bar = &foo.bar;
   |               -------- immutable borrow occurs here
5  |
6  |     foo.baz();
   |     ^^^^^^^^^ mutable borrow occurs here
7  |
8  |     println!("{}", bar);
   |                    --- immutable borrow later used here
```

**What makes this great:**
- E-code (E0502) is linkable to detailed documentation (`rustc --explain E0502`)
- Three locations shown: where the borrow started, where the conflict happens, where the borrow is used
- Each location has a label explaining its role in the error
- The labels tell a story: "immutable borrow occurs here" -> "mutable borrow occurs here" -> "immutable borrow later used here"

### 4.3 "Did You Mean?" Techniques

| Technique | How It Works | When to Use |
|-----------|-------------|-------------|
| **Levenshtein distance** | Count minimum edits (insert/delete/substitute) | Typos in identifiers |
| **Trie + fuzzy search** | Prefix tree with distance threshold | Auto-complete suggestions |
| **Frequency-weighted ranking** | Prioritize commonly-used symbols | Ambiguous matches |
| **Contextual ranking** | Local variables first, then module scope, then imports | Scope-aware suggestions |
| **N-gram matching** | Substring overlap scoring | Partial name matches |

**Best practice**: Combine edit distance with contextual ranking. A variable name with edit distance 1 in the current scope beats a perfect match from an unimported module.

### 4.4 Progressive Disclosure of Errors

**Level 1 -- Inline squiggly**: Just the red underline in the editor. Developer knows something is wrong.

**Level 2 -- Hover summary**: One-line message + suggested fix. "Unknown task ID `summarize`. Did you mean `summarise`?"

**Level 3 -- Diagnostic panel**: Full error with code, context, and explanation. Multiple related locations.

**Level 4 -- Documentation link**: `--explain` style detailed article about the error category with examples.

**Principle**: Start terse, expand on demand. Beginners drill into Level 4; experts fix at Level 1.

---

## 5. YAML Workflow Engine LSPs

### 5.1 GitHub Actions (Best in Class for YAML Workflows)

**Architecture:**
- Built on `yaml-language-server` (Red Hat) for YAML parsing + JSON Schema validation
- Adds a custom **GitHub Actions Expressions parser** for `${{ }}` syntax
- Schema from `json.schemastore.org/github-workflow.json` for structural validation

**Features:**
- **Schema-driven completions**: Auto-generates templates for workflows, jobs, steps. Fills required/optional properties with defaults
- **Expression-aware intelligence**: Inside `${{ }}`, provides completions for functions (`github.ref`), contexts (`needs.job.outputs`), event payloads
- **Action/reusable workflow intelligence**: Parses `uses:` references, extracts inputs/outputs from the referenced action, provides validation and completions with inline docs
- **Cross-reference validation**: Validates job `needs` dependencies, step output references
- **Semantic highlighting**: Distinguishes static YAML from expression values
- **Workflow management**: Integrated runs, logs, secrets management (beyond LSP)

**What makes it the reference implementation:**
- Layered architecture: generic YAML LSP + domain-specific expression parser
- Schema + runtime awareness: knows both the YAML structure AND the expression language
- External reference resolution: fetches action metadata from GitHub for completions

### 5.2 Red Hat yaml-language-server (Foundation Layer)

**How it works:**
- Parses YAML into AST using `eemeli/yaml` (YAML 1.2 spec)
- Associates schemas via priority: (1) in-file modeline, (2) custom provider API, (3) settings globs, (4) notifications, (5) Schema Store
- Validates AST against JSON Schema for structure + values
- Provides completions from schema properties/defaults, hover from schema descriptions

**Schema association configuration:**
```json
{
  "yaml.schemas": {
    "https://json.schemastore.org/github-workflow.json": [".github/workflows/*.yml"]
  }
}
```

**For Nika**: This is the foundation to build on. Associate `nika-workflow.schema.json` with `*.nika.yaml` files, then layer Nika-specific intelligence on top.

### 5.3 CircleCI, Temporal, Others

- **CircleCI**: No official LSP. Basic YAML support via generic extensions. No expression-aware validation for orbs, jobs, or workflows
- **Temporal**: No YAML LSP. Temporal uses SDK-based workflow definitions (Go, Java, TypeScript), relying on language-specific IDE support
- **Airflow/Prefect/Dagster**: Python-based, rely on Python LSP. No YAML-specific workflow tooling
- **ArgoCD/Tekton**: Rely on generic YAML schema validation only

**Gap in the market**: No YAML workflow engine has a truly excellent, domain-specific LSP. GitHub Actions is closest but still primarily schema-driven rather than semantically aware.

---

## 6. Patterns Applicable to Nika LSP

### 6.1 Architecture Patterns (from rust-analyzer)

| Pattern | rust-analyzer Approach | Nika LSP Application |
|---------|----------------------|---------------------|
| **Lossless syntax tree** | Rowan CST preserves all tokens including errors | Parse `.nika.yaml` into a CST that preserves comments, formatting, and error nodes |
| **Incremental computation** | Salsa query database with fine-grained invalidation | Cache parsed workflows, invalidate only changed files. Re-resolve `include:` trees incrementally |
| **Error recovery** | Parser always produces a tree + errors | YAML parser continues past errors, providing completions and diagnostics for the valid parts |
| **Three-phase pipeline** | Parse -> Analyze -> Lower | Parse YAML -> Validate against schema + resolve references -> Provide LSP features |

### 6.2 Feature Ideas (Prioritized)

**Tier 1 -- Foundation (Must Have)**

1. **Schema-driven validation**: Validate against `nika-workflow.schema.json` for structural correctness
2. **Verb-aware completions**: After `infer:`, suggest provider/model. After `exec:`, suggest common commands. After `invoke:`, suggest MCP tools
3. **Task ID completions**: In `depends_on:`, `with:` bindings (`$task_id`), and templates (`{{with.alias}}`), suggest known task IDs
4. **Template expression completions**: Inside `{{...}}`, suggest available bindings, transforms, and paths
5. **Go-to-definition**: Click a task reference (`$summarize`) -> jump to that task's definition. Click `include:` -> open the referenced file
6. **Find references**: Right-click a task -> see all places it's referenced in `depends_on:`, `with:`, templates
7. **Diagnostic messages a la Elm**: Human-readable errors with suggestions, not just "schema validation failed"

**Tier 2 -- Intelligence (Should Have)**

8. **Inlay hints for resolved values**:
   - Show resolved template values inline: `prompt: "Summarize {{with.data}}"` -> hint shows what `data` resolves to
   - Show inferred provider for `infer:` tasks when auto-detected
   - Show timeout in human-readable format: `timeout: 30` -> hint shows "(30s)"
9. **DAG visualization as code lens**: Above workflow name, show a clickable "View DAG" lens that renders the dependency graph
10. **Cross-file include resolution**: `include:` paths resolve and provide diagnostics for missing files, circular includes
11. **MCP tool schema awareness**: When connected to MCP servers, provide completions for available tools and their parameters
12. **Transform chain validation**: Validate that transform chains are type-compatible (e.g., `| split | length` is valid, `| split | round` is not)

**Tier 3 -- Magic (Could Have)**

13. **Live workflow preview**: Show what the resolved workflow looks like after all includes, bindings, and templates are expanded
14. **Contextual highlighting** (from rust-analyzer):
    - Hover a task ID -> highlight all references and dependent tasks
    - Hover `depends_on:` -> highlight the entire dependency chain
    - Hover a `with:` binding -> highlight where the data flows
15. **"Run task" code lens**: Above each task, show a clickable "Run" / "Debug" lens
16. **Workflow-aware refactoring**:
    - Rename a task ID -> update all `depends_on:`, `with:`, and template references
    - Extract a task group into a separate file (like "extract function")
    - Inline an `include:` (expand it in place)
17. **AI-assisted workflow generation**: Natural language -> workflow YAML (could integrate with `agent:` verb)
18. **Execution history overlay**: Show last run status/timing as ghost text next to each task

### 6.3 Error Message Design (from Elm/Rust)

**Error format for Nika:**

```
-- UNKNOWN TASK REFERENCE --------------------------------- workflow.nika.yaml

Task `analyze` references unknown task `sumamrize` in its bindings.

12|   with:
13|     data: $sumamrize
              ^^^^^^^^^^

Did you mean `summarize`? (edit distance: 1)

The `with:` block binds data from other tasks using the `$task_id` syntax.
Available tasks in this workflow: fetch_data, summarize, format_output
```

**Error categories for Nika (human-readable headers):**

| NIKA Code | Human Header | Example |
|-----------|-------------|---------|
| NIKA-010 | INVALID WORKFLOW STRUCTURE | Missing required `tasks:` field |
| NIKA-020 | CIRCULAR DEPENDENCY | Task A -> B -> C -> A |
| NIKA-030 | UNKNOWN PROVIDER | Provider `openai` not configured |
| NIKA-040 | BROKEN TEMPLATE | `{{with.missing}}` has no binding |
| NIKA-050 | MISSING TASK | `depends_on: [nonexistent]` |
| NIKA-060 | INVALID OUTPUT | JSON schema validation failed |
| NIKA-070 | BINDING ERROR | `with:` references unavailable data |
| NIKA-100 | MCP CONNECTION FAILED | Cannot reach MCP server |

**Progressive disclosure for Nika:**
- **Level 1 (squiggly)**: Red underline on `$sumamrize`
- **Level 2 (hover)**: "Unknown task `sumamrize`. Did you mean `summarize`?"
- **Level 3 (diagnostic panel)**: Full error with code snippet, available tasks, and explanation of `$` binding syntax
- **Level 4 (documentation)**: Link to `nika check --explain NIKA-050` with detailed examples

### 6.4 Inlay Hint Strategy for Nika

| Hint Type | What It Shows | Example |
|-----------|--------------|---------|
| **Resolved template** | What `{{with.x}}` evaluates to | `"Summarize {{with.data}}"` -> `(from: fetch_data.output)` |
| **Inferred provider** | Auto-detected provider | `infer:` -> `(openai/gpt-4o)` |
| **Timeout display** | Human-readable timeout | `timeout: 120` -> `(2m)` |
| **Task count** | Number of tasks in workflow | After `tasks:` -> `(5 tasks)` |
| **Dependency depth** | Longest path to this task | After task ID -> `(depth: 3)` |
| **Include expansion** | What an include brings in | `include: ./lib.yaml` -> `(+3 tasks, +2 skills)` |

### 6.5 Competitive Positioning

The YAML workflow LSP landscape has a clear gap:

```
                    Schema-only          Semantically-aware
                    +--------------------------------------------+
Generic YAML LSP    |  [yaml-language-server]                     |
                    |                                            |
CI/CD YAML LSP      |  [CircleCI]  [GitHub Actions]              |
                    |                                            |
Workflow DSL LSP    |                               [NIKA TARGET]|
                    +--------------------------------------------+
                    Basic                          Intelligent
```

No workflow YAML tool today provides:
- Expression-aware completions inside templates
- Cross-file reference resolution for includes
- DAG-aware refactoring
- Runtime context integration (MCP tool discovery, provider auto-detection)
- Elm-quality error messages

**Nika LSP can be the rust-analyzer of YAML workflow engines.**

---

## Sources

1. rust-analyzer GitHub repository and architecture docs -- https://github.com/rust-lang/rust-analyzer
2. rust-analyzer joins Rust org (blog post) -- https://blog.rust-lang.org/2022/02/21/rust-analyzer-joins-rust-org/
3. Rowan crate (concrete syntax trees) -- https://github.com/rust-analyzer/rowan
4. Salsa incremental computation framework -- https://github.com/salsa-rs/salsa
5. Prisma language-tools -- https://github.com/prisma/language-tools
6. Prisma language server docs -- https://github.com/prisma/language-tools/blob/main/docs/language-server.md
7. GraphQL language service -- https://github.com/graphql/graphql-language-service
8. Terraform language server -- https://github.com/hashicorp/terraform-ls
9. dbt Power User extension -- VS Code Marketplace
10. GitHub Actions extension -- VS Code Marketplace
11. Red Hat yaml-language-server -- https://github.com/redhat-developer/yaml-language-server
12. Elm error message design -- https://elm-lang.org/news/compiler-errors-for-humans
13. Rust compiler error format -- https://blog.rust-lang.org/2016/08/10/Shape-of-errors-to-come.html
14. Cursor AI -- https://cursor.com
15. Sourcegraph Cody -- https://sourcegraph.com/cody
16. LSP specification -- https://microsoft.github.io/language-server-protocol/

## Methodology

- Tools used: Perplexity (sonar-pro) for web search, cross-referenced across 10+ queries
- Sources analyzed: ~50 pages across documentation, blog posts, GitHub repos, HN discussions
- Coverage: rust-analyzer internals, 6 DSL-specific LSPs, 3 AI tools, compiler error philosophies (Elm, Rust), 5 YAML workflow tools

## Confidence Level

**High** for rust-analyzer architecture, error UX patterns, and GitHub Actions LSP features (well-documented, multiple corroborating sources).

**Medium** for Prisma/GraphQL LSP internals and AI tool architecture (fewer deep technical sources available).

**Low** for CircleCI/Temporal/Dagster LSP specifics (minimal public documentation exists, confirming the market gap).
