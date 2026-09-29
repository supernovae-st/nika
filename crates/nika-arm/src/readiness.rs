// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The schedule readiness receipt of one beat (R4 71 · 89 · 112), read-only.
//!
//! The SCHEDULE side is judged here by the firer's own pure law
//! (`nika-cadence`): the cadence, its zone and next slot, the v0 policy
//! refusals, the locus, the pause bound, and the tick decision a fire at
//! `now` would take. The PROGRAM side (captured world, bindings, model and
//! cost) is judged by its owner and handed in as [`ProgramFacts`]. The
//! EVIDENCE side binds the ledger's last record to this beat by what the
//! record holds — its slot identity, its instant and its generation — and
//! nothing else: no record carries a project or a label, so a match proves
//! the generation and the slot, never the project or the host.
//!
//! READY is the current configuration's scheduling readiness. It never
//! means an activated schedule: OS activation stays `not_verified`, the
//! activation and monetary authorities stay `not_acquired`, `arm_ready` is
//! never true, and the requested-trigger bridge (A3) is open, so the
//! trigger requirement is unknown and outside the status scope.

use std::fmt::Write as _;

use jiff::Zoned;
use nika_cadence::firing::{ArmGeneration, SlotId};
use nika_cadence::registry::{ArmRegistry, Beat, Cadence, Locus};
use nika_cadence::{TickDecision, next_slots, tick_decision, v0_unsupported};
use serde_json::{Value, json};

use crate::state::{ArmInspection, LastRecord};

/// What the status word covers — every receipt states it.
pub const STATUS_SCOPE: &str = "current configuration only: captured program, trigger binding, required inputs, model and cost admission, firing evidence; never OS activation, acquired authority or the current window";

/// The program owner's document keys, `null` when it could not judge them.
const PROGRAM_KEYS: [&str; 6] = [
    "required_inputs",
    "optional_inputs",
    "undeclared_bindings",
    "unbound_inputs",
    "model_summary",
    "authority_summary",
];

/// The trigger requirement is the compile-side `requested_trigger` (A3).
const REQUIREMENT_OPEN: &str = "open: the requested_trigger bridge is not implemented; an arm entry is a deployment binding, never a semantic trigger requirement";

/// One value-free blocker of a receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Blocker {
    /// The stable kind slug.
    pub kind: String,
    /// The registered code when the law has one (`NIKA-1708` · `NIKA-1709`).
    pub code: Option<String>,
    /// The input, model or task it names.
    pub subject: Option<String>,
    /// The reason class, when the owner has one.
    pub reason: Option<String>,
    /// One sentence without any input value.
    pub message: String,
}

impl Blocker {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        kind: impl Into<String>,
        code: Option<String>,
        subject: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            code,
            subject,
            reason: None,
            message: message.into(),
        }
    }

    /// The same blocker with its reason class.
    #[must_use]
    pub fn with_reason(mut self, reason: Option<String>) -> Self {
        self.reason = reason;
        self
    }

    fn document(&self) -> Value {
        json!({"kind": self.kind, "code": self.code, "subject": self.subject,
            "reason": self.reason, "message": self.message})
    }
}

/// The program side of a receipt, judged by its owner (the service driver):
/// the captured world, the bindings, the model and cost law. A `None` axis
/// is unknown and never counts as ready.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ProgramFacts {
    /// The world was captured and admitted (Check included).
    pub program_ready: bool,
    /// Every binding fits and every required input has a source.
    pub required_inputs_ready: Option<bool>,
    /// Model and cost admission of an unattended fire.
    pub model_cost_ready: Option<bool>,
    /// The owner's blockers.
    pub blockers: Vec<Blocker>,
    /// What the owner cannot derive, in words.
    pub unknowns: Vec<String>,
    /// The owner's machine document (`inputs` · `model_cost` · `authority`).
    pub document: Value,
}

