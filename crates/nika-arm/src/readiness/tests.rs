// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::state::{ArmState, Claim, FireKind, HistoryEntry};
use jiff::Timestamp;
use nika_cadence::FencingToken;

const WORKFLOW: &str = "workflows/report.nika";
const CADENCE: &str = "TZ=UTC 0 3 * * *";
const SAUTER: &str = "    manqué: sauter\n";

fn at(text: &str) -> Zoned {
    ts(text).to_zoned(jiff::tz::TimeZone::UTC)
}

fn ts(text: &str) -> Timestamp {
    text.parse::<Timestamp>().expect("ts")
}

fn registry(body: &str) -> ArmRegistry {
    let source = format!(
        "nika: proj\narm:\n  - workflow: {WORKFLOW}\n    cadence: \"{CADENCE}\"\n    plafond: 0.05\n{body}"
    );
    let registry = nika_cadence::parse_registry(&source).expect("parse");
    assert!(
        nika_cadence::validate(&registry).next().is_none(),
        "the fixture must be lawful"
    );
    registry
}

fn project(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("nika-arm-readiness-{tag}-"))
        .tempdir()
        .expect("tmp dir")
}

fn ready() -> ProgramFacts {
    ProgramFacts::new(
        true,
        Some(true),
        Some(true),
        Vec::new(),
        Vec::new(),
        json!({}),
    )
}

fn generation(registry: &ArmRegistry, digest: &str) -> ArmGeneration {
    let beat = registry.beats().next().expect("one beat");
    ArmGeneration::compute(beat, digest)
}

fn identity(registry: &ArmRegistry, label: &str) -> ReceiptIdentity {
    ReceiptIdentity::new(
        Some("w".repeat(64)),
        Some("digest-now".to_owned()),
        Some(generation(registry, "digest-now")),
        ProjectBinding::new(
            Some("f".repeat(64)),
            "nika.yaml".to_owned(),
            label.to_owned(),
        ),
    )
}

fn receipt<'a>(
    registry: &'a ArmRegistry,
    root: &std::path::Path,
    now: &str,
    program: ProgramFacts,
) -> ScheduleReadinessReceipt<'a> {
    let now = at(now);
    let evidence = ArmState::at_project(root).inspect("report", &now.timestamp());
    ScheduleReadinessReceipt::assemble(
        registry,
        0,
        &now,
        identity(registry, "report"),
        program,
        evidence,
    )
    .expect("beat 0")
}

/// One claimed-then-receipted fire, as the firer journals it.
fn fired(root: &std::path::Path, workflow: &str, slot: &str, pinned: Option<ArmGeneration>) {
    let state = ArmState::open(root).expect("project");
    let slot = at(slot);
    let decided = slot.timestamp();
    let identity = SlotId::derive(workflow, CADENCE, &slot);
    let mut claim = Claim::new(identity.clone(), ts("2027-01-01T00:00:00Z"), decided);
    claim.generation = pinned.clone();
    let claimed = state.record_claim("report", &claim).expect("claim");
    let mut entry = HistoryEntry::new(Some(decided), decided, FireKind::Fired);
    entry.exit = Some(0);
    entry.slot_id = Some(identity);
    entry.fencing = Some(FencingToken::new(claimed.seq));
    entry.generation = pinned;
    state.record("report", &entry).expect("receipt");
}

fn sidecar_bytes(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let dir = root.join(".nika/arm/report");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("sidecar")
        .map(|entry| {
            let path = entry.expect("entry").path();
            let name = path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned();
            (name, std::fs::read(&path).expect("bytes"))
        })
        .collect();
    files.sort();
    files
}

fn kinds(receipt: &ScheduleReadinessReceipt<'_>) -> Vec<String> {
    receipt.blockers().into_iter().map(|b| b.kind).collect()
}

