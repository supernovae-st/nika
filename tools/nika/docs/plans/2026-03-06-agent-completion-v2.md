# Agent Completion Architecture v2.0

**Date**: 2026-03-06
**Version Target**: v0.21.0 - v0.25.0
**Status**: Implementation Plan

## Executive Summary

World-class agent termination architecture combining the best patterns from LangGraph, CrewAI, AutoGen, and OpenAI Swarm into Nika's YAML-first approach.

## Killer Features (Unique to Nika)

| Feature | Description | Competitors |
|---------|-------------|-------------|
| **Confidence Routing** | Agent self-evaluates, low confidence → escalate | None |
| **Cost Limits** | `max_cost_usd` with partial completion | None |
| **Programmatic Completion** | Config-driven, incassable | None |
| **YAML-First** | Declarative, versionable, no code | None |

## Architecture Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  EXECUTION PIPELINE                                                           ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  1. Agent runs with tools                                                     ║
║  2. Agent calls nika:complete(result, confidence)                             ║
║  3. Check: confidence >= threshold? ──NO──→ retry | escalate                  ║
║  4. Check: output matches schema? ──NO──→ retry (OutputPolicy)                ║
║  5. Check: guardrails pass? ──NO──→ retry with feedback                       ║
║  6. Check: limits ok? ──NO──→ terminate with partial                          ║
║  7. ✅ SUCCESS → task complete                                                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

## Schema Definition

### Phase 1: CompletionConfig (v0.21)

```yaml
completion:
  mode: explicit | natural | pattern

  signal:
    tool: nika:complete
    fields:
      required: [result]
      optional: [confidence, reason, sources]

  patterns:  # for mode: pattern
    - value: "COMPLETE"
      type: exact | contains | regex

  instruction:
    tone: concise | detailed
    lang: auto | en | fr
```

### Phase 2: ConfidenceConfig (v0.22)

```yaml
completion:
  confidence:
    threshold: 0.7
    on_low:
      action: retry | escalate | accept
      max_retries: 2
      feedback: "Confidence too low. Please verify sources."
    routing:
      high: { min: 0.85, action: accept }
      medium: { min: 0.7, action: accept_with_flag }
      low: { action: escalate, escalate_to: human }
```

### Phase 3: GuardrailsConfig (v0.23)

```yaml
guardrails:
  - id: min_length
    type: length
    min_words: 200
    on_fail:
      action: retry
      feedback: "Response too short. Minimum 200 words."

  - id: valid_schema
    type: schema
    schema: $schemas.report
    on_fail:
      action: retry

  - id: has_pattern
    type: regex
    pattern: "\\[SOURCE:\\d+\\]"
    on_fail:
      action: retry
      feedback: "Must include source citations [SOURCE:N]"

  chain: sequential | parallel
  max_retries: 3
```

### Phase 4: LimitsConfig (v0.24)

```yaml
limits:
  max_turns: 20
  max_tokens: 50000
  max_cost_usd: 2.00
  max_duration_secs: 300

  on_limit_reached:
    action: complete_partial | fail | escalate
    save_progress: true
```

### Phase 5: LLM Guardrails (v0.25)

```yaml
guardrails:
  - id: quality_check
    type: llm
    prompt: "Rate this response quality 1-10. Output just the number."
    pass_if: ">= 7"
    on_fail:
      action: retry
      feedback: "Quality score too low. Please improve."

  - id: factual_check
    type: llm
    prompt: "Are all claims factually verifiable? YES/NO"
    pass_if: "YES"
    on_fail:
      action: escalate
```

---

## Implementation Phases

### Phase 1: completion: mode + signal (v0.21)

**Files to create/modify:**

| File | Action | Description |
|------|--------|-------------|
| `src/ast/completion.rs` | CREATE | CompletionConfig, CompletionMode, SignalConfig structs |
| `src/ast/agent.rs` | MODIFY | Add `completion: Option<CompletionConfig>` field |
| `src/runtime/completion.rs` | CREATE | CompletionHandler trait + implementations |
| `src/runtime/rig_agent_loop.rs` | MODIFY | Integrate CompletionHandler |
| `src/runtime/builtin/complete.rs` | CREATE | nika:complete tool implementation |

**AST Structs:**