impl ProgramFacts {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        program_ready: bool,
        required_inputs_ready: Option<bool>,
        model_cost_ready: Option<bool>,
        blockers: Vec<Blocker>,
        unknowns: Vec<String>,
        document: Value,
    ) -> Self {
        Self {
            program_ready,
            required_inputs_ready,
            model_cost_ready,
            blockers,
            unknowns,
            document,
        }
    }

    /// A program that could not be captured: every later axis is unknown.
    #[must_use]
    pub fn unavailable(kind: &str, message: String) -> Self {
        Self::new(
            false,
            None,
            None,
            vec![Blocker::new(kind, None, None, message)],
            Vec::new(),
            json!({}),
        )
    }
}

/// Where the beat is bound. None of it enters any generation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectBinding {
    /// The project root's fingerprint (the runtime's one derivation).
    pub root_fingerprint: Option<String>,
    /// The project file that declares the beat.
    pub project_file: String,
    /// The beat's positional label.
    pub label: String,
}

impl ProjectBinding {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(root_fingerprint: Option<String>, project_file: String, label: String) -> Self {
        Self {
            root_fingerprint,
            project_file,
            label,
        }
    }
}

/// The identity a receipt holds for: the existing digests of the captured
/// world and of the beat's declaration, with the project binding apart. A
/// change to any of them makes an earlier receipt stale.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReceiptIdentity {
    /// The root workflow's captured bytes.
    pub workflow_sha256: Option<String>,
    /// The captured world (root, children, skills, imports).
    pub snapshot_digest: Option<String>,
    /// The declaration (cadence, policies, inputs and their sources) over the world.
    pub generation: Option<ArmGeneration>,
    /// The project binding.
    pub project: ProjectBinding,
}

impl ReceiptIdentity {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        workflow_sha256: Option<String>,
        snapshot_digest: Option<String>,
        generation: Option<ArmGeneration>,
        project: ProjectBinding,
    ) -> Self {
        Self {
            workflow_sha256,
            snapshot_digest,
            generation,
            project,
        }
    }
}

/// What the ledger's last record proves about THIS beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProofBinding {
    /// No record: the registry declares, the machine never fired.
    Declared,
    /// This beat's slot at the current generation: it proves the generation
    /// and the slot, never the project or the host.
    CurrentGeneration,
    /// This beat's slot at another generation (an earlier declaration or
    /// world, or the resident edge's revision): history, never current proof.
    HistoricalGeneration,
    /// This beat's slot, recorded before generations existed.
    Legacy,
    /// Its slot identity or instant does not derive from this beat's current
    /// workflow and cadence: another beat's or project's record, or this
    /// beat's own from before a cadence change (records carry no declaration
    /// to tell them apart). Refused as evidence here.
    Unattributed,
    /// The verified replay refuses the sidecar (corrupt, torn, redirected).
    Refused,
}

impl ProofBinding {
    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::CurrentGeneration => "current_generation",
            Self::HistoricalGeneration => "historical_generation",
            Self::Legacy => "legacy",
            Self::Unattributed => "unattributed",
            Self::Refused => "refused",
        }
    }
}

/// The receipt's status word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReceiptStatus {
    /// Every axis in [`STATUS_SCOPE`] is proven ready.
    Ready,
    /// An axis in scope is false or unknown.
    Unready,
    /// Registered but declared inactive (`actif: false`).
    Dormant,
}

impl ReceiptStatus {
    /// The wire word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Unready => "UNREADY",
            Self::Dormant => "DORMANT",
        }
    }
}

/// What one verified read showed, kept for the receipt.
#[derive(Debug)]
struct Evidence {
    last: Option<LastRecord>,
    state: Option<&'static str>,
    /// The 8-char slot of a lifecycle beyond the last record.
    current_slot: Option<String>,
    tallies: Option<(usize, usize)>,
}

/// The schedule side, judged by the firer's own law.
#[derive(Debug)]
struct ScheduleSide {
    timezone: Option<String>,
    next_fire: Option<Zoned>,
    binding_ready: Option<bool>,
    blockers: Vec<Blocker>,
    unknowns: Vec<String>,
}

/// One beat's schedule readiness receipt.
#[derive(Debug)]
#[non_exhaustive]
pub struct ScheduleReadinessReceipt<'a> {
    beat: &'a Beat,
    now: Zoned,
    identity: ReceiptIdentity,
    program: ProgramFacts,
    schedule: ScheduleSide,
    proof: ProofBinding,
    evidence: Option<Evidence>,
    evidence_error: Option<String>,
    decision: TickDecision,
}

