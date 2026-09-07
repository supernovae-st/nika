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
    RejectionLayer, Trust,
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
    /// How far this path's identity is proven (ADR-134) — the floor the
    /// probe's evidence earns, raised only by [`Self::with_trust`].
    pub trust: Trust,
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
            trust: Trust::from_evidence(class, configured, None),
        }
    }

    /// The rung the probe's evidence earned (ADR-134).
    #[must_use]
    pub const fn with_trust(mut self, trust: Trust) -> Self {
        self.trust = trust;
        self
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
    let prefix = model.split_once('/').map_or(model, |(prefix, _)| prefix);
    crate::profile::canonical_provider(prefix)
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
            // Custody is NAMED (R4 · « key present » used to render
            // without ever saying WHERE the key lives): keys are
            // env-only, so the witness says the var is unset IN THE
            // PROCESS ENV — derivable, never a value probe.
            candidate.fix_var.as_ref().map_or_else(
                || String::from("no credential configured"),
                |var| format!("{var} unset in process env"),
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
    let mut outranked = Vec::new();
    let mut chosen: Option<&AccessCandidate> = None;
    for candidate in ordered {
        if let Some(rejection) = judge(candidate, provider, allow_providers, pin) {
            rejected.push(rejection);
        } else if let Some(winner) = chosen {
            // W3-F3 · a ready path that lost the ranking stays available to
            // a pin (never rejected) and BOTH rows name it: the witness
            // below is what the prose tail prints (« chosen over api
            // (ready · ranked below `codex` (harness outranks api)) ») and
            // what the JSON carries.
            outranked.push(nika_types::access::AccessRejection::new(
                candidate.access.clone(),
                nika_types::access::RejectionDimension::Outranked,
                nika_types::access::RejectionLayer::Access,
                format!(
                    "ready · ranked below `{}` ({} outranks {})",
                    winner.access,
                    winner.class.as_str(),
                    candidate.class.as_str()
                ),
            ));
        } else {
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
        )
        .with_outranked(outranked)
        .with_trust(c.trust)),
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
    refuse_pin_with(models, probes, pin, VerbNeeds::default())
}

/// [`refuse_pin`] judged for the verbs the workflow carries (W3-F1).
#[must_use]
pub fn refuse_pin_with<'m>(
    models: impl IntoIterator<Item = &'m str>,
    probes: &[ProviderProbe],
    pin: &str,
    verbs: VerbNeeds,
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
        return refuse_named_runtime(rt, probes, verbs);
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
        && let Some(refusal) = refuse_pin_with(
            models.iter().copied(),
            probes,
            pin,
            VerbNeeds::new(has_infer, has_agent),
        )
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
    has_infer: bool,
    has_agent: bool,
) -> Option<PinRefusal> {
    refuse_pin_with(models, probes, pin, VerbNeeds::new(has_infer, has_agent))
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

/// The verbs a pinned seat must serve — decides WHICH binary presence the
/// pin needs: an infer-grade seat spawns the PRODUCT (`codex`), an
/// agent-grade seat speaks ACP (`codex-acp`). Unknown verbs (no
/// `infer:`/`agent:` task at all) accept either (W3-F1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct VerbNeeds {
    /// An `infer:` task rides the seat.
    pub infer: bool,
    /// An `agent:` task rides the seat.
    pub agent: bool,
}

impl VerbNeeds {
    /// Both halves, explicit.
    #[must_use]
    pub fn new(infer: bool, agent: bool) -> Self {
        Self { infer, agent }
    }
}

fn refuse_named_runtime(
    rt: HarnessRuntime,
    probes: &[ProviderProbe],
    verbs: VerbNeeds,
) -> Option<PinRefusal> {
    if !any_harness_row(probes) {
        return Some(adapters_not_compiled_in());
    }
    let Some(row) = probes.iter().find(|p| p.id == rt.id) else {
        return Some(PinRefusal::Unavailable {
            message: rt.not_installed.to_owned(),
        });
    };
    // W3-F1 · the seat is TWO binaries: the product an infer-grade seat
    // spawns and the ACP speaker an agent-grade seat talks to. A pin is
    // judged for the verbs the workflow carries — `doctor` and the dry-run
    // must not disagree on « present ».
    if verbs.infer && !row.product_present {
        return Some(PinRefusal::Unavailable {
            message: rt.not_installed.to_owned(),
        });
    }
    if verbs.agent && !row.key_present {
        let message = if row.fix_var.is_empty() {
            rt.acp_missing()
        } else {
            row.fix_var.clone()
        };
        return Some(PinRefusal::Unavailable { message });
    }
    if !row.key_present && !row.product_present {
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
pub(crate) fn named_infer_grade_ready(rt: HarnessRuntime, probes: &[ProviderProbe]) -> bool {
    // W3-F1 · the infer-grade seat spawns the PRODUCT binary: its presence
    // is the readiness, never the ACP speaker's.
    probes.iter().any(|probe| {
        probe.id == rt.id
            && probe.readiness.configured
            && probe.product_present
            && nika_harness::meet_infer_grade(rt.id, nika_harness::StructuredOutputGrade::Text)
                .is_ok()
    })
}

#[cfg(not(feature = "access-harness"))]
pub(crate) const fn named_infer_grade_ready(
    _rt: HarnessRuntime,
    _probes: &[ProviderProbe],
) -> bool {
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
    let provider = crate::profile::canonical_provider(provider);
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
    let candidate = AccessCandidate::new(p.id.clone(), p.readiness.access, p.readiness.configured)
        .with_trust(Trust::from_evidence(
            p.readiness.access,
            p.readiness.configured,
            p.readiness.reachable,
        ));
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
            if infer_grade_ready || refuse_named_runtime(rt, probes, VerbNeeds::default()).is_none()
            {
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
                    // The lane is never priced from here: a subscription
                    // is not free and its quota is not observable, so the
                    // plan says `unknown` — the same word the task
                    // terminal stamps (never a guessed `included_quota`).
                    BillingClass::Unknown,
                    true,
                    Vec::new(),
                )
                // A pinned seat was found on PATH (the pin refuses when it is
                // absent) — discovered, never observed: nothing dialed it
                // (#1253's trust half · ADR-134).
                .with_trust(Trust::Discovered),
            )
        })
        .collect()
}

#[cfg(test)]
mod prop_tests;
#[cfg(test)]
mod tests;
