# Session F: Stringly-Typed Migration (~4-5h)

## Context

Nika workflow engine. **916 string literals for provider names**, **417 for extract modes**,
**92 for event turn/status strings** across 116/79/40 files respectively.
Root cause RC2 from v0.51 master quality plan.

`strum = "0.27"` and `derive_more = "2.0"` are already in `[workspace.dependencies]`
but **zero crates currently use them**. This session activates both.

## Mission

Replace critical stringly-typed APIs with enums. Compile-time safety for provider names,
extract modes, response modes, event kinds, guardrail types, and finish reasons.

---

## Inventory: Every String That Should Be An Enum

### Provider names (916 occurrences, 116 files)

| File | Count | Context |
|------|-------|---------|
| `nika-core/catalogs/providers.rs` | 30 | `KNOWN_PROVIDERS` static list, `id:` fields |
| `nika-engine/provider/cost.rs` | 23 | `ProviderKind` enum + `parse()` + pricing tables |
| `nika-engine/provider/rig.rs` | 51 | Provider routing, client construction |
| `nika-engine/ast/tests_200_workflows.rs` | 28 | Test assertions |
| `nika-engine/runtime/executor/tests.rs` | 84 | Test fixtures |
| `nika-tui/providers/icons.rs` | 46 | Icon mapping by provider string |
| `nika-tui/widgets/provider_modal/**` | 79 | Provider modal UI |
| `nika-cli/provider.rs` | 31 | CLI provider commands |
| `nika-daemon/services/secrets.rs` | 17 | Keychain per-provider |
| All other files | ~527 | Scattered |

**Values**: `"anthropic"`, `"openai"`, `"mistral"`, `"groq"`, `"deepseek"`, `"gemini"`, `"xai"`, `"native"`, `"mock"`

### Extract modes (417 occurrences, 79 files)

| File | Count | Context |
|------|-------|---------|
| `nika-engine/runtime/executor/extract.rs` | 31 | 9 `Some("mode")` arms in `apply_extract()` |
| `nika-engine/runtime/executor/tests_extract_e2e.rs` | 64 | E2E extract tests |
| `nika-engine/ast/tests_200_workflows.rs` | 70 | Workflow parse assertions |
| `nika-mcp/types.rs` | 30 | MCP type mapping |
| `nika-media/tools/css_select.rs` | 18 | CSS selector tool |
| `nika-engine/runtime/executor/fetch.rs` | 3 | `extract.as_deref() == Some("llm_txt")` |
| All other files | ~201 | LSP, init, display, core |

**Values**: `"markdown"`, `"article"`, `"text"`, `"selector"`, `"metadata"`, `"links"`, `"jsonpath"`, `"feed"`, `"llm_txt"`

### Response modes (5 occurrences, 3 files)

Fetch `response:` field: `"full"`, `"binary"`, `None` (default = raw body text).

| File | Count |
|------|-------|
| `nika-engine/runtime/executor/fetch.rs` | 2 |
| `nika-engine/runtime/executor/tests_wiremock.rs` | 3 |

### Agent turn kind (92 occurrences, 40 files)

`AgentTurn.kind: String` in `EventKind::AgentTurn`.

**Values found**: `"started"`, `"continue"`, `"natural_completion"`, `"explicit_completion"`

| File | Count |
|------|-------|
| `nika-engine/runtime/rig_agent_loop/providers.rs` | 6x `"started"` |
| `nika-engine/runtime/rig_agent_loop/chat.rs` | 1x `"started"` |
| `nika-engine/runtime/rig_agent_loop/thinking.rs` | 1x `"started"` |
| TUI agent_steps.rs | 2 (comment docs) |

### Finish reason (20+ occurrences)

`ProviderResponded.finish_reason: String` and `AgentTurnMetadata.stop_reason: String`.

**finish_reason values**: `"stop"`, `"end_turn"`, `"tool_use"`, `"mock"`, `"structured_output_retry"`, `"structured_output_repair"`

**stop_reason values**: `"end_turn"`, `"natural_completion"`, `"max_turns"`, `"low_confidence_retry"`, `"guardrail_retry"`, `"natural"`

### Guardrail type (20+ occurrences)

`GuardrailPassed/Failed/Escalation.guardrail_type: String`

**Values**: `"length"`, `"schema"`, `"regex"`, `"llm"`

