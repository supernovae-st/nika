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
    AccessClass, AccessPlan, AccessRejection, BillingClass, HarnessRuntime, RejectionDimension,
    RejectionLayer,
};

use crate::probe::ProviderProbe;

/// One enumerated way to reach a provider — the resolver's INPUT row.
/// A provider row yields its profile's candidate (class · key state);
/// a harness-class probe row yields the ADAPTER's beside it (R-5c ·
/// one channel, one derivation — `oauth` lands with its own adapter).
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

/// The provider prefix of a `provider/name` model id (the whole id
/// when no slash rides it) — the ONE parse every access lane shares
/// (a fourth hand copy is how lanes drift).
#[must_use]
pub fn provider_of(model: &str) -> &str {
    model.split_once('/').map_or(model, |(prefix, _)| prefix)
}

/// A pin names a path by its ID or by its CLASS wire string —
/// `--access ollama` and `--access local` both read naturally, and the
/// shipped agentic CLI tokens (`claude-code` · `codex` · …) ride the
/// same grammar.
fn pin_matches(pin: &str, candidate: &AccessCandidate) -> bool {
    pin == candidate.access || pin == candidate.class.as_str()
}

/// The strict total order (rank · id · configured-first · class ·
/// billing · fix-var) — total over ANY input, so even pathological
/// duplicate ids resolve identically under permutation. `fix_var`
/// keys as the `Option` itself (`None < Some("")` · the empty-string
/// collapse broke totality — the review's P0). The class discriminant
/// immunizes against two FUTURE classes sharing the fallback rank;
/// the billing leg is an arbitrary-but-deterministic tiebreak
/// (codepoint order · twins differing only in billing are
/// pathological inputs, not machine truth).
fn order_key(c: &AccessCandidate) -> (u8, &str, bool, &'static str, &'static str, Option<&str>) {
    (
        sovereign_rank(c.class),
        c.access.as_str(),
        !c.configured,
        c.class.as_str(),
        c.billing.as_str(),
        c.fix_var.as_deref(),
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
        // The harness class's fix is the harness's OWN sign-in gesture
        // (never a credential nika holds) — the fix line rides verbatim;
        // every other class names the env var to set. An EMPTY fix line
        // never renders (the witness totality law · proptest 2026-08-07):
        // the default gesture stands in.
        let witness = if candidate.class == AccessClass::Harness {
            candidate
                .fix_var
                .as_deref()
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .map_or_else(
                    || String::from("not signed in to the harness itself"),
                    str::to_owned,
                )
        } else {
            candidate.fix_var.as_ref().map_or_else(
                || String::from("no credential configured"),
                |var| format!("{var} unset"),
            )
        };
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
    let provider = provider_of(model);
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
/// engine maps the variants 1:1 onto NIKA-1802/1801/1800/1803; the
/// message rides here so every consumer speaks one voice (A-8).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PinRefusal {
    /// The token is neither an access class nor a known agentic CLI
    /// nor an id this machine offers (NIKA-1802) — taught before any
    /// resolution runs.
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
    /// A known agentic CLI token (or the `harness` class) cannot run
    /// here (NIKA-1803) — binary absent, ACP speaker missing, or this
    /// nika was built without adapters. Never an unknown-token 1802.
    Unavailable {
        /// Dummy-readable install / rebuild line.
        message: String,
    },
}

/// Judge an explicit `--access` pin over statically-known models — the
/// admission gate's core (D-2026-08-04-N1 · pure · zero I/O). A pin
/// token is an access CLASS wire string, a shipped agentic CLI id
/// ([`HarnessRuntime`]), or a provider id this machine offers.
/// Known CLI tokens are NEVER NIKA-1802 — a missing binary is 1803
/// with a dummy-readable install line. `None` = the pin is satisfied
/// everywhere (or nothing static to judge).
#[must_use]
pub fn refuse_pin<'m>(
    models: impl IntoIterator<Item = &'m str>,
    probes: &[ProviderProbe],
    pin: &str,
) -> Option<PinRefusal> {
    if let Some(retired) = HarnessRuntime::retired_alias(pin) {
        return Some(PinRefusal::UnknownToken {
            message: format!(
                "`--access {pin}` is retired — use `--access {}` for {} \
                 (known: {})",
                retired.id,
                retired.display,
                pin_vocabulary()
            ),
        });
    }
    if let Some(rt) = HarnessRuntime::lookup(pin) {
        return refuse_named_runtime(rt, probes);
    }
    if pin == AccessClass::Harness.as_str() {
        return refuse_harness_class(probes);
    }
    let class_token = AccessClass::ALL.iter().any(|c| c.as_str() == pin);
    if !class_token && !probes.iter().any(|p| p.id == pin) {
        return Some(PinRefusal::UnknownToken {
            message: format!(
                "`--access {pin}` names neither an access class nor a known \
                 agentic CLI ({}) — `nika doctor` lists every path",
                pin_vocabulary()
            ),
        });
    }
    for model in models {
        let candidates = candidates_for(probes, provider_of(model));
        if let Err(refusal) = resolve_access(model, &candidates, None, Some(pin)) {
            return Some(classify_pin_refusal(model, pin, &refusal));
        }
    }
    None
}

