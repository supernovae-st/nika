// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The admission-time access resolver (D-2026-08-04-N1 · P2) — a PURE
//! function from enumerated candidates to an attested [`AccessPlan`]:
//! zero I/O, zero LLM, property-tested.
//!
//! `model:` picks the intelligence; the resolver picks the PATH among
//! the paths this machine offers — deterministically: the same inputs
//! produce the same plan whatever order the candidates were enumerated
//! in, and every drop carries a WITNESS (dimension + layer + teaching
//! sentence · A-8). A pin is a pin: `--access` unsatisfied is a
//! refusal, never a substitute (A-4). Run time re-verifies LIVENESS
//! only; it never re-plans (B-5).

use nika_types::access::{
    AccessClass, AccessPlan, AccessRejection, BillingClass, RejectionDimension, RejectionLayer,
};

use crate::probe::ProviderProbe;

/// One enumerated way to reach a provider — the resolver's INPUT row.
/// Today a provider yields exactly one candidate (its profile's class ·
/// key state); the P3 adapters add `harness`/`oauth` rows beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AccessCandidate {
    /// The candidate's id — a provider id today, an adapter id at P3.
    pub access: String,
    /// The path's class.
    pub class: AccessClass,
    /// Credential present when one is required (keyless: always true).
    pub configured: bool,
    /// The env var a fix line names when `configured` is false.
    pub fix_var: Option<String>,
    /// The economic lane — the class's honest default until a probe
    /// reports better evidence (subscription ≠ free · unknown ≠ $0).
    pub billing: BillingClass,
}

impl AccessCandidate {
    /// Construct (INV-019) — billing defaults from the class.
    #[must_use]
    pub fn new(access: impl Into<String>, class: AccessClass, configured: bool) -> Self {
        Self {
            access: access.into(),
            class,
            configured,
            fix_var: None,
            billing: class.default_billing(),
        }
    }

    /// Name the credential env var an unconfigured refusal teaches.
    #[must_use]
    pub fn with_fix_var(mut self, var: impl Into<String>) -> Self {
        self.fix_var = Some(var.into());
        self
    }

    /// Override the economic lane with better probe evidence.
    #[must_use]
    pub fn with_billing(mut self, billing: BillingClass) -> Self {
        self.billing = billing;
        self
    }
}

/// The TOTAL refusal — no candidate survived, and every one is
/// accounted with its witness (a refusal is never a shrug · A-8).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AccessRefusal {
    /// The requested `provider/name`.
    pub model: String,
    /// The provider prefix.
    pub provider: String,
    /// Every candidate with the dimension + layer that dropped it —
    /// empty ONLY when no candidate was enumerated at all (an
    /// unrecognized provider dies upstream at the MODELS rung).
    pub rejected: Vec<AccessRejection>,
}

impl AccessRefusal {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        model: impl Into<String>,
        provider: impl Into<String>,
        rejected: Vec<AccessRejection>,
    ) -> Self {
        Self {
            model: model.into(),
            provider: provider.into(),
            rejected,
        }
    }
}

/// The sovereign preference order (research §7.2 step 7) — a STRICT
/// total order across classes so no tie ever needs a coin: local <
/// mock < harness < oauth < api. Local compute leads (sovereignty),
/// the test lane spends nothing, the user's own plan (harness · then
/// the sanctioned oauth grant) beats metered USD. A class this fn has
/// not learned ranks LAST — conservative, never silently preferred.
const fn sovereign_rank(class: AccessClass) -> u8 {
    match class {
        AccessClass::Local => 0,
        AccessClass::Mock => 1,
        AccessClass::Harness => 2,
        AccessClass::Oauth => 3,
        AccessClass::Api => 4,
        _ => u8::MAX,
    }
}

/// A pin names a path by its ID or by its CLASS wire string —
/// `--access ollama` and `--access local` both read naturally, and the
/// P3 adapter ids (`codex-acp`) ride the same grammar unchanged.
fn pin_matches(pin: &str, candidate: &AccessCandidate) -> bool {
    pin == candidate.access || pin == candidate.class.as_str()
}

/// The strict total order (rank · id · configured-first · billing ·
/// fix-var) — total over ANY input, so even pathological duplicate ids
/// resolve identically under permutation.
fn order_key(c: &AccessCandidate) -> (u8, &str, bool, &'static str, &str) {
    (
        sovereign_rank(c.class),
        c.access.as_str(),
        !c.configured,
        c.billing.as_str(),
        c.fix_var.as_deref().unwrap_or(""),
    )
}