### Escalation severity (8 occurrences)

`GuardrailEscalation.severity: String`

**Values**: `"low"`, `"medium"`, `"high"`, `"critical"`

### Log level (in `EventKind::Log`)

**Values**: `"trace"`, `"debug"`, `"info"`, `"warn"`, `"error"`

### Boot phase name (in `EventKind::BootPhaseCompleted`)

**Values**: `"config_discovery"`, `"config_validation"`, `"memory_loading"`, `"secrets_loading"`, `"mcp_startup"`, `"provider_validation"`, `"ready"`

### Native model kind (in `EventKind::NativeModelLoaded`)

**Values**: `"gguf"`, `"huggingface"`

---

## Part 1: ExtractMode Enum (~1h)

### Current state

`extract: Option<String>` in:
- `nika-core/src/ast/analyzed/task.rs:221` (AnalyzedFetchAction)
- `nika-engine/src/ast/action.rs:371` (FetchParams)
- `nika-engine/src/runtime/executor/extract.rs:13` (function parameter `Option<&str>`)

The `apply_extract()` function in `extract.rs` does **9 string matches** with `#[cfg(feature)]` guards.
Feature-gated fallbacks add **4 more** string matches for disabled features.
The `"unknown"` catch-all returns a runtime error with a hardcoded list of valid modes.

### Target

```rust
// nika-core/src/ast/schema.rs (or new file nika-core/src/ast/extract.rs)
use serde::{Deserialize, Serialize};

/// Post-processing extraction mode for the fetch: verb.
///
/// Each mode may require a Cargo feature flag at compile time.
/// Serde deserializes from snake_case YAML strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(strum::EnumString, strum::Display, strum::EnumIter)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ExtractMode {
    /// Clean Markdown from HTML (feature: fetch-markdown)
    Markdown,
    /// Main article content via Readability (feature: fetch-article)
    Article,
    /// Visible text, optionally filtered by CSS selector (feature: fetch-html)
    Text,
    /// Raw HTML of matching elements (feature: fetch-html)
    Selector,
    /// OG, Twitter Cards, JSON-LD, SEO metadata (feature: fetch-html)
    Metadata,
    /// Rich link classification (feature: fetch-html)
    Links,
    /// JSONPath query on JSON responses (zero deps)
    Jsonpath,
    /// RSS/Atom/JSON Feed parsing (feature: fetch-feed)
    Feed,
    /// AI content discovery /.well-known/llm.txt
    #[serde(rename = "llm_txt")]
    #[strum(serialize = "llm_txt")]
    LlmTxt,
}

impl ExtractMode {
    /// Check if this mode requires a specific feature flag.
    pub fn required_feature(&self) -> Option<&'static str> {
        match self {
            Self::Markdown => Some("fetch-markdown"),
            Self::Article => Some("fetch-article"),
            Self::Text | Self::Selector | Self::Metadata | Self::Links => Some("fetch-html"),
            Self::Feed => Some("fetch-feed"),
            Self::Jsonpath | Self::LlmTxt => None,
        }
    }
}
```

### Migration steps

1. Create `ExtractMode` enum in `nika-core/src/ast/` (re-export from `lib.rs`)
2. Add `strum.workspace = true` to `nika-core/Cargo.toml`
3. Change `AnalyzedFetchAction.extract` from `Option<String>` to `Option<ExtractMode>` in `nika-core`
4. Change `FetchParams.extract` from `Option<String>` to `Option<ExtractMode>` in `nika-engine`
5. Update `apply_extract()` signature from `Option<&str>` to `Option<ExtractMode>`
6. Replace all `Some("markdown")` arms with `Some(ExtractMode::Markdown)` etc.
7. Remove the `Some(unknown) =>` catch-all (now impossible via type system)
8. Update `ast/lower.rs` to map string-in-YAML to `ExtractMode` (serde handles it)
9. Update all tests from `Some("markdown".to_string())` to `Some(ExtractMode::Markdown)`
10. Update LSP completion to use `ExtractMode::iter()` for completions

### ResponseMode enum (do at the same time)

```rust
/// Response output mode for the fetch: verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    /// JSON with status, headers, body, final URL
    Full,
    /// Store raw bytes in CAS, return hash
    Binary,
}
```