/// Judge a pin with the verb capability boundary applied. An infer-only
/// workflow may bypass provider serving only through a proved one-shot seat;
/// agentic readiness alone never satisfies that meet.
#[must_use]
#[cfg(feature = "access-harness")]
pub fn refuse_pin_for_verbs<'m>(
    models: impl IntoIterator<Item = &'m str>,
    probes: &[ProviderProbe],
    pin: &str,
    has_infer: bool,
    has_agent: bool,
) -> Option<PinRefusal> {
    let models: Vec<&str> = models.into_iter().collect();
    let infer_only = has_infer && !has_agent;
    let named = HarnessRuntime::lookup(pin);
    if (named.is_some() || pin == AccessClass::Harness.as_str())
        && models.iter().any(|model| provider_of(model) == "mock")
    {
        return Some(PinRefusal::PinUnsatisfied {
            message: String::from(
                "model `mock/echo` is the isolated rehearsal backend and cannot run through a \
                 harness seat — use `--access mock` (refusal, never a live substitute)",
            ),
        });
    }
    let direct = named.is_some_and(|rt| infer_only && named_infer_grade_ready(rt, probes));
    let generic = pin == AccessClass::Harness.as_str()
        && infer_only
        && first_ready_infer_harness(probes).is_some();
    if !direct
        && !generic
        && let Some(refusal) = refuse_pin(models.iter().copied(), probes, pin)
    {
        return Some(refusal);
    }
    let seat = named.map(|rt| rt.id).or_else(|| {
        (pin == AccessClass::Harness.as_str())
            .then(|| first_ready_infer_harness(probes).unwrap_or(AccessClass::Harness.as_str()))
    });
    if has_infer
        && let Some(seat) = seat
        && let Err(error) =
            nika_harness::meet_infer_grade(seat, nika_harness::StructuredOutputGrade::JsonSchema)
    {
        return Some(PinRefusal::NoPath {
            message: error.to_string(),
        });
    }
    None
}

/// Feature-off twin: no harness capability can satisfy the pin.
#[must_use]
#[cfg(not(feature = "access-harness"))]
pub fn refuse_pin_for_verbs<'m>(
    models: impl IntoIterator<Item = &'m str>,
    probes: &[ProviderProbe],
    pin: &str,
    _has_infer: bool,
    _has_agent: bool,
) -> Option<PinRefusal> {
    refuse_pin(models, probes, pin)
}

fn pin_vocabulary() -> String {
    format!(
        "{} · {}",
        AccessClass::ALL.map(AccessClass::as_str).join(" \u{b7} "),
        HarnessRuntime::vocabulary()
    )
}

fn any_harness_row(probes: &[ProviderProbe]) -> bool {
    probes
        .iter()
        .any(|p| p.readiness.access == AccessClass::Harness)
}

fn adapters_not_compiled_in() -> PinRefusal {
    PinRefusal::Unavailable {
        message: String::from(
            "This nika binary was built without agentic CLI adapters. \
             Rebuild with --features access-harness, or pick Nika local / Nika Cloud.",
        ),
    }
}

