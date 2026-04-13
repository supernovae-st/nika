// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Type definitions for catalog entries.

pub mod builtin;
pub mod mcp_alias;
pub mod model;
pub mod provider;
pub mod transform;

pub use builtin::{Builtin, BuiltinCategory};
pub use mcp_alias::{McpAlias, McpPricing};
pub use model::{CostEstimate, ModelCapabilities, ModelPricing, TokenLimitParam};
pub use provider::Provider;
pub use transform::{NullBehavior, TransformArity, TransformCategory, TransformDef};