/// READY needs every axis in scope PROVEN; an unknown axis is never ready,
/// and READY itself never claims activation or authority.
#[test]
fn every_axis_in_scope_must_be_proven_for_ready() {
    let registry = registry(SAUTER);
    let dir = project("ready");
    let proven = receipt(&registry, dir.path(), "2026-09-28T03:00:10Z", ready());
    assert_eq!(proven.status(), ReceiptStatus::Ready);
    assert!(proven.run_ready_now(), "on time, never fired");
    let doc = proven.to_json();
    assert_eq!(doc["status"], "READY");
    assert_eq!(doc["status_scope"], STATUS_SCOPE);
    assert_eq!(doc["arm_ready"], Value::Null, "never true");
    assert_eq!(doc["trigger_requirement_ready"], Value::Null);
    assert_eq!(doc["trigger_binding_ready"], true);
    assert_eq!(doc["activation"]["status"], "not_verified");
    assert_eq!(doc["timezone"]["zone"], "UTC");
    assert_eq!(doc["next_fire"], "2026-09-29T03:00:00+00:00[UTC]");
    assert_eq!(doc["firing_evidence"]["binding"], "declared");
    let human = proven.human_lines(3);
    assert_eq!(
        human[0],
        "  [READY] workflows/report.nika · TZ=UTC 0 3 * * *"
    );
    assert!(human[1].contains("OS activation not verified"), "{human:?}");

    let mut unknown_cost = ready();
    unknown_cost.model_cost_ready = None;
    unknown_cost.unknowns = vec!["task `ask`: only the run decides its model".to_owned()];
    let unknown = receipt(&registry, dir.path(), "2026-09-28T03:00:10Z", unknown_cost);
    assert_eq!(
        unknown.status(),
        ReceiptStatus::Unready,
        "unknown is never ready"
    );
    assert!(!unknown.run_ready_now());
    let doc = unknown.to_json();
    assert_eq!(doc["model_cost_ready"], Value::Null);
    assert_eq!(
        doc["arm_ready"],
        Value::Null,
        "no blocker: unknown, not false"
    );
    assert!(
        unknown
            .human_lines(3)
            .iter()
            .any(|l| l.contains("? task `ask`"))
    );

    let broken = ProgramFacts::unavailable("workflow_unavailable", "no such workflow".to_owned());
    let broken = receipt(&registry, dir.path(), "2026-09-28T03:00:10Z", broken);
    assert_eq!(broken.status(), ReceiptStatus::Unready);
    let doc = broken.to_json();
    assert_eq!(doc["arm_ready"], false);
    assert_eq!(
        doc["required_inputs"],
        Value::Null,
        "unknown, never an empty list"
    );
    assert_eq!(doc["model_summary"], Value::Null);
    assert!(!dir.path().join(".nika").exists(), "a read creates nothing");
}

/// Out of the on-time window a fire truthfully skips; the status of the
/// prepared schedule does not move.
#[test]
fn out_of_window_is_a_truthful_skip_and_leaves_the_status() {
    let registry = registry(SAUTER);
    let dir = project("window");
    let later = receipt(&registry, dir.path(), "2026-09-28T10:00:00Z", ready());
    assert_eq!(later.status(), ReceiptStatus::Ready);
    assert!(!later.run_ready_now());
    let doc = later.to_json();
    assert_eq!(doc["run_now"]["decision"], "skip");
    assert_eq!(doc["run_now"]["reason"], "not-due");
    assert_eq!(doc["run_ready_now"], false);
}

