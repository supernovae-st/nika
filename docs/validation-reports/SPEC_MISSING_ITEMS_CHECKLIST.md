# Nika Spec - Missing Items Checklist

**For spec writers:** Use this as a checklist when rewriting spec/SPEC.md

---

## VERBS (5 total - spec has 3, missing 2)

### Section: Actions (Currently §4, will expand)

#### Existing (Documented)
- [x] Section 4.1: `infer:` (LLM generation)
- [x] Section 4.2: `exec:` (shell commands)
- [x] Section 4.3: `fetch:` (HTTP requests)

#### Missing (Add to §4.4 and §4.5)
- [ ] Section 4.4: `invoke:` (MCP tool calls) — ~3 pages
- [ ] Section 4.5: `agent:` (multi-turn loops) — ~4 pages

---

## SCHEMA VERSIONS (9 total - spec has 1, missing 8)

### New Section: Schema Evolution (insert after §1)

- [ ] Schema Version Table (v0.1 through v0.9) — 1 page
  - [ ] Columns: Version, Schema, Date, Key Changes, Breaking Changes
  - [ ] v0.1 (2025-01-27): infer, exec, fetch
  - [ ] v0.2 (2026-02-18): +invoke, +agent, +MCP
  - [ ] v0.3 (2026-02-18): +for_each parallelism
  - [ ] v0.4 (2026-02-19): rig-core migration
  - [ ] v0.5 (2026-02-20): +decompose, +lazy, +spawn_agent
  - [ ] v0.6 (2026-02-20): +6 LLM providers
  - [ ] v0.7 (2026-02-21): +full streaming
  - [ ] v0.8 (2026-02-23): +Studio DX
  - [ ] v0.9 (2026-02-25): +context:, +include:

---

## PROVIDERS (7 total - spec has 3, missing 4)

### Update Section: 2. Workflow → Providers

- [x] Claude (ANTHROPIC_API_KEY)
- [x] OpenAI (OPENAI_API_KEY)
- [x] Mock (no key)
- [ ] Mistral (MISTRAL_API_KEY) — Added v0.6
- [ ] Groq (GROQ_API_KEY) — Added v0.6
- [ ] DeepSeek (DEEPSEEK_API_KEY) — Added v0.6
- [ ] Ollama (OLLAMA_API_BASE_URL) — Added v0.6
- [ ] Gemini (GEMINI_API_KEY) — Added v0.15.0

### Add: Auto-Detection Priority Table
```yaml
| Priority | Env Variable | Provider | Added |
|----------|--------------|----------|-------|
| 1 | ANTHROPIC_API_KEY | Claude | v0.1 |
| 2 | OPENAI_API_KEY | OpenAI | v0.1 |
| 3 | MISTRAL_API_KEY | Mistral | v0.6 |
| 4 | GROQ_API_KEY | Groq | v0.6 |
| 5 | DEEPSEEK_API_KEY | DeepSeek | v0.6 |
| 6 | GEMINI_API_KEY | Gemini | v0.15.0 |
| 7 | OLLAMA_API_BASE_URL | Ollama | v0.6 |
```

---

## ERROR CODES (192 total - spec has 41, missing 151)

### New Major Section: 11. Error Codes Reference (will expand from current §11)

Current spec covers: NIKA-010, NIKA-050-056, NIKA-060-061, NIKA-070-074, NIKA-080-082, NIKA-090-092

#### Existing Error Code Ranges (41 codes total)
- [x] 010 (Schema errors): 1 code
- [x] 050-056 (Path/task errors): 7 codes
- [x] 060-061 (Output errors): 2 codes
- [x] 070-074 (Use block errors): 5 codes
- [x] 080-082 (DAG errors): 3 codes
- [x] 090-092 (JSONPath errors): 3 codes
- [x] 000-009 (Workflow errors): 5 codes documented elsewhere
- [x] 020-029 (DAG errors): 2 codes documented elsewhere
- [x] 030-039 (Provider errors): 4 codes documented elsewhere
- [x] 040-049 (Binding errors): 6 codes documented elsewhere

#### Missing Error Code Ranges (151 codes total)

**NEW: NIKA-100-109 (MCP Errors - 10 codes, v0.2)**
- [ ] NIKA-100: MCP server not connected
- [ ] NIKA-101: MCP server failed to start
- [ ] NIKA-102: MCP tool call failed
- [ ] NIKA-103: MCP resource not found
- [ ] NIKA-104: MCP protocol error
- [ ] NIKA-105: MCP not configured
- [ ] NIKA-106: MCP invalid response
- [ ] NIKA-107: MCP validation failed
- [ ] NIKA-108: MCP schema error
- [ ] NIKA-109: MCP timeout