Change `FetchParams.response` from `Option<String>` to `Option<ResponseMode>`.
Only 5 occurrences to update.

### Backward compatibility

**YAML files continue to work unchanged.** Serde's `#[serde(rename_all = "snake_case")]`
deserializes `extract: markdown` directly into `ExtractMode::Markdown`. No migration needed
for existing `.nika.yaml` files.

Test this explicitly:
```rust
#[test]
fn extract_mode_yaml_compat() {
    let yaml = r#"extract: markdown"#;
    let mode: ExtractMode = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(mode, ExtractMode::Markdown);
}

#[test]
fn extract_mode_llm_txt_yaml_compat() {
    let yaml = r#"extract: llm_txt"#;
    let mode: ExtractMode = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(mode, ExtractMode::LlmTxt);
}
```

---

## Part 2: ProviderName as First-Class Type (~1.5h)

### Current state

Two overlapping types exist:

1. **`ProviderKind`** in `nika-engine/src/provider/cost.rs:25` — 8 variants (Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, XAi, Native). Manual `parse()` with aliases (claude, gpt, google, grok, x-ai). Manual `name()` and `to_provider_id()`. **No Mock variant.**

2. **`KNOWN_PROVIDERS`** in `nika-core/src/catalogs/providers.rs:68` — 19 `Provider` structs with `id: &'static str`. Includes MCP providers (neo4j, github, slack...) and local (native). **No enum.**

3. **`provider: Option<String>`** appears in 11 struct fields across nika-core, nika-engine, nika-tui.

### Decision: Extend ProviderKind, not new type

`ProviderKind` already has the right shape. Plan:

1. **Move** `ProviderKind` from `nika-engine/provider/cost.rs` to `nika-core/src/catalogs/providers.rs`
2. **Add** `Mock` variant
3. **Add** `Custom(String)` variant for custom endpoints (openai-compat)
4. **Derive** `strum::EnumString`, `strum::Display` with aliases
5. **Add** `Serialize`/`Deserialize` for YAML roundtrip
6. **Re-export** from `nika-engine` to avoid breaking internal APIs
7. Delete the manual `parse()`, `name()`, `to_provider_id()` — strum generates these

### Target

```rust
// nika-core/src/catalogs/providers.rs

/// LLM provider identifier.
///
/// Covers the 7 cloud providers, 1 local, 1 mock, and custom endpoints.
/// Custom endpoints are parsed as `Custom("endpoint-name")` and route through
/// OpenAI-compatible API.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderName {
    Anthropic,
    #[serde(alias = "gpt")]
    OpenAI,
    Mistral,
    Groq,
    DeepSeek,
    #[serde(alias = "google")]
    Gemini,
    #[serde(alias = "grok")]
    XAi,
    Native,
    Mock,
    /// Custom OpenAI-compatible endpoint from config.toml
    #[serde(untagged)]
    Custom(String),
}

impl ProviderName {
    /// Map to cost calculation enum (for backward compat during migration)
    pub fn to_cost_kind(&self) -> Option<ProviderKind> {
        match self {
            Self::Anthropic => Some(ProviderKind::Claude),
            Self::OpenAI | Self::Custom(_) => Some(ProviderKind::OpenAI),
            Self::Mistral => Some(ProviderKind::Mistral),
            Self::Groq => Some(ProviderKind::Groq),
            Self::DeepSeek => Some(ProviderKind::DeepSeek),
            Self::Gemini => Some(ProviderKind::Gemini),
            Self::XAi => Some(ProviderKind::XAi),
            Self::Native => Some(ProviderKind::Native),
            Self::Mock => None,
        }
    }

    /// Canonical lowercase ID for config/env lookups
    pub fn canonical_id(&self) -> &str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAI => "openai",
            Self::Mistral => "mistral",
            Self::Groq => "groq",
            Self::DeepSeek => "deepseek",
            Self::Gemini => "gemini",
            Self::XAi => "xai",
            Self::Native => "native",
            Self::Mock => "mock",
            Self::Custom(name) => name.as_str(),
        }
    }

    /// Is this a cloud provider requiring an API key?
    pub fn requires_api_key(&self) -> bool {
        !matches!(self, Self::Native | Self::Mock | Self::Custom(_))
    }
}
```

### Serde challenge: Custom(String)