const NO_RUNTIME_INSTALLED: &str = "No agentic CLI runtime is installed. Install \
     Claude Code, Codex, Gemini CLI, Kimi Code or Qwen Code, or pick Nika local / Nika Cloud.";

fn refuse_named_runtime(rt: HarnessRuntime, probes: &[ProviderProbe]) -> Option<PinRefusal> {
    if !any_harness_row(probes) {
        return Some(adapters_not_compiled_in());
    }
    let Some(row) = probes.iter().find(|p| p.id == rt.id) else {
        return Some(PinRefusal::Unavailable {
            message: rt.not_installed.to_owned(),
        });
    };
    if !row.key_present {
        let message = if row.fix_var.is_empty() {
            rt.not_installed.to_owned()
        } else {
            row.fix_var.clone()
        };
        return Some(PinRefusal::Unavailable { message });
    }
    if !row.readiness.configured {
        return Some(PinRefusal::NoPath {
            message: if row.fix_var.is_empty() {
                rt.not_signed_in()
            } else {
                row.fix_var.clone()
            },
        });
    }
    None
}

fn refuse_harness_class(probes: &[ProviderProbe]) -> Option<PinRefusal> {
    if !any_harness_row(probes) {
        return Some(PinRefusal::Unavailable {
            message: NO_RUNTIME_INSTALLED.to_owned(),
        });
    }
    let harness: Vec<&ProviderProbe> = probes
        .iter()
        .filter(|p| p.readiness.access == AccessClass::Harness)
        .collect();
    if harness
        .iter()
        .any(|p| p.key_present && p.readiness.configured)
    {
        return None;
    }
    if harness.iter().any(|p| p.key_present) {
        return Some(PinRefusal::NoPath {
            message: String::from(
                "An agentic CLI is installed but not signed in. Sign in to that \
                 CLI, or pick Nika local / Nika Cloud.",
            ),
        });
    }
    Some(PinRefusal::Unavailable {
        message: NO_RUNTIME_INSTALLED.to_owned(),
    })
}

/// First ready harness id in G-3 order (detected + signed in).
#[must_use]
pub fn first_ready_harness(probes: &[ProviderProbe]) -> Option<&str> {
    for rt in HarnessRuntime::ALL {
        if probes
            .iter()
            .any(|p| p.id == rt.id && p.key_present && p.readiness.configured)
        {
            return Some(rt.id);
        }
    }
    None
}

/// First ready seat whose adapter has a proved infer-grade row. The
/// ordinary harness order remains the agent-loop order; this projection is
/// deliberately capability-specific so a ready ACP-only seat cannot shadow
/// Codex for a one-shot `infer:`.
#[cfg(feature = "access-harness")]
#[must_use]
pub fn first_ready_infer_harness(probes: &[ProviderProbe]) -> Option<&str> {
    HarnessRuntime::ALL.into_iter().find_map(|rt| {
        let ready = probes
            .iter()
            .any(|p| p.id == rt.id && p.readiness.configured);
        (ready
            && nika_harness::meet_infer_grade(rt.id, nika_harness::StructuredOutputGrade::Text)
                .is_ok())
        .then_some(rt.id)
    })
}

#[cfg(feature = "access-harness")]
fn named_infer_grade_ready(rt: HarnessRuntime, probes: &[ProviderProbe]) -> bool {
    probes.iter().any(|probe| {
        probe.id == rt.id
            && probe.readiness.configured
            && nika_harness::meet_infer_grade(rt.id, nika_harness::StructuredOutputGrade::Text)
                .is_ok()
    })
}

#[cfg(not(feature = "access-harness"))]
const fn named_infer_grade_ready(_rt: HarnessRuntime, _probes: &[ProviderProbe]) -> bool {
    false
}

