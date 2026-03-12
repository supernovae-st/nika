# Phase 1: Core Types Migration

## Overview

**Goal**: Migrate zero-dependency types from spn to nika's `src/core/` module.
**Lines**: ~500
**Types**: 15
**Tests**: 8

---

## Design Principles

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  ZERO-DEPENDENCY TYPES                                                        ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  These types have NO external dependencies beyond std + serde.                ║
║  They can be imported anywhere without pulling in async/tokio/network.        ║
║                                                                               ║
║  Pattern: Follow existing nika core types                                     ║
║  ├── src/core/providers.rs  → KNOWN_PROVIDERS                                 ║
║  ├── src/core/models.rs     → KNOWN_MODELS                                    ║
║  └── src/core/mcp_aliases.rs → MCP_ALIASES                                    ║
║                                                                               ║
║  New files:                                                                   ║
║  ├── src/core/autonomy.rs   → AutonomyLevel, ApprovalLevel, Decision          ║
║  └── src/core/agent_types.rs → AgentRole, AgentState, AgentStatus             ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Types to Migrate

### 1. Autonomy Types (`src/core/autonomy.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/autonomy.rs`

```rust
/// Level of autonomous behavior for the AI assistant.
/// Controls how much human approval is required for actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    /// Manual: All actions require explicit user approval
    Manual = 0,
    /// Assisted: Suggestions provided, user confirms each
    Assisted = 1,
    /// Semi-Autonomous: Low-risk actions auto-approved, high-risk need approval
    SemiAutonomous = 2,
    /// Autonomous: Most actions auto-approved, only critical need approval
    Autonomous = 3,
    /// Full: All actions auto-approved (dangerous - use with caution)
    Full = 4,
}

impl Default for AutonomyLevel {
    fn default() -> Self {
        Self::Assisted // Safe default
    }
}

impl AutonomyLevel {
    /// Check if this level allows auto-approval for a given action risk
    pub fn allows_auto_approval(&self, risk: RiskLevel) -> bool {
        match (self, risk) {
            (Self::Manual, _) => false,
            (Self::Assisted, _) => false,
            (Self::SemiAutonomous, RiskLevel::Low) => true,
            (Self::SemiAutonomous, _) => false,
            (Self::Autonomous, RiskLevel::Critical) => false,
            (Self::Autonomous, _) => true,
            (Self::Full, _) => true,
        }
    }
}

/// Risk level for an action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Approval requirement for an action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalLevel {
    /// No approval needed
    None,
    /// Silent approval (logged but not prompted)
    Silent,
    /// Explicit user approval required
    Explicit,
    /// Multiple confirmations required (for destructive actions)
    MultiStep,
}

/// Decision made by the autonomy system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Unique identifier for this decision
    pub id: String,
    /// What action was being considered
    pub action: String,
    /// The outcome of the decision
    pub outcome: DecisionOutcome,
    /// When the decision was made
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Reasoning for the decision (optional)
    pub reasoning: Option<String>,
}

/// Outcome of an autonomy decision
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcome {
    Approved,
    Denied,
    Deferred,
    TimedOut,
}
```

### 2. Agent Types (`src/core/agent_types.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/agents.rs`