The `#[serde(untagged)]` on `Custom` means serde first tries all named variants,
then falls back to `Custom(String)` for unknown names. This gives us:
- `provider: anthropic` -> `ProviderName::Anthropic`
- `provider: h100` -> `ProviderName::Custom("h100".to_string())`

**Test this explicitly:**
```rust
#[test]
fn provider_name_yaml_compat() {
    assert_eq!(serde_yaml::from_str::<ProviderName>("anthropic").unwrap(), ProviderName::Anthropic);
    assert_eq!(serde_yaml::from_str::<ProviderName>("mock").unwrap(), ProviderName::Mock);
    // Custom endpoint falls through
    assert_eq!(serde_yaml::from_str::<ProviderName>("h100").unwrap(), ProviderName::Custom("h100".into()));
}
```

**Note:** `#[serde(untagged)]` on a single variant inside a non-untagged enum may need
a custom deserializer. If serde can't handle it natively, implement `Deserialize` manually
with a try-from-str approach:

```rust
impl<'de> Deserialize<'de> for ProviderName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "anthropic" | "claude" => Ok(Self::Anthropic),
            "openai" | "gpt" => Ok(Self::OpenAI),
            "mistral" => Ok(Self::Mistral),
            "groq" => Ok(Self::Groq),
            "deepseek" | "deep-seek" => Ok(Self::DeepSeek),
            "gemini" | "google" => Ok(Self::Gemini),
            "xai" | "grok" | "x-ai" => Ok(Self::XAi),
            "native" => Ok(Self::Native),
            "mock" => Ok(Self::Mock),
            other => Ok(Self::Custom(other.to_string())),
        }
    }
}
```

### Migration steps

1. Create `ProviderName` enum in `nika-core/src/catalogs/providers.rs`
2. Keep `ProviderKind` in `nika-engine/provider/cost.rs` for now (cost-only, internal)
3. Add `ProviderName::to_cost_kind()` bridge
4. Change `AnalyzedWorkflow.provider` from `Option<String>` to `Option<ProviderName>`
5. Change `AnalyzedTask.provider` from `Option<String>` to `Option<ProviderName>`
6. Change `FetchParams.provider` (if exists), `LoadedWorkflow.provider`, etc.
7. Update `RigProvider::auto()` to accept `ProviderName` instead of `&str`
8. Update `nika-tui` provider modal to use `ProviderName` instead of string matching
9. Update all tests

### What about `ProviderKind` in cost.rs?

Keep it. `ProviderKind` is cost-domain specific (has `Claude` not `Anthropic`, handles model
pricing). `ProviderName` is the user-facing YAML type. Bridge with `to_cost_kind()`.
Long-term: merge them (Session G or later).

---

## Part 3: Small Enums (~45min)

### 3a: GuardrailType

```rust
// nika-core or nika-event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum GuardrailType {
    Length,
    Schema,
    Regex,
    Llm,
}
```

Replace `guardrail_type: String` in `GuardrailPassed`, `GuardrailFailed`, `GuardrailEscalation`.
**20+ occurrences** across `log.rs`, `display/tests.rs`, `rig_agent_loop/thinking.rs`.

### 3b: Severity

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[derive(strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}
```

Replace `severity: String` in `GuardrailEscalation`. **8 occurrences**.

### 3c: AgentTurnKind

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AgentTurnKind {
    Started,
    Continue,
    NaturalCompletion,
    ExplicitCompletion,
}
```

Replace `kind: String` in `EventKind::AgentTurn`. **8+ occurrences** in providers.rs.

### 3d: FinishReason

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    Mock,
    StructuredOutputRetry,
    StructuredOutputRepair,
    /// Unknown reason from provider
    #[serde(other)]
    Unknown,
}
```

Replace `finish_reason: String` in `ProviderResponded`. **20+ occurrences**.

### 3e: AgentStopReason

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AgentStopReason {
    EndTurn,
    NaturalCompletion,
    MaxTurns,
    LowConfidenceRetry,
    GuardrailRetry,
    Natural,
}
```

Replace `stop_reason: String` in both `AgentComplete` and `AgentTurnMetadata`.

### Where to define these

Small enums used **only in events** go in `nika-event/src/types.rs` (new file).
Enums used in **AST + events** go in `nika-core/src/ast/`.

