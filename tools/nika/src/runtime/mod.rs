//! Runtime Module - Workflow execution (v0.24.0)
//!
//! Contains the runtime execution components:
//! - `runner`: DAG execution with tokio concurrency
//! - `executor`: Individual task execution (infer, exec, fetch, invoke, agent)
//!   - Includes decompose: modifier expansion (v0.5 MVP 8 Phase 4)
//! - `output`: Output format handling and schema validation
//! - `structured_output`: 4-layer StructuredOutputEngine for ~99.99% compliance (v0.21)
//! - `rig_agent_loop`: Rig-based agentic execution (v0.3+)
//! - `spawn`: Nested agent spawning (v0.5 MVP 8 Phase 2)
//! - `chat_workflow`: Chat-as-DAG wrapper (v0.9.1)
//! - `builtin`: Builtin nika:* tools (v0.9.3)
//! - `hitl`: Human-In-The-Loop handler trait (v0.10.0)
//! - `context_loader`: Context file loading at workflow start (v0.14.2 Schema @0.9)
//! - `resolver`: Agent and skill resolution (v0.13 Schema @0.6)
//! - `boot`: Boot sequence with 6-phase initialization (v0.13.1)
//! - `policy`: Security policy enforcement for exec/fetch/tokens (v0.13.1)
//! - `security`: Command validation and blocklist (v0.15.0)
//! - `artifact_processor`: Task output file persistence (v0.18.0)
//! - `limit_tracker`: Agent execution limits tracking (v0.24)
//! - `partial`: Partial completion checkpointing (v0.24)
//!
//! This module represents the "how" - runtime execution.
//! For static structure, see the `ast` module.

pub mod artifact_processor;
pub mod boot;
pub mod builtin;
pub mod chat_workflow;
pub mod context_loader;
mod executor;
pub mod hitl;
pub mod limit_tracker;
pub mod output;
pub mod partial;
pub mod policy;
pub mod resolver;
mod rig_agent_loop;
mod runner;
pub mod security;
mod skill_injector;
pub mod spawn;
pub mod structured_output;

// Re-export public types
pub use builtin::{
    AssertTool, BuiltinTool, BuiltinToolRouter, EmitTool, LogLevel, LogTool,
    NikaBuiltinToolAdapter, PromptParams, PromptResponse, PromptTool, RunParams, RunResponse,
    RunTool,
};
pub use chat_workflow::{ChatMessage, ChatWorkflow, Role};
pub use context_loader::{load_context, LoadedContext};
pub use executor::TaskExecutor;
pub use hitl::{DefaultHitlHandler, HitlError, HitlHandler, HitlRequest, HitlResponse};
pub use output::make_task_result;
pub use resolver::{
    resolve_assets, AgentSource, ResolvedAgent, ResolvedAgents, ResolvedAssets, ResolvedSkills,
};
pub use rig_agent_loop::{RigAgentLoop, RigAgentLoopResult, RigAgentStatus};
pub use runner::Runner;
pub use skill_injector::SkillInjector;
pub use spawn::{SpawnAgentParams, SpawnAgentTool};

// v0.13.1: Boot sequence and policy enforcement
pub use boot::{
    BootContext, BootPhase, BootSequence, EditorConfig, NikaConfig, PhaseResult, PolicyConfig,
    ProviderConfig, SessionConfig, ToolsConfig, TraceConfig,
};
pub use policy::{PolicyDecision, PolicyEnforcer, TokenBudget};

// v0.15.0: Security module for exec command validation
pub use security::{check_blocklist, validate_command_string, validate_exec_command};

// v0.18.0: Artifact processor for task output persistence
pub use artifact_processor::{process_task_artifacts, ArtifactProcessResult};

// v0.21.0: Structured output engine for JSON Schema compliance
// v0.24.0: Added InferCallback for real Layer 3 & 4 LLM calls
pub use structured_output::{
    validate_structured_output, InferCallback, StructuredOutputEngine, StructuredOutputResult,
};

// v0.24.0: Agent execution limits tracking
pub use limit_tracker::LimitTracker;

// v0.24.0: Partial completion for limit-stopped tasks
pub use partial::{PartialCheckpoint, PartialResult, StopReason};
