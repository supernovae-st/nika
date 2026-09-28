// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Selective observation does not replace other routes' host cost policy.
use super::{InferenceAdmission, InferenceTariff};

#[derive(Clone, Copy, Debug)]
pub(super) enum RouteSelection {
    All,
    DeclaredFree,
    /// `DeclaredFree`, and an unknown-cost API route (never chosen) refuses.
    Run,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HandleScope {
    pub(super) bound: bool,
    pub(super) routes: RouteSelection,
}

impl InferenceAdmission {
    /// Observe only exact catalog-declared-free routes in a mixed registry.
    /// Other routes retain their existing host admission and budget behavior;
    /// this account grants them no authority and reports none of their costs.
    /// Selected routes retain strict request bounds, response identity and
    /// complete-usage settlement. Unknown charge still closes further calls
    /// on this account, even when the held catalog estimate is zero.
    #[must_use]
    pub fn observe_declared_free() -> Self {
        let mut account = Self::unbudgeted();
        account.1.routes = RouteSelection::DeclaredFree;
        account
    }

    /// A Run's observer when a `model:` may be rendered at run time: exact
    /// declared-free routes are observed as by [`Self::observe_declared_free`],
    /// and an API route whose USD cost is unknown ([`super::unknown_cost_route`])
    /// refuses before any byte, since no fresh choice covers a run-time route.
    /// Every other route keeps its host policy and transport.
    #[must_use]
    pub fn observe_run() -> Self {
        let mut account = Self::unbudgeted();
        account.1.routes = RouteSelection::Run;
        account
    }

    /// Whether this handle observes only exact declared-free routes. Hosts
    /// use this to preserve other routes' HTTP transport policy as well.
    #[must_use]
    pub fn observes_declared_free_only(&self) -> bool {
        matches!(
            self.1.routes,
            RouteSelection::DeclaredFree | RouteSelection::Run
        )
    }

    /// Whether an unknown-cost API route this handle does not track refuses.
    pub(crate) fn refuses_unknown_cost(&self) -> bool {
        matches!(self.1.routes, RouteSelection::Run)
    }

    pub(crate) fn tracks_route(&self, provider: &str, model: &str, endpoint: &str) -> bool {
        match self.1.routes {
            RouteSelection::All => true,
            RouteSelection::DeclaredFree | RouteSelection::Run => {
                InferenceTariff::new(provider, model, endpoint)
                    .is_some_and(|t| t.currency == "USD" && t.is_declared_free())
            }
        }
    }
}