| Enum | Crate |
|------|-------|
| `ExtractMode` | nika-core (used in AST + runtime) |
| `ResponseMode` | nika-core (used in AST + runtime) |
| `ProviderName` | nika-core (used everywhere) |
| `GuardrailType` | nika-event (event-only, but also used in AST guardrails) -> nika-core |
| `Severity` | nika-event (event-only) |
| `AgentTurnKind` | nika-event (event-only) |
| `FinishReason` | nika-event (event-only) |
| `AgentStopReason` | nika-event (event-only) |

---

## Part 4: EventKind Grouping (RC7) (~1.5h)

### Current state

**55 variants** in a single flat `EventKind` enum. Doc comment says "44 variants across 15 categories" -- stale.

**Consumers** (files that match on `EventKind::*`):

| Consumer | Matches | Style |
|----------|---------|-------|
| `nika-engine/display/renderer.rs` (CliRenderer) | 73 | Exhaustive, 3 `_ => {}` catch-alls |
| `nika-engine/display/live.rs` (LiveRenderer) | 98 | Exhaustive match |
| `nika-engine/display/tests.rs` | 36 | Test assertions |
| `nika-tui/state/event_handler/mod.rs` | 59 | Exhaustive, dispatches to 5 sub-modules |
| `nika-tui/state/event_handler/{workflow,task,agent,provider,telemetry}.rs` | ~40 | Individual handlers |
| `nika-tui/state/tests.rs` | 131 | Test event construction |
| `nika-tui/state/workflow_ops.rs` | 5 | Workflow-level ops |
| `nika-tui/views/chat/mouse.rs` | 5 | Chat event handling |
| `nika-tui/test_helpers.rs` | 10 | Test factories |
| `nika-event/src/log.rs` (task_id, is_workflow_event) | 55+ | Helper methods |

### Grouping design

Based on the TUI's existing 5-module split, which is the natural grouping:

```rust
// nika-event/src/log.rs

/// All possible event types, grouped by category.
///
/// 55 variants across 10 groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Workflow(WorkflowEvent),
    Task(TaskEvent),
    Provider(ProviderEvent),
    Mcp(McpEvent),
    Agent(AgentEvent),
    Guardrail(GuardrailEvent),
    Binding(BindingEvent),
    Media(MediaEvent),
    Fetch(FetchEvent),
    System(SystemEvent),
}
```

#### WorkflowEvent (6 variants)
```
WorkflowStarted, WorkflowCompleted, WorkflowFailed,
WorkflowAborted, WorkflowPaused, WorkflowResumed
```

#### TaskEvent (7 variants)
```
TaskScheduled, TaskStarted, TaskCompleted, TaskFailed,
TaskSkipped, TaskRetry, TemplateResolved
```

#### ProviderEvent (5 variants)
```
ProviderCalled, ProviderResponded, ProviderInitialized,
StreamingDelta, FallbackTriggered
```

#### McpEvent (5 variants)
```
McpInvoke, McpResponse, McpConnected, McpError, McpRetry
```

#### AgentEvent (4 variants)
```
AgentStart, AgentTurn, AgentComplete, AgentSpawned
```

#### GuardrailEvent (3 variants)
```
GuardrailPassed, GuardrailFailed, GuardrailEscalation
```

#### BindingEvent (3 variants)
```
BindingDefaultApplied, BindingTransformApplied, BindingEnvResolved
```

#### MediaEvent (7 variants)
```
MediaExtracted, MediaProcessed, MediaStored, MediaStoreFailed,
MediaIntegrityCheck, MediaCleanup, VisionContentResolved
```

#### FetchEvent (5 variants)
```
HttpRequest, HttpResponse, FetchRetry, ExtractApplied, ContextAssembled
```

#### SystemEvent (10 variants)
```
BootPhaseCompleted, NativeModelLoaded, BuiltinToolInvoked,
Log, Custom, ArtifactWritten, ArtifactFailed,
StructuredOutputAttempt, StructuredOutputSuccess,
ExecCompleted, PolicyBlocked, DecomposeStarted, DecomposeCompleted,
ForEachStarted, ForEachCompleted
```

### Incremental strategy

**Do NOT refactor all 55 variants at once.** Migrate one group at a time.

#### Phase 1: Extract `TaskEvent` (most used, highest ROI)

