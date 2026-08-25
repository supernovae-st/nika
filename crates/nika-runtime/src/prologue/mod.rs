// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `workflow_started` prologue's field helpers + test module (A5
//! evidence fields — the tests split from the crate-root `tests.rs`,
//! which rides the 1,500-LOC ceiling; the field helpers descend here
//! from `lib.rs` under the same ceiling, cohesion intact: a prologue
//! field belongs to the prologue module).

use std::collections::BTreeMap;

use nika_event::{Event, EventKind};
use nika_schema::raw::RawWorkflow;
use nika_types::resource::{KeyValue, Value as FieldValue};

use crate::origins::InputOrigin;
use crate::stamp::{EventSink, Stamper};
use crate::{approval, emit, i, s};

#[cfg(test)]
mod tests;

/// The F-P18 cost fields (NEP-0017 · la table de prix DANS le pin) —
/// additive, the `permits_json` posture: older readers ignore unknown
/// fields, newer readers find them absent where no claim exists.
///
/// - `pricing` — the versioned pricing table that gives sense to the
///   run's ρ (usd), journaled as ONE compact JSON text
///   (`{"as_of","schema","sha256_16"}` — the `outcome` idiom: a nested
///   object rides as a JSON string, the wire's `Value` having no object
///   variant). This IS the law's pin: a future cost-replay reads the
///   run's costs against THIS table — « un coût rejoué en 2031 se lit
///   contre la table 2026 pinnée ». Compile-time statics only
///   ([`nika_catalog::pricing_snapshot`] + [`nika_catalog::PRICING_SCHEMA`]
///   — the `spec_pin` posture: no clock, no I/O, determinism intact);
/// - `budget` — the resolved operator budget (`--max-cost-usd`), ONE
///   compact JSON text (`{"max_cost_usd":dollars}` — dollars ride as a
///   JSON number, the `total_cost_usd` float convention). ABSENT when
///   the run carries no budget (absent is honest — an unbounded run
///   journals no claim, never a fake zero/unbounded). A non-finite
///   budget cannot reach here (the CLI refuses NaN/inf at the flag);
///   the guard stays so the journal can never carry `null` where a
///   number was claimed.
pub(crate) fn cost_pin_fields(max_cost_usd: Option<f64>) -> Vec<(&'static str, FieldValue)> {
    let snapshot = nika_catalog::pricing_snapshot();
    let pin = serde_json::json!({
        "schema": nika_catalog::PRICING_SCHEMA,
        "as_of": snapshot.as_of,
        "sha256_16": snapshot.source_sha256_16,
    });
    let mut fields = vec![("pricing", s(&pin.to_string()))];
    if let Some(budget) = max_cost_usd.filter(|b| b.is_finite()) {
        let resolved = serde_json::json!({ "max_cost_usd": budget });
        fields.push(("budget", s(&resolved.to_string())));
    }
    fields
}

/// The run banner's boundary clause — it reflects the ACTUAL boundary:
/// a declared `permits:` block is a default-deny boundary, so the
/// banner must not keep saying "no boundary declared" once one is
/// present (it misled operators into thinking permits were inert). We
/// state only what is unconditionally true and DO NOT claim
/// "(enforced)": runtime enforcement is axis-dependent (fs+exec gate at
/// dispatch; tools+net are validated by `nika check`), so a blanket
/// enforcement claim would over-state for a tools/net-only block
/// (NIKA-SEC-004 · spn-nika review). And when `exec:` is granted,
/// `default-deny` would itself over-state (user gauntlet 2026-07-31 ·
/// G-10: a sub-process read files no `fs.read` admitted, under this
/// very banner) — the grant is named as the opening it is, until the
/// engine binds sub-process I/O to the fs boundary (spec-first ·
/// operator Q2).
pub(crate) fn permits_banner(wf: &RawWorkflow) -> &'static str {
    match wf.permits.as_ref() {
        Some(p) if p.value.allows_exec() => "declared boundary · exec outside the fs bounds",
        Some(_) => "declared boundary · default-deny",
        None => "zero authority (no `permits:` declared · F-O8)",
    }
}

/// #889 — the policy posture + the witnessed waiver: `sandbox_policy`
/// names the knob the run was judged under (every composed run — the
/// `sandbox:` backend field's sibling, always attested); `sandbox_waived`
/// rides ONLY when the run proceeds unconfined with `permits:` declared
/// under `NIKA_SANDBOX=off` — a sealed trace SHOWS the choice. Additive:
/// older readers ignore them, newer say "unrecorded", never guess.
pub(crate) fn sandbox_policy_fields(
    policy: Option<&str>,
    waived: bool,
) -> Vec<(&'static str, FieldValue)> {
    let mut fields = Vec::new();
    if let Some(policy) = policy {
        fields.push(("sandbox_policy", s(policy)));
    }
    if waived {
        fields.push(("sandbox_waived", s("true")));
    }
    fields
}

