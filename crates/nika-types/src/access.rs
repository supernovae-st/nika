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
    /// Every class, in sovereign presentation order — the ONE list the
    /// pin-token grammar and the teaching messages derive from (a
    /// hand-typed mirror is the drift class the parity tests kill).
    pub const ALL: [Self; 5] = [
        Self::Local,
        Self::Mock,
        Self::Harness,
        Self::Oauth,
        Self::Api,
    ];

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

/// One shipped agentic CLI runtime `--access` can name (product token).
///
/// The id is the pin token (`claude-code`). Spawn may use a different
/// ACP speaker (`claude-agent-acp`) — that wrapper is never a pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct HarnessRuntime {
    /// The `--access` token (`claude-code` · `codex` · `gemini-cli` ·
    /// `kimi-code` · `qwen-code`).
    pub id: &'static str,
    /// Human name for doctor and refusal lines (`Claude Code`).
    pub display: &'static str,
    /// Product CLI on `PATH` (`claude` · `codex` · `gemini`).
    pub detect_bin: &'static str,
    /// ACP speaker `argv[0]` (may differ from [`Self::detect_bin`]).
    pub acp_bin: &'static str,
    /// Dummy-readable line when the product CLI is absent (NIKA-1803).
    pub not_installed: &'static str,
}

impl HarnessRuntime {
    /// Gemini CLI (native ACP).
    pub const GEMINI_CLI: Self = Self {
        id: "gemini-cli",
        display: "Gemini CLI",
        detect_bin: "gemini",
        acp_bin: "gemini",
        not_installed: "Gemini CLI is not installed. Install it or pick Nika local / Nika Cloud.",
    };
    /// Qwen Code (native ACP).
    pub const QWEN_CODE: Self = Self {
        id: "qwen-code",
        display: "Qwen Code",
        detect_bin: "qwen",
        acp_bin: "qwen",
        not_installed: "Qwen Code is not installed. Install it or pick Nika local / Nika Cloud.",
    };
    /// Kimi Code (native ACP).
    pub const KIMI_CODE: Self = Self {
        id: "kimi-code",
        display: "Kimi Code",
        detect_bin: "kimi",
        acp_bin: "kimi",
        not_installed: "Kimi Code is not installed. Install it or pick Nika local / Nika Cloud.",
    };
    /// Codex (ACP speaker is `codex-acp`).
    pub const CODEX: Self = Self {
        id: "codex",
        display: "Codex",
        detect_bin: "codex",
        acp_bin: "codex-acp",
        not_installed: "Codex is not installed. Install it or pick Nika local / Nika Cloud.",
    };
    /// Claude Code (ACP speaker is `claude-agent-acp`).
    pub const CLAUDE_CODE: Self = Self {
        id: "claude-code",
        display: "Claude Code",
        detect_bin: "claude",
        acp_bin: "claude-agent-acp",
        not_installed: "Claude Code is not installed. Install it or pick Nika local / Nika Cloud.",
    };

    /// Shipped runtimes in G-3 probe order (gemini first · Anthropic last).
    pub const ALL: [Self; 5] = [
        Self::GEMINI_CLI,
        Self::QWEN_CODE,
        Self::KIMI_CODE,
        Self::CODEX,
        Self::CLAUDE_CODE,
    ];

    /// Look up a live `--access` token. Retired aliases are not here.
    #[must_use]
    pub fn lookup(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|row| row.id == id)
    }

    /// Retired public tokens that must never resolve (NIKA-1802 + rename).
    #[must_use]
    pub fn retired_alias(id: &str) -> Option<Self> {
        match id {
            "claude-agent-acp" => Some(Self::CLAUDE_CODE),
            "codex-acp" => Some(Self::CODEX),
            _ => None,
        }
    }

    /// The one help/docs vocabulary (` · ` separated, same day as `--help`).
    #[must_use]
    pub const fn vocabulary() -> &'static str {
        "claude-code · codex · gemini-cli · kimi-code · qwen-code"
    }

    /// Dummy-readable line when the product CLI is present but the ACP
    /// speaker is not (`claude` without `claude-agent-acp`).
    #[must_use]
    pub fn acp_missing(&self) -> alloc::string::String {
        if self.detect_bin == self.acp_bin {
            alloc::format!(
                "{} is installed, but Nika could not start its ACP mode. \
                 Upgrade it or pick Nika local / Nika Cloud.",
                self.display
            )
        } else {
            alloc::format!(
                "{} is installed, but Nika talks to it through ACP. Install `{}` \
                 (`npm i -g @zed-industries/{}`) or pick Nika local / Nika Cloud.",
                self.display,
                self.acp_bin,
                self.acp_bin
            )
        }
    }

    /// Dummy-readable line when the CLI is present but not signed in.
    #[must_use]
    pub fn not_signed_in(&self) -> alloc::string::String {
        alloc::format!(
            "{} is installed but not signed in. Sign in to {} itself, or pick \
             Nika local / Nika Cloud.",
            self.display,
            self.display
        )
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

/// WHICH layer of the meet dropped a candidate (A-8) — every refusal
/// names its layer, so « unavailable » is never a shrug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum RejectionLayer {
    /// The access path itself lacks the capability (key absent · class
    /// cannot satisfy the verb contract).
    Access,
    /// A hard `policy:` family refused it (`allow.providers` today ·
    /// `allow.access` when the NEP lands).
    Policy,
    /// An explicit pin (`--access` · config · lock) names another path
    /// — a pin is a pin: unsatisfied means refusal, never substitute.
    Pin,
    /// The run-start liveness re-verify found nothing listening (the
    /// B-5 gate) — the ONLY runtime rejection; run-time never re-selects.
    Liveness,
}

