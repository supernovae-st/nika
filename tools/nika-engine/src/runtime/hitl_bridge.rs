// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! HitlBridge — adapts the engine's [`HitlHandler`] to the kernel's
//! [`HitlPrompt`] trait so that [`nika_builtin::PromptTool`] can be
//! constructed with a concrete HITL implementation.
//!
//! # Usage
//!
//! ```ignore
//! use crate::runtime::hitl_bridge::HitlBridge;
//! use nika_builtin::PromptTool;
//! use std::sync::Arc;
//!
//! let handler: Arc<dyn HitlHandler> = /* TUI or CLI handler */;
//! let bridge = Arc::new(HitlBridge::new(handler));
//! let prompt_tool = PromptTool::new(bridge);
//! ```

use super::hitl::{HitlHandler, HitlRequest};
use async_trait::async_trait;
use nika_kernel::builtin::BuiltinError;
use nika_kernel::scope::HitlPrompt;
use std::sync::Arc;

/// Adapts a [`HitlHandler`] (engine-level, returns [`HitlResponse`]) into
/// the kernel [`HitlPrompt`] trait (returns a plain `String`).
///
/// Used by [`BuiltinToolRouter::with_hitl()`] to inject a TUI or CLI
/// HITL handler into `PromptTool` without coupling nika-builtin to
/// nika-engine types.
///
/// [`HitlResponse`]: crate::runtime::hitl::HitlResponse
pub struct HitlBridge {
    inner: Arc<dyn HitlHandler>,
}

impl HitlBridge {
    /// Wrap a [`HitlHandler`] as a [`HitlPrompt`].
    pub fn new(handler: Arc<dyn HitlHandler>) -> Self {
        Self { inner: handler }
    }
}

#[async_trait]
impl HitlPrompt for HitlBridge {
    async fn ask(&self, message: &str, default: Option<&str>) -> Result<String, BuiltinError> {
        let mut request = HitlRequest::new(message);
        if let Some(d) = default {
            request = request.with_default(d);
        }
        self.inner
            .prompt(request)
            .await
            .map(|r| r.response)
            .map_err(|e| BuiltinError::Other {
                tool: "nika:prompt".into(),
                reason: format!("HITL handler error: {e}"),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::hitl::{HitlError, HitlResponse};

    struct MockHandler {
        response: String,
    }

    #[async_trait]
    impl HitlHandler for MockHandler {
        async fn prompt(&self, _req: HitlRequest) -> Result<HitlResponse, HitlError> {
            Ok(HitlResponse::new(&self.response))
        }
    }

    struct ErrorHandler;

    #[async_trait]
    impl HitlHandler for ErrorHandler {
        async fn prompt(&self, _req: HitlRequest) -> Result<HitlResponse, HitlError> {
            Err(HitlError::Cancelled)
        }
    }

    #[tokio::test]
    async fn test_bridge_returns_handler_response() {
        let bridge = HitlBridge::new(Arc::new(MockHandler {
            response: "handler_says_yes".to_string(),
        }));
        let result = bridge.ask("Continue?", None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "handler_says_yes");
    }

    #[tokio::test]
    async fn test_bridge_passes_default_to_handler() {
        // The handler receives the request (with default) and returns its own response.
        // The bridge does not short-circuit on the default value.
        struct EchoDefault;

        #[async_trait]
        impl HitlHandler for EchoDefault {
            async fn prompt(&self, req: HitlRequest) -> Result<HitlResponse, HitlError> {
                Ok(HitlResponse::new(req.default.as_deref().unwrap_or("none")))
            }
        }

        let bridge = HitlBridge::new(Arc::new(EchoDefault));
        let result = bridge.ask("Continue?", Some("default_val")).await;
        assert_eq!(result.unwrap(), "default_val");
    }

    #[tokio::test]
    async fn test_bridge_maps_handler_error_to_builtin_error() {
        let bridge = HitlBridge::new(Arc::new(ErrorHandler));
        let result = bridge.ask("Continue?", None).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("HITL handler error"), "expected 'HITL handler error' in: {msg}");
    }

    #[tokio::test]
    async fn test_bridge_no_default_passes_none_to_handler() {
        struct CheckDefault {
            got_default: std::sync::Mutex<Option<String>>,
        }

        #[async_trait]
        impl HitlHandler for CheckDefault {
            async fn prompt(&self, req: HitlRequest) -> Result<HitlResponse, HitlError> {
                *self.got_default.lock().unwrap() = req.default.clone();
                Ok(HitlResponse::new("ok"))
            }
        }

        let handler = Arc::new(CheckDefault {
            got_default: std::sync::Mutex::new(Some("initial".to_string())),
        });
        let bridge = HitlBridge::new(Arc::clone(&handler) as Arc<dyn HitlHandler>);
        bridge.ask("Question?", None).await.unwrap();
        assert_eq!(*handler.got_default.lock().unwrap(), None);
    }
}