impl<'a> ScheduleReadinessReceipt<'a> {
    /// Assemble the receipt of `registry`'s beat `index` at `now`. The
    /// evidence is one verified read ([`crate::state::ArmState::inspect`]);
    /// nothing here writes, locks or repairs. `None` past the registry.
    #[must_use]
    pub fn assemble(
        registry: &'a ArmRegistry,
        index: usize,
        now: &Zoned,
        identity: ReceiptIdentity,
        program: ProgramFacts,
        evidence: std::io::Result<ArmInspection>,
    ) -> Option<Self> {
        let beat = registry.beats().nth(index)?;
        let cadence = Cadence::parse(&beat.cadence).ok();
        let schedule = schedule_side(beat, cadence.as_ref(), now);
        let (proof, evidence, evidence_error) = match evidence {
            Ok(inspection) => {
                let current = identity.generation.as_ref();
                let proof = bind_proof(&inspection, beat, cadence.as_ref(), current);
                (proof, Some(keep(&inspection)), None)
            }
            Err(error) => (ProofBinding::Refused, None, Some(error.to_string())),
        };
        // The firer's own `last`: the recorded slot, read in UTC.
        let last = evidence
            .as_ref()
            .and_then(|kept| kept.last.as_ref())
            .map(|record| record.slot.to_zoned(jiff::tz::TimeZone::UTC));
        let decision = tick_decision(registry, index, &identity.project.label, now, last.as_ref());
        Some(Self {
            beat,
            now: now.clone(),
            identity,
            program,
            schedule,
            proof,
            evidence,
            evidence_error,
            decision,
        })
    }

