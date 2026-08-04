// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Execution access vocabulary — HOW the engine reaches a model.
//!
//! `model:` picks the intelligence; **access** names the path used to
//! reach it (D-2026-08-04-N1). Three classes already run today (api ·
//! local · mock) — this module names them so probes, traces and
//! receipts can record the path instead of implying it. `harness` and
//! `oauth` are declared for the P3+ adapters (feature-gated there;
//! the vocabulary is complete from day one so wire strings never move).

use core::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// HOW the engine reaches the model a task named — the access path's
/// class. Orthogonal to the provider (the `model:` prefix): a request
/// for `openai/<name>` may travel over an API key, a local
/// OpenAI-compatible server, or the user's own harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum AccessClass {
    /// A local inference server (ollama · lmstudio · llama.cpp · localai
    /// · vllm · the candle sidecar) — loopback/LAN, no vendor account.
    Local,
    /// A cloud provider reached directly over its API with a key the
    /// operator configured — metered USD spend.
    Api,
    /// The user's own agent harness driven through its official
    /// protocol — the harness owns auth; the engine never holds a
    /// credential (A-3).
    Harness,
    /// A vendor OAuth grant the vendor formally provides to third-party
    /// tools (device-flow class) — sanctioned flows only (A-3).
    Oauth,
    /// The in-crate mock provider — tests and conformance, spends
    /// nothing by construction.
    Mock,
}

impl AccessClass {
    /// The `snake_case` wire form (trace `access` field · probe JSON).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Api => "api",
            Self::Harness => "harness",
            Self::Oauth => "oauth",
            Self::Mock => "mock",
        }
    }

    /// The billing class this access implies BEFORE an adapter reports
    /// better evidence — honest defaults only: a harness or oauth path
    /// is `Unknown` until its probe says otherwise (subscription is
    /// never presented as free · unknown is never presented as zero).
    #[must_use]
    pub const fn default_billing(self) -> BillingClass {
        match self {
            Self::Local | Self::Mock => BillingClass::Local,
            Self::Api => BillingClass::ApiMetered,
            Self::Harness | Self::Oauth => BillingClass::Unknown,
        }
    }
}

impl fmt::Display for AccessClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// HOW an execution is paid for — declared before the run, recorded in
/// the trace. Distinct from cost EVIDENCE (`Cost` · `UnpricedReason`):
/// the class names the economic lane, the evidence names what was
/// actually measured on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum BillingClass {
    /// Local compute — time + machine, not priced, never « free ».
    Local,
    /// Spend absorbed by a subscription's included quota/limits.
    IncludedQuota,
    /// Billed per token to a subscription's extra-usage lane (real
    /// marginal money, outside plan limits).
    ExtraUsage,
    /// Metered per-token USD on a provider API key.
    ApiMetered,
    /// Drawn from a prepaid credit balance.
    CreditMetered,
    /// The lane is not observable from here — stays unknown, never
    /// guessed (A-7).
    Unknown,
}

impl BillingClass {
    /// The `snake_case` wire form (trace `billing` field).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::IncludedQuota => "included_quota",
            Self::ExtraUsage => "extra_usage",
            Self::ApiMetered => "api_metered",
            Self::CreditMetered => "credit_metered",
            Self::Unknown => "unknown",
        }
    }

    /// Whether this lane spends METERED USD — the only spend class the
    /// `--max-cost-usd` ceiling bounds; every other lane rides the
    /// honest-absence channel (`UnpricedReason`), never a fake $0.
    #[must_use]
    pub const fn is_usd_metered(self) -> bool {
        matches!(self, Self::ApiMetered | Self::CreditMetered)
    }
}

impl fmt::Display for BillingClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_class_wire_strings_are_snake_case_and_distinct() {
        assert_eq!(AccessClass::Local.as_str(), "local");
        assert_eq!(AccessClass::Api.as_str(), "api");
        assert_eq!(AccessClass::Harness.as_str(), "harness");
        assert_eq!(AccessClass::Oauth.as_str(), "oauth");
        assert_eq!(AccessClass::Mock.as_str(), "mock");
        // Distinctness kills any two-arms-one-string mutant.
        let all = [
            AccessClass::Local,
            AccessClass::Api,
            AccessClass::Harness,
            AccessClass::Oauth,
            AccessClass::Mock,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.as_str(), b.as_str());
            }
        }
    }

    #[test]
    fn access_class_display_matches_wire() {
        assert_eq!(AccessClass::Harness.to_string(), "harness");
        assert_eq!(AccessClass::Local.to_string(), "local");
    }

    #[test]
    fn billing_class_wire_strings_are_snake_case_and_distinct() {
        assert_eq!(BillingClass::Local.as_str(), "local");
        assert_eq!(BillingClass::IncludedQuota.as_str(), "included_quota");
        assert_eq!(BillingClass::ExtraUsage.as_str(), "extra_usage");
        assert_eq!(BillingClass::ApiMetered.as_str(), "api_metered");
        assert_eq!(BillingClass::CreditMetered.as_str(), "credit_metered");
        assert_eq!(BillingClass::Unknown.as_str(), "unknown");
        let all = [
            BillingClass::Local,
            BillingClass::IncludedQuota,
            BillingClass::ExtraUsage,
            BillingClass::ApiMetered,
            BillingClass::CreditMetered,
            BillingClass::Unknown,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.as_str(), b.as_str());
            }
        }
    }

    #[test]
    fn billing_class_display_matches_wire() {
        assert_eq!(BillingClass::IncludedQuota.to_string(), "included_quota");
        assert_eq!(BillingClass::Unknown.to_string(), "unknown");
    }

    #[test]
    fn usd_metering_is_exactly_the_two_metered_classes() {
        // The --max-cost-usd ceiling bounds METERED USD spend only; every
        // other class rides the honest-absence channel (never a fake $0).
        assert!(BillingClass::ApiMetered.is_usd_metered());
        assert!(BillingClass::CreditMetered.is_usd_metered());
        assert!(!BillingClass::Local.is_usd_metered());
        assert!(!BillingClass::IncludedQuota.is_usd_metered());
        assert!(!BillingClass::ExtraUsage.is_usd_metered());
        assert!(!BillingClass::Unknown.is_usd_metered());
    }

    #[test]
    fn default_billing_for_access_class_is_honest() {
        // api → metered USD · local/mock → local · harness/oauth → unknown
        // until the adapter's probe reports better (subscription ≠ free ·
        // unknown ≠ zero · A-7).
        assert_eq!(AccessClass::Api.default_billing(), BillingClass::ApiMetered);
        assert_eq!(AccessClass::Local.default_billing(), BillingClass::Local);
        assert_eq!(AccessClass::Mock.default_billing(), BillingClass::Local);
        assert_eq!(
            AccessClass::Harness.default_billing(),
            BillingClass::Unknown
        );
        assert_eq!(AccessClass::Oauth.default_billing(), BillingClass::Unknown);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_wire_is_snake_case_and_roundtrips() {
        let json = serde_json::to_string(&AccessClass::Harness).expect("serialize");
        assert_eq!(json, "\"harness\"");
        let back: AccessClass = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, AccessClass::Harness);

        let json = serde_json::to_string(&BillingClass::IncludedQuota).expect("serialize");
        assert_eq!(json, "\"included_quota\"");
        let back: BillingClass = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, BillingClass::IncludedQuota);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn access_types_are_send_sync() {
        _assert_send_sync::<AccessClass>();
        _assert_send_sync::<BillingClass>();
    }
}
