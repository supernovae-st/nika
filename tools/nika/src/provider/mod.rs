//! Provider abstraction layer
//!
//! ## Provider Strategy (v0.4)
//!
//! Nika uses [rig-core](https://github.com/0xPlaygrounds/rig) for LLM providers.
//!
//! | Component | Implementation |
//! |-----------|----------------|
//! | `agent:` verb | [`RigAgentLoop`](crate::runtime::RigAgentLoop) + rig-core |
//! | `infer:` verb | [`RigProvider`](rig::RigProvider) + rig-core |
//! | Tool calling | [`NikaMcpTool`](rig::NikaMcpTool) (rig `ToolDyn`) |
//!
//! ## Example
//!
//! ```rust,ignore
//! use nika::runtime::RigAgentLoop;
//! use nika::ast::AgentParams;
//! use nika::event::EventLog;
//!
//! let params = AgentParams {
//!     prompt: "Generate a landing page".to_string(),
//!     mcp: vec!["novanet".to_string()],
//!     max_turns: Some(5),
//!     ..Default::default()
//! };
//! let mut agent = RigAgentLoop::new("task-1".into(), params, EventLog::new(), mcp_clients)?;
//! let result = agent.run_claude().await?;
//! ```

pub mod rig;

// Re-export main types for convenience
pub use rig::{NikaMcpTool, RigProvider};
