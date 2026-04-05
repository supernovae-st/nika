# S4+ Execution Playbook — Autonomous Multi-Session

> Each task is a self-contained commit. Read this file at session start.
> Status: `[ ]` = pending, `[x]` = done, `[~]` = in progress.
> After each commit: update status + test count below.

**Test baseline**: 9,909 tests (after S3) → **9,981 tests** (after Session A)
**Workspace**: `cd /Users/thibaut/dev/supernovae/nika/tools`
**Test cmd**: `cargo test --workspace --lib`
**Commit format**: `type(scope): description` + both co-authors

---

## Mandatory Workflow Per Session

```
1. Read THIS file first
2. Check /spn-powers:yo for available skills
3. For each task:
   a. Use /spn-powers:test-driven-development skill (RED-GREEN-REFACTOR)
   b. Use /spn-rust:rust-core skill for Rust patterns (ownership, error handling)
   c. Use /find-docs or Context7 BEFORE writing code that touches external APIs
   d. Research with /spn-search:search if unsure about provider API details
   e. Use /spn-powers:verification-before-completion before claiming done
   f. Use /spn-powers:requesting-code-review after major refactors (C2, C3)
4. After ALL tasks in a session:
   a. cargo clippy --workspace
   b. git push
   c. Update this file (status + test count)
   d. Update handoff doc: nika/docs/plans/2026-04-05-grand-nettoyage-s2-handoff.md
```

## Skills & Tools Reference

| When | Use This |
|------|----------|
| Writing any Rust | `/spn-rust:rust-core` (ownership, error handling, type-state) |
| Writing tests | `/spn-powers:test-driven-development` (RED-GREEN-REFACTOR mandatory) |
| Before coding provider APIs | `/find-docs` + `/spn-search:search` for up-to-date API docs |
| Refactoring provider enum (C2) | `/spn-rust:rust-architect` for macro design |
| Async agent loop (C3) | `/spn-rust:rust-async` for tokio patterns |
| After completing a phase | `/spn-powers:requesting-code-review` (dispatch code-reviewer agent) |
| Debugging test failures | `/spn-powers:systematic-debugging` (4-phase framework) |
| Multiple independent fixes | `/spn-powers:dispatching-parallel-agents` (3+ agents) |
| Before every commit | `/spn-powers:verification-before-completion` (evidence before assertions) |
| Context7 for rig-core docs | `ctx7 library rig-core "completion client streaming"` |
| Context7 for reqwest | `ctx7 library reqwest "POST json timeout"` |
| Context7 for tower-lsp | `ctx7 library tower-lsp "publish diagnostics"` (Session D) |

## MCP Tools Available

| MCP Server | Use For |
|------------|---------|
| `nika` | `nika_check`, `nika_schema`, `nika_list_workflows` — validate workflows |
| `neo4j` | `read_neo4j_cypher`, `get_neo4j_schema` — if touching NovaNet integration |
| `linear` | `list_issues`, `get_issue` — check for related Linear tickets |

## Research Checkpoints

Before starting each session, verify these are still accurate:
- [ ] OpenRouter API: `https://openrouter.ai/api/v1` (use `/spn-search:scrape` on their docs)
- [ ] Together AI API: `https://api.together.xyz/v1`
- [ ] rig-core version: check `Cargo.toml` for current version, `ctx7` for API changes
- [ ] Provider test counts: grep `KNOWN_PROVIDERS.len()` across workspace

---

## Session A — Quick Wins (HTTP dedup + ModelResolver + providers)

### Task A1: Extract `raw_chat_completion()` helper

**Status**: `[x]` ✅ Done (commit a1218f1)
**Skills**: `/spn-powers:test-driven-development`, `/spn-rust:rust-core`
**Research**: `ctx7 library reqwest "POST json bearer auth timeout"` for HTTP patterns
**Files**: `nika-engine/src/provider/rig/mod.rs`
**Lines**: Copy A at 537-613, Copy B inline at 1104-1203

**Steps**:

1. Read `mod.rs:537-613` (Copy A: `raw_openai_compat_infer`)
2. Read `mod.rs:1104-1203` (Copy B: inline in `infer_with_tools`)
3. **Write test first**: In `nika-engine/src/provider/rig/tests.rs`, add test that verifies
   `infer_with_tools` for OpenAiCompat emits non-zero token counts (currently broken — it
   doesn't track tokens). Use wiremock to fake an OpenAI-compatible endpoint.
4. Extract shared method:
   ```rust
   /// POST to /chat/completions, return (parsed_json, prompt_tokens, completion_tokens)
   async fn raw_chat_completion(
       http_client: &reqwest::Client,
       base_url: &str,
       api_key: &str,
       body: serde_json::Value,
       timeout_secs: u64,
   ) -> Result<(serde_json::Value, u64, u64), RigInferError>
   ```
5. Refactor Copy A (`raw_openai_compat_infer`) to call `raw_chat_completion()` then extract
   `choices[0].message.content`
6. Refactor Copy B (`infer_with_tools` OpenAiCompat arm) to call `raw_chat_completion()` then
   extract `choices[0].message.tool_calls[0].function.arguments` with content fallback.
   **Also capture token usage** (bug fix).
7. Run: `cargo test -p nika-engine --lib -- provider::rig`
8. Commit: `refactor(provider): extract raw_chat_completion helper, fix tools token tracking`

**Verification**: Token usage in `infer_with_tools` is no longer zero for OpenAiCompat.

---

### Task A2: Wire ModelResolver in agent loop (replace hardcoded defaults)

**Status**: `[x]` ✅ Done (commit 37c212f)
**Skills**: `/spn-powers:test-driven-development`, `/spn-rust:rust-core` (error handling with thiserror)
**Files**: `nika-engine/src/runtime/rig_agent_loop/providers.rs`
**Lines**: 225, 241, 257, 273, 289

**Steps**:

1. Read `providers.rs` lines 220-300 — five `run_*()` methods with `.unwrap_or_else(|| "hardcoded")`
2. Also read `run_claude()` and `run_openai()` — they use `.ok_or_else(|| NikaError)` (correct pattern)
3. **Write test first**: In `providers.rs` or a test file, add test that verifies `run_groq()`
   returns error when `params.model` is `None` (currently silently defaults to
   `"llama-3.3-70b-versatile"`)
4. In each of the 5 methods, replace:
   ```rust
   // BEFORE
   let model = params.model.clone()
       .unwrap_or_else(|| "llama-3.3-70b-versatile".to_string());
   // AFTER
   let model = params.model.clone()
       .ok_or_else(|| NikaError::ValidationError {
           reason: format!("model field is required (provider: {})", provider_name),
       })?;
   ```
5. Verify: `agent.rs:232-239` always sets `params.model = Some(resolved_model_id)` before
   calling the agent loop — so the `None` branch is dead code in practice.
6. Run: `cargo test --workspace --lib`
7. Commit: `fix(agent): require model in agent loop, remove hardcoded defaults (ModelResolver)`

**Verification**: All 5 `run_*()` methods now error on missing model, matching `run_claude/run_openai`.

---

### Task A3: Replace CLI display-only model defaults

**Status**: `[x]` ✅ Done (commit e6082bb)
**Files**: `nika-cli/src/verbs.rs:74-85`

**Steps**:

1. Read `verbs.rs:74-85` — local `fn default_model_for_provider(provider: &str) -> &'static str`
2. Replace body with: `nika_core::catalogs::default_model_for_provider(provider).unwrap_or("default")`
3. Remove the local function if unused elsewhere (search for calls).
4. Check that `nika_core::catalogs::default_model_for_provider` is public and returns `Option<&str>`.
5. Run: `cargo test -p nika-cli --lib`
6. Commit: `fix(cli): use ModelResolver catalog for CLI header model display`

---

### Task A4: Add OpenRouter as first-class provider

**Status**: `[x]` ✅ Done (commit c051c3c — combined A4+A5+A6)
**Skills**: `/spn-powers:test-driven-development`
**Research**: `/spn-search:scrape` on `https://openrouter.ai/docs/api-reference/overview` for latest API details.
  Verify: base URL, auth header format, model name format (`provider/model`), streaming support.
  Also: `ctx7 library rig "openai client custom base url"` for rig-core OpenAI compat patterns.
**Files**:
- `nika-core/src/catalogs/providers.rs` — catalog entry
- `nika-core/src/catalogs/resolver.rs` — default model
- `nika-engine/src/provider/rig/mod.rs` — `from_name()` match arm

**Steps**:

1. Add to `KNOWN_PROVIDERS` in `providers.rs`:
   ```rust
   Provider {
       id: "openrouter",
       name: "OpenRouter",
       aliases: &["or"],
       env_var: "OPENROUTER_API_KEY",
       key_prefix: Some("sk-or-"),
       category: ProviderCategory::Llm,
       requires_key: true,
       description: "200+ models via unified gateway",
   },
   ```
2. Update test count: `KNOWN_PROVIDERS.len()` → 21 (was 20), LLM count → 8 (was 7)
3. Update TUI test counts in `nika-tui/src/providers/mod.rs`
4. Add default model in `PROVIDER_DEFAULTS` (resolver.rs):
   `("openrouter", "anthropic/claude-sonnet-4-6")`
5. Add match arm in `RigProvider::from_name()`:
   ```rust
   "openrouter" => {
       let key = crate::secrets::store::resolve_env("OPENROUTER_API_KEY")
           .unwrap_or_default();
       Ok(Self::openai_compat(
           "openrouter",
           "https://openrouter.ai/api/v1",
           &key,
           None,
           300,
       ))
   }
   ```
6. Add to `from_name_with_key()` similarly.
7. Run: `cargo test --workspace --lib`
8. Commit: `feat(provider): add OpenRouter as first-class provider (200+ models)`

---

### Task A5: Add Together, Fireworks, Cerebras providers

**Status**: `[x]` ✅ Done (commit c051c3c — combined A4+A5+A6)
**Skills**: `/spn-powers:test-driven-development`
**Research**: Before coding, verify base URLs with `/spn-search:search`:
  - "Together AI API base URL v1 chat completions 2026"
  - "Fireworks AI API endpoint inference v1"
  - "Cerebras API cloud inference endpoint"
  Cross-check env var names with each provider's official docs.
**Files**: Same as A4

**Steps** (same pattern as A4, batched):

Add 3 catalog entries + 3 match arms + 3 default models:

| Provider | ID | Env Var | Base URL | Default Model | Key Prefix |
|----------|----|---------|----------|---------------|------------|
| Together | `together` | `TOGETHER_API_KEY` | `https://api.together.xyz/v1` | `meta-llama/Llama-3.3-70B-Instruct` | — |
| Fireworks | `fireworks` | `FIREWORKS_API_KEY` | `https://api.fireworks.ai/inference/v1` | `accounts/fireworks/models/llama-v3p3-70b-instruct` | `fw_` |
| Cerebras | `cerebras` | `CEREBRAS_API_KEY` | `https://api.cerebras.ai/v1` | `llama-3.3-70b` | `csk-` |

Update all test counts: KNOWN_PROVIDERS → 24, LLM → 11.
Aliases: `together` → `["together-ai"]`, `fireworks` → `["fw"]`, `cerebras` → `[]`

Commit: `feat(provider): add Together, Fireworks, Cerebras providers`

---

### Task A6: Add SambaNova, Cohere, AI21 providers

**Status**: `[x]` ✅ Done (commit c051c3c — combined A4+A5+A6)
**Files**: Same as A4

| Provider | ID | Env Var | Base URL | Default Model | Key Prefix |
|----------|----|---------|----------|---------------|------------|
| SambaNova | `sambanova` | `SAMBANOVA_API_KEY` | `https://api.sambanova.ai/v1` | `Meta-Llama-3.3-70B-Instruct` | — |
| Cohere | `cohere` | `COHERE_API_KEY` | `https://api.cohere.com/compatibility/v1` | `command-r-plus` | — |
| AI21 | `ai21` | `AI21_API_KEY` | `https://api.ai21.com/studio/v1` | `jamba-1.5-large` | — |

Update counts: KNOWN_PROVIDERS → 27, LLM → 14.
Aliases: `cohere` → `["command-r"]`, `ai21` → `["jamba"]`, `sambanova` → `["samba"]`

Commit: `feat(provider): add SambaNova, Cohere, AI21 providers`

---

## Session B — find_project_root + infer.rs Tests

### Task B1: Unify find_project_root in nika-core

**Status**: `[ ]`
**Skills**: `/spn-powers:test-driven-development`, `/spn-rust:rust-core` (Path handling, Result types)
**Files**:
- NEW: `nika-core/src/project.rs`
- `nika-core/src/lib.rs` — add `pub mod project;`
- `nika-tui/src/lib.rs:505` — replace impl
- `nika-engine/src/core/mcp_config.rs:155` — replace impl
- `nika-cli/src/config.rs:35` — delegate to nika-core

**Steps**:

1. Create `nika-core/src/project.rs` with:
   - `ProjectRootSource` enum: `NikaToml, DotNika, GitRepository, CargoWorkspace, Fallback`
   - `ProjectRoot` struct: `{ root: PathBuf, source: ProjectRootSource }`
   - `find_project_root(start: Option<&Path>) -> ProjectRoot`
   - Priority: nika.toml > .nika/ > .git/ > Cargo.toml > start dir
2. Port nika-cli's 4 tests + add new tests for GitRepository and CargoWorkspace markers
3. Replace nika-tui impl (just call `nika_core::project::find_project_root(None)`)
4. Replace nika-engine impl (same pattern)
5. Simplify nika-cli to delegate to nika-core (keep `ProjectRoot` re-export for compat)
6. Run: `cargo test --workspace --lib`
7. Commit: `refactor(core): unify find_project_root (3 impls → 1 in nika-core)`

---

### Task B2: Add infer.rs guardrail tests

**Status**: `[ ]`
**Skills**: `/spn-powers:test-driven-development`
**Note**: Use `build_test_executor()` from `runtime/executor/tests.rs` to create TaskExecutor.
  Read `crate::ast::guardrails` to understand `Guardrail` struct and `OnFailure` enum.
**Files**: `nika-engine/src/runtime/executor/infer.rs` (test module at bottom)

**Steps**:

1. Read `infer.rs:1743-1797` — `check_infer_guardrails()` implementation
2. Read `crate::ast::guardrails::{run_sync_guardrails, immediate_failures}`
3. Use `build_test_executor()` helper (from `tests.rs`) to create a TaskExecutor
4. Write tests:
   - `test_guardrails_pass_emits_event` — regex guardrail that matches, verify GuardrailPassed event
   - `test_guardrails_fail_returns_error` — regex guardrail that doesn't match with `on_failure: fail`
   - `test_guardrails_length_min_max` — length guardrail, verify min/max word count
   - `test_guardrails_empty_list_is_noop` — no guardrails = Ok(())
5. Run: `cargo test -p nika-engine --lib -- runtime::executor::infer::tests`
6. Commit: `test(engine): add guardrail tests for infer.rs`

---

### Task B3: Add infer.rs provider chain fallback tests

**Status**: `[ ]`
**Files**: `nika-engine/src/runtime/executor/infer.rs` (test module) or `tests_e2e_workflow.rs`

**Steps**:

1. The mock provider supports `NIKA_MOCK_FAIL_COUNT=N` (lines 316-333) to simulate failures
2. Write E2E test using provider chain:
   ```yaml
   provider: [mock, mock]
   model: test-model
   tasks:
     - id: resilient
       infer: "test"
   ```
   With `NIKA_MOCK_FAIL_COUNT=1` — first call fails, second (fallback to same mock) succeeds.
3. Verify: task succeeds, events include `FallbackTriggered`
4. Write negative test: `NIKA_MOCK_FAIL_COUNT=10` with single provider — verify error
5. Run: `cargo test --workspace --lib`
6. Commit: `test(engine): add provider chain fallback tests for infer.rs`

---

### Task B4: Add infer.rs response_format injection test

**Status**: `[ ]`
**Files**: `nika-engine/src/runtime/executor/infer.rs` (test module)

**Steps**:

1. Read lines 115-131 — `response_format` code that injects format instruction into system prompt
2. Write E2E test with `output: { format: json }` on an infer task with `provider: mock`
3. Verify the mock response is valid JSON (mock already generates JSON)
4. Check events for correct structured output flow
5. Commit: `test(engine): add response_format injection test for infer.rs`

---

## Session C — Provider Enum Collapse (BIG refactor, post-launch)

### Task C1: Extract OpenAiCompatProvider struct

**Status**: `[ ]`
**Files**: `nika-engine/src/provider/rig/mod.rs`

**Steps**:

1. Create `pub struct OpenAiCompatProvider` with fields from current `OpenAiCompat { ... }` variant
2. Move `raw_openai_compat_infer()` and `raw_chat_completion()` (from A1) as methods on it
3. Add `infer()`, `infer_stream()` methods that wrap the raw helpers
4. Change `RigProvider::OpenAiCompat { ... }` to `RigProvider::OpenAiCompat(OpenAiCompatProvider)`
5. Update all match arms (find/replace pattern)
6. Run: `cargo test --workspace --lib`
7. Commit: `refactor(provider): extract OpenAiCompatProvider struct`

---

### Task C2: Extract RigCoreProvider + dispatch_rig! macro

**Status**: `[ ]`
**Skills**: `/spn-rust:rust-architect` (macro design), `/spn-rust:rust-core` (enum dispatch),
  `/spn-powers:requesting-code-review` (AFTER — this is the riskiest commit)
**Research**: `ctx7 library rig-core "CompletionClient CompletionModel stream"` for trait bounds.
  Use `/spn-rust:rust-expand` to verify macro expansion if unsure.
**Files**: `nika-engine/src/provider/rig/mod.rs`

**Steps**:

1. Define `RigClient` enum with all 7 provider client variants
2. Define `ProviderFlags` struct + static `PROVIDER_FLAGS` table
3. Define `RigCoreProvider { client: RigClient, flags: &'static ProviderFlags }`
4. Define `dispatch_rig!` macro
5. Collapse 7 enum variants (Claude, OpenAI, ..., XAi) into `RigCore(RigCoreProvider)`
6. Rewrite all 17 match blocks using `dispatch_rig!`
7. Handle edge cases:
   - Vision: check `flags.supports_vision` before dispatch
   - Streaming: check `flags.is_anthropic` for content_block_delta parsing
   - Structured output: check `flags.supports_native_structured`
8. Run: `cargo test --workspace --lib` — ALL tests must pass
9. Commit: `refactor(provider): collapse 7 variants into RigCore with dispatch_rig! macro`

**This is the biggest single commit. Target: 2146 → ~700 lines.**

---

### Task C3: Unify agent loop

**Status**: `[ ]`
**Skills**: `/spn-rust:rust-async` (tokio patterns, mpsc channels in agent loop),
  `/spn-powers:requesting-code-review` (AFTER — second riskiest commit),
  `/spn-powers:systematic-debugging` if tests fail
**Research**: `ctx7 library rig-core "agent prompt tools stream"` for agent API.
**Files**:
- `nika-engine/src/runtime/rig_agent_loop/providers.rs`
- `nika-engine/src/runtime/rig_agent_loop/mod.rs`
- `nika-engine/src/runtime/executor/agent.rs`

**Steps**:

1. Add `pub async fn run(&mut self, provider: &RigProvider) -> Result<...>` to `RigAgentLoop`
2. Inside, dispatch to `dispatch_rig!` for RigCore, direct call for OpenAiCompat
3. Delete: `run_claude()`, `run_openai()`, `run_mistral()`, `run_groq()`, `run_deepseek()`,
   `run_gemini()`, `run_xai()`, `run_auto()`
4. Update `agent.rs` to call `loop.run(&provider)` instead of `loop.run_auto()`
5. Update `spawn.rs` to pass the parent provider to child agent loop
6. Run: `cargo test --workspace --lib`
7. Commit: `refactor(agent): unify agent loop, delete 8 run_*() methods`

---

### Task C4: Consolidate provider enums

**Status**: `[ ]`
**Files**:
- `nika-core/src/provider_name.rs`
- `nika-engine/src/provider/cost.rs`
- `nika-engine/src/provider/rig/mod.rs`

**Steps**:

1. Add `to_cost_id() -> Option<&'static str>` on `ProviderName`
2. Store `ProviderName` in `ProviderFlags` (replace string `id`)
3. Simplify `cost_provider_kind()` to a flags lookup
4. Run: `cargo test --workspace --lib`
5. Commit: `refactor(provider): consolidate ProviderName as single source of truth`

---

### Task C5: Structured HTTP errors

**Status**: `[ ]`
**Files**: `nika-engine/src/provider/rig/error.rs`, `mod.rs`

**Steps**:

1. Add `RigInferError::HttpError { endpoint: String, status: u16, body: String }`
2. Migrate `OpenAiCompatProvider` to use it (replace `PromptError(format!("HTTP {status}:..."))`)
3. Update error matching in executor (if any match on the string format)
4. Run: `cargo test --workspace --lib`
5. Commit: `refactor(provider): add structured HttpError variant`

---

## Session D — LSP Diagnostics (P2, post-launch)

### Task D1: Add diagnostics to nika-lsp-core

**Status**: `[ ]`
**Skills**: `/spn-powers:test-driven-development`, `/spn-rust:rust-core`
**Research**: `ctx7 library tower-lsp "publish diagnostics notification"` for LSP protocol.
  Also: `/find-docs` for LSP spec `textDocument/publishDiagnostics` notification format.
  Read nika-lsp/src/diagnostics.rs FIRST — it's the reference implementation (380 lines, 14 tests).
**Files**:
- NEW: `nika-lsp-core/src/handlers/diagnostics.rs`
- `nika-lsp-core/src/handlers/mod.rs` — add module
- `nika-lsp-core/src/handler.rs` — add trait method

**Steps**:

1. Read `parse/bridge.rs:86-102` — `collect_error_ranges()` produces `Vec<TextRange>`
2. Read `position.rs:80` — `LineIndex.to_lsp_position(offset)`
3. Create `DiagnosticEntry` struct (protocol-agnostic):
   ```rust
   pub struct DiagnosticEntry {
       pub range: (u32, u32, u32, u32), // start_line, start_char, end_line, end_char
       pub severity: DiagnosticSeverity,
       pub code: String,
       pub message: String,
   }
   ```
4. Implement `pub fn syntax_diagnostics(text: &str) -> Vec<DiagnosticEntry>`
5. Add `fn diagnostics(&self, text: &str) -> Vec<DiagnosticEntry>` to `LspHandler` trait
6. Wire into DefaultHandler
7. Add tests: valid YAML → 0 diagnostics, broken YAML → 1+ diagnostics with correct ranges
8. Commit: `feat(lsp): add syntax diagnostics to nika-lsp-core`

**Note**: Semantic diagnostics (AnalyzeError) stay in nika-lsp — they depend on nika-engine
which nika-lsp-core deliberately avoids.

---

## Provider Catalog After All Tasks

```
LLM (14): anthropic, openai, mistral, groq, deepseek, gemini, xai,
           openrouter, together, fireworks, cerebras, sambanova, cohere, ai21
MCP (11): neo4j, github, slack, perplexity, firecrawl, supadata,
           elevenlabs, ahrefs, context7, brave, notion
Local (2): native, mock
TOTAL: 27 providers
```

---

## Parallel Agent Strategy

For complex tasks, use `/spn-powers:dispatching-parallel-agents` to launch concurrent work:

### Session A Parallelism
- **Agent 1** (Explore): Read all provider API docs (OpenRouter, Together, Fireworks, Cerebras)
  and verify base URLs, env vars, model names are current
- **Agent 2** (rust-pro): Implement A1 (HTTP dedup) in worktree
- **Agent 3** (rust-pro): Implement A2 (ModelResolver) in worktree
- After both return: review, merge, test, commit sequentially

### Session B Parallelism
- **Agent 1**: Implement B1 (find_project_root) in worktree
- **Agent 2**: Implement B2 + B3 (infer.rs tests) in worktree
- **Agent 3** (code-reviewer): Review Session A commits against this plan

### Session C Parallelism
- **Agent 1** (rust-architect): Design and implement C1 (OpenAiCompatProvider struct)
- **Agent 2** (Explore): Deep-read all 17 match blocks in mod.rs, catalog every difference
- After C1 done: **Agent 3** (rust-pro): Implement C2 (dispatch_rig! macro)
- After C2 done: **Agent 4** (code-reviewer): Full review of C1+C2 before proceeding to C3

### When to Use Worktrees
Use `/spn-powers:using-git-worktrees` for:
- Any task that might break the build (C2, C3)
- Parallel agent work on the same files
- Experimentation with macro design

## Notes for Autonomous Execution

- Always read the target files BEFORE writing tests or code
- Run `cargo test --workspace --lib` after EVERY commit (no more --exclude nika-py)
- Provider catalog tests check exact counts — update them when adding providers
- The `dispatch_rig!` macro must work with all rig-core generic constraints
- OpenAiCompat providers need no new Rust code — just catalog + match arm
- When in doubt about architecture: keep the enum, use the macro
- The agent loop refactor (C3) is the riskiest commit — do it after C2 stabilizes
- Use `/spn-powers:verification-before-completion` before EVERY commit claim
- Use `/spn-search:search` to verify provider API details are current (APIs change)
- Use `ctx7 library <name> "<query>"` for rig-core, reqwest, tokio docs — never guess
- If a test fails unexpectedly: `/spn-powers:systematic-debugging` (4-phase framework)
- After Session C completes: `/spn-powers:requesting-code-review` on the full refactor

## Architecture Decision Records

If making significant architectural choices during execution, create ADRs in `nika/docs/adr/`:
- Use `/spn-writing:architecture-decision-records` skill
- Document: dispatch_rig! macro decision, enum vs trait object, config-driven providers