```rust
use std::time::Duration;

/// Role of an agent in the multi-agent system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// Primary agent interacting with user
    Primary,
    /// Worker agent handling delegated tasks
    Worker,
    /// Supervisor coordinating other agents
    Supervisor,
    /// Specialist agent for specific domains
    Specialist,
}

impl Default for AgentRole {
    fn default() -> Self {
        Self::Worker
    }
}

/// Current state of an agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Agent is initializing
    Initializing,
    /// Agent is idle, waiting for work
    Idle,
    /// Agent is actively working on a task
    Working,
    /// Agent is waiting for external input (HITL, MCP, etc.)
    Waiting,
    /// Agent has completed its task
    Completed,
    /// Agent encountered an error
    Failed,
    /// Agent was cancelled
    Cancelled,
}

impl AgentState {
    /// Check if agent is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Check if agent is active (not terminal, not idle)
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Initializing | Self::Working | Self::Waiting)
    }
}

/// Status report for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    /// Agent identifier
    pub id: AgentId,
    /// Current state
    pub state: AgentState,
    /// Role in the system
    pub role: AgentRole,
    /// Current task (if any)
    pub current_task: Option<String>,
    /// Number of completed tasks
    pub completed_tasks: usize,
    /// Time spent working
    pub total_work_time: Duration,
    /// Depth in agent hierarchy (0 = root)
    pub depth: usize,
    /// Parent agent (if spawned by another)
    pub parent_id: Option<AgentId>,
}

/// Unique identifier for an agent
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn generate() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(format!("agent-{:x}", timestamp))
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Task delegated to an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegatedTask {
    /// Unique task identifier
    pub id: String,
    /// Target agent
    pub agent_id: AgentId,
    /// Task prompt/description
    pub prompt: String,
    /// Context data (JSON)
    pub context: Option<serde_json::Value>,
    /// Maximum turns for this task
    pub max_turns: usize,
    /// Priority (higher = more important)
    pub priority: u8,
    /// When the task was created
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

### 3. Memory Types (`src/core/memory_types.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/memory.rs`

```rust
/// Namespace for memory entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryNamespace {
    /// User preferences and settings
    Preferences,
    /// Command execution history
    CommandHistory,
    /// Project-specific context
    ProjectContext,
    /// Conversation summaries
    ConversationSummary,
    /// Usage analytics
    Analytics,
}

impl MemoryNamespace {
    /// All namespace variants
    pub const ALL: &'static [Self] = &[
        Self::Preferences,
        Self::CommandHistory,
        Self::ProjectContext,
        Self::ConversationSummary,
        Self::Analytics,
    ];

    /// Get the string key for this namespace
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preferences => "preferences",
            Self::CommandHistory => "command_history",
            Self::ProjectContext => "project_context",
            Self::ConversationSummary => "conversation_summary",
            Self::Analytics => "analytics",
        }
    }
}

/// Key for a memory entry
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryKey {
    pub namespace: MemoryNamespace,
    pub key: String,
}

impl MemoryKey {
    pub fn new(namespace: MemoryNamespace, key: impl Into<String>) -> Self {
        Self {
            namespace,
            key: key.into(),
        }
    }

    /// Create a preferences key
    pub fn preference(key: impl Into<String>) -> Self {
        Self::new(MemoryNamespace::Preferences, key)
    }

    /// Create a project context key
    pub fn project(key: impl Into<String>) -> Self {
        Self::new(MemoryNamespace::ProjectContext, key)
    }
}

impl std::fmt::Display for MemoryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.namespace.as_str(), self.key)
    }
}
```

### 4. Trace Types (`src/core/trace_types.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/traces.rs`

```rust
/// Kind of reasoning trace step
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceStepKind {
    /// Observation of current state
    Observation,
    /// Reasoning about what to do
    Reasoning,
    /// Planning future actions
    Planning,
    /// Executing an action
    Execution,
    /// Evaluating results
    Evaluation,
    /// Tool call
    ToolCall,
    /// Tool result
    ToolResult,
    /// Error encountered
    Error,
}

impl TraceStepKind {
    /// Get emoji for this step kind (for TUI display)
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Observation => "👁️",
            Self::Reasoning => "🧠",
            Self::Planning => "📋",
            Self::Execution => "⚡",
            Self::Evaluation => "🔍",
            Self::ToolCall => "🔧",
            Self::ToolResult => "📤",
            Self::Error => "❌",
        }
    }
}

/// Unique identifier for a trace
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub String);