/// All-`pin_unsatisfied` (or empty) = the pin names nothing usable ·
/// any other dimension = the pinned path itself failed. Witnesses ride
/// the message either way (a refusal is never a shrug · A-8).
fn classify_pin_refusal(model: &str, pin: &str, refusal: &AccessRefusal) -> PinRefusal {
    if refusal.rejected.is_empty() {
        // Zero candidates enumerated: the provider itself is unknown to
        // this binary — name IT, never a witness-less shrug (A-8). The
        // MODELS rung is advisory, so this CAN reach the run-time gate.
        return PinRefusal::PinUnsatisfied {
            message: format!(
                "model `{model}` names provider `{}` — no access candidate \
                 exists for it here (`nika doctor` lists the providers this \
                 binary drives)",
                refusal.provider
            ),
        };
    }
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
/// offers for ONE provider. Two row kinds ride the SAME vec (R-5c · one
/// channel, one derivation): a PROFILE row matches by `id`; a HARNESS
/// row (an adapter detected on this machine · `serves` set ·
/// `access == Harness`) matches every provider it fronts. The mock
/// backend is compiled in and keyless — probes exclude it, the bridge
/// must not (a `mock/echo` rehearsal is always an offered path).
#[must_use]
pub fn candidates_for(probes: &[ProviderProbe], provider: &str) -> Vec<AccessCandidate> {
    if provider == "mock" {
        return vec![AccessCandidate::new("mock", AccessClass::Mock, true)];
    }
    probes
        .iter()
        .filter(|p| {
            p.id == provider
                || (p.readiness.access == AccessClass::Harness
                    && p.serves.iter().any(|s| s == provider))
        })
        .map(|p| {
            if p.readiness.access == AccessClass::Harness {
                harness_candidate(p)
            } else {
                profile_candidate(p)
            }
        })
        .collect()
}

/// The candidate a PROFILE probe row yields: the provider's own class
/// and key state, the conventional env var named when a required key
/// is absent.
fn profile_candidate(p: &ProviderProbe) -> AccessCandidate {
    let candidate = AccessCandidate::new(p.id.clone(), p.readiness.access, p.readiness.configured);
    if p.requires_key {
        candidate.with_fix_var(p.fix_var.clone())
    } else {
        candidate
    }
}

/// The candidate a HARNESS probe row yields (R-5c): the ADAPTER is the
/// path (sovereign rank 2 — the operator's own plan beats the metered
/// key), and an unauthenticated adapter teaches its own sign-in gesture
/// verbatim (the judge's Harness arm prints it instead of `<var> unset`).
fn harness_candidate(p: &ProviderProbe) -> AccessCandidate {
    let candidate =
        AccessCandidate::new(p.id.clone(), AccessClass::Harness, p.readiness.configured);
    if p.readiness.configured {
        candidate
    } else {
        candidate.with_fix_var(format!("sign in to `{}` itself", p.id))
    }
}

/// The admission-time access decision per model (R-1/R-2 · P3 B5) —
/// ONE derivation for the trace prologue's `access_plan` stamp and the
/// resume identity's `access.` keys, over the composer's probe rows.
/// Templated models (`${{ }}`) are not static facts and never appear;
/// a model the resolver refuses is ABSENT from the map (absent is
/// honest — never a guessed row, and the resume pair stamps the same
/// absence on both sides).
#[must_use]
pub fn access_plan_map(
    models: &[String],
    probes: &[ProviderProbe],
    pin: Option<&str>,
) -> std::collections::BTreeMap<String, AccessPlan> {
    // An explicit ready harness pin is the infer/agent path for EVERY
    // static model (the envelope `model:` is a hint, not a serves-filter).
    if let Some(pin) = pin {
        let requests_mock = models
            .iter()
            .any(|model| !model.contains("${{") && provider_of(model) == "mock");
        if requests_mock
            && (HarnessRuntime::lookup(pin).is_some() || pin == AccessClass::Harness.as_str())
        {
            return std::collections::BTreeMap::new();
        }
        if let Some(rt) = HarnessRuntime::lookup(pin) {
            let infer_grade_ready = named_infer_grade_ready(rt, probes);
            if infer_grade_ready || refuse_named_runtime(rt, probes).is_none() {
                return stamp_harness_plans(models, rt.id);
            }
            return std::collections::BTreeMap::new();
        }
        if pin == AccessClass::Harness.as_str() {
            #[cfg(feature = "access-harness")]
            let ready = first_ready_infer_harness(probes);
            #[cfg(not(feature = "access-harness"))]
            let ready = first_ready_harness(probes);
            if let Some(id) = ready {
                return stamp_harness_plans(models, id);
            }
            return std::collections::BTreeMap::new();
        }
    }
    models
        .iter()
        .map(String::as_str)
        .filter(|m| !m.contains("${{"))
        .filter_map(|model| {
            let candidates = candidates_for(probes, provider_of(model));
            resolve_access(model, &candidates, None, pin)
                .ok()
                .map(|plan| (model.to_owned(), plan))
        })
        .collect()
}

fn stamp_harness_plans(
    models: &[String],
    access: &str,
) -> std::collections::BTreeMap<String, AccessPlan> {
    models
        .iter()
        .filter(|m| !m.contains("${{"))
        .map(|model| {
            (
                model.clone(),
                AccessPlan::new(
                    model,
                    provider_of(model),
                    access,
                    AccessClass::Harness,
                    BillingClass::IncludedQuota,
                    true,
                    Vec::new(),
                ),
            )
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

    // ── P3 B6 · harness adapters ride the probe vec (R-5c) ────────

    /// A harness-class probe row as the composer hands it over once the
    /// adapter is detected on this machine: `serves` names the providers
    /// it fronts, `configured` carries the auth surface's verdict.
    fn harness_probe(id: &str, serves: &[&str], authenticated: bool) -> ProviderProbe {
        ProviderProbe::new(
            id,
            false,
            true,
            "",
            false,
            crate::probe::ProviderReadiness::new(
                true,
                authenticated,
                None,
                None,
                false,
                crate::probe::ExecutionLocus::Cloud,
                AccessClass::Harness,
            ),
            "",
        )
        .with_serves(serves.iter().map(|s| (*s).to_owned()).collect())
    }

    #[test]
    fn a_detected_authenticated_adapter_row_becomes_a_harness_candidate() {
        let probes = vec![harness_probe("codex", &["openai"], true)];
        let candidates = candidates_for(&probes, "openai");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].access, "codex");
        assert_eq!(candidates[0].class, AccessClass::Harness);
        assert!(candidates[0].configured);
    }

    #[test]
    fn a_harness_row_offers_nothing_to_a_provider_it_does_not_serve() {
        let probes = vec![harness_probe("codex", &["openai"], true)];
        assert!(candidates_for(&probes, "anthropic").is_empty());
    }

    #[test]
    fn an_unauthenticated_adapter_row_is_a_candidate_marked_unconfigured() {
        let probes = vec![harness_probe("codex", &["openai"], false)];
        let candidates = candidates_for(&probes, "openai");
        assert_eq!(
            candidates.len(),
            1,
            "the refusal must NAME the harness's sign-in"
        );
        assert!(!candidates[0].configured);
        let refusal = resolve_access("openai/gpt-5", &candidates, None, None)
            .expect_err("unauthenticated never resolves");
        let witness = &refusal.rejected[0].witness;
        assert!(
            witness.contains("sign in to `codex` itself"),
            "the harness fix line rides verbatim, never `unset`: {witness}"
        );
        assert!(!witness.contains("unset"), "{witness}");
    }

    #[test]
    fn the_harness_row_outranks_the_metered_api_when_both_serve() {
        // The sovereign order (local < mock < harness < oauth < api):
        // an authenticated harness wins over the metered key — the
        // access doctrine's whole point (the operator's own plan first).
        let api_row = ProviderProbe::new(
            "anthropic",
            true,
            true,
            "ANTHROPIC_API_KEY",
            true,
            crate::probe::ProviderReadiness::new(
                true,
                true,
                None,
                None,
                true,
                crate::probe::ExecutionLocus::Cloud,
                AccessClass::Api,
            ),
            "https://api.anthropic.com",
        );
        let harness_row = harness_probe("claude-code", &["anthropic"], true);
        let plan = resolve_access(
            "anthropic/claude-sonnet-4-5",
            &candidates_for(&[api_row, harness_row], "anthropic"),
            None,
            None,
        )
        .expect("both paths configured → a plan");
        assert_eq!(plan.access, "claude-code");
        assert_eq!(plan.chosen, AccessClass::Harness);
        assert_eq!(
            plan.rejected.len(),
            0,
            "the api row is outranked, NOT rejected (dispo au pin): {:?}",
            plan.rejected
        );
    }

    #[test]
    fn a_pin_names_an_adapter_id_once_its_row_rides_the_probes() {
        let probes = vec![harness_probe("codex", &["openai"], true)];
        assert!(
            refuse_pin(["openai/gpt-5"], &probes, "codex").is_none(),
            "a detected adapter id is a legal pin"
        );
        assert!(
            matches!(
                refuse_pin(["openai/gpt-5"], &[], "codex"),
                Some(PinRefusal::Unavailable { .. })
            ),
            "known token + no harness rows → 1803, never 1802"
        );
    }

    #[test]
    fn a_known_cli_token_is_never_1802() {
        for id in [
            "claude-code",
            "codex",
            "gemini-cli",
            "kimi-code",
            "qwen-code",
        ] {
            match refuse_pin(["ollama/qwen3.5:4b"], &[], id) {
                Some(PinRefusal::Unavailable { message }) => {
                    assert!(
                        !message.contains("NIKA-1802") && !message.contains("neither"),
                        "{id}: {message}"
                    );
                }
                other => panic!("{id} must be a known token, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_missing_cli_teaches_the_dummy_install_line() {
        let rt = nika_types::access::HarnessRuntime::CLAUDE_CODE;
        let probes = vec![harness_probe_absent(rt.id, rt.not_installed)];
        match refuse_pin(["ollama/qwen3.5:4b"], &probes, "claude-code") {
            Some(PinRefusal::Unavailable { message }) => {
                assert!(
                    message.contains("Claude Code is not installed"),
                    "{message}"
                );
                assert!(message.contains("Nika local / Nika Cloud"), "{message}");
            }
            other => panic!("expected 1803 Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn a_retired_alias_teaches_the_live_token() {
        match refuse_pin(["openai/gpt-5"], &[], "claude-agent-acp") {
            Some(PinRefusal::UnknownToken { message }) => {
                assert!(message.contains("retired"), "{message}");
                assert!(message.contains("--access claude-code"), "{message}");
            }
            other => panic!("retired alias must be 1802, got {other:?}"),
        }
        match refuse_pin(["openai/gpt-5"], &[], "codex-acp") {
            Some(PinRefusal::UnknownToken { message }) => {
                assert!(message.contains("--access codex"), "{message}");
            }
            other => panic!("retired alias must be 1802, got {other:?}"),
        }
    }

    #[test]
    fn access_harness_class_with_no_runtime_is_1803_not_api_keys() {
        let api_row = ProviderProbe::new(
            "anthropic",
            true,
            false,
            "ANTHROPIC_API_KEY",
            true,
            crate::probe::ProviderReadiness::new(
                true,
                false,
                None,
                None,
                true,
                crate::probe::ExecutionLocus::Cloud,
                AccessClass::Api,
            ),
            "https://api.anthropic.com",
        );
        match refuse_pin(["anthropic/claude-sonnet-4-5"], &[api_row], "harness") {
            Some(PinRefusal::Unavailable { message }) => {
                assert!(
                    message.contains("No agentic CLI runtime is installed"),
                    "{message}"
                );
                assert!(!message.contains("ANTHROPIC_API_KEY"), "{message}");
                assert!(!message.contains("GEMINI_API_KEY"), "{message}");
            }
            other => panic!("--access harness must not walk API keys, got {other:?}"),
        }
    }

    fn harness_probe_absent(id: &str, message: &str) -> ProviderProbe {
        ProviderProbe::new(
            id,
            false,
            false,
            message,
            false,
            crate::probe::ProviderReadiness::new(
                true,
                false,
                None,
                None,
                false,
                crate::probe::ExecutionLocus::Cloud,
                AccessClass::Harness,
            ),
            "",
        )
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

    /// `order_key`'s configured-first leg (the `!` is load-bearing):
    /// twin candidates differing only in key state resolve to the
    /// CONFIGURED one, whichever order they arrive in.
    #[test]
    fn the_configured_twin_wins_either_order() {
        for pair in [
            vec![
                api("mistral", false, "MISTRAL_API_KEY"),
                api("mistral", true, "MISTRAL_API_KEY"),
            ],
            vec![
                api("mistral", true, "MISTRAL_API_KEY"),
                api("mistral", false, "MISTRAL_API_KEY"),
            ],
        ] {
            let plan = resolve_access("mistral/mistral-large", &pair, None, None)
                .expect("the configured twin resolves");
            assert_eq!(plan.access, "mistral");
            assert_eq!(plan.rejected.len(), 1, "the unconfigured twin is rejected");
            assert!(
                plan.rejected[0].witness.contains("MISTRAL_API_KEY unset"),
                "with its witness: {}",
                plan.rejected[0].witness
            );
        }
    }

    /// The FULL sovereign chain, pinned pairwise (mutation-killers for
    /// every `sovereign_rank` arm — a deleted arm must flip a winner).
    #[test]
    fn the_sovereign_chain_is_total_local_mock_harness_oauth_api() {
        let all = [
            AccessCandidate::new("the-local", AccessClass::Local, true),
            AccessCandidate::new("the-mock", AccessClass::Mock, true),
            AccessCandidate::new("the-harness", AccessClass::Harness, true),
            AccessCandidate::new("the-oauth", AccessClass::Oauth, true),
            AccessCandidate::new("the-api", AccessClass::Api, true),
        ];
        for (winner, loser_rank) in [
            ("the-local", 0),
            ("the-mock", 1),
            ("the-harness", 2),
            ("the-oauth", 3),
        ] {
            let _ = loser_rank;
            let idx = all
                .iter()
                .position(|c| c.access == winner)
                .expect("present");
            let mut subset = all[idx..].to_vec();
            subset.reverse(); // enumeration order must never matter
            let plan = resolve_access("p/m", &subset, None, None).expect("configured");
            assert_eq!(
                plan.access, winner,
                "against every lower-ranked class, {winner} wins"
            );
        }
    }

    /// `classify_pin_refusal` (NIKA-1800 vs 1801): the pin-layer-only
    /// refusal reads 1801; a candidate failing EARLIER (not configured)
    /// reads 1800 with its access-layer witness.
    #[test]
    fn the_pin_refusal_classification_reads_the_failing_layer() {
        // Every rejection at the PIN layer → PinUnsatisfied (1801).
        let pin_refusal = resolve_access(
            "openai/gpt-5",
            &[
                api("openai", true, "OPENAI_API_KEY"),
                AccessCandidate::new("codex", AccessClass::Harness, true),
            ],
            None,
            Some("local"),
        )
        .expect_err("nothing matches the pin");
        assert!(matches!(
            classify_pin_refusal("openai/gpt-5", "local", &pin_refusal),
            PinRefusal::PinUnsatisfied { .. }
        ));
        // The api row unconfigured → it fails at the ACCESS layer first
        // → NoPath (1800) naming that witness.
        let access_refusal = resolve_access(
            "openai/gpt-5",
            &[api("openai", false, "OPENAI_API_KEY")],
            None,
            Some("codex"),
        )
        .expect_err("unconfigured fails before the pin");
        assert!(matches!(
            classify_pin_refusal("openai/gpt-5", "codex", &access_refusal),
            PinRefusal::NoPath { .. }
        ));
    }

    /// `access_plan_map`: a templated model is not a static fact — it
    /// never appears in the map (the `!contains` filter, mutation-pinned).
    #[test]
    fn the_plan_map_skips_templated_models() {
        let map = access_plan_map(
            &["mock/echo".to_owned(), "${{ inputs.model }}".to_owned()],
            &[],
            None,
        );
        assert!(map.contains_key("mock/echo"));
        assert_eq!(map.len(), 1, "the templated row is absent, never guessed");
    }

    #[test]
    fn a_ready_harness_pin_stamps_every_static_model() {
        let probes = vec![harness_probe("claude-code", &["anthropic"], true)];
        let map = access_plan_map(
            &[
                "ollama/qwen3.5:4b".to_owned(),
                "anthropic/claude-sonnet-4-5".to_owned(),
            ],
            &probes,
            Some("claude-code"),
        );
        assert_eq!(map.len(), 2);
        for plan in map.values() {
            assert_eq!(plan.access, "claude-code");
            assert_eq!(plan.chosen, AccessClass::Harness);
            assert_eq!(plan.billing, BillingClass::IncludedQuota);
            assert!(plan.pinned);
        }
        assert_eq!(first_ready_harness(&probes), Some("claude-code"));
    }

    #[test]
    fn a_codex_pin_stamps_requested_model_and_subscription_quota_without_a_price() {
        let probes = vec![harness_probe("codex", &["anthropic"], true)];
        let map = access_plan_map(
            &["anthropic/claude-sonnet-4-6".to_owned()],
            &probes,
            Some("codex"),
        );
        let plan = map
            .get("anthropic/claude-sonnet-4-6")
            .expect("the requested model is receipt evidence");
        assert_eq!(plan.model, "anthropic/claude-sonnet-4-6");
        assert_eq!(plan.access, "codex");
        assert_eq!(plan.billing, BillingClass::IncludedQuota);
        assert!(!plan.billing.is_usd_metered());
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

    /// Pins draw from BOTH grammars the flag accepts: candidate-id
    /// shaped tokens AND the class wire strings (the review's blind
    /// spot: `[a-e]{1,6}` can never spell `local`).
    fn pin_strategy() -> impl Strategy<Value = Option<String>> {
        proptest::option::of(prop_oneof![
            "[a-e]{1,6}".prop_map(String::from),
            class_strategy().prop_map(|c| c.as_str().to_owned()),
        ])
    }

    prop_compose! {
        fn candidate_strategy()(
            id in "[a-e]{1,6}",
            class in class_strategy(),
            configured in any::<bool>(),
            fix in proptest::option::of("[A-Z_]{0,12}"),
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
            pin in pin_strategy(),
            provider in "[a-e]{1,3}",
            allow in proptest::option::of(proptest::collection::vec("[a-e]{1,3}", 0..3)),
            seed in any::<u64>(),
        ) {
            let model = format!("{provider}/m");
            let first = resolve_access(&model, &candidates, allow.as_deref(), pin.as_deref());
            let shuffled = shuffle(candidates, seed);
            let second = resolve_access(&model, &shuffled, allow.as_deref(), pin.as_deref());
            prop_assert_eq!(first, second);
        }

        /// Witness totality: every rejection names an input candidate
        /// and carries a non-empty witness; a chosen path is genuinely
        /// admissible; a refusal accounts for EVERY candidate.
        #[test]
        fn witnesses_are_total_and_the_chosen_is_admissible(
            candidates in proptest::collection::vec(candidate_strategy(), 0..8),
            pin in pin_strategy(),
            provider in "[a-e]{1,3}",
            allow in proptest::option::of(proptest::collection::vec("[a-e]{1,3}", 0..3)),
        ) {
            let model = format!("{provider}/m");
            let ids: Vec<&str> = candidates.iter().map(|c| c.access.as_str()).collect();
            match resolve_access(&model, &candidates, allow.as_deref(), pin.as_deref()) {
                Ok(plan) => {
                    prop_assert!(ids.contains(&plan.access.as_str()));
                    prop_assert!(plan.rejected.len() < candidates.len());
                    for r in &plan.rejected {
                        prop_assert!(ids.contains(&r.access.as_str()));
                        prop_assert!(!r.witness.is_empty());
                    }
                    // (id · class) does not single out a row when twin
                    // ids ride the input (the shrunk regression case:
                    // one configured, one not — the resolver picks the
                    // configured twin). The honest predicate: SOME
                    // configured input carries the chosen (id · class).
                    prop_assert!(
                        candidates.iter().any(|c| c.access == plan.access
                            && c.class == plan.chosen
                            && c.configured),
                        "the chosen path must name a configured input row"
                    );
                    if let Some(list) = allow.as_deref() {
                        prop_assert!(list.iter().any(|p| p == &provider));
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
