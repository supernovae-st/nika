# Session F: Stringly-Typed Migration + Polish (~4-5h)

## Context
Nika workflow engine. 333+ string literals for provider names, extract modes, event types.
Architecture audit found this as root cause #2 for recurring bugs.

## Mission: Replace critical stringly-typed APIs with enums. Clean up remaining issues.

---

### Part 1: ExtractMode enum (~1h)

**Current**: `extract: Option<String>` in FetchParams. Validated at runtime with string matching.
**Target**: `ExtractMode` enum validated at parse time.

Create in `nika-core/src/ast/`:
```rust
#[derive(Debug, Clone, PartialEq, Eq, strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub enum ExtractMode {
    Markdown,
    Article,
    Text,
    Selector,
    Metadata,
    Links,
    Jsonpath,
    Feed,
    LlmTxt,
}
```

Update `RawFetchAction.extract` from `Option<String>` to `Option<ExtractMode>`.
Update all match arms in `executor/extract.rs` from string matching to enum matching.

### Part 2: ProviderName as first-class type (~1.5h)

`ProviderKind` exists in cost.rs but is only used for cost. Extend it:

```rust
// In nika-core/src/catalogs/providers.rs (or new file)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub enum ProviderName {
    Anthropic,
    OpenAI,
    Mistral,
    Groq,
    DeepSeek,
    Gemini,
    XAi,
    Native,
    Mock,
}
```

Replace `provider: Option<String>` in AST types with `Option<ProviderName>`.
This catches typos like `"claudee"` at parse time instead of runtime.

### Part 3: EventKind grouping (RC7) (~1.5h)

**Current**: 55-variant flat enum. Every consumer must match all.
**Target**: Nested enums by category.

```rust
pub enum EventKind {
    Workflow(WorkflowEvent),
    Task(TaskEvent),
    Agent(AgentEvent),
    Provider(ProviderEvent),
    Mcp(McpEvent),
    Media(MediaEvent),
    Guardrail(GuardrailEvent),
    Binding(BindingEvent),
    Fetch(FetchEvent),
    System(SystemEvent),
}

pub enum TaskEvent {
    Scheduled { task_id: Arc<str>, dependencies: Vec<String> },
    Started { task_id: Arc<str>, verb: String, inputs: Vec<String> },
    Completed { task_id: Arc<str>, output: Arc<Value>, duration_ms: u64 },
    Failed { task_id: Arc<str>, error: String, duration_ms: u64, error_code: Option<String> },
    Skipped { task_id: Arc<str>, reason: String },
    Retry { task_id: Arc<str>, attempt: u32, reason: String, delay_ms: u64 },
}
```

**WARNING**: This is a large refactor touching every event consumer (renderer, live, TUI, trace writer).
Do it methodically: one group at a time. Start with TaskEvent (most used), then ProviderEvent.

### Part 4: Update doc comment (~15min)

`log.rs:5` says "44 variants across 15 categories" — actual is 55 across 18.
Fix the comment. Add a test that counts variants (static assertion).

### Part 5: Remaining LOW bugs (~30min)

- L2: `compact` doesn't filter empty strings → add `s.is_empty()` check
- L3: `round` returns float, `ceil`/`floor` return int → normalize
- L5: `python3 -c` not in blocklist (should be done in Session A)
- L9: Stale comments in writer.rs
- Update CLAUDE.md error code ranges

---

## After All Fixes
1. `cargo test --workspace --lib` — ALL pass
2. `cargo clippy --workspace -- -D warnings` — 0 warnings
3. Zero string-matching for extract modes and provider names
4. EventKind grouped — no more 55-arm matches in consumers
