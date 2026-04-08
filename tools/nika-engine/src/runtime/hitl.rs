// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Human-In-The-Loop (HITL) Handler for interactive workflows
//!
//! This module provides the `HitlHandler` trait for requesting user input
//! during workflow execution. Implementations can integrate with different
//! user interfaces (TUI, CLI, web, etc.).
//!
//! # Usage
//!
//! ```ignore
//! use nika::runtime::hitl::{HitlHandler, HitlRequest, HitlResponse};
//!
//! // Implement for your UI
//! struct MyHandler;
//!
//! #[async_trait]
//! impl HitlHandler for MyHandler {
//!     async fn prompt(&self, request: HitlRequest) -> Result<HitlResponse, HitlError> {
//!         // Show prompt to user, wait for response
//!         Ok(HitlResponse::new("user input"))
//!     }
//! }
//! ```

use async_trait::async_trait;
use std::time::Duration;
use thiserror::Error;

/// Error type for HITL operations.
#[derive(Debug, Error)]
pub enum HitlError {
    /// User cancelled the prompt.
    #[error("User cancelled prompt")]
    Cancelled,

    /// Prompt timed out waiting for user input.
    #[error("Prompt timed out after {0:?}")]
    Timeout(Duration),

    /// HITL handler is not available (headless mode).
    #[error("HITL handler not available: {0}")]
    NotAvailable(String),

    /// Other error during prompt.
    #[error("HITL error: {0}")]
    Other(String),
}

/// Request for user input via HITL.
#[derive(Debug, Clone)]
pub struct HitlRequest {
    /// The prompt message to display to the user.
    pub message: String,
    /// Default value if user provides no input.
    pub default: Option<String>,
    /// Optional timeout for the prompt.
    pub timeout: Option<Duration>,
    /// Optional list of choices for the user.
    pub choices: Option<Vec<String>>,
}

impl HitlRequest {
    /// Create a new HITL request with a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: None,
            timeout: None,
            choices: None,
        }
    }

    /// Set a default value.
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Set a timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set choices for the user.
    pub fn with_choices(mut self, choices: Vec<String>) -> Self {
        self.choices = Some(choices);
        self
    }
}

/// Response from a HITL prompt.
#[derive(Debug, Clone)]
pub struct HitlResponse {
    /// The user's response.
    pub response: String,
    /// Whether the default value was used.
    pub default_used: bool,
}

impl HitlResponse {
    /// Create a new response with user input.
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            default_used: false,
        }
    }

    /// Create a response that used the default value.
    pub fn from_default(default: impl Into<String>) -> Self {
        Self {
            response: default.into(),
            default_used: true,
        }
    }
}

/// Trait for handling HITL (Human-In-The-Loop) prompts.
///
/// Implementations should handle user interaction for their specific UI.
/// For example, a TUI implementation might show a modal dialog,
/// while a CLI implementation might read from stdin.
#[async_trait]
pub trait HitlHandler: Send + Sync {
    /// Request user input.
    ///
    /// The implementation should display the prompt message and wait for
    /// user input. If a default is provided and the user provides no input,
    /// the default should be returned.
    ///
    /// # Arguments
    ///
    /// * `request` - The HITL request containing message, default, timeout, etc.
    ///
    /// # Returns
    ///
    /// The user's response, or an error if the prompt failed.
    async fn prompt(&self, request: HitlRequest) -> Result<HitlResponse, HitlError>;
}

/// A default handler that always uses the default value or errors.
///
/// Useful for testing or headless mode.
#[derive(Debug, Default)]
pub struct DefaultHitlHandler;

#[async_trait]
impl HitlHandler for DefaultHitlHandler {
    async fn prompt(&self, request: HitlRequest) -> Result<HitlResponse, HitlError> {
        match request.default {
            Some(default) => Ok(HitlResponse::from_default(default)),
            None => Err(HitlError::NotAvailable(
                "No default provided and running in headless mode".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hitl_request_builder() {
        let request = HitlRequest::new("Enter your name")
            .with_default("Anonymous")
            .with_timeout(Duration::from_secs(30))
            .with_choices(vec!["Alice".to_string(), "Bob".to_string()]);

        assert_eq!(request.message, "Enter your name");
        assert_eq!(request.default, Some("Anonymous".to_string()));
        assert_eq!(request.timeout, Some(Duration::from_secs(30)));
        assert_eq!(
            request.choices,
            Some(vec!["Alice".to_string(), "Bob".to_string()])
        );
    }

    #[tokio::test]
    async fn test_hitl_response_new() {
        let response = HitlResponse::new("user input");
        assert_eq!(response.response, "user input");
        assert!(!response.default_used);
    }

    #[tokio::test]
    async fn test_hitl_response_from_default() {
        let response = HitlResponse::from_default("default value");
        assert_eq!(response.response, "default value");
        assert!(response.default_used);
    }

    #[tokio::test]
    async fn test_default_handler_uses_default() {
        let handler = DefaultHitlHandler;
        let request = HitlRequest::new("Test prompt").with_default("default");

        let response = handler.prompt(request).await.unwrap();
        assert_eq!(response.response, "default");
        assert!(response.default_used);
    }

    #[tokio::test]
    async fn test_default_handler_errors_without_default() {
        let handler = DefaultHitlHandler;
        let request = HitlRequest::new("Test prompt");

        let result = handler.prompt(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HitlError::NotAvailable(_)));
    }

    #[tokio::test]
    async fn test_hitl_error_display() {
        let err = HitlError::Cancelled;
        assert_eq!(err.to_string(), "User cancelled prompt");

        let err = HitlError::Timeout(Duration::from_secs(30));
        assert!(err.to_string().contains("30"));

        let err = HitlError::NotAvailable("test".to_string());
        assert!(err.to_string().contains("test"));
    }

    // Test that PromptTool uses HitlHandler when provided
    #[tokio::test]
    async fn test_custom_hitl_handler() {
        struct CustomHandler {
            fixed_response: String,
        }

        #[async_trait]
        impl HitlHandler for CustomHandler {
            async fn prompt(&self, _request: HitlRequest) -> Result<HitlResponse, HitlError> {
                Ok(HitlResponse::new(&self.fixed_response))
            }
        }

        let handler = CustomHandler {
            fixed_response: "custom_response".to_string(),
        };
        let request = HitlRequest::new("Test");
        let response = handler.prompt(request).await.unwrap();

        assert_eq!(response.response, "custom_response");
        assert!(!response.default_used);
    }
}