impl RejectionLayer {
    /// The `snake_case` wire form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Policy => "policy",
            Self::Pin => "pin",
            Self::Liveness => "liveness",
        }
    }
}

impl fmt::Display for RejectionLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// WHAT failed the meet — the P2 dimensions (the harness-era guarantee
/// dimensions join additively in P3/P4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum RejectionDimension {
    /// An explicit pin named a path this candidate is not.
    PinUnsatisfied,
    /// The provider prefix is outside a declared `allow.providers`.
    ProviderNotAllowed,
    /// The path needs a credential the environment does not carry.
    NotConfigured,
    /// The local server answered nothing at run start.
    NotLive,
}

impl RejectionDimension {
    /// The `snake_case` wire form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PinUnsatisfied => "pin_unsatisfied",
            Self::ProviderNotAllowed => "provider_not_allowed",
            Self::NotConfigured => "not_configured",
            Self::NotLive => "not_live",
        }
    }
}

impl fmt::Display for RejectionDimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One rejected candidate with its WITNESS — the dimension that failed
/// AND the layer that dropped it AND the teaching sentence (A-8).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct AccessRejection {
    /// The candidate's id (a provider id today · an adapter id at P3).
    pub access: alloc::string::String,
    /// What failed.
    pub dimension: RejectionDimension,
    /// Who dropped it.
    pub layer: RejectionLayer,
    /// The one-line teaching witness (e.g. `ANTHROPIC_API_KEY unset`).
    pub witness: alloc::string::String,
}

impl AccessRejection {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        access: impl Into<alloc::string::String>,
        dimension: RejectionDimension,
        layer: RejectionLayer,
        witness: impl Into<alloc::string::String>,
    ) -> Self {
        Self {
            access: access.into(),
            dimension,
            layer,
            witness: witness.into(),
        }
    }

    /// The rendered witness line — `explain`'s row, one truth:
    /// `<access> · <dimension> (<layer> layer) · <witness>`.
    #[must_use]
    pub fn witness_line(&self) -> alloc::string::String {
        alloc::format!(
            "{} · {} ({} layer) · {}",
            self.access,
            self.dimension,
            self.layer,
            self.witness
        )
    }
}

/// The admission-time access decision (A-9 · A-10) — recorded in
/// `check --json`, the trace prologue, and the lock when one exists.
/// Run time may only REFUSE on liveness; it never re-plans.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct AccessPlan {
    /// The requested `provider/name` (the author's explicit choice —
    /// the resolver NEVER picks a model).
    pub model: alloc::string::String,
    /// The provider prefix.
    pub provider: alloc::string::String,
    /// The chosen path's id (a provider id today · an adapter id at
    /// P3) — a plan that cannot NAME its path is not attested (A-9):
    /// two P3 harness adapters share one class, never one id.
    pub access: alloc::string::String,
    /// The chosen path's class.
    pub chosen: AccessClass,
    /// The chosen path's economic lane.
    pub billing: BillingClass,
    /// Whether an explicit pin forced the choice.
    pub pinned: bool,
    /// Every rejected candidate with its witness.
    pub rejected: alloc::vec::Vec<AccessRejection>,
}

