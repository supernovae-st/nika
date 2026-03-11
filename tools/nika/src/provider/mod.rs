//! Provider abstraction layer
//!
//! ## Provider Strategy (v0.4)
//!
//! Nika uses [rig-core](https://github.com/0xPlaygrounds/rig) for LLM providers.
//!
//! | Component | Implementation |
//! |-----------|----------------|
//! | `agent:` verb | [`RigAgentLoop`](crate::runtime::RigAgentLoop) + rig-core |
//! | `infer:` verb | `RigProvider` + rig-core |
//! | Tool calling | `NikaMcpTool` (rig `ToolDyn`) |
//!
//! ## Cost Calculation (v0.24)
//!
//! The `cost` module provides pricing tables and cost calculation for all providers.
//!
//! ```rust,ignore
//! use nika::provider::cost::{calculate_cost, ProviderKind};
//!
//! let cost = calculate_cost(ProviderKind::Claude, "claude-sonnet-4-6", 1000, 500);
//! println!("Cost: ${:.4}", cost);
//! ```
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

pub mod cost;
pub mod rig;

// Native inference via spn-native (requires native-inference feature)
#[cfg(feature = "native-inference")]
pub mod native;

// Re-export main types for convenience
pub use cost::{calculate_cost, format_cost, get_model_pricing, ModelPricing, ProviderKind};
pub use rig::{NikaMcpTool, RigProvider, StreamResult};

// Re-export native runtime when feature is enabled (v0.26: NativeRuntime replaces NativeClient)
#[cfg(feature = "native-inference")]
pub use native::{NativeRuntime, ChatOptions, ChatResponse, LoadConfig, ModelInfo};

// Backwards compatibility alias (deprecated in v0.26)
#[cfg(feature = "native-inference")]
#[allow(deprecated)]
pub use native::NativeClient;