impl TraceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn generate() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(format!("trace-{:x}", timestamp))
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Metadata for a reasoning trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetadata {
    /// Unique identifier
    pub id: TraceId,
    /// Associated workflow (if any)
    pub workflow_id: Option<String>,
    /// Associated task (if any)
    pub task_id: Option<String>,
    /// When the trace started
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// When the trace ended (if complete)
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Total token count
    pub total_tokens: u64,
    /// Model used
    pub model: Option<String>,
}
```

---

## File Structure

```
src/core/
├── mod.rs              # Updated with new exports
├── providers.rs        # ✅ Existing
├── models.rs           # ✅ Existing
├── mcp_aliases.rs      # ✅ Existing
├── mcp_config.rs       # ✅ Existing
├── autonomy.rs         # 🆕 AutonomyLevel, ApprovalLevel, Decision, RiskLevel
├── agent_types.rs      # 🆕 AgentRole, AgentState, AgentStatus, AgentId, DelegatedTask
├── memory_types.rs     # 🆕 MemoryNamespace, MemoryKey
└── trace_types.rs      # 🆕 TraceStepKind, TraceId, TraceMetadata
```

---

## TDD Implementation Plan

### Step 1: Create Test Files First

```rust
// tests/core_autonomy_test.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autonomy_level_ordering() {
        assert!(AutonomyLevel::Manual < AutonomyLevel::Full);
        assert!(AutonomyLevel::Assisted < AutonomyLevel::Autonomous);
    }

    #[test]
    fn autonomy_level_default_is_safe() {
        assert_eq!(AutonomyLevel::default(), AutonomyLevel::Assisted);
    }

    #[test]
    fn auto_approval_rules() {
        let level = AutonomyLevel::SemiAutonomous;
        assert!(level.allows_auto_approval(RiskLevel::Low));
        assert!(!level.allows_auto_approval(RiskLevel::Critical));

        let level = AutonomyLevel::Manual;
        assert!(!level.allows_auto_approval(RiskLevel::Low));
    }

    #[test]
    fn agent_state_terminal_check() {
        assert!(AgentState::Completed.is_terminal());
        assert!(AgentState::Failed.is_terminal());
        assert!(!AgentState::Working.is_terminal());
    }

    #[test]
    fn agent_id_generation() {
        let id1 = AgentId::generate();
        let id2 = AgentId::generate();
        assert_ne!(id1, id2);
        assert!(id1.0.starts_with("agent-"));
    }

    #[test]
    fn memory_key_display() {
        let key = MemoryKey::preference("theme");
        assert_eq!(key.to_string(), "preferences:theme");
    }

    #[test]
    fn trace_step_kind_emoji() {
        assert_eq!(TraceStepKind::Reasoning.emoji(), "🧠");
        assert_eq!(TraceStepKind::Error.emoji(), "❌");
    }

    #[test]
    fn serde_roundtrip() {
        let level = AutonomyLevel::SemiAutonomous;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"semi_autonomous\"");
        let parsed: AutonomyLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, level);
    }
}
```

### Step 2: Implement Types (RED → GREEN)

1. Create `src/core/autonomy.rs` - watch tests fail
2. Implement types - tests pass
3. Create `src/core/agent_types.rs` - watch tests fail
4. Implement types - tests pass
5. Repeat for memory_types.rs and trace_types.rs

### Step 3: Update mod.rs Exports

```rust
// src/core/mod.rs
pub mod providers;
pub mod models;
pub mod mcp_aliases;
pub mod mcp_config;
pub mod autonomy;      // 🆕
pub mod agent_types;   // 🆕
pub mod memory_types;  // 🆕
pub mod trace_types;   // 🆕

// Re-exports for convenience
pub use providers::*;
pub use models::*;
pub use mcp_aliases::*;
pub use mcp_config::*;
pub use autonomy::*;      // 🆕
pub use agent_types::*;   // 🆕
pub use memory_types::*;  // 🆕
pub use trace_types::*;   // 🆕
```

---

## Validation Checklist

- [ ] All types compile without external dependencies (only std, serde, chrono)
- [ ] Serde serialize/deserialize works with snake_case
- [ ] Default implementations are safe/conservative
- [ ] Display implementations are human-readable
- [ ] Hash/Eq implementations for use in HashMap keys
- [ ] All tests pass
- [ ] Zero clippy warnings
- [ ] Documentation comments on all public items

---

## Dependencies

No new dependencies. Uses existing:
- `serde` (already in Cargo.toml)
- `serde_json` (already in Cargo.toml)
- `chrono` (already in Cargo.toml)

---

## Migration Notes

### From spn-core

The original types in `spn-core` were used by both `spn` and `nika`. After this migration:

1. `nika` uses its own types in `src/core/`
2. `spn` continues using `spn-core` (deprecated)
3. `spn-client` (IPC library) remains separate for backward compatibility

### Breaking Changes

None - these are new types in nika. No existing nika code uses them yet.

---

## Estimated Effort

| Task | Hours |
|------|-------|
| Write test files | 1 |
| Implement autonomy.rs | 1 |
| Implement agent_types.rs | 1 |
| Implement memory_types.rs | 0.5 |
| Implement trace_types.rs | 0.5 |
| Update mod.rs | 0.25 |
| Documentation | 0.5 |
| **Total** | **~5 hours** |