impl AccessPlan {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        model: impl Into<alloc::string::String>,
        provider: impl Into<alloc::string::String>,
        access: impl Into<alloc::string::String>,
        chosen: AccessClass,
        billing: BillingClass,
        pinned: bool,
        rejected: alloc::vec::Vec<AccessRejection>,
    ) -> Self {
        Self {
            model: model.into(),
            provider: provider.into(),
            access: access.into(),
            chosen,
            billing,
            pinned,
            rejected,
        }
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;

    #[test]
    fn rejection_vocabulary_is_snake_case_and_distinct() {
        assert_eq!(RejectionLayer::Access.as_str(), "access");
        assert_eq!(RejectionLayer::Policy.as_str(), "policy");
        assert_eq!(RejectionLayer::Pin.as_str(), "pin");
        assert_eq!(RejectionLayer::Liveness.as_str(), "liveness");
        assert_eq!(
            RejectionDimension::PinUnsatisfied.as_str(),
            "pin_unsatisfied"
        );
        assert_eq!(
            RejectionDimension::ProviderNotAllowed.as_str(),
            "provider_not_allowed"
        );
        assert_eq!(RejectionDimension::NotConfigured.as_str(), "not_configured");
        assert_eq!(RejectionDimension::NotLive.as_str(), "not_live");
        let layers = [
            RejectionLayer::Access,
            RejectionLayer::Policy,
            RejectionLayer::Pin,
            RejectionLayer::Liveness,
        ];
        for (i, a) in layers.iter().enumerate() {
            for b in &layers[i + 1..] {
                assert_ne!(a.as_str(), b.as_str());
            }
        }
    }

    #[test]
    fn a_rejection_witness_names_dimension_and_layer() {
        let r = AccessRejection::new(
            "anthropic",
            RejectionDimension::NotConfigured,
            RejectionLayer::Access,
            "ANTHROPIC_API_KEY unset",
        );
        let line = r.witness_line();
        assert!(line.contains("anthropic"), "{line}");
        assert!(line.contains("not_configured"), "{line}");
        assert!(line.contains("access"), "{line}");
        assert!(line.contains("ANTHROPIC_API_KEY unset"), "{line}");
    }

    #[test]
    fn a_plan_records_choice_and_rejections() {
        let plan = AccessPlan::new(
            "mistral/mistral-small-latest",
            "mistral",
            "mistral",
            AccessClass::Api,
            BillingClass::ApiMetered,
            false,
            vec![AccessRejection::new(
                "ollama",
                RejectionDimension::NotLive,
                RejectionLayer::Liveness,
                "nothing listening on loopback:11434",
            )],
        );
        assert_eq!(plan.model, "mistral/mistral-small-latest");
        assert_eq!(plan.access, "mistral");
        assert_eq!(plan.chosen, AccessClass::Api);
        assert!(!plan.pinned);
        assert_eq!(plan.rejected.len(), 1);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn plan_serde_wire_is_snake_case() {
        let plan = AccessPlan::new(
            "mock/echo",
            "mock",
            "mock",
            AccessClass::Mock,
            BillingClass::Local,
            true,
            Vec::new(),
        );
        let json = serde_json::to_string(&plan).expect("serialize");
        assert!(json.contains("\"access\":\"mock\""), "{json}");
        assert!(json.contains("\"chosen\":\"mock\""), "{json}");
        assert!(json.contains("\"billing\":\"local\""), "{json}");
        assert!(json.contains("\"pinned\":true"), "{json}");
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
        // Distinctness over the CANONICAL list (`ALL`) — kills both the
        // two-arms-one-string mutant and a dropped-from-ALL mutant.
        assert_eq!(AccessClass::ALL.len(), 5);
        for (i, a) in AccessClass::ALL.iter().enumerate() {
            for b in &AccessClass::ALL[i + 1..] {
                assert_ne!(a.as_str(), b.as_str());
            }
        }
        // Sovereign presentation order: local leads, api never first.
        assert_eq!(AccessClass::ALL[0], AccessClass::Local);
        assert_eq!(AccessClass::ALL[4], AccessClass::Api);
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
        _assert_send_sync::<HarnessRuntime>();
    }

    #[test]
    fn harness_runtime_tokens_are_one_name_each_and_never_a_class() {
        assert_eq!(HarnessRuntime::ALL.len(), 5);
        for (i, a) in HarnessRuntime::ALL.iter().enumerate() {
            for b in &HarnessRuntime::ALL[i + 1..] {
                assert_ne!(a.id, b.id, "one name per runtime");
            }
            assert!(
                AccessClass::ALL.iter().all(|c| c.as_str() != a.id),
                "{} must never collide with an access class",
                a.id
            );
            assert!(HarnessRuntime::lookup(a.id).is_some());
        }
        assert_eq!(
            HarnessRuntime::lookup("claude-code"),
            Some(HarnessRuntime::CLAUDE_CODE)
        );
        assert_eq!(HarnessRuntime::lookup("codex"), Some(HarnessRuntime::CODEX));
        assert!(HarnessRuntime::lookup("claude-agent-acp").is_none());
        assert!(HarnessRuntime::lookup("codex-acp").is_none());
        assert_eq!(
            HarnessRuntime::retired_alias("claude-agent-acp"),
            Some(HarnessRuntime::CLAUDE_CODE)
        );
        assert_eq!(
            HarnessRuntime::retired_alias("codex-acp"),
            Some(HarnessRuntime::CODEX)
        );
        let vocab = HarnessRuntime::vocabulary();
        for id in [
            "claude-code",
            "codex",
            "gemini-cli",
            "kimi-code",
            "qwen-code",
        ] {
            assert!(vocab.contains(id), "{vocab} missing {id}");
        }
        assert!(
            HarnessRuntime::CLAUDE_CODE
                .not_installed
                .contains("Claude Code is not installed")
        );
        assert!(
            HarnessRuntime::CLAUDE_CODE
                .acp_missing()
                .contains("claude-agent-acp")
        );
    }
}