/// The first failing step wins the witness (research §7.2 steps 4→5→6:
/// access · policy · pin) — deterministic, and the earliest layer is
/// the one whose fix line helps most.
fn judge(
    candidate: &AccessCandidate,
    provider: &str,
    allow_providers: Option<&[String]>,
    pin: Option<&str>,
) -> Option<AccessRejection> {
    if !candidate.configured {
        let witness = candidate.fix_var.as_ref().map_or_else(
            || String::from("no credential configured"),
            |var| format!("{var} unset"),
        );
        return Some(AccessRejection::new(
            candidate.access.clone(),
            RejectionDimension::NotConfigured,
            RejectionLayer::Access,
            witness,
        ));
    }
    if let Some(allowed) = allow_providers
        && !allowed.iter().any(|p| p == provider)
    {
        return Some(AccessRejection::new(
            candidate.access.clone(),
            RejectionDimension::ProviderNotAllowed,
            RejectionLayer::Policy,
            format!("provider `{provider}` outside policy.allow.providers"),
        ));
    }
    if let Some(pinned) = pin
        && !pin_matches(pinned, candidate)
    {
        return Some(AccessRejection::new(
            candidate.access.clone(),
            RejectionDimension::PinUnsatisfied,
            RejectionLayer::Pin,
            format!("pin `--access {pinned}` names another path"),
        ));
    }
    None
}

/// Resolve the access path for `model` among `candidates` — the pure
/// admission-time decision (research §7.2 steps 3-8).
///
/// Deterministic by construction: candidates are ordered internally by
/// the sovereign rank then the id (codepoint order) BEFORE judgment,
/// so enumeration order never matters — the property tests pin this.
/// The plan's `rejected` carries the candidates that FAILED a
/// dimension; an admissible candidate merely outranked stays silently
/// available (a pin can still name it).
///
/// # Errors
///
/// Refuses with [`AccessRefusal`] — every candidate accounted with its
/// witness — when none survives the access/policy/pin judgment.
pub fn resolve_access(
    model: &str,
    candidates: &[AccessCandidate],
    allow_providers: Option<&[String]>,
    pin: Option<&str>,
) -> Result<AccessPlan, AccessRefusal> {
    let provider = model.split_once('/').map_or(model, |(prefix, _)| prefix);
    let mut ordered: Vec<&AccessCandidate> = candidates.iter().collect();
    ordered.sort_by(|a, b| order_key(a).cmp(&order_key(b)));

    let mut rejected = Vec::new();
    let mut chosen: Option<&AccessCandidate> = None;
    for candidate in ordered {
        if let Some(rejection) = judge(candidate, provider, allow_providers, pin) {
            rejected.push(rejection);
        } else if chosen.is_none() {
            chosen = Some(candidate);
        }
    }
    match chosen {
        Some(c) => Ok(AccessPlan::new(
            model,
            provider,
            c.access.clone(),
            c.class,
            c.billing,
            pin.is_some(),
            rejected,
        )),
        None => Err(AccessRefusal::new(model, provider, rejected)),
    }
}

/// A refused `--access` pin, classified for its teaching code — the
/// engine maps the variants 1:1 onto NIKA-1802/1801/1800; the message
/// rides here so every consumer speaks one voice (A-8).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PinRefusal {
    /// The token is neither an access class nor an id this machine
    /// offers (NIKA-1802) — taught before any resolution runs.
    UnknownToken {
        /// The teaching line (vocabulary included).
        message: String,
    },
    /// No candidate matches the pin (NIKA-1801) — never a substitute.
    PinUnsatisfied {
        /// The teaching line (witnesses included).
        message: String,
    },
    /// The pinned path itself failed admission (NIKA-1800).
    NoPath {
        /// The witness lines (`<access> · <dimension> (<layer>) · <fix>`).
        message: String,
    },
}

/// Judge an explicit `--access` pin over statically-known models — the
/// admission gate's core (D-2026-08-04-N1 · pure · zero I/O). `None` =
/// the pin is satisfied everywhere (or nothing static to judge). The
/// mock backend is compiled in and keyless — probes exclude it, this
/// judgment must not (a pinned `mock/echo` rehearsal runs).
#[must_use]
pub fn refuse_pin<'m>(
    models: impl IntoIterator<Item = &'m str>,
    probes: &[ProviderProbe],
    pin: &str,
) -> Option<PinRefusal> {
    let class_token = ["local", "api", "harness", "oauth", "mock"].contains(&pin);
    if !class_token && !probes.iter().any(|p| p.id == pin) {
        return Some(PinRefusal::UnknownToken {
            message: format!(
                "`--access {pin}` names neither an access class (local · api · \
                 harness · oauth · mock) nor an access id this machine offers — \
                 `nika doctor` lists every path"
            ),
        });
    }
    for model in models {
        let provider = model.split_once('/').map_or(model, |(p, _)| p);
        let mut candidates = candidates_for(probes, provider);
        if provider == "mock" && candidates.is_empty() {
            candidates.push(AccessCandidate::new("mock", AccessClass::Mock, true));
        }
        if let Err(refusal) = resolve_access(model, &candidates, None, Some(pin)) {
            return Some(classify_pin_refusal(model, pin, &refusal));
        }
    }
    None
}