/// The NEP-0014 boot-manifest fields (F-P13 · F-P21) — additive, the
/// `permits_json` posture: older readers ignore unknown fields, newer
/// readers find them absent where no claim exists.
///
/// - `inputs` (F-P13 law 2) — every input the run binds names its
///   channel, one JSON map field (`{"name":"origin"}`); absent when the
///   run binds no input (no claim, never a guess);
/// - `resumed_from_engine` + `resume_compat: declared` (F-P21 law 4) —
///   the cross-version crossing the operator DECLARED (`--resume-compat`),
///   attested; absent on an exact resume (no crossing, no claim).
pub(crate) fn nep_0014_fields(
    input_origins: &BTreeMap<String, InputOrigin>,
    resume_compat: Option<&str>,
) -> Vec<(&'static str, FieldValue)> {
    let mut fields = Vec::new();
    if !input_origins.is_empty() {
        let map: BTreeMap<&str, &str> = input_origins
            .iter()
            .map(|(name, origin)| (name.as_str(), origin.as_str()))
            .collect();
        if let Ok(json) = serde_json::to_string(&map) {
            fields.push(("inputs", s(&json)));
        }
    }
    if let Some(recorded) = resume_compat {
        fields.push(("resumed_from_engine", s(recorded)));
        fields.push(("resume_compat", s("declared")));
    }
    fields
}

/// ADR-099 trust amendment (2026-08-08) · every resume that proceeded
/// WITHOUT a verified chain attests it: the posture (`declared` — the
/// operator named `--resume-unverified` past a finding · `unchained` —
/// the chainless-capture compat the strip-the-chain forgery lands in) +
/// the finding. Absent when the chain verified: no claim, never a flag
/// echo (the journal says what HAPPENED, not what was asked).
pub(crate) fn trust_amendment_fields(
    unverified: Option<&crate::resume::ResumeUnverified>,
) -> Vec<(&'static str, FieldValue)> {
    let mut fields = Vec::new();
    if let Some(unverified) = unverified {
        fields.push(("resume_unverified", s(unverified.posture())));
        fields.push(("resume_unverified_finding", s(unverified.finding())));
    }
    fields
}

/// Compile-time engine and platform identity for the opening frame.
fn environment_attestation_fields() -> [(&'static str, FieldValue); 2] {
    let identity = crate::engine_identity();
    let platform = format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH);
    [
        ("engine_version", s(identity.engine_version())),
        ("platform", s(&platform)),
    ]
}

/// The F-P2 boot attestation fields. They are additive: older readers ignore
/// them and newer readers classify their absence as unrecorded, never guessed.
///
/// - `spec_pin` names the exact spec commit compiled into the engine;
/// - `stamper_kind` is `deterministic` under `entropy: none | seeded(N)`;
/// - `clock` is the resolved run clock (`virtual` or `system`);
/// - `seed` keys retry jitter and exists only for a deterministic run.
///
/// `time_source`/`time_scale` remain F-N10 receipt fields. `nika.lock` has no
/// recording surface yet, so the manifest claims no digest for it.
pub(crate) fn boot_attestation_fields(wf: &RawWorkflow) -> Vec<(&'static str, FieldValue)> {
    let mut fields = Vec::new();
    fields.push(("spec_pin", s(crate::engine_identity().spec_sha())));
    let decl = wf
        .run
        .as_ref()
        .map_or_else(nika_schema::types::RunDecl::default, |decl| decl.value);
    let entropy = decl.entropy_or_default();
    fields.push((
        "stamper_kind",
        s(if entropy.is_deterministic() {
            "deterministic"
        } else {
            "system"
        }),
    ));
    fields.push(("clock", s(decl.clock_or_default().name())));
    if entropy.is_deterministic() {
        // The u64→wire idiom (settle.rs `delay_ms`): saturate at i64::MAX.
        fields.push((
            "seed",
            i(i64::try_from(entropy.jitter_seed()).unwrap_or(i64::MAX)),
        ));
    }
    fields
}