/// A dormant definition is registered, reported, and never computed or
/// read as armed.
#[test]
fn a_dormant_definition_never_reads_as_armed() {
    let registry = registry(
        "    manqué: sauter\n    actif: false\n    raison: vacances\n    jusqu_au: 2026-10-01\n",
    );
    let dir = project("dormant");
    let dormant = receipt(&registry, dir.path(), "2026-09-28T03:00:10Z", ready());
    assert_eq!(dormant.status(), ReceiptStatus::Dormant);
    let doc = dormant.to_json();
    assert_eq!(doc["status"], "DORMANT");
    assert_eq!(doc["trigger_binding_ready"], false);
    assert_eq!(doc["arm_ready"], false);
    assert_eq!(doc["run_ready_now"], false);
    assert_eq!(doc["next_fire"], Value::Null);
    assert_eq!(doc["run_now"]["reason"], "inactive");
    let human = dormant.human_lines(3);
    assert_eq!(
        human[0],
        "  [DORMANT] workflows/report.nika · TZ=UTC 0 3 * * *"
    );
    assert!(
        human
            .iter()
            .any(|l| l.contains("dormant — actif: false · vacances · jusqu_au 2026-10-01"))
    );
    assert!(
        !human.iter().any(|l| l.contains('→')),
        "never computed: {human:?}"
    );
}

/// E16-2 · a policy every fire refuses is a schedule blocker, never a
/// registered beat.
#[test]
fn an_unsupported_miss_policy_is_a_schedule_blocker() {
    let registry = registry("    manqué: rattraper\n");
    let dir = project("policy");
    let refused = receipt(&registry, dir.path(), "2026-09-28T03:00:10Z", ready());
    assert_eq!(refused.status(), ReceiptStatus::Unready);
    assert_eq!(kinds(&refused), ["policy_unsupported"]);
    let doc = refused.to_json();
    assert_eq!(doc["trigger_binding_ready"], false);
    assert_eq!(doc["arm_ready"], false);
    assert_eq!(doc["next_fire"], Value::Null);
    assert_eq!(doc["run_now"]["decision"], "refuse");
    let human = refused.human_lines(3).join("\n");
    assert!(human.contains("  [UNREADY] "), "{human}");
    assert!(
        human.contains("✗ manqué: rattraper is not supported by this firer"),
        "{human}"
    );
}

/// A record of this beat's slot at the current generation proves the
/// generation and the slot, and says it proves nothing more.
#[test]
fn the_current_generation_proves_its_generation_and_slot_only() {
    let registry = registry(SAUTER);
    let dir = project("current");
    fired(
        dir.path(),
        WORKFLOW,
        "2026-09-28T03:00:00Z",
        Some(generation(&registry, "digest-now")),
    );
    let proven = receipt(&registry, dir.path(), "2026-09-28T03:01:00Z", ready());
    assert_eq!(proven.proof(), ProofBinding::CurrentGeneration);
    assert_eq!(proven.status(), ReceiptStatus::Ready);
    assert!(!proven.run_ready_now(), "the slot is answered");
    let doc = proven.to_json();
    let evidence = &doc["firing_evidence"];
    assert_eq!(evidence["binding"], "current_generation");
    assert_eq!(evidence["proof_scope"], "generation_and_slot");
    assert_eq!(evidence["project_authenticity"], "not_proven");
    assert_eq!(
        evidence["recorded_generation"],
        evidence["current_generation"]
    );
    assert_eq!(doc["run_now"]["reason"], "already");
    let human = proven.human_lines(3).join("\n");
    assert!(
        human.contains("✓ PROUVÉ (génération + créneau · ni projet ni hôte) · fired · "),
        "{human}"
    );
    assert!(human.contains(" · 0 sauts / 1 tir"), "{human}");
}

/// E16-6 · the same slot under another generation (a rebinding since) is
/// history: never PROUVÉ, never a refusal.
#[test]
fn a_record_of_another_generation_is_history_never_current_proof() {
    let registry = registry(SAUTER);
    let dir = project("historical");
    fired(
        dir.path(),
        WORKFLOW,
        "2026-09-28T03:00:00Z",
        Some(generation(&registry, "digest-old")),
    );
    let stale = receipt(&registry, dir.path(), "2026-09-28T03:01:00Z", ready());
    assert_eq!(stale.proof(), ProofBinding::HistoricalGeneration);
    assert!(!stale.evidence_refused());
    assert_eq!(stale.status(), ReceiptStatus::Ready);
    let doc = stale.to_json();
    assert_eq!(doc["firing_evidence"]["binding"], "historical_generation");
    assert_ne!(
        doc["firing_evidence"]["recorded_generation"],
        doc["firing_evidence"]["current_generation"]
    );
    let human = stale.human_lines(3).join("\n");
    let current = generation(&registry, "digest-now");
    assert!(
        human.contains(&format!(
            "· HISTORIQUE (autre génération que la courante {})",
            current.short()
        )),
        "{human}"
    );
    assert!(!human.contains("PROUVÉ"), "{human}");
}