    /// The beat's positional label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.identity.project.label
    }

    /// What the last record proves about this beat.
    #[must_use]
    pub const fn proof(&self) -> ProofBinding {
        self.proof
    }

    /// The firing evidence is refused or not this beat's (the report exits 3).
    #[must_use]
    pub const fn evidence_refused(&self) -> bool {
        matches!(
            self.proof,
            ProofBinding::Refused | ProofBinding::Unattributed
        )
    }

    /// Every blocker: the schedule's, the program's, then the evidence's.
    #[must_use]
    pub fn blockers(&self) -> Vec<Blocker> {
        let mut all = self.schedule.blockers.clone();
        all.extend(self.program.blockers.iter().cloned());
        let label = Some(self.identity.project.label.clone());
        match self.proof {
            ProofBinding::Refused => all.push(Blocker::new(
                "firing_evidence_invalid",
                None,
                label,
                self.evidence_error.clone().unwrap_or_default(),
            )),
            ProofBinding::Unattributed => all.push(Blocker::new(
                "firing_evidence_unattributed",
                None,
                label,
                "the last record's slot does not derive from this beat's current workflow and cadence: another beat's or project's record, or this beat's before a cadence change",
            )),
            _ => {}
        }
        all
    }

    /// Every unknown: the schedule's, then the program's.
    #[must_use]
    pub fn unknowns(&self) -> Vec<String> {
        let mut all = self.schedule.unknowns.clone();
        all.extend(self.program.unknowns.iter().cloned());
        all
    }

    /// DORMANT when declared inactive; READY only when every axis in
    /// [`STATUS_SCOPE`] is proven true; UNREADY otherwise (false OR unknown).
    #[must_use]
    pub fn status(&self) -> ReceiptStatus {
        if !self.beat.is_active() {
            return ReceiptStatus::Dormant;
        }
        let proven = self.program.program_ready
            && self.schedule.binding_ready == Some(true)
            && self.program.required_inputs_ready == Some(true)
            && self.program.model_cost_ready == Some(true)
            && self.blockers().is_empty()
            && self.unknowns().is_empty();
        if proven {
            ReceiptStatus::Ready
        } else {
            ReceiptStatus::Unready
        }
    }

    /// A fire at `now` would start a run: the tick law says fire AND the
    /// status is READY. An out-of-window skip is `false` here and leaves
    /// the status alone.
    #[must_use]
    pub fn run_ready_now(&self) -> bool {
        matches!(self.decision, TickDecision::Fire { .. }) && self.status() == ReceiptStatus::Ready
    }

    /// The machine document: the v1 keys keep their meaning (flat axes,
    /// `authority`, the digests, `required_inputs`), the receipt adds the
    /// R4 89 fields. A program key the owner could not derive is `null`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let blockers = self.blockers();
        let required = self.program.required_inputs_ready;
        // Never true: activation authority is not acquired by a report.
        let arm_ready = (!blockers.is_empty() || !self.beat.is_active()).then_some(false);
        let mut document = json!({
            "label": self.identity.project.label,
            "workflow": self.beat.workflow,
            "cadence": self.beat.cadence,
            "active": self.beat.is_active(),
            "scope": "schedule_readiness_receipt",
            "status": self.status().as_str(),
            "status_scope": STATUS_SCOPE,
            "program_ready": self.program.program_ready,
            "trigger_requirement_ready": null,
            "trigger_binding_ready": self.schedule.binding_ready,
            "required_inputs_ready": required,
            "model_cost_ready": self.program.model_cost_ready,
            "arm_ready": arm_ready,
            "run_ready_now": self.run_ready_now(),
            "binding_status": if required == Some(true) { "ready" } else { "unready" },
            "authority": "not_acquired",
            "workflow_sha256": self.identity.workflow_sha256,
            "snapshot_digest": self.identity.snapshot_digest,
            "generation": self.identity.generation.as_ref().map(ArmGeneration::as_str),
            "project_identity": {
                "root_fingerprint": self.identity.project.root_fingerprint,
                "project_file": self.identity.project.project_file,
                "label": self.identity.project.label,
            },
            "timezone": self.schedule.timezone.as_ref().map(|zone| json!({
                "zone": zone, "source": "cadence_expression", "tzdb": "embedded"})),
            "next_fire": self.schedule.next_fire.as_ref().map(ToString::to_string),
            "trigger_requirement": REQUIREMENT_OPEN,
            "run_now": decision_document(&self.decision),
            "firing_evidence": self.evidence_document(),
            "activation": {
                "status": "not_verified",
                "os_unit": "not observed: `nika arm --emit` prints a unit and `--write` writes its file; neither loads it, and no loaded unit is read back",
                "resident_serve": "not observed: a resident serve fires this ledger only while it runs",
            },
            "host_assumptions": [
                "readiness reads this process environment (provider keys, @env bindings); an OS unit's environment can differ",
                "the cadence zone resolves against the embedded IANA tzdb, never the host's",
                "no harness seat is spawned: seat-only routes stay unknown",
            ],
            "blockers": blockers.iter().map(Blocker::document).collect::<Vec<_>>(),
            "unknowns": self.unknowns(),
        });
        if let Some(map) = document.as_object_mut() {
            let program = self.program.document.as_object();
            for key in PROGRAM_KEYS {
                let value = program.and_then(|program| program.get(key)).cloned();
                map.insert(key.to_owned(), value.unwrap_or(Value::Null));
            }
        }
        document
    }

    fn evidence_document(&self) -> Value {
        let Some(evidence) = &self.evidence else {
            return json!({"status": "refused", "binding": self.proof.as_str(),
                "message": self.evidence_error});
        };
        let last = evidence.last.as_ref();
        json!({
            "status": "verified_read",
            "binding": self.proof.as_str(),
            "proof_scope": "generation_and_slot",
            "project_authenticity": "not_proven",
            "last_fired_at": last.map(|record| record.fired_at.to_string()),
            "recorded_slot": last.map(|record| record.slot.to_string()),
            "recorded_generation": last
                .and_then(|record| record.generation.as_ref())
                .map(ArmGeneration::as_str),
            "current_generation": self.identity.generation.as_ref().map(ArmGeneration::as_str),
            "state": evidence.state,
        })
    }

    /// The human block: the same status, blockers and unknowns as the JSON.
    #[must_use]
    pub fn human_lines(&self, slots_shown: usize) -> Vec<String> {
        let status = self.status();
        let mut lines = vec![format!(
            "  [{}] {} · {}",
            status.as_str(),
            self.beat.workflow,
            self.beat.cadence.trim()
        )];
        for blocker in self.blockers() {
            let code = blocker
                .code
                .map_or(String::new(), |code| format!("{code} · "));
            lines.push(format!(
                "         ✗ {code}{} ({})",
                blocker.message, blocker.kind
            ));
        }
        for unknown in self.unknowns() {
            lines.push(format!("         ? {unknown}"));
        }
        match status {
            ReceiptStatus::Ready => lines.push(
                "         · ready in the current configuration; OS activation not verified · authority not acquired"
                    .to_owned(),
            ),
            ReceiptStatus::Dormant => {
                let why = self.beat.raison.as_deref().unwrap_or("sans raison");
                let until = self.beat.jusqu_au.as_deref().unwrap_or("?");
                lines.push(format!(
                    "         · dormant — actif: false · {why} · jusqu_au {until}"
                ));
            }
            _ => {}
        }
        lines.extend(self.proof_line().map(|line| format!("         {line}")));
        if let Some(par) = &self.beat.par {
            lines.push(format!("         par: {par} — déclaré · non vérifié"));
        }
        // A dormant beat is REPORTED, never COMPUTED.
        if self.beat.is_active() {
            lines.extend(self.slot_lines(slots_shown));
        }
        lines
    }

    fn slot_lines(&self, slots_shown: usize) -> Vec<String> {
        match Cadence::parse(&self.beat.cadence) {
            Err(error) => vec![format!("         ✗ {error}")],
            Ok(cadence) => {
                let slots: Vec<String> = next_slots(&cadence, &self.now, slots_shown)
                    .map(|slot| format!("         → {}", slot.at.strftime("%Y-%m-%d %H:%M %Z")))
                    .collect();
                if slots.is_empty() {
                    vec![
                        "         → no upcoming slot (a webhook beat fires on its event)"
                            .to_owned(),
                    ]
                } else {
                    slots
                }
            }
        }
    }

    /// PROUVÉ only for the current generation, and with the scope it
    /// proves; history, legacy and foreign evidence are named as such.
    fn proof_line(&self) -> Option<String> {
        let evidence = self.evidence.as_ref()?;
        let mut line = match &evidence.last {
            None => "· DÉCLARÉ — le registre le dit, la machine ne l'a jamais tiré".to_owned(),
            Some(last) => self.recorded_head(last),
        };
        if let Some(state) = evidence.state {
            let current = evidence
                .current_slot
                .as_ref()
                .map_or(String::new(), |slot| format!(" · slot courant {slot}"));
            let _ = write!(line, " · état {state}{current}");
        }
        if let Some((skips, fires)) = evidence.tallies {
            let _ = write!(
                line,
                " · {} / {}",
                count(skips, "saut"),
                count(fires, "tir")
            );
        }
        if let Some(tolerance) = &self.beat.tolerance {
            let _ = write!(line, " · tolérance {tolerance}");
        }
        Some(line)
    }

    fn recorded_head(&self, last: &LastRecord) -> String {
        let head = match self.proof {
            ProofBinding::CurrentGeneration => {
                "✓ PROUVÉ (génération + créneau · ni projet ni hôte)".to_owned()
            }
            ProofBinding::HistoricalGeneration => format!(
                "· HISTORIQUE (autre génération que la courante {})",
                self.identity
                    .generation
                    .as_ref()
                    .map_or("inconnue", ArmGeneration::short)
            ),
            ProofBinding::Legacy => "· HÉRITÉ (aucune génération enregistrée)".to_owned(),
            _ => "✗ NON ATTRIBUÉ (créneau non dérivable de ce beat)".to_owned(),
        };
        let generation = last
            .generation
            .as_ref()
            .map_or(String::new(), |generation| {
                format!(" · gen {}", generation.short())
            });
        format!(
            "{head} · {} · {} · slot {}{generation}",
            last.kind.as_str(),
            last.fired_at,
            last.slot
        )
    }
}