1. Create `TaskEvent` enum with 7 variants
2. Add `EventKind::Task(TaskEvent)` variant
3. Keep old `EventKind::TaskStarted { .. }` etc. as `#[deprecated]` aliases
4. Update TUI `event_handler/task.rs` first (5 matches)
5. Update `renderer.rs` (5 matches)
6. Update `live.rs` (5 matches)
7. Remove deprecated aliases
8. Run tests

#### Phase 2: Extract `AgentEvent` (contained scope)

Same pattern. 4 variants, 3 consumers.

#### Phase 3: Extract remaining groups

`ProviderEvent`, `McpEvent`, `GuardrailEvent`, `BindingEvent`, `MediaEvent`,
`FetchEvent`, `SystemEvent`, `WorkflowEvent`.

### Consumer migration pattern

Before:
```rust
EventKind::TaskStarted { task_id, verb, inputs } => {
    self.on_task_started(task_id, verb, inputs);
}
```

After:
```rust
EventKind::Task(TaskEvent::Started { task_id, verb, inputs }) => {
    self.on_task_started(task_id, verb, inputs);
}
```

Or with group-level dispatch:
```rust
EventKind::Task(te) => self.handle_task_event(te, timestamp_ms),
```

The TUI already uses this pattern -- its `handle_event()` dispatches to `on_task_started()` etc.
The nested match is a 1:1 transformation.

### Helper methods update

`EventKind::task_id()` and `EventKind::is_workflow_event()` need updating.
With groups, this becomes cleaner:

```rust
impl EventKind {
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::Task(te) => te.task_id(),
            Self::Agent(ae) => ae.task_id(),
            Self::Provider(pe) => pe.task_id(),
            Self::Mcp(me) => me.task_id(),
            // etc.
            Self::Workflow(_) => None,
        }
    }

    pub fn is_workflow_event(&self) -> bool {
        matches!(self, Self::Workflow(_))
    }
}
```

### Serde compatibility

Current: `#[serde(tag = "type", rename_all = "snake_case")]`
produces `{"type": "task_started", "task_id": "..."}`.

After grouping, we need the same JSON shape for trace compatibility.
Options:
1. Custom `Serialize`/`Deserialize` that flattens groups
2. Use `#[serde(untagged)]` on outer enum + `#[serde(tag = "type")]` on inner enums

**Recommended**: Custom serde. The trace NDJSON format is a contract. We cannot break it.

```rust
impl Serialize for EventKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Task(te) => te.serialize(serializer),
            Self::Agent(ae) => ae.serialize(serializer),
            // Each inner enum already has #[serde(tag = "type")]
        }
    }
}
```

**WARNING**: This is the hardest part. Get serde right FIRST, then migrate consumers.
Write a roundtrip test for every variant before touching any consumer code.

---

## Part 5: strum + derive_more Integration Points

### Current manual impls that strum replaces

| Location | Manual impl | strum replacement |
|----------|-------------|-------------------|
| `ProviderKind::parse()` (cost.rs:41-66) | 15-line if/else chain | `#[derive(EnumString)]` + `#[strum(serialize = "...")]` for aliases |
| `ProviderKind::name()` (cost.rs:70-80) | 9-arm match | `#[derive(Display)]` |
| `ProviderKind::to_provider_id()` (cost.rs:85-96) | 9-arm match | `#[strum(to_string = "...")]` or keep manual (different from Display) |
| `HttpMethod::parse()` (analyzed/task.rs:245) | 8-arm match | `#[derive(EnumString)]` |
| Extract mode validation (action.rs:418) | Hardcoded string list | `ExtractMode::iter().map(|m| m.to_string())` |
| LSP completions for extract/provider | Hardcoded string arrays | `strum::IntoEnumIterator` |

### derive_more opportunities

| Location | Current | derive_more |
|----------|---------|-------------|
| `NikaError` variants | Manual `Display` impl (96+ variants) | `#[derive(Display)]` with format strings |
| Domain sub-enums in `error_domains.rs` | Manual `From` impls | `#[derive(From)]` |
| Newtype wrappers | Manual trait impls | `#[derive(From, Into, Display)]` |

### Activation plan

1. Add `strum.workspace = true` to `nika-core/Cargo.toml` and `nika-event/Cargo.toml`
2. Add `derive_more.workspace = true` to `nika-engine/Cargo.toml` (for error refactor, future session)
3. Do NOT activate derive_more for errors in this session -- that is RC3 work (Session G)

