// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tool-definition source — the agent loop's catalog seam.
//!
//! The `agent` verb hands the model its whitelisted tools as
//! [`ToolDef`]s (name · description · parameter schema); the names alone
//! are not enough. This trait is WHERE those definitions come from:
//! the wiring layer implements it over the builtin catalog (`nika:*`)
//! and, later, live MCP `tools/list` (`mcp:server/*`). The consumer
//! filters the returned universe against its own whitelist — glob
//! semantics stay with the whitelist owner, enumeration stays here.
//!
//! Resolves the `nika-verb-agent` §8 blocker (the missing seam found
//! 2026-06-11): a kernel trait, same pattern as every other effect
//! seam (`trait_variant` Send-future `Dyn` twin · typed error ·
//! mock in `nika-kernel-mock`).

use nika_error::codes::NIKA_234;
use nika_error::prelude::*;

use crate::provider::ToolDef;

/// Typed error for tool-definition enumeration (NIKA-234 · tools range ·
/// constant registry-owned in `nika_error::codes` per the ai-sibling
/// convention — vision/audio/memory precedent · `lookup()` resolves it).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ToolDefsError {
    /// The definitions backend is unavailable (catalog unloadable ·
    /// MCP server unreachable for `tools/list`).
    #[error("tool definitions unavailable: {reason}")]
    #[diagnostic(code(nika::kernel::tool_defs_unavailable))]
    Unavailable {
        /// Why the source could not enumerate.
        reason: String,
    },
}

impl NikaErrorCode for ToolDefsError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::Unavailable { .. } => NIKA_234,
        }
    }

    fn is_transient(&self) -> bool {
        match self {
            // An unreachable MCP server MAY recover, but the kernel
            // cannot distinguish that from a permanently missing catalog
            // — terminal by default, the agent's retry policy decides.
            Self::Unavailable { .. } => false,
        }
    }
}

/// Source of model-facing tool definitions.
///
/// Implementations enumerate EVERY tool they can offer; consumers
/// (the agent loop) filter against their whitelist. Enumeration MUST be
/// side-effect free and SHOULD be cheap (called once per agent run).
#[trait_variant::make(ToolDefinitionProviderDyn: Send)]
pub trait ToolDefinitionProvider: Send + Sync {
    /// Every tool definition this source can offer a model.
    ///
    /// CANCEL SAFETY: read-only enumeration — dropping the future has
    /// no side effects.
    ///
    /// # Errors
    ///
    /// [`ToolDefsError::Unavailable`] when the backing source cannot
    /// enumerate (catalog unloadable · MCP server unreachable).
    async fn tool_defs(&self) -> Result<Vec<ToolDef>, ToolDefsError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_maps_to_the_registry_code() {
        let err = ToolDefsError::Unavailable {
            reason: "mcp server down".to_owned(),
        };
        assert_eq!(err.nika_code().num, 234);
        assert!(!err.is_transient());
        assert!(err.to_string().contains("mcp server down"));
    }
}
