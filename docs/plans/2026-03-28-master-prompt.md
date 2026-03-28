# Master Prompt — Custom Endpoints Fix Session

Use this prompt with Claude Code to execute the fix plan. Copy-paste it as the initial message.

---

## PROMPT

```
I need you to fix critical bugs in the Nika workflow engine's custom endpoint/provider system. Use the skills: rust-pro, rust-architect, test-driven-development, systematic-debugging, verification-before-completion.

## Context

Nika is a Rust workflow engine (workspace at tools/). After deploying a VPS + H100 with vLLM (Qwen3.5-27B) + NLLB-200, 5 code review agents found critical bugs. The detailed plan is at:

docs/plans/2026-03-28-custom-endpoints-fix-plan.md

Read it FIRST before doing anything.

## Critical Fixes (do these in order)

### Fix 1.1: Box::leak memory leak in rig.rs

File: tools/nika-engine/src/provider/rig.rs

The `name()` method at ~line 460 uses `Box::leak` for OpenAiCompat variant, permanently leaking memory:
```rust
RigProvider::OpenAiCompat { endpoint_name, .. } => {
    Box::leak(format!("openai-compat:{}", endpoint_name).into_boxed_str())
}
```

Same issue at ~line 491 in `default_model()`.

**Fix**: Add a `cached_name: String` field to the `OpenAiCompat` variant. Set it during construction in `openai_compat()`. Return `&self.cached_name` from `name()`. The `name()` return type may need to change from `&'static str` — check all callsites. If changing the signature is too invasive, store as `&'static str` using a `once_cell::sync::Lazy` or string interner.

TDD: Write a test first that creates 100 OpenAiCompat providers and checks no leak.

### Fix 1.2: Cost $0.00 for custom endpoints

File: tools/nika-engine/src/provider/cost.rs + tools/nika-engine/src/runtime/executor/infer.rs

`ProviderKind::parse("qwen")` returns None for custom endpoint names. Cost is always 0.

**Fix**: Add logic that resolves custom endpoint names to their underlying provider kind. Since all custom endpoints use OpenAI-compatible API, treat them as `ProviderKind::OpenAi` for cost estimation. The best approach is to add a `cost_provider_kind()` method to `RigProvider` that returns the appropriate `ProviderKind` based on the variant.

TDD: Test that `RigProvider::OpenAiCompat` returns a non-zero cost for a known model.

### Fix 1.3: Wire custom endpoints in Runner

Files: tools/nika-engine/src/runtime/runner.rs, tools/nika/src/main.rs, tools/nika-tui/src/app/routing.rs, tools/nika-tui/src/lib.rs

The custom endpoints from config.toml are loaded by `NikaConfig` but NOT passed to the Runner/TaskExecutor. The executor's `custom_endpoints` is always empty when running from CLI/TUI.

**Fix**:
1. Add `custom_endpoints: Option<Arc<CustomEndpointMap>>` field to `Runner`
2. Add `Runner::with_endpoints(map: CustomEndpointMap) -> Self` builder
3. In runner.rs: pass `self.custom_endpoints` to `TaskExecutor::with_policy()`
4. In main.rs: load config, resolve endpoints, call `runner.with_endpoints()`
5. Same in TUI routing.rs and lib.rs

TDD: Integration test that creates a Runner with mock endpoints and verifies the executor receives them.

### Fix 1.4: Schema JSON sync

File: tools/nika/schemas/nika-workflow.schema.json

Already partially done (provider enum removed, base_url added at workflow level). Remaining:
- Rename `"imports"` to `"include"` in schema
- Add `base_url` to task-level properties
- Add `pkg` block to workflow properties
- Fix MCP server config (make `command` optional, add `url` and `transport`)
- Fix `oneOf` for tasks to allow verb-less (decompose-only) tasks
- Fix InvokeParams oneOf to allow `tool: "server::tool_name"` without explicit `mcp:`
- Sync to tools/nika-engine/schemas/ after changes

TDD: Write a test that loads 10 representative .nika.yaml files and validates against schema.

## Important Fixes (after critical)

### Fix 2.1: NIKA-035/036 error codes

File: tools/nika-engine/src/error_domains.rs, tools/nika-engine/src/error.rs

Add NikaError::EndpointNotFound (NIKA-035) and NikaError::EndpointConnectionFailed (NIKA-036) variants. Update the From<ProviderError> impl to preserve these codes instead of collapsing to NIKA-030/031.

### Fix 2.2: Strip <think> tags from responses

File: tools/nika-engine/src/runtime/executor/infer.rs

After collecting the streamed response, strip `<think>...</think>` blocks. Use regex or simple string parsing. This should be done for ALL providers (any model might emit thinking blocks).

### Fix 2.3: Add endpoint_url to ProviderCalled event

File: tools/nika-event/src/log.rs

Add `endpoint_url: Option<String>` to `ProviderCalled` and `ProviderResponded`. Populate from the resolved provider in infer.rs.

### Fix 2.5: Add provider/model to ProviderCallStat

File: tools/nika-engine/src/display/renderer.rs

Add `provider: String` and `model: String` fields to ProviderCallStat. Update the summary table to show a provider column.

## Rules

1. Read the plan at docs/plans/2026-03-28-custom-endpoints-fix-plan.md FIRST
2. Use TDD: write the test FIRST, see it fail, then implement
3. Run `cargo test -p nika-engine --lib` after each fix — ALL tests must pass
4. Run `cargo clippy --workspace -- -D warnings` at the end
5. Each fix = 1 commit (conventional commits: `fix(provider): ...`)
6. Co-author lines on every commit
7. Do NOT touch code unrelated to these fixes
8. If a fix is more complex than expected, STOP and explain before proceeding
```
