// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Type definitions for catalog entries.

pub mod builtin;
pub mod category;
pub mod credential;
pub mod distribution;
pub mod mcp_alias;
pub mod mcp_server;
pub mod model;
pub mod provider;
pub mod provider_v3;
pub mod transform;

pub use builtin::{Builtin, BuiltinCategory};
pub use category::{Category, ParseCategoryError};
pub use credential::Credential;
pub use distribution::{AuthMode, EnvVarSpec, McpPackage, McpRemote, PyRunner, RegistryType, Transport};
pub use mcp_alias::{McpAlias, McpPricing};
pub use mcp_server::McpServer;
pub use model::{CostEstimate, ModelCapabilities, ModelPricing, TokenLimitParam};
pub use provider::Provider;
pub use provider_v3::{ProviderDef, ProviderModel};
pub use transform::{NullBehavior, TransformArity, TransformCategory, TransformDef};