/// Emit the run's opening frames · `WorkflowStarted` + one
/// `TaskScheduled` per task (the storyboard's fixed prologue) — then
/// open the F-P4 approval book on the opening frame's id (the run
/// nonce every ticket this run mints is scoped to · NEP-0013 law 2).
#[allow(clippy::too_many_arguments)] // the prologue parts + the pens
pub(crate) fn emit_prologue(
    wf: &RawWorkflow,
    workflow_name: &str,
    source_sha256: Option<&str>,
    source_sha256_lf: Option<&str>,
    sandbox_backend: Option<&str>,
    sandbox_policy: Option<&str>,
    sandbox_waived: bool,
    input_origins: &BTreeMap<String, InputOrigin>,
    resume_compat: Option<&str>,
    resume_unverified: Option<&crate::resume::ResumeUnverified>,
    max_cost_usd: Option<f64>,
    model_override: Option<&str>,
    access: Vec<(&'static str, FieldValue)>,
    harness_seat: Option<&str>,
    approvals: &approval::ApprovalBook,
    opening_stamp: (nika_types::id::EventId, nika_types::timestamp::Timestamp),
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let mut opening = vec![
        ("workflow", s(workflow_name)),
        ("permits", s(permits_banner(wf))),
    ];
    if let Some(hex) = source_sha256 {
        opening.push(("workflow_sha256", s(hex)));
    }
    if let Some(hex) = source_sha256_lf {
        opening.push(("workflow_sha256_lf", s(hex)));
    }
    // The evidence-pack fields (A5): the journal carries the run's
    // identity + boundary + confinement in its OWN bytes — all three are
    // deterministic projections (no clock, no I/O) and additive (older
    // readers ignore them, newer readers say "unrecorded", never guess).
    if let Some(hash) = crate::proof::ir::semantic_ir_hash(wf) {
        opening.push(("semantic_hash", s(hash.as_hex())));
    }
    if let Some(permits) = wf.permits.as_ref()
        && let Ok(json) = serde_json::to_string(&permits.value)
    {
        opening.push(("permits_json", s(&json)));
    }
    if let Some(backend) = sandbox_backend {
        opening.push(("sandbox", s(backend)));
    }
    // #889 · the policy posture + the witnessed waiver (own helper).
    opening.extend(sandbox_policy_fields(sandbox_policy, sandbox_waived));
    // F-P13 + F-P21 · the NEP-0014 attestation fields (own helper).
    opening.extend(nep_0014_fields(input_origins, resume_compat));
    // ADR-099 trust amendment (2026-08-08) · the unverified-trust
    // attestation (own helper).
    opening.extend(trust_amendment_fields(resume_unverified));
    // The trace-format marker (spec 13 §trace · the graph_format: 3
    // precedent): the run's opening frame — the trace's header — names
    // the format it speaks. ONE source: `TraceFormatVersion::CURRENT`.
    opening.push((
        "trace_format",
        i(i64::from(nika_types::TraceFormatVersion::CURRENT.version)),
    ));
    // Environment attestation (Q11): WHICH engine on WHICH platform —
    // compile-time constants only; no clock, no I/O, determinism intact.
    opening.extend(environment_attestation_fields());
    // F-P2 · the boot attestation (spec pin · the declaration-resolved
    // entropy seam · the seed under a determinism demand).
    opening.extend(boot_attestation_fields(wf));
    // F-P18 · the pricing-table pin + the resolved operator budget
    // (NEP-0017 — the table that gives sense to ρ rides the pin).
    opening.extend(cost_pin_fields(max_cost_usd));
    // Issue 772 · the composer's `--model` override rides the boot
    // manifest (additive · spec 17 §manifest law — engines MAY add
    // fields): a resume can only judge what the trace records. Absent
    // when the run carried no override — no claim, never a guess.
    if let Some(model) = model_override {
        opening.push(("model_override", s(model)));
    }
    opening.extend(access);
    opening.extend(harness_seat.map(|seat| ("harness_seat", s(seat))));
    // Stamped by hand (not via `emit`) so the opening frame's id comes
    // back as the run nonce — the F-P4 approval scope.
    let (nonce, ts) = opening_stamp;
    let mut event = Event::new(nonce, ts, EventKind::WorkflowStarted);
    for (key, value) in opening {
        event = event.with_field(KeyValue::new(key, value));
    }
    sink.emit(event);
    for task in &wf.tasks {
        emit(
            stamper,
            sink,
            EventKind::TaskScheduled,
            &[("task", s(&task.value.id.value))],
        );
    }
    // F-P4 · the approval book opens with the run: the nonce is the
    // opening frame's id, and every prompt's unleashed closure is
    // precomputed over THESE bytes (the resumed run recomputes the same
    // closure — the shown hash stays comparable).
    approvals.begin_run(wf, nonce.uuid.to_string());
}
