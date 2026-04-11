// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hand-written mock for [`nika_kernel::builtin::BuiltinRouter`].
//!
//! Supports success and error paths. `BuiltinError` is not `Clone`, so the
//! error variant is stored as a (tool, reason) pair and reconstructed as
//! `BuiltinError::Other` on each dispatch.
//!
//! The `knows()` method always returns true by default; override by setting
//! a list of tool names with `with_known()`.

use std::pin::Pin;

use nika_kernel::builtin::{BuiltinError, BuiltinRouter};

/// Mock [`BuiltinRouter`] for unit tests.
///
/// # Examples
///
/// ```
/// use nika_kernel_mock::builtin::MockBuiltinRouter;
///
/// let router = MockBuiltinRouter::ok("stub response".into());
/// assert!(router.knows("log"));
/// ```
#[derive(Debug, Clone)]
pub struct MockBuiltinRouter {
    response: Result<String, (String, String)>,
    known: Option<Vec<String>>,
}

impl MockBuiltinRouter {
    /// Create a router that returns `Ok(body)` for every dispatch.
    pub fn ok(body: String) -> Self {
        Self {
            response: Ok(body),
            known: None,
        }
    }

    /// Create a router that returns `Err(BuiltinError::Other { tool, reason })`
    /// for every dispatch. `BuiltinError` is not `Clone`, so the fields are
    /// stored separately and reconstructed on each call.
    pub fn err_other(tool: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            response: Err((tool.into(), reason.into())),
            known: None,
        }
    }

    /// Restrict the set of tools this router claims to know.
    pub fn with_known(mut self, tools: Vec<String>) -> Self {
        self.known = Some(tools);
        self
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
                Err((tool, reason)) => Err(BuiltinError::Other { tool, reason }),
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
        let router = MockBuiltinRouter::ok("body".into());
        let out = router.dispatch("log", String::from("{}")).await.unwrap();
        assert_eq!(out, "body");
    }

    #[tokio::test]
    async fn err_dispatches_error() {
        let router = MockBuiltinRouter::err_other("log", "boom");
        let err = router.dispatch("log", String::from("{}")).await.unwrap_err();
        assert!(matches!(err, BuiltinError::Other { .. }));
    }

    #[test]
    fn knows_returns_true_by_default() {
        let router = MockBuiltinRouter::ok("x".into());
        assert!(router.knows("anything"));
    }

    #[test]
    fn with_known_restricts_the_set() {
        let router = MockBuiltinRouter::ok("x".into()).with_known(vec!["log".into()]);
        assert!(router.knows("log"));
        assert!(!router.knows("other"));
    }
}