/// E16-1 · a record whose slot another workflow derived, or whose instant
/// this cadence never produces, is not this beat's evidence.
#[test]
fn foreign_evidence_is_unattributed_and_refused() {
    let registry = registry(SAUTER);
    let current = generation(&registry, "digest-now");
    for (tag, workflow, slot) in [
        ("copied", "workflows/other.nika", "2026-09-28T03:00:00Z"),
        ("off-cadence", WORKFLOW, "2026-09-28T03:30:00Z"),
    ] {
        let dir = project(tag);
        fired(dir.path(), workflow, slot, Some(current.clone()));
        let foreign = receipt(&registry, dir.path(), "2026-09-28T03:01:00Z", ready());
        assert_eq!(foreign.proof(), ProofBinding::Unattributed, "{tag}");
        assert!(foreign.evidence_refused(), "{tag}");
        assert_eq!(foreign.status(), ReceiptStatus::Unready, "{tag}");
        assert_eq!(kinds(&foreign), ["firing_evidence_unattributed"], "{tag}");
        assert_eq!(foreign.to_json()["arm_ready"], false, "{tag}");
        let human = foreign.human_lines(3).join("\n");
        assert!(
            human.contains("✗ NON ATTRIBUÉ (créneau non dérivable de ce beat)"),
            "{tag}: {human}"
        );
        assert!(!human.contains("PROUVÉ"), "{tag}: {human}");
    }
}

/// A pre-generation record of this beat's slot is named legacy.
#[test]
fn a_legacy_record_is_named_legacy() {
    let registry = registry(SAUTER);
    let dir = project("legacy");
    let state = ArmState::open(dir.path()).expect("project");
    let slot = ts("2026-09-28T03:00:00Z");
    let mut entry = HistoryEntry::new(Some(slot), slot, FireKind::Fired);
    entry.exit = Some(0);
    state.record("report", &entry).expect("legacy receipt");
    let legacy = receipt(&registry, dir.path(), "2026-09-28T03:01:00Z", ready());
    assert_eq!(legacy.proof(), ProofBinding::Legacy);
    assert!(!legacy.evidence_refused());
    assert_eq!(
        legacy.to_json()["firing_evidence"]["recorded_generation"],
        Value::Null
    );
    let human = legacy.human_lines(3).join("\n");
    assert!(
        human.contains("· HÉRITÉ (aucune génération enregistrée) · fired"),
        "{human}"
    );
}

/// A sidecar the verified replay refuses is a blocker, and reading it
/// leaves every byte where it was.
#[test]
fn refused_evidence_blocks_and_reading_writes_nothing() {
    let registry = registry(SAUTER);
    let dir = project("refused");
    fired(
        dir.path(),
        WORKFLOW,
        "2026-09-28T03:00:00Z",
        Some(generation(&registry, "digest-now")),
    );
    let ledger = dir.path().join(".nika/arm/report/history.ndjson");
    let mut text = std::fs::read_to_string(&ledger).expect("ledger");
    text.push_str("{\"schema\":\"nika/arm-event@1\",\"seq\":3");
    std::fs::write(&ledger, text).expect("torn append");
    let before = sidecar_bytes(dir.path());
    let refused = receipt(&registry, dir.path(), "2026-09-28T03:01:00Z", ready());
    assert_eq!(refused.proof(), ProofBinding::Refused);
    assert!(refused.evidence_refused());
    assert_eq!(kinds(&refused), ["firing_evidence_invalid"]);
    let doc = refused.to_json();
    assert_eq!(doc["firing_evidence"]["status"], "refused");
    assert_eq!(doc["status"], "UNREADY");
    assert!(refused.human_lines(3).iter().all(|l| !l.contains("PROUVÉ")));
    assert_eq!(
        sidecar_bytes(dir.path()),
        before,
        "inspection repairs nothing"
    );
}