---

## Part 6: Remaining LOW Bugs (~30min)

- **L2**: `compact` transform doesn't filter empty strings -- add `s.is_empty()` check in `nika-core/src/binding/transform.rs`
- **L3**: `round` returns float, `ceil`/`floor` return int -- normalize all to same type
- **L5**: `python3 -c` not in blocklist (if not already done in Session A)
- **L9**: Stale comments in `nika-event/src/log.rs:5` -- update "44 variants" to actual count
- Update CLAUDE.md error code ranges if any changed

---

## Part 7: Update Doc Comment + Static Assertion

```rust
// nika-event/src/log.rs
// After grouping:

/// Count of EventKind groups
#[cfg(test)]
mod static_checks {
    use super::*;

    #[test]
    fn event_kind_variant_count() {
        // Update this when adding/removing variants.
        // Forces deliberate acknowledgment of event changes.
        let count = 55; // Total variants across all groups
        let groups = 10;
        // If this fails, update the count AND the doc comment.
        assert_eq!(count, 55, "EventKind variant count changed -- update doc comment");
        assert_eq!(groups, 10, "EventKind group count changed -- update doc comment");
    }
}
```

With strum's `EnumIter`, we can make this automatic:
```rust
use strum::IntoEnumIterator;
assert_eq!(TaskEvent::iter().count(), 7);
```

---

## Part 8: E2E Verification Workflows

### 8a: Provider routing verification

```yaml
# tests/workflows/e2e_provider_routing.nika.yaml
schema: "nika/workflow@0.12"
workflow: provider-routing-e2e
description: "Verify provider enum migration doesn't break routing"

tasks:
  - id: mock_provider
    provider: mock
    model: test-model
    infer: "Test mock provider"

  - id: mock_anthropic_alias
    provider: anthropic
    model: claude-sonnet-4-20250514
    infer: "Test anthropic provider (will fail without key, but should parse)"
    # NOTE: This task tests PARSING, not execution.
    # Run with --dry-run to verify.

  - id: mock_openai_alias
    provider: openai
    model: gpt-4o
    infer: "Test openai provider parsing"
```

Run: `nika check tests/workflows/e2e_provider_routing.nika.yaml`

### 8b: Extract mode verification

```yaml
# tests/workflows/e2e_extract_modes.nika.yaml
schema: "nika/workflow@0.12"
workflow: extract-modes-e2e
description: "Verify all 9 extract modes parse correctly"
provider: mock

tasks:
  - id: fetch_markdown
    fetch:
      url: "https://example.com"
      extract: markdown

  - id: fetch_article
    fetch:
      url: "https://example.com"
      extract: article

  - id: fetch_text
    fetch:
      url: "https://example.com"
      extract: text

  - id: fetch_selector
    fetch:
      url: "https://example.com"
      extract: selector
      selector: "main article"

  - id: fetch_metadata
    fetch:
      url: "https://example.com"
      extract: metadata

  - id: fetch_links
    fetch:
      url: "https://example.com"
      extract: links

  - id: fetch_jsonpath
    fetch:
      url: "https://api.example.com/data"
      extract: jsonpath
      selector: "$.items[0].name"

  - id: fetch_feed
    fetch:
      url: "https://example.com/feed.xml"
      extract: feed

  - id: fetch_llm_txt
    fetch:
      url: "https://example.com"
      extract: llm_txt
```

Run: `nika check tests/workflows/e2e_extract_modes.nika.yaml`

### 8c: Agent + events verification

```yaml
# tests/workflows/e2e_agent_events.nika.yaml
schema: "nika/workflow@0.12"
workflow: agent-events-e2e
description: "Verify agent events still work after enum migration"
provider: mock
model: test-model

tasks:
  - id: agent_task
    agent:
      prompt: "Say hello"
      max_turns: 2
      completion:
        mode: natural
      guardrails:
        - type: length
          min_words: 1
          max_words: 100
          on_failure: retry
```

Run: `nika run tests/workflows/e2e_agent_events.nika.yaml --provider mock`

### 8d: Rust-level E2E tests