/// The schedule side, in the tick law's own order: the declared intention,
/// the locus, the pause bound, the v0 refusals, the cadence.
fn schedule_side(beat: &Beat, cadence: Option<&Cadence>, now: &Zoned) -> ScheduleSide {
    let mut blockers = Vec::new();
    let mut unknowns = Vec::new();
    let active = beat.is_active();
    if active && beat.locus() == Locus::Cloud {
        unknowns.push(
            "où: cloud — the cloud executor fires this beat, never this host (not observed)"
                .to_owned(),
        );
    }
    if let Some(date) = nika_cadence::tick::expiry_passed(beat, now).filter(|_| active) {
        blockers.push(Blocker::new(
            "schedule_expired",
            None,
            None,
            format!("jusqu_au {date} has passed: every fire skips as expired"),
        ));
    }
    if let Some((what, arrives)) = v0_unsupported(beat) {
        blockers.push(Blocker::new(
            "policy_unsupported",
            None,
            None,
            format!(
                "{what} is not supported by this firer (arrives with {arrives}): every fire refuses"
            ),
        ));
    }
    let timezone = match cadence {
        None => {
            blockers.push(Blocker::new(
                "cadence_invalid",
                None,
                None,
                "the cadence does not parse",
            ));
            None
        }
        Some(Cadence::Cron { tz, .. } | Cadence::Every { tz, .. }) => Some(tz.clone()),
        Some(_) => {
            unknowns.push(
                "cadence on-webhook: its event route fires it, never the clock (not observed)"
                    .to_owned(),
            );
            None
        }
    };
    let binding_ready = if !blockers.is_empty() || !active {
        Some(false)
    } else if unknowns.is_empty() {
        Some(true)
    } else {
        None
    };
    // A next fire exists only for a binding that can fire.
    let next_fire = cadence
        .filter(|_| binding_ready != Some(false))
        .and_then(|cadence| next_slots(cadence, now, 1).next())
        .map(|slot| slot.at);
    ScheduleSide {
        timezone,
        next_fire,
        binding_ready,
        blockers,
        unknowns,
    }
}

