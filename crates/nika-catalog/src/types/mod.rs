// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Type definitions for catalog entries.

pub mod builtin;
#[cfg(feature = "capabilities")]
pub mod capabilities;
pub mod category;
pub mod distribution;
pub mod embedding;
pub mod mcp_server;
#[cfg(feature = "capabilities")]
pub mod modality;
pub mod model;
#[cfg(feature = "capabilities")]
pub mod param_flag;
pub mod provider;
pub mod tags;
#[cfg(feature = "capabilities")]
pub mod tokenizer;
pub mod transform;

pub use builtin::{Builtin, BuiltinCategory};
pub use category::{Category, ParseCategoryError};
pub use distribution::{
    AuthMode, EnvVarSpec, McpPackage, McpRemote, PyRunner, RegistryType, Transport,
};
pub use embedding::{Embedding, Similarity};
pub use mcp_server::{McpPricing, McpServer};
#[cfg(feature = "capabilities")]
pub use modality::{Modality, ParseModalityError};
pub use model::{CostEstimate, ModelCapabilities, ModelPricing, TokenLimitParam};
#[cfg(feature = "capabilities")]
pub use param_flag::{ParamFlag, ParseParamFlagError};
pub use provider::{Provider, ProviderModel, validate_key_format};
pub use tags::{ParseTagError, Tag};
#[cfg(feature = "capabilities")]
pub use tokenizer::{ParseTokenizerFamilyError, TokenizerFamily};
pub use transform::{NullBehavior, TransformArity, TransformCategory, TransformDef};