```rust
#[test]
fn extract_mode_roundtrip_yaml() {
    for mode in ExtractMode::iter() {
        let yaml = serde_yaml::to_string(&mode).unwrap();
        let parsed: ExtractMode = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(mode, parsed, "Roundtrip failed for {mode}");
    }
}

#[test]
fn provider_name_roundtrip_yaml() {
    for name in &["anthropic", "openai", "mock", "native", "h100-custom"] {
        let parsed: ProviderName = serde_yaml::from_str(name).unwrap();
        let reserialized = serde_yaml::to_string(&parsed).unwrap().trim().to_string();
        // Known providers roundtrip exactly; custom endpoints roundtrip as string
    }
}

#[test]
fn event_kind_serde_compat() {
    // Verify that EventKind serializes to the same JSON format as before grouping.
    // Load a golden NDJSON trace file and verify it still deserializes.
}
```

---

## Part 9: Backward Compatibility Analysis

### Will existing .nika.yaml files break?

**No.** Here is why:

| Field | Before | After | YAML value | Works? |
|-------|--------|-------|------------|--------|
| `provider:` | `Option<String>` | `Option<ProviderName>` | `anthropic` | Yes (serde deserializes string to enum) |
| `extract:` | `Option<String>` | `Option<ExtractMode>` | `markdown` | Yes (serde with rename_all = snake_case) |
| `extract:` | `Option<String>` | `Option<ExtractMode>` | `llm_txt` | Yes (explicit `#[serde(rename)]`) |
| `response:` | `Option<String>` | `Option<ResponseMode>` | `full` | Yes (serde) |
| `response:` | `Option<String>` | `Option<ResponseMode>` | `binary` | Yes (serde) |
| `provider:` custom | `Option<String>` | `Option<ProviderName>` | `h100` | Yes (Custom fallback) |

### What could break?

1. **NDJSON trace files** -- `EventKind` serde format. Must maintain `{"type": "task_started"}` shape.
2. **MCP tool responses** that return provider names as strings -- bridge with `ProviderName::canonical_id()`
3. **Config.toml** endpoint names -- `ProviderName::Custom` handles these
4. **Tests comparing `.to_string()` output** -- `strum::Display` format may differ from manual impl

### Mitigation

- Write golden serde tests BEFORE migration
- Run `cargo test --workspace --lib` after every Part
- Run `nika check` on all test workflows
- Keep manual `to_provider_id()` during migration, deprecate after

---

## Execution Order

| # | Part | Time | Risk | Dependencies |
|---|------|------|------|-------------|
| 1 | ExtractMode + ResponseMode enum | 45min | LOW | None |
| 2 | ProviderName enum | 1h | MEDIUM | None (can parallel with 1) |
| 3 | Small enums (guardrail, severity, turn kind, finish reason) | 45min | LOW | None |
| 4 | EventKind grouping Phase 1 (TaskEvent) | 45min | HIGH | Part 3 done |
| 5 | EventKind grouping Phase 2-3 (remaining) | 45min | HIGH | Part 4 done |
| 6 | Doc comment + static assertions | 15min | LOW | Part 5 done |
| 7 | LOW bugs (compact, round, python3, stale comments) | 30min | LOW | None |
| 8 | E2E workflows + backward compat tests | 30min | LOW | Parts 1-5 done |

**Total: ~5h**

---

## After All Fixes

1. `cargo test --workspace --lib` -- ALL pass
2. `cargo clippy --workspace -- -D warnings` -- 0 warnings
3. Zero `Option<String>` for extract modes, response modes, provider names in AST types
4. Zero `Some("markdown")` pattern matches in runtime code
5. Zero `"started".to_string()` for agent turn kinds
6. EventKind grouped into 10 sub-enums
7. NDJSON trace format unchanged (golden test proves it)
8. All existing `.nika.yaml` files parse without changes
9. strum activated in nika-core + nika-event
10. `nika check` passes on all E2E verification workflows

---

## Risk Matrix

| Risk | Impact | Mitigation |
|------|--------|------------|
| Serde format change breaks traces | HIGH | Golden NDJSON roundtrip test |
| Custom endpoint deserialization edge case | MEDIUM | Manual `Deserialize` impl with tests |
| EventKind grouping breaks TUI | HIGH | Phase incrementally, test each group |
| strum version conflict with rig-core | LOW | Already in workspace deps, version pinned |
| `llm_txt` serde rename edge case | LOW | Explicit `#[serde(rename = "llm_txt")]` |