/// The project binding rides the identity apart: the generation never
/// hashes a label, so two bindings of one declaration share it.
#[test]
fn identity_keeps_the_project_binding_apart() {
    let registry = registry(SAUTER);
    let now = at("2026-09-28T03:00:10Z");
    let one = ScheduleReadinessReceipt::assemble(
        &registry,
        0,
        &now,
        identity(&registry, "report"),
        ready(),
        Err(std::io::Error::other("unused")),
    )
    .expect("beat");
    let mut other = identity(&registry, "report-2");
    other.project.root_fingerprint = None;
    let two = ScheduleReadinessReceipt::assemble(
        &registry,
        0,
        &now,
        other,
        ready(),
        Err(std::io::Error::other("unused")),
    )
    .expect("beat");
    let (one, two) = (one.to_json(), two.to_json());
    assert_eq!(one["generation"], two["generation"]);
    assert_eq!(one["project_identity"]["label"], "report");
    assert_eq!(two["project_identity"]["label"], "report-2");
    assert_eq!(two["project_identity"]["root_fingerprint"], Value::Null);
    assert_eq!(one["project_identity"]["project_file"], "nika.yaml");
}

/// JSON and human carry one verdict: the status word and every blocker.
#[test]
fn json_and_human_render_one_verdict() {
    let registry = registry("    manqué: rattraper\n");
    let dir = project("agree");
    let mut program = ready();
    program.required_inputs_ready = Some(false);
    program.blockers = vec![Blocker::new(
        "input_unbound",
        Some("NIKA-1708".to_owned()),
        Some("locale".to_owned()),
        "required input `locale` has no binding source",
    )];
    let unready = receipt(&registry, dir.path(), "2026-09-28T03:00:10Z", program);
    let doc = unready.to_json();
    let human = unready.human_lines(3).join("\n");
    assert!(human.starts_with(&format!("  [{}] ", doc["status"].as_str().expect("word"))));
    let blockers = doc["blockers"].as_array().expect("blockers");
    assert_eq!(blockers.len(), 2);
    for blocker in blockers {
        let kind = blocker["kind"].as_str().expect("kind");
        assert!(
            human.contains(&format!("({kind})")),
            "{kind} missing: {human}"
        );
    }
    assert!(
        human.contains("✗ NIKA-1708 · required input `locale`"),
        "{human}"
    );
}

/// A cadence change leaves the beat's own earlier record underivable (its
/// slot identity hashed the old cadence): it reads unattributed, with a
/// message that says so, and never PROUVÉ.
#[test]
fn a_cadence_change_leaves_the_earlier_record_unattributed() {
    let before = registry(SAUTER);
    let dir = project("recadenced");
    let pinned = Some(generation(&before, "digest-now"));
    fired(dir.path(), WORKFLOW, "2026-09-28T03:00:00Z", pinned);
    let source = format!(
        "nika: proj\narm:\n  - workflow: {WORKFLOW}\n    cadence: \"TZ=UTC 30 3 * * *\"\n    plafond: 0.05\n{SAUTER}"
    );
    let after = nika_cadence::parse_registry(&source).expect("parse");
    let moved = receipt(&after, dir.path(), "2026-09-28T03:31:00Z", ready());
    assert_eq!(moved.proof(), ProofBinding::Unattributed);
    assert!(
        moved
            .blockers()
            .iter()
            .any(|b| b.message.contains("before a cadence change")),
        "{:?}",
        moved.blockers()
    );
    assert!(!moved.human_lines(3).join("\n").contains("PROUVÉ"));
}
