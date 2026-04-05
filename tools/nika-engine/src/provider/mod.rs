//! Provider abstraction layer
//!
//! ## Provider Strategy
//!
//! Nika uses [rig-core](https://github.com/0xPlaygrounds/rig) for LLM providers.
//!
//! | Component | Implementation |
//! |-----------|----------------|
//! | `agent:` verb | [`RigAgentLoop`](crate::runtime::RigAgentLoop) + rig-core |
//! | `infer:` verb | `RigProvider` + rig-core |
//! | Tool calling | `NikaMcpTool` (rig `ToolDyn`) |
//!
//! ## Cost Calculation
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
//! let result = agent.run().await?;
//! ```

pub mod cost;
pub mod endpoints;
pub mod rig;

// Re-export endpoint types
pub use endpoints::{CustomEndpointConfig, CustomEndpointMap, ResolvedEndpoint};

// Native inference via mistral.rs (requires native-inference feature)
#[cfg(feature = "native-inference")]
pub mod native;

// Re-export main types for convenience
pub use cost::{calculate_cost, format_cost, get_model_pricing, ModelPricing, ProviderKind};
pub use rig::{NikaMcpTool, RigProvider, StreamResult};

// Re-export native runtime when feature is enabled
#[cfg(feature = "native-inference")]
pub use native::{ChatOptions, ChatResponse, LoadConfig, ModelInfo, NativeRuntime};

// Re-export storage types for model management
#[cfg(feature = "native-inference")]
pub use native::{
    default_model_dir, DownloadRequest, HuggingFaceStorage, ModelStorage, PullProgress,
};