/// All-`pin_unsatisfied` (or empty) = the pin names nothing usable ·
/// any other dimension = the pinned path itself failed. Witnesses ride
/// the message either way (a refusal is never a shrug · A-8).
fn classify_pin_refusal(model: &str, pin: &str, refusal: &AccessRefusal) -> PinRefusal {
    let witnesses: Vec<String> = refusal
        .rejected
        .iter()
        .map(AccessRejection::witness_line)
        .collect();
    let rendered = if witnesses.is_empty() {
        String::new()
    } else {
        format!(" · {}", witnesses.join(" · "))
    };
    let all_pin = refusal
        .rejected
        .iter()
        .all(|r| r.dimension == RejectionDimension::PinUnsatisfied);
    if all_pin {
        PinRefusal::PinUnsatisfied {
            message: format!(
                "model `{model}` has no access path matching `--access {pin}` — \
                 a pin is a pin (refusal, never a substitute){rendered}"
            ),
        }
    } else {
        PinRefusal::NoPath {
            message: format!(
                "no access path survives admission for `{model}` under \
                 `--access {pin}`{rendered}"
            ),
        }
    }
}

/// Bridge the probe layer to resolver input — the candidates a machine
/// offers for ONE provider (exactly the profile row today; the P3
/// access registry adds harness/oauth rows beside it).
#[must_use]
pub fn candidates_for(probes: &[ProviderProbe], provider: &str) -> Vec<AccessCandidate> {
    probes
        .iter()
        .filter(|p| p.id == provider)
        .map(|p| {
            let candidate =
                AccessCandidate::new(p.id.clone(), p.readiness.access, p.readiness.configured);
            if p.requires_key {
                candidate.with_fix_var(p.fix_var.clone())
            } else {
                candidate
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(id: &str, configured: bool) -> AccessCandidate {
        AccessCandidate::new(id, AccessClass::Local, configured)
    }

    fn api(id: &str, configured: bool, fix: &str) -> AccessCandidate {
        AccessCandidate::new(id, AccessClass::Api, configured).with_fix_var(fix)
    }

    #[test]
    fn a_single_configured_api_candidate_is_the_plan() {
        let plan = resolve_access(
            "mistral/mistral-small-latest",
            &[api("mistral", true, "MISTRAL_API_KEY")],
            None,
            None,
        )
        .expect("one configured candidate must resolve");
        assert_eq!(plan.model, "mistral/mistral-small-latest");
        assert_eq!(plan.provider, "mistral");
        assert_eq!(plan.access, "mistral");
        assert_eq!(plan.chosen, AccessClass::Api);
        assert_eq!(plan.billing, BillingClass::ApiMetered);
        assert!(!plan.pinned);
        assert!(plan.rejected.is_empty());
    }

    #[test]
    fn an_unconfigured_candidate_is_rejected_with_its_fix_var() {
        let plan = resolve_access(
            "openai/gpt-5",
            &[
                api("openai", false, "OPENAI_API_KEY"),
                local("lmstudio", true),
            ],
            None,
            None,
        )
        .expect("the local path must survive");
        assert_eq!(plan.access, "lmstudio");
        assert_eq!(plan.chosen, AccessClass::Local);
        assert_eq!(plan.rejected.len(), 1);
        let r = &plan.rejected[0];
        assert_eq!(r.access, "openai");
        assert_eq!(r.dimension, RejectionDimension::NotConfigured);
        assert_eq!(r.layer, RejectionLayer::Access);
        assert!(r.witness.contains("OPENAI_API_KEY unset"), "{}", r.witness);
    }

    #[test]
    fn sovereign_order_prefers_local_over_metered_api() {
        let plan = resolve_access(
            "qwen/qwen3",
            &[
                api("qwen", true, "DASHSCOPE_API_KEY"),
                local("ollama", true),
            ],
            None,
            None,
        )
        .expect("both admissible");
        assert_eq!(plan.access, "ollama");
        assert_eq!(plan.chosen, AccessClass::Local);
        // The outranked api path FAILED nothing — it stays available,
        // silently (a pin can still name it), never a fake witness.
        assert!(plan.rejected.is_empty());
    }

    #[test]
    fn a_pin_by_class_forces_the_outranked_path() {
        let plan = resolve_access(
            "qwen/qwen3",
            &[
                local("ollama", true),
                api("qwen", true, "DASHSCOPE_API_KEY"),
            ],
            None,
            Some("api"),
        )
        .expect("the pinned api path is admissible");
        assert_eq!(plan.access, "qwen");
        assert_eq!(plan.chosen, AccessClass::Api);
        assert!(plan.pinned);
        assert_eq!(plan.rejected.len(), 1);
        assert_eq!(
            plan.rejected[0].dimension,
            RejectionDimension::PinUnsatisfied
        );
        assert_eq!(plan.rejected[0].layer, RejectionLayer::Pin);
    }

    #[test]
    fn a_pin_by_id_matches_the_candidate_id() {
        let plan = resolve_access(
            "ollama/llama3.2",
            &[local("ollama", true)],
            None,
            Some("ollama"),
        )
        .expect("pin names the only candidate");
        assert_eq!(plan.access, "ollama");
        assert!(plan.pinned);
    }

    #[test]
    fn an_unsatisfied_pin_refuses_and_never_substitutes() {
        // A-4: both paths are admissible, the pin names neither — the
        // resolver must refuse, never quietly hand back a survivor.
        let refusal = resolve_access(
            "qwen/qwen3",
            &[
                local("ollama", true),
                api("qwen", true, "DASHSCOPE_API_KEY"),
            ],
            None,
            Some("harness"),
        )
        .expect_err("an unsatisfied pin is a refusal");
        assert_eq!(refusal.provider, "qwen");
        assert_eq!(refusal.rejected.len(), 2);
        for r in &refusal.rejected {
            assert_eq!(r.dimension, RejectionDimension::PinUnsatisfied);
            assert!(r.witness.contains("--access harness"), "{}", r.witness);
        }
    }

    #[test]
    fn policy_allowlist_rejects_the_provider_with_a_witness() {
        let allowed = vec![String::from("ollama"), String::from("mistral")];
        let refusal = resolve_access(
            "openai/gpt-5",
            &[api("openai", true, "OPENAI_API_KEY")],
            Some(&allowed),
            None,
        )
        .expect_err("provider outside the allowlist");
        assert_eq!(refusal.rejected.len(), 1);
        let r = &refusal.rejected[0];
        assert_eq!(r.dimension, RejectionDimension::ProviderNotAllowed);
        assert_eq!(r.layer, RejectionLayer::Policy);
        assert!(r.witness.contains("openai"), "{}", r.witness);
    }

    #[test]
    fn the_access_step_wins_the_witness_over_policy_and_pin() {
        // Steps 4→5→6: an unconfigured candidate under a forbidding
        // policy AND a foreign pin still teaches the credential first.
        let allowed = vec![String::from("nobody")];
        let refusal = resolve_access(
            "openai/gpt-5",
            &[api("openai", false, "OPENAI_API_KEY")],
            Some(&allowed),
            Some("harness"),
        )
        .expect_err("nothing survives");
        assert_eq!(refusal.rejected.len(), 1);
        assert_eq!(
            refusal.rejected[0].dimension,
            RejectionDimension::NotConfigured
        );
    }

    #[test]
    fn no_candidates_refuses_with_the_parsed_provider() {
        let refusal = resolve_access("mistral/mistral-small-latest", &[], None, None)
            .expect_err("nothing to choose from");
        assert_eq!(refusal.model, "mistral/mistral-small-latest");
        assert_eq!(refusal.provider, "mistral");
        assert!(refusal.rejected.is_empty());
    }

    #[test]
    fn same_class_ties_break_on_codepoint_id_order() {
        let plan = resolve_access(
            "prov/m",
            &[local("bbb", true), local("aaa", true)],
            None,
            None,
        )
        .expect("both admissible");
        assert_eq!(plan.access, "aaa");
    }

    #[test]
    fn billing_evidence_override_reaches_the_plan() {
        let candidate = AccessCandidate::new("anthropic", AccessClass::Api, true)
            .with_billing(BillingClass::CreditMetered);
        let plan = resolve_access("anthropic/claude-sonnet-4-6", &[candidate], None, None)
            .expect("admissible");
        assert_eq!(plan.billing, BillingClass::CreditMetered);
    }

    #[test]
    fn candidates_bridge_reads_the_probe_truth() {
        use crate::registry::{ProviderRegistry, ProvidersConfig};
        let registry = ProviderRegistry::without_http(ProvidersConfig::new());
        let probes = crate::probe::collect_provider_probes(&registry);
        let candidates = candidates_for(&probes, "ollama");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].access, "ollama");
        assert_eq!(candidates[0].class, AccessClass::Local);
        // Keyless local: no fix var to teach.
        assert!(candidates[0].fix_var.is_none());
        let candidates = candidates_for(&probes, "mistral");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].class, AccessClass::Api);
        assert!(candidates[0].fix_var.is_some());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    fn class_strategy() -> impl Strategy<Value = AccessClass> {
        prop_oneof![
            Just(AccessClass::Local),
            Just(AccessClass::Api),
            Just(AccessClass::Harness),
            Just(AccessClass::Oauth),
            Just(AccessClass::Mock),
        ]
    }

    prop_compose! {
        fn candidate_strategy()(
            id in "[a-e]{1,6}",
            class in class_strategy(),
            configured in any::<bool>(),
            fix in proptest::option::of("[A-Z_]{3,12}"),
        ) -> AccessCandidate {
            let candidate = AccessCandidate::new(id, class, configured);
            match fix {
                Some(var) => candidate.with_fix_var(var),
                None => candidate,
            }
        }
    }

    /// A deterministic Fisher-Yates from a seed — the shuffle itself
    /// must not depend on ambient randomness (instrument law).
    fn shuffle(mut v: Vec<AccessCandidate>, seed: u64) -> Vec<AccessCandidate> {
        let mut state = seed | 1;
        for i in (1..v.len()).rev() {
            // Numerical Recipes LCG — cheap, deterministic, test-only.
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            #[allow(clippy::cast_possible_truncation)]
            let j = (state % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
        v
    }

    proptest! {
        /// THE determinism law: enumeration order never matters — the
        /// resolver totally orders candidates internally.
        #[test]
        fn permutation_never_changes_the_outcome(
            candidates in proptest::collection::vec(candidate_strategy(), 0..8),
            pin in proptest::option::of("[a-e]{1,6}"),
            allow in proptest::option::of(proptest::collection::vec("[a-e]{1,6}", 0..3)),
            seed in any::<u64>(),
        ) {
            let first = resolve_access("prov/m", &candidates, allow.as_deref(), pin.as_deref());
            let shuffled = shuffle(candidates, seed);
            let second = resolve_access("prov/m", &shuffled, allow.as_deref(), pin.as_deref());
            prop_assert_eq!(first, second);
        }

        /// Witness totality: every rejection names an input candidate
        /// and carries a non-empty witness; a chosen path is genuinely
        /// admissible; a refusal accounts for EVERY candidate.
        #[test]
        fn witnesses_are_total_and_the_chosen_is_admissible(
            candidates in proptest::collection::vec(candidate_strategy(), 0..8),
            pin in proptest::option::of("[a-e]{1,6}"),
            allow in proptest::option::of(proptest::collection::vec("[a-e]{1,6}", 0..3)),
        ) {
            let ids: Vec<&str> = candidates.iter().map(|c| c.access.as_str()).collect();
            match resolve_access("prov/m", &candidates, allow.as_deref(), pin.as_deref()) {
                Ok(plan) => {
                    prop_assert!(ids.contains(&plan.access.as_str()));
                    prop_assert!(plan.rejected.len() < candidates.len());
                    for r in &plan.rejected {
                        prop_assert!(ids.contains(&r.access.as_str()));
                        prop_assert!(!r.witness.is_empty());
                    }
                    let chosen = candidates
                        .iter()
                        .find(|c| c.access == plan.access && c.class == plan.chosen)
                        .expect("the chosen id must exist among inputs");
                    prop_assert!(chosen.configured);
                    if let Some(list) = allow.as_deref() {
                        prop_assert!(list.iter().any(|p| p == "prov"));
                    }
                    if let Some(p) = pin.as_deref() {
                        prop_assert!(plan.pinned);
                        prop_assert!(p == plan.access || p == plan.chosen.as_str());
                    }
                }
                Err(refusal) => {
                    prop_assert_eq!(refusal.rejected.len(), candidates.len());
                    for r in &refusal.rejected {
                        prop_assert!(ids.contains(&r.access.as_str()));
                        prop_assert!(!r.witness.is_empty());
                    }
                }
            }
        }
    }
}