```rust
// src/ast/completion.rs

/// Completion configuration for agent tasks
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CompletionConfig {
    /// Completion mode
    #[serde(default)]
    pub mode: CompletionMode,

    /// Signal configuration (for mode: explicit)
    #[serde(default)]
    pub signal: Option<SignalConfig>,

    /// Pattern matching (for mode: pattern)
    #[serde(default)]
    pub patterns: Vec<PatternConfig>,

    /// Instruction generation settings
    #[serde(default)]
    pub instruction: Option<InstructionConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompletionMode {
    /// Agent must call nika:complete tool
    #[default]
    Explicit,
    /// Completes when no more tool calls
    Natural,
    /// Completes on pattern match (backward compat with stop_conditions)
    Pattern,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignalConfig {
    /// Tool to call for completion (default: nika:complete)
    #[serde(default = "default_signal_tool")]
    pub tool: String,

    /// Field requirements
    #[serde(default)]
    pub fields: SignalFields,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SignalFields {
    /// Required fields in completion call
    #[serde(default)]
    pub required: Vec<String>,

    /// Optional fields
    #[serde(default)]
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatternConfig {
    /// Pattern value
    pub value: String,

    /// Match type
    #[serde(default)]
    pub r#type: PatternType,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatternType {
    #[default]
    Exact,
    Contains,
    Regex,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstructionConfig {
    /// Tone of generated instruction
    #[serde(default)]
    pub tone: InstructionTone,

    /// Language for instruction
    #[serde(default)]
    pub lang: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstructionTone {
    #[default]
    Concise,
    Detailed,
}

fn default_signal_tool() -> String {
    "nika:complete".to_string()
}
```

**nika:complete Tool:**

```rust
// src/runtime/builtin/complete.rs

/// Result from nika:complete tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteResult {
    /// Final result from agent
    pub result: Value,

    /// Agent's confidence in the result (0.0-1.0)
    #[serde(default)]
    pub confidence: Option<f64>,

    /// Reason for completion
    #[serde(default)]
    pub reason: Option<String>,

    /// Sources used
    #[serde(default)]
    pub sources: Option<Vec<String>>,
}

pub struct NikaCompleteTool {
    completion_tx: mpsc::Sender<CompleteResult>,
}

impl ToolDyn for NikaCompleteTool {
    fn name(&self) -> &'static str {
        "nika:complete"
    }

    fn description(&self) -> &'static str {
        "Signal task completion with result and optional confidence score"
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "result": {
                    "description": "Final result of the task"
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "description": "Confidence level (0.0-1.0)"
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for completion"
                },
                "sources": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Sources used"
                }
            },
            "required": ["result"]
        })
    }
}
```

**Tests (TDD - Write First):**

```rust
// tests/completion_config_test.rs

#[test]
fn parse_completion_mode_explicit() {
    let yaml = r#"
completion:
  mode: explicit
  signal:
    tool: nika:complete
    fields:
      required: [result]
      optional: [confidence, reason]
"#;
    let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.mode, CompletionMode::Explicit);
    assert_eq!(config.signal.unwrap().tool, "nika:complete");
}

#[test]
fn parse_completion_mode_natural() {
    let yaml = r#"
completion:
  mode: natural
"#;
    let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.mode, CompletionMode::Natural);
}

#[test]
fn parse_completion_mode_pattern() {
    let yaml = r#"
completion:
  mode: pattern
  patterns:
    - value: "COMPLETE"
      type: exact
    - value: "\\[DONE:\\w+\\]"
      type: regex
"#;
    let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.mode, CompletionMode::Pattern);
    assert_eq!(config.patterns.len(), 2);
}

#[test]
fn completion_generates_system_instruction_concise() {
    let config = CompletionConfig {
        mode: CompletionMode::Explicit,
        signal: Some(SignalConfig {
            tool: "nika:complete".to_string(),
            fields: SignalFields {
                required: vec!["result".to_string()],
                optional: vec!["confidence".to_string()],
            },
        }),
        instruction: Some(InstructionConfig {
            tone: InstructionTone::Concise,
            lang: "en".to_string(),
        }),
        ..Default::default()
    };

    let instruction = config.generate_system_instruction();
    assert!(instruction.contains("nika:complete"));
    assert!(instruction.contains("result"));
    assert!(instruction.contains("confidence"));
}

#[test]
fn nika_complete_tool_validates_confidence_range() {
    // confidence must be 0.0-1.0
    let invalid = json!({
        "result": "done",
        "confidence": 1.5  // Invalid
    });

    let result = validate_complete_params(&invalid);
    assert!(result.is_err());
}

#[test]
fn nika_complete_tool_requires_result() {
    let missing_result = json!({
        "confidence": 0.8
    });

    let result = validate_complete_params(&missing_result);
    assert!(result.is_err());
}
```