**NEW: NIKA-110-119 (Agent Errors - 8 codes, v0.2)**
- [ ] NIKA-110: Agent max turns exceeded
- [ ] NIKA-111: Stop condition not met
- [ ] NIKA-112: Invalid tool name
- [ ] NIKA-113: Agent validation failed
- [ ] NIKA-115: Agent execution failed
- [ ] NIKA-116: Thinking capture failed
- [ ] NIKA-117: Thinking not supported
- [ ] (NIKA-114, NIKA-118-119: Not used)

**NEW: NIKA-120-129 (Resilience Errors - 3 codes, v0.2)**
- [ ] NIKA-120: Provider error
- [ ] NIKA-121: Operation timeout
- [ ] NIKA-125: Tool call failed

**NEW: NIKA-130-139 (TUI Errors - 1 code, v0.2)**
- [ ] NIKA-130: TUI error

**NEW: NIKA-140-149 (Config Errors - 1 code, v0.5)**
- [ ] NIKA-140: Config error

**NEW: NIKA-150-159 (Startup Errors - 1 code, v0.8)**
- [ ] NIKA-150: Startup verification failed

**NEW: NIKA-160-169 (Policy Errors - 2 codes, v0.13)**
- [ ] NIKA-160: Policy violation
- [ ] NIKA-161: Boot failed

**NEW: NIKA-170-179 (Runtime Errors - 1 code, v0.14)**
- [ ] NIKA-170: Runtime error

**NEW: NIKA-210-219 (Builtin Tool Errors - 4 codes, v0.9)**
- [ ] NIKA-210: Builtin tool error
- [ ] NIKA-211: Builtin tool not found
- [ ] NIKA-212: Invalid parameters
- [ ] NIKA-213: Assertion failed

**NEW: NIKA-250-259 (Context Errors - 1 code, v0.14.2)**
- [ ] NIKA-250: Context load error

**NEW: NIKA-260-269 (pkg: URI Errors - 1 code, v0.15.2)**
- [ ] NIKA-260: Invalid pkg: URI

---

## ADVANCED FEATURES (6 major - all missing)

### New Section: Advanced Features (insert after §10)

#### Feature 1: `for_each` Parallelism (v0.3+) — 2 pages
- [ ] Configuration parameters (for_each, as, concurrency, fail_fast)
- [ ] Example workflows (parallel locales, stress test)
- [ ] Binding expressions
- [ ] Error handling

#### Feature 2: `context:` File Loading (v0.14.2+) — 2 pages
- [ ] Configuration (files, session)
- [ ] Supported file types (markdown, JSON, YAML, glob)
- [ ] Access syntax `{{context.files.alias}}`
- [ ] Example: Brand context, persona, examples

#### Feature 3: `include:` DAG Fusion (v0.14.2+) — 2 pages
- [ ] Configuration (path, prefix)
- [ ] Task ID prefixing
- [ ] Recursive includes
- [ ] Cycle detection
- [ ] Example: Setup/cleanup pattern

#### Feature 4: `decompose:` Runtime Expansion (v0.5+) — 2 pages
- [ ] Configuration (strategy, traverse, source, max_items)
- [ ] Strategies (semantic, static, nested)
- [ ] MCP traversal
- [ ] Example: Graph-driven parallelism

#### Feature 5: `lazy: true` Bindings (v0.5+) — 1 page
- [ ] Long form syntax
- [ ] Default values
- [ ] Deferred resolution
- [ ] Use cases

#### Feature 6: `spawn_agent` Tool (v0.5+) — 1 page
- [ ] Parameters (task_id, prompt, context, max_turns)
- [ ] Depth protection
- [ ] Integration with agent: verb
- [ ] Example: Multi-level orchestration

---

## MCP INTEGRATION (entirely missing)

### New Section: MCP Integration (insert after §14)

- [ ] MCP Server Configuration — 2 pages
  - [ ] mcp: section syntax
  - [ ] command and args
  - [ ] Environment variables
  - [ ] Example: NovaNet, Perplexity, Filesystem servers

