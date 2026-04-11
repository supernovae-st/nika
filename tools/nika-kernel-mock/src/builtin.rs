// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hand-written mock for [`nika_kernel::builtin::BuiltinRouter`].
//!
//! Supports success and all 4 relevant error paths (Other, Denied,
//! InvalidArgs, AssertionFailed). `BuiltinError` is not `Clone`, so the
//! error shape is stored as a private [`BuiltinErrorSpec`] enum and the
//! concrete `BuiltinError` is reconstructed on each dispatch.
//!
//! The `knows()` method returns true by default; override with
//! [`MockBuiltinRouter::with_known`] to restrict the set of known tools.

use std::pin::Pin;

use nika_kernel::builtin::{BuiltinError, BuiltinRouter};

/// Private error shape stored inside the mock. Translated to a concrete
/// [`BuiltinError`] on each `dispatch()` call because `BuiltinError`
/// itself is not `Clone`.
#[derive(Debug, Clone)]
enum BuiltinErrorSpec {
    Other { tool: String, reason: String },
    Denied { tool: String, reason: String },
    InvalidArgs { tool: String, reason: String },
    AssertionFailed { message: String, condition: String },
}

impl BuiltinErrorSpec {
    fn build(self) -> BuiltinError {
        match self {
            Self::Other { tool, reason } => BuiltinError::Other { tool, reason },
            Self::Denied { tool, reason } => BuiltinError::Denied { tool, reason },
            Self::InvalidArgs { tool, reason } => BuiltinError::InvalidArgs { tool, reason },
            Self::AssertionFailed { message, condition } => {
                BuiltinError::AssertionFailed { message, condition }
            }
        }
    }
}

/// Mock [`BuiltinRouter`] for unit tests.
///
/// # Examples
///
/// ```
/// use nika_kernel_mock::builtin::MockBuiltinRouter;
///
/// let router = MockBuiltinRouter::ok("stub response");
/// assert!(router.knows("log"));
/// ```
#[derive(Debug, Clone)]
pub struct MockBuiltinRouter {
    response: Result<String, BuiltinErrorSpec>,
    known: Option<Vec<String>>,
}

impl MockBuiltinRouter {
    /// Create a router that returns `Ok(body)` for every dispatch.
    pub fn ok(body: impl Into<String>) -> Self {
        Self {
            response: Ok(body.into()),
            known: None,
        }
    }

    /// Create a router that returns [`BuiltinError::Other`] on every dispatch.
    pub fn err_other(tool: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            response: Err(BuiltinErrorSpec::Other {
                tool: tool.into(),
                reason: reason.into(),
            }),
            known: None,
        }
    }

    /// Create a router that returns [`BuiltinError::Denied`] (NIKA-380) on every dispatch.
    pub fn err_denied(tool: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            response: Err(BuiltinErrorSpec::Denied {
                tool: tool.into(),
                reason: reason.into(),
            }),
            known: None,
        }
    }

    /// Create a router that returns [`BuiltinError::InvalidArgs`] (NIKA-212) on every dispatch.
    pub fn err_invalid_args(tool: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            response: Err(BuiltinErrorSpec::InvalidArgs {
                tool: tool.into(),
                reason: reason.into(),
            }),
            known: None,
        }
    }

    /// Create a router that returns [`BuiltinError::AssertionFailed`] (NIKA-213) on every dispatch.
    pub fn err_assertion_failed(
        message: impl Into<String>,
        condition: impl Into<String>,
    ) -> Self {
        Self {
            response: Err(BuiltinErrorSpec::AssertionFailed {
                message: message.into(),
                condition: condition.into(),
            }),
            known: None,
        }
    }

    /// Restrict the set of tools this router claims to know.
    pub fn with_known<I, S>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.known = Some(tools.into_iter().map(Into::into).collect());
        self
    }
}

impl Default for MockBuiltinRouter {
    fn default() -> Self {
        Self::ok("{}")
    }
}

impl BuiltinRouter for MockBuiltinRouter {
    fn dispatch<'a>(
        &'a self,
        _tool: &'a str,
        _args: String,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        let response = self.response.clone();
        Box::pin(async move {
            match response {
                Ok(body) => Ok(body),
                Err(spec) => Err(spec.build()),
            }
        })
    }

    fn knows(&self, tool: &str) -> bool {
        match &self.known {
            Some(list) => list.iter().any(|t| t == tool),
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ok_dispatches_body() {
        let router = MockBuiltinRouter::ok("body");
        let out = router.dispatch("log", String::from("{}")).await.unwrap();
        assert_eq!(out, "body");
    }

    #[tokio::test]
    async fn err_other_dispatches_error() {
        let router = MockBuiltinRouter::err_other("log", "boom");
        let err = router
            .dispatch("log", String::from("{}"))
            .await
            .unwrap_err();
        assert!(matches!(err, BuiltinError::Other { .. }));
        assert!(err.to_string().contains("NIKA-210"));
    }

    #[tokio::test]
    async fn err_denied_dispatches_denied() {
        let router = MockBuiltinRouter::err_denied("nika:write", "untrusted agent");
        let err = router
            .dispatch("write", String::from("{}"))
            .await
            .unwrap_err();
        assert!(matches!(err, BuiltinError::Denied { .. }));
        assert!(err.to_string().contains("NIKA-380"));
    }

    #[tokio::test]
    async fn err_invalid_args_dispatches_invalid() {
        let router = MockBuiltinRouter::err_invalid_args("nika:sleep", "missing duration");
        let err = router
            .dispatch("sleep", String::from("{}"))
            .await
            .unwrap_err();
        assert!(matches!(err, BuiltinError::InvalidArgs { .. }));
        assert!(err.to_string().contains("NIKA-212"));
    }

    #[tokio::test]
    async fn err_assertion_failed_dispatches_assertion() {
        let router = MockBuiltinRouter::err_assertion_failed("expected X", "x == 42");
        let err = router
            .dispatch("assert", String::from("{}"))
            .await
            .unwrap_err();
        assert!(matches!(err, BuiltinError::AssertionFailed { .. }));
        assert!(err.to_string().contains("NIKA-213"));
    }

    #[test]
    fn knows_returns_true_by_default() {
        let router = MockBuiltinRouter::ok("x");
        assert!(router.knows("anything"));
    }

    #[test]
    fn with_known_restricts_the_set() {
        let router = MockBuiltinRouter::ok("x").with_known(["log"]);
        assert!(router.knows("log"));
        assert!(!router.knows("other"));
    }

    #[test]
    fn with_known_accepts_strings_and_str() {
        // Ergonomic polish: with_known now takes any IntoIterator of Into<String>.
        let router = MockBuiltinRouter::default().with_known(vec!["log".to_string(), "emit".into()]);
        assert!(router.knows("log"));
        assert!(router.knows("emit"));
        assert!(!router.knows("write"));
    }

    #[test]
    fn default_is_ok_empty_object() {
        let router = MockBuiltinRouter::default();
        assert!(router.knows("anything"));
    }
}