**Tasks for Phase 1:**

- [ ] P1.1: Create `src/ast/completion.rs` with CompletionConfig structs
- [ ] P1.2: Add tests for YAML parsing (5 tests)
- [ ] P1.3: Add `completion` field to AgentParams
- [ ] P1.4: Create `src/runtime/builtin/complete.rs` (nika:complete tool)
- [ ] P1.5: Add system instruction generation
- [ ] P1.6: Integrate into RigAgentLoop
- [ ] P1.7: Add integration tests (3 tests)
- [ ] P1.8: Update schema to @0.11

---

### Phase 2: confidence: threshold + routing (v0.22)

**Files to create/modify:**

| File | Action | Description |
|------|--------|-------------|
| `src/ast/completion.rs` | MODIFY | Add ConfidenceConfig struct |
| `src/runtime/confidence.rs` | CREATE | ConfidenceRouter with routing logic |
| `src/event/log.rs` | MODIFY | Add ConfidenceRouted event |

**AST Structs:**

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfidenceConfig {
    /// Minimum confidence to accept
    #[serde(default = "default_confidence_threshold")]
    pub threshold: f64,

    /// Action when confidence is below threshold
    #[serde(default)]
    pub on_low: OnLowConfidenceConfig,

    /// Confidence-based routing (advanced)
    #[serde(default)]
    pub routing: Option<ConfidenceRouting>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OnLowConfidenceConfig {
    /// Action to take
    #[serde(default)]
    pub action: LowConfidenceAction,

    /// Max retries before escalating
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Feedback message for retry
    #[serde(default)]
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LowConfidenceAction {
    #[default]
    Retry,
    Escalate,
    Accept,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfidenceRouting {
    pub high: ConfidenceRoute,
    pub medium: ConfidenceRoute,
    pub low: ConfidenceRoute,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfidenceRoute {
    #[serde(default)]
    pub min: Option<f64>,
    pub action: RouteAction,
    #[serde(default)]
    pub escalate_to: Option<String>,
}

fn default_confidence_threshold() -> f64 {
    0.7
}

fn default_max_retries() -> u32 {
    2
}
```

**Tasks for Phase 2:**

- [ ] P2.1: Add ConfidenceConfig to completion.rs
- [ ] P2.2: Create `src/runtime/confidence.rs` with ConfidenceRouter
- [ ] P2.3: Add ConfidenceRouted event to EventLog
- [ ] P2.4: Integrate into RigAgentLoop completion flow
- [ ] P2.5: Add 8 unit tests for routing logic
- [ ] P2.6: Add 3 integration tests

---

### Phase 3: guardrails: basic types (v0.23)

**Files to create/modify:**

| File | Action | Description |
|------|--------|-------------|
| `src/ast/guardrails.rs` | CREATE | GuardrailConfig, GuardrailType structs |
| `src/runtime/guardrails/mod.rs` | CREATE | GuardrailRunner trait |
| `src/runtime/guardrails/length.rs` | CREATE | LengthGuardrail |
| `src/runtime/guardrails/schema.rs` | CREATE | SchemaGuardrail |
| `src/runtime/guardrails/regex.rs` | CREATE | RegexGuardrail |
| `src/event/log.rs` | MODIFY | Add GuardrailFailed, GuardrailPassed events |

**Tasks for Phase 3:**

- [ ] P3.1: Create `src/ast/guardrails.rs` with GuardrailConfig
- [ ] P3.2: Create guardrails runtime module structure
- [ ] P3.3: Implement LengthGuardrail (min/max words/chars)
- [ ] P3.4: Implement SchemaGuardrail (JSON Schema validation)
- [ ] P3.5: Implement RegexGuardrail (pattern matching)
- [ ] P3.6: Add guardrail events to EventLog
- [ ] P3.7: Integrate into completion flow
- [ ] P3.8: Add 15 tests

---

### Phase 4: limits: cost + partial (v0.24)

**Files to create/modify:**

| File | Action | Description |
|------|--------|-------------|
| `src/ast/limits.rs` | CREATE | LimitsConfig struct |
| `src/runtime/limits.rs` | CREATE | LimitTracker, CostCalculator |
| `src/event/log.rs` | MODIFY | Add LimitReached, PartialCompletion events |

**Tasks for Phase 4:**

- [ ] P4.1: Create `src/ast/limits.rs` with LimitsConfig
- [ ] P4.2: Create LimitTracker for turns/tokens/cost/duration
- [ ] P4.3: Add cost calculation per provider
- [ ] P4.4: Implement partial completion (save progress)
- [ ] P4.5: Add limit events to EventLog
- [ ] P4.6: Integrate into RigAgentLoop
- [ ] P4.7: Add 10 tests

---

### Phase 5: guardrails: llm type + escalation (v0.25)

**Files to create/modify:**

| File | Action | Description |
|------|--------|-------------|
| `src/runtime/guardrails/llm.rs` | CREATE | LlmGuardrail using secondary LLM call |
| `src/runtime/escalation.rs` | CREATE | EscalationHandler (human-in-the-loop) |
| `src/tui/views/approval.rs` | CREATE | Human approval view |

**Tasks for Phase 5:**

- [ ] P5.1: Implement LlmGuardrail with prompt/pass_if
- [ ] P5.2: Create EscalationHandler
- [ ] P5.3: Add TUI approval view (human-in-the-loop)
- [ ] P5.4: Integrate escalation into confidence routing
- [ ] P5.5: Add 12 tests

---

## Migration from stop_conditions

### Backward Compatibility

```yaml
# OLD (v0.20 and earlier)
agent:
  prompt: "Research {{topic}}"
  stop_conditions: ["COMPLETE", "DONE"]

# NEW (v0.21+) - auto-converted
agent:
  prompt: "Research {{topic}}"
  completion:
    mode: pattern
    patterns:
      - value: "COMPLETE"
        type: contains
      - value: "DONE"
        type: contains
```

**Migration function:**

```rust
impl AgentParams {
    /// Migrate legacy stop_conditions to completion config
    pub fn migrate_stop_conditions(&self) -> Option<CompletionConfig> {
        if self.stop_conditions.is_empty() {
            return None;
        }

        Some(CompletionConfig {
            mode: CompletionMode::Pattern,
            patterns: self.stop_conditions.iter().map(|s| PatternConfig {
                value: s.clone(),
                r#type: PatternType::Contains,
            }).collect(),
            ..Default::default()
        })
    }
}
```

---

## Event Additions

```rust
// New events for Agent Completion v2.0

/// Agent signaled completion via nika:complete
CompletionSignaled {
    task_id: String,
    confidence: Option<f64>,
    result_preview: String,
},

/// Confidence routing decision
ConfidenceRouted {
    task_id: String,
    confidence: f64,
    route: String,  // "high", "medium", "low"
    action: String, // "accept", "retry", "escalate"
},

/// Guardrail check result
GuardrailResult {
    task_id: String,
    guardrail_id: String,
    guardrail_type: String,
    passed: bool,
    feedback: Option<String>,
},

/// Limit reached during execution
LimitReached {
    task_id: String,
    limit_type: String,  // "turns", "tokens", "cost", "duration"
    value: f64,
    threshold: f64,
    action: String,
},

/// Partial completion (on limit)
PartialCompletion {
    task_id: String,
    progress: f64,  // 0.0-1.0
    result_preview: String,
},

/// Human escalation requested
EscalationRequested {
    task_id: String,
    reason: String,
    context: Value,
},
```

---

## Test Coverage Requirements

| Phase | Unit Tests | Integration Tests | Total |
|-------|------------|-------------------|-------|
| P1 | 12 | 3 | 15 |
| P2 | 10 | 3 | 13 |
| P3 | 15 | 5 | 20 |
| P4 | 10 | 3 | 13 |
| P5 | 12 | 4 | 16 |
| **Total** | **59** | **18** | **77** |

---

## Commit Strategy

Each phase gets multiple commits:

1. `feat(ast): add CompletionConfig structs`
2. `test(completion): add parsing tests`
3. `feat(runtime): implement nika:complete tool`
4. `feat(agent): integrate completion handler`
5. `test(integration): add completion flow tests`
6. `docs(schema): update to @0.11`

---

## Timeline Estimate

| Phase | Complexity | Estimate |
|-------|------------|----------|
| P1 | Medium | 2-3 days |
| P2 | Medium | 2-3 days |
| P3 | High | 3-4 days |
| P4 | Medium | 2-3 days |
| P5 | High | 3-4 days |
| **Total** | | **12-17 days** |