- [ ] 8 MCP Tools Reference — 4 pages
  - [ ] invoke: verb (calls MCP tools)
  - [ ] Tool discovery
  - [ ] Parameter validation
  - [ ] Common tools (novanet_*, web_search, filesystem)

- [ ] MCP vs invoke: differences — 1 page

---

## SECURITY SECTION (entirely missing)

### New Section: Security (§15.X)

- [ ] Shell-free Execution (v0.15.0+) — 1 page
  - [ ] exec: shell: false (default)
  - [ ] shlex parsing
  - [ ] Command blocklist
  - [ ] NIKA-053 BlockedCommand error

- [ ] Path Traversal Protection (v0.14.2+) — 1 page
  - [ ] include: path validation
  - [ ] context: file boundaries
  - [ ] Prevents ../../../ attacks

- [ ] LLM Control Parity (v0.15.0+) — 1 page
  - [ ] infer: temperature, system, max_tokens

---

## TYPE DEFINITIONS (all need updating)

### Update Section: 3. Task

Current types are incomplete. Add missing fields:

#### Task struct needs:
- [ ] for_each: Option<ForEachConfig>
- [ ] decompose: Option<DecomposeSpec>
- [ ] depends_on: Option<Vec<String>>
- [ ] shell: Option<bool>
- [ ] skills: Option<Vec<SkillDef>>

#### Workflow struct needs:
- [ ] context: Option<ContextConfig>
- [ ] include: Option<Vec<IncludeSpec>>
- [ ] skills: Option<Vec<SkillDef>>
- [ ] mcp: Option<HashMap<String, McpServer>>

#### UseEntry struct needs:
- [ ] lazy: bool
- [ ] default: Option<Value>

---

## TUI/STUDIO SECTION (entirely missing)

### New Section: Terminal UI (§16)

- [ ] 6 Views overview — 1 page
  - [ ] Home view (browse workflows)
  - [ ] Chat view (conversational agent)
  - [ ] Studio view (YAML editor)
  - [ ] Monitor view (real-time execution)
  - [ ] Settings view
  - [ ] Help view

- [ ] Studio Features — 2 pages
  - [ ] Edit history (Undo/Redo)
  - [ ] Session persistence
  - [ ] Syntax highlighting
  - [ ] Schema validation
  - [ ] Keyboard shortcuts

- [ ] Keyboard Shortcuts table — 1 page

---

## EXAMPLE WORKFLOWS (all need updating)

### Update §13: Examples

Current example is v0.1 only. Add examples showing:

- [ ] Basic workflow (v0.1 features)
- [ ] MCP integration (invoke:, v0.2)
- [ ] Agent loops (agent:, v0.2)
- [ ] Parallel processing (for_each, v0.3)
- [ ] Context loading (context:, v0.14.2)
- [ ] DAG fusion (include:, v0.14.2)
- [ ] Runtime expansion (decompose:, v0.5)
- [ ] Multi-provider (pick different provider)
- [ ] Advanced (combining 3+ features)
- [ ] Real-world (NovaNet + web search + email)

---

## SUMMARY CHECKLIST

### Must-Have (Blocking)
- [ ] Update version header (v0.15.1, @0.9)
- [ ] Document `invoke:` verb
- [ ] Document `agent:` verb
- [ ] Document all 192 error codes
- [ ] Update all Rust types

### Should-Have (High Priority)
- [ ] Schema version history
- [ ] for_each, context:, include:, decompose:, lazy:
- [ ] MCP integration section
- [ ] All 7 providers
- [ ] Security section

### Nice-to-Have (Medium Priority)
- [ ] TUI/Studio documentation
- [ ] Refresh all examples
- [ ] Advanced use cases
- [ ] Troubleshooting guide

---

## Effort Breakdown by Section

| Section | Pages | Hours | Priority |
|---------|-------|-------|----------|
| Version update | 1 | 0.5 | Critical |
| Schema history | 1 | 1 | High |
| Verbs (2 missing) | 7 | 3 | Critical |
| Providers | 1 | 1 | High |
| Error codes | 5 | 3 | Critical |
| for_each + advanced | 8 | 4 | High |
| MCP integration | 6 | 3 | High |
| Type definitions | 3 | 2 | High |
| Security | 2 | 1 | Medium |
| TUI/Studio | 4 | 2 | Medium |
| Examples | 2 | 1.5 | Medium |
| **TOTAL** | **40** | **20.5** | - |

---

**Use this checklist to track progress while rewriting spec/SPEC.md**

Track completion by marking checkboxes as you write each section.

