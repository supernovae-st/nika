//! Runtime Module - Workflow execution
//!
//! Contains the runtime execution components:
//! - `runner`: DAG execution with tokio concurrency
//! - `executor`: Individual task execution (infer, exec, fetch, invoke, agent)
//!   - Includes decompose: modifier expansion
//! - `output`: Output format handling and schema validation
//! - `structured_output`: 4-layer StructuredOutputEngine for ~99.99% compliance
//! - `submit_tool`: DynamicSubmitTool for provider-native structured output (Layer 0)
//! - `rig_agent_loop`: Rig-based agentic execution
//! - `spawn`: Nested agent spawning
//! - `chat_workflow`: Chat-as-DAG wrapper
//! - `builtin`: Builtin nika:* tools
//! - `hitl`: Human-In-The-Loop handler trait
//! - `context_loader`: Context file loading at workflow start
//! - `resolver`: Agent and skill resolution
//! - `boot`: Boot sequence with 6-phase initialization
//! - `policy`: Security policy enforcement for exec/fetch/tokens
//! - `security`: Command validation and blocklist
//! - `artifact_processor`: Task output file persistence
//! - `limit_tracker`: Agent execution limits tracking
//! - `partial`: Partial completion checkpointing
//!
//! This module represents the "how" - runtime execution.
//! For static structure, see the `ast` module.

pub mod artifact_processor;
pub mod boot;
pub mod builtin;
pub mod chat_workflow;
pub mod context;
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
pub mod submit_tool;

// Re-export public types
pub use builtin::{
    AssertTool, BuiltinTool, BuiltinToolRouter, EmitTool, LogLevel, LogTool,
    NikaBuiltinToolAdapter, PromptParams, PromptResponse, PromptTool, RunParams, RunResponse,
    RunTool,
};
pub use chat_workflow::{ChatMessage, ChatWorkflow, Role};
pub use context::WorkflowMeta;
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

pub use boot::{
    BootContext, BootPhase, BootSequence, BootstrapConfig, EditorConfig, PhaseResult, PolicyConfig,
    ProviderConfig, SessionConfig, ToolsConfig, TraceConfig,
};
pub use policy::{PolicyDecision, PolicyEnforcer, TokenBudget};

pub use security::{check_blocklist, validate_command_string, validate_exec_command};

pub use artifact_processor::{process_task_artifacts, ArtifactProcessResult};

pub use structured_output::{
    validate_structured_output, InferCallback, StructuredOutputEngine, StructuredOutputResult,
};

pub use limit_tracker::LimitTracker;

pub use submit_tool::DynamicSubmitTool;

pub use partial::{PartialCheckpoint, PartialResult, StopReason};