/// Bind the last record to this beat by what it recorded: the slot identity
/// the lifecycle keyed it by, against the one this beat's workflow and
/// cadence derive for its instant; the instant against the cadence; the
/// generation against the current one. Nothing is inferred beyond that.
fn bind_proof(
    inspection: &ArmInspection,
    beat: &Beat,
    cadence: Option<&Cadence>,
    current: Option<&ArmGeneration>,
) -> ProofBinding {
    let Some(last) = inspection.last() else {
        return ProofBinding::Declared;
    };
    let instant = last.slot.to_zoned(jiff::tz::TimeZone::UTC);
    let recorded = inspection
        .folded()
        .filter(|folded| !folded.is_beyond_last())
        .and_then(|folded| folded.slot());
    let derived = SlotId::derive(&beat.workflow, &beat.cadence, &instant);
    let slot_matches = recorded.is_none_or(|slot| slot == derived.as_str());
    let in_cadence = cadence
        .and_then(|cadence| cadence.prev_before(&instant))
        .is_some_and(|slot| slot.at.timestamp() == last.slot);
    if !slot_matches || !in_cadence {
        return ProofBinding::Unattributed;
    }
    match (&last.generation, current) {
        (None, _) => ProofBinding::Legacy,
        (Some(recorded), Some(current)) if recorded == current => ProofBinding::CurrentGeneration,
        (Some(_), _) => ProofBinding::HistoricalGeneration,
    }
}

fn keep(inspection: &ArmInspection) -> Evidence {
    let folded = inspection.folded();
    Evidence {
        last: inspection.last().cloned(),
        state: folded.map(|folded| folded.state().as_str()),
        current_slot: folded
            .filter(|folded| folded.is_beyond_last())
            .and_then(|folded| folded.slot())
            .and_then(|slot| slot.get(..8))
            .map(str::to_owned),
        tallies: inspection.tallies(),
    }
}

fn decision_document(decision: &TickDecision) -> Value {
    match decision {
        TickDecision::Fire { slot, slots } => {
            json!({"decision": "fire", "slot": slot.to_string(), "catch_up_slots": slots})
        }
        TickDecision::Skip { reason, .. } => json!({"decision": "skip", "reason": reason}),
        TickDecision::Refuse { line } => json!({"decision": "refuse", "reason": line}),
        _ => json!({"decision": "unknown"}),
    }
}

/// `1 saut` · `3 tirs` — the report's plural, as the CLI's `count`.
fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests;
