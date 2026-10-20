// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Public Session → real deterministic Compiler → Save → activation preview → project
//! bytes → canonical cadence parser. No injected `CompileOutcome`, model, firer or clock.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use nika_session::lifecycle::Stage;
use nika_session::reasoner::NoReasoner;
use nika_session::turn::{RoutingMethod, TurnAct, TurnClassifier, TurnContext, TurnDecision};
use nika_session::{ProposalId, SessionRuntime, TurnOutcome};
use std::path::Path;

fn open(root: &Path) -> SessionRuntime {
    let resolved = ResolvedSessionIntelligence::resolve(
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        &IntelligenceCensus::empty(),
    );
    SessionRuntime::open(root, resolved, Box::new(NoReasoner))
}

fn intent(phrase: &str) -> String {
    format!("{phrase}, read ./notes/brief.md and write it to ./out/copy.md")
}

fn saved(root: &Path, phrase: &str) -> SessionRuntime {
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("notes/brief.md"), "unchanged input\n").unwrap();
    let mut session = open(root);
    save(&mut session, root, &intent(phrase));
    session
}

fn save(session: &mut SessionRuntime, root: &Path, request: &str) {
    let out = session.turn(request);
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("{request}: {out:?}")
    };
    let out = session.consent_to(&id, "yes");
    assert!(matches!(out, TurnOutcome::Facts(_)), "{out:?}");
    assert!(
        !root.join("nika.yaml").exists(),
        "Save never declares or activates"
    );
    assert!(!root.join("out/copy.md").exists(), "Save never runs");
    assert_eq!(session.lifecycle().saved, Stage::Done);
    assert_eq!(session.lifecycle().active, Stage::Pending);
    assert_eq!(session.lifecycle().run, Stage::Pending);
}

fn review(session: &mut SessionRuntime, root: &Path) -> (ProposalId, String) {
    for (line, expected) in [
        ("activate", "project.timezone"),
        ("Europe/Paris", "project.missed"),
        ("2", "project.ceiling"),
    ] {
        let out = session.turn(line);
        assert!(
            matches!(out, TurnOutcome::Question { ref key, .. } if key == expected),
            "{out:?}"
        );
    }
    let out = session.turn("0.2");
    let TurnOutcome::Proposal { id, preview } = out else {
        panic!("{out:?}")
    };
    assert!(!root.join("nika.yaml").exists());
    assert!(!root.join("out/copy.md").exists());
    // The Session says its own lines in the person's language: English or French requests.
    assert!(
        preview.contains("Declared is not active") || preview.contains("Déclaré n'est pas actif"),
        "{preview}"
    );
    (id, preview)
}

fn declared(session: &mut SessionRuntime, root: &Path, id: &ProposalId, expected: &str) {
    let out = session.consent_to(id, "yes");
    assert!(
        matches!(out, TurnOutcome::Facts(ref text) if text.contains("Declared in `nika.yaml` · not active") || text.contains("Déclaré dans `nika.yaml` · pas actif")),
        "{out:?}"
    );
    let text = std::fs::read_to_string(root.join("nika.yaml")).unwrap();
    let registry = nika_cadence::parse::parse_registry(&text).unwrap();
    assert_eq!(registry.beat_count(), 1);
    assert_eq!(session.lifecycle().active, Stage::Declared);
    assert_eq!(session.lifecycle().run, Stage::Pending);
    let beat = registry.beats().next().unwrap();
    assert_eq!(beat.cadence, expected);
    assert_eq!(beat.plafond, Some(0.2));
    assert!(text.contains("manqué: sauter"), "{text}");
    let (_, project) = nika_vocab::project::discover(root).unwrap().unwrap();
    assert_eq!(project.arm()[0].cadence, expected);
    assert!(
        matches!(nika_cadence::registry::Cadence::parse(&beat.cadence).unwrap(), nika_cadence::registry::Cadence::Cron { ref tz, .. } if tz == "Europe/Paris")
    );
    assert!(
        !root.join("out/copy.md").exists(),
        "Declaration is not execution"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("notes/brief.md")).unwrap(),
        "unchanged input\n"
    );
    assert!(matches!(
        session.consent_to(id, "yes"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(
        std::fs::read_to_string(root.join("nika.yaml")).unwrap(),
        text
    );
}

#[test]
fn compile_to_persisted_cadence_preserves_weekday_interval_and_explicit_time() {
    for (phrase, fields) in [
        ("Every Tuesday at 9:15", "15 9 * * 2"),
        ("Every Friday at 18:30", "30 18 * * 5"),
        ("Chaque mardi à 9h15", "15 9 * * 2"),
        ("Tous les vendredis à 18h30", "30 18 * * 5"),
        ("Every 2 hours", "0 */2 * * *"),
        ("Toutes les 2 heures", "0 */2 * * *"),
        ("Toutes les deux heures", "0 */2 * * *"),
        ("Every hour", "0 * * * *"),
        ("Chaque heure", "0 * * * *"),
        ("Every day at 16:45", "45 16 * * *"),
        ("Tous les jours à 16h45", "45 16 * * *"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut session = saved(dir.path(), phrase);
        let (id, preview) = review(&mut session, dir.path());
        let expected = format!("TZ=Europe/Paris {fields}");
        assert!(preview.contains(&expected), "{phrase}: {preview}");
        assert!(
            preview.contains(&format!("in `nika.yaml`: `{expected}`")),
            "{phrase}: {preview}"
        );
        declared(&mut session, dir.path(), &id, &expected);
    }
}

#[test]
fn incomplete_conflicting_and_unsupported_periods_never_declare_a_schedule() {
    for phrase in [
        "Every Tuesday",
        "Chaque vendredi",
        "Every day",
        "Tous les matins",
        "Every week at 9",
        "Every month at 9",
        "Chaque mois à 9h",
        "Every day and every 2 hours at 9",
        "Chaque jour et chaque semaine à 9h",
        "Every Tuesday at 9 and 10",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut session = saved(dir.path(), phrase);
        let out = session.turn("activate");
        assert!(
            matches!(out, TurnOutcome::Refusal(ref r) if r.text.contains("incomplete, conflicting or unsupported")
                || r.text.contains("incomplète, contradictoire ou pas encore prise en charge")),
            "{phrase}: {out:?}"
        );
        assert!(session.pending_activation().is_none());
        assert!(session.pending_proposal().is_none());
        assert!(matches!(session.consent("yes"), TurnOutcome::Refusal(_)));
        assert!(!dir.path().join("nika.yaml").exists());
        assert!(!dir.path().join("out/copy.md").exists());
    }
}

#[test]
fn unbindable_periods_require_explicit_authoring_clarification_before_save() {
    for phrase in [
        "Every 2 days at 9",
        "Toutes les 2 semaines à 9h",
        "Every 5 hours",
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("notes")).unwrap();
        std::fs::write(dir.path().join("notes/brief.md"), "unchanged input\n").unwrap();
        let mut session = open(dir.path());
        let out = session.turn(&intent(phrase));
        assert!(matches!(out, TurnOutcome::Question { ref key, .. } if key == "trigger.cadence"));
        assert!(session.pending_proposal().is_none());
        assert!(session.pending_activation().is_none());
        assert_eq!(session.lifecycle().saved, Stage::Pending);
        let invalid = session.turn("yes");
        assert!(
            matches!(invalid, TurnOutcome::Question { ref key, .. } if key == "trigger.cadence")
        );
        assert!(!dir.path().join("nika.yaml").exists());
        assert!(!dir.path().join("out/copy.md").exists());
        let clarified = session.turn("Every Monday at 09:00");
        let TurnOutcome::Proposal { id, .. } = clarified else {
            panic!("clarified cadence did not produce a proposal");
        };
        assert!(matches!(
            session.consent_to(&id, "yes"),
            TurnOutcome::Facts(_)
        ));
        let (id, _) = review(&mut session, dir.path());
        declared(&mut session, dir.path(), &id, "TZ=Europe/Paris 0 9 * * 1");
    }
}

struct Modify;
impl TurnClassifier for Modify {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        TurnDecision::new(TurnAct::Modify, RoutingMethod::Fallback)
    }
}

#[test]
fn cancel_and_recognized_revision_expire_old_declaration_consent() {
    for line in ["cancel", "instead use Friday at 10"] {
        let dir = tempfile::tempdir().unwrap();
        let mut session = saved(dir.path(), "Every Tuesday at 9");
        let (old, _) = review(&mut session, dir.path());
        session.with_classifier(Box::new(Modify));
        let out = session.consent_to(&old, line);
        assert!(
            matches!(out, TurnOutcome::Facts(ref text) if text.starts_with("discarded")),
            "{out:?}"
        );
        assert!(session.pending_proposal().is_none());
        assert!(matches!(
            session.consent_to(&old, "yes"),
            TurnOutcome::Refusal(_)
        ));
        assert!(!dir.path().join("nika.yaml").exists());
        save(&mut session, dir.path(), &intent("Every Friday at 10"));
        let (fresh, preview) = review(&mut session, dir.path());
        assert_ne!(fresh, old);
        assert!(preview.contains("TZ=Europe/Paris 0 10 * * 5"));
        assert!(matches!(
            session.consent_to(&old, "yes"),
            TurnOutcome::Refusal(_)
        ));
        assert!(!dir.path().join("nika.yaml").exists());
        declared(
            &mut session,
            dir.path(),
            &fresh,
            "TZ=Europe/Paris 0 10 * * 5",
        );
    }
}

#[test]
fn question_cancel_stale_project_and_restart_have_no_declaration_effect() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = saved(dir.path(), "Every Tuesday at 9");
    assert!(matches!(
        session.turn("activate"),
        TurnOutcome::Question { .. }
    ));
    assert!(matches!(session.turn("cancel"), TurnOutcome::Facts(_)));
    assert!(session.pending_activation().is_none());
    assert!(!dir.path().join("nika.yaml").exists());
    let (id, _) = review(&mut session, dir.path());
    std::fs::write(dir.path().join("nika.yaml"), "nika: changed\n").unwrap();
    assert!(matches!(
        session.consent_to(&id, "yes"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("nika.yaml")).unwrap(),
        "nika: changed\n"
    );
    drop(session);
    let mut restarted = open(dir.path());
    assert!(matches!(
        restarted.consent_to(&id, "yes"),
        TurnOutcome::Refusal(_)
    ));
    assert!(!dir.path().join("out/copy.md").exists());
}

#[test]
fn a_clarified_cadence_reaches_the_same_activation_consumer() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("notes")).unwrap();
    std::fs::write(dir.path().join("notes/brief.md"), "unchanged input\n").unwrap();
    let mut session = open(dir.path());
    let out = session.turn("Régulièrement, lis ./notes/brief.md et écris-le dans ./out/copy.md");
    assert!(
        matches!(out, TurnOutcome::Question { ref key, .. } if key == "trigger.cadence"),
        "{out:?}"
    );
    let out = session.turn("chaque vendredi à 10h");
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("{out:?}")
    };
    assert!(matches!(
        session.consent_to(&id, "yes"),
        TurnOutcome::Facts(_)
    ));
    assert!(!dir.path().join("nika.yaml").exists());
    let (id, preview) = review(&mut session, dir.path());
    assert!(preview.contains("TZ=Europe/Paris 0 10 * * 5"), "{preview}");
    declared(&mut session, dir.path(), &id, "TZ=Europe/Paris 0 10 * * 5");
}

/// The saved workflow's name, as its `nika:` line states it.
fn workflow_name(root: &Path) -> String {
    let saved = (std::fs::read_dir(root).unwrap().flatten())
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "nika"))
        .expect("a saved workflow");
    let text = std::fs::read_to_string(saved).unwrap();
    let line = text.lines().find_map(|line| line.strip_prefix("nika: "));
    line.expect("its name").trim().to_owned()
}

/// A succeeded run of `name` under `root` whose trace states `cost` as its total.
fn ran(root: &Path, name: &str, cost: f64) {
    ran_with(
        root,
        name,
        &format!(r#",{{"key":"total_cost_usd","value":{cost}}}"#),
    );
}

/// A succeeded run of `name` whose settlement carries `cost` (its `total_cost_usd` field, or
/// nothing when the run recorded no priced cost).
fn ran_with(root: &Path, name: &str, cost: &str) {
    let frame = |kind: &str, ms: u64, fields: &str| {
        format!(
            r#"{{"id":{{"uuid":"01a0e819-b6cd-70f2-8edf-c29fbd00bc4d"}},"timestamp":{ms}000000,"kind":"{kind}","execution":{{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"}},"run":null,"correlation":null,"fields":[{fields}]}}"#
        )
    };
    let named = format!(r#"{{"key":"workflow","value":"{name}"}}"#);
    let settled = format!(r#"{named},{{"key":"status","value":"succeeded"}}{cost}"#);
    let traces = root.join(".nika/traces");
    std::fs::create_dir_all(&traces).unwrap();
    let body = format!(
        "{}\n{}\n",
        frame("workflow_started", 1_790_600_394_445, &named),
        frame("workflow_completed", 1_790_600_394_945, &settled)
    );
    std::fs::write(traces.join("2026-10-15T09-00-00Z-run.ndjson"), body).unwrap();
}

/// The activation of a saved weekly copy, answered up to the policy for a missed run.
fn at_the_missed_policy(root: &Path) -> SessionRuntime {
    let mut session = saved(root, "Every Tuesday at 9");
    assert!(matches!(
        session.turn("activate"),
        TurnOutcome::Question { .. }
    ));
    let out = session.turn("Europe/Paris");
    let TurnOutcome::Question { key, question } = out else {
        panic!("{out:?}")
    };
    assert_eq!(key, "project.missed");
    // A host with no rows still reads every option as text.
    assert!(
        question.contains("1  run once when it comes back"),
        "{question}"
    );
    session
}

#[test]
fn the_finite_activation_questions_offer_their_options_natively() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = at_the_missed_policy(dir.path());
    let (key, said, options, typed) = session.waiting_activation_choices().expect("rows");
    assert_eq!(key, "project.missed");
    assert!(
        said.starts_with("If this machine is off at 09:00 (Europe/Paris)"),
        "{said}"
    );
    assert_eq!(
        said.lines().count(),
        2,
        "the sentence and one short line: {said}"
    );
    assert!(
        !said.contains("run once"),
        "the rows are never repeated as text: {said}"
    );
    let keys: Vec<&str> = options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, ["1", "2", "3"]);
    assert!(options.iter().all(|o| !o.recommended) && !typed);
    let out = session.turn("2");
    let TurnOutcome::Question { key, question } = out else {
        panic!("{out:?}")
    };
    assert_eq!(key, "project.ceiling");
    let fallback = "0.25  recommended · no run of this workflow observed yet";
    assert!(question.contains(fallback), "{question}");
    assert!(question.contains("or type another amount"), "{question}");
    let (key, said, options, typed) = session.waiting_activation_choices().expect("rows");
    assert_eq!(key, "project.ceiling");
    assert_eq!(said.lines().count(), 2, "{said}");
    let keys: Vec<&str> = options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, ["0.10", "0.25", "0.50", "1.00"]);
    let recommended: Vec<&str> = (options.iter())
        .filter(|o| o.recommended)
        .map(|o| o.label.as_str())
        .collect();
    assert_eq!(
        recommended,
        ["$0.25 · no run of this workflow observed yet"]
    );
    assert!(typed, "another amount is typed");
    // No amount is ever applied for the person: a consent word answers nothing.
    assert!(matches!(session.turn("yes"), TurnOutcome::Refusal(_)));
    assert_eq!(session.pending_activation(), Some("project.ceiling"));
    let out = session.turn("0.25");
    let TurnOutcome::Proposal { preview, .. } = out else {
        panic!("{out:?}")
    };
    assert!(
        preview.contains("ceiling · $0.25 per scheduled run"),
        "{preview}"
    );
    assert!(session.waiting_activation_choices().is_none());
}

#[test]
fn the_ceiling_recommended_reads_the_workflows_last_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = saved(dir.path(), "Every Tuesday at 9");
    ran(dir.path(), &workflow_name(dir.path()), 0.033);
    assert!(matches!(
        session.turn("activate"),
        TurnOutcome::Question { .. }
    ));
    assert!(
        session.waiting_activation_choices().is_none(),
        "a zone is typed"
    );
    assert!(matches!(
        session.turn("Europe/Paris"),
        TurnOutcome::Question { .. }
    ));
    assert!(matches!(session.turn("1"), TurnOutcome::Question { .. }));
    let (_, _, options, _) = session.waiting_activation_choices().expect("rows");
    let recommended: Vec<(&str, &str)> = (options.iter())
        .filter(|o| o.recommended)
        .map(|o| (o.key.as_str(), o.label.as_str()))
        .collect();
    assert_eq!(recommended, [("0.10", "$0.10 · 3 × the last run ($0.033)")]);
    // Picking the offered amount answers as typing it does.
    let out = session.turn("0.10");
    let TurnOutcome::Proposal { preview, .. } = out else {
        panic!("{out:?}")
    };
    assert!(
        preview.contains("ceiling · $0.1 per scheduled run"),
        "{preview}"
    );
}

/// A green run that recorded no priced cost (no model call) is still a run observed: the
/// ceiling says it cost nothing priced, never that no run was observed (live gauntlet,
/// 2026-10-16: three personas read « no run of this workflow observed yet » after green runs).
#[test]
fn a_run_without_a_priced_cost_is_still_observed() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = saved(dir.path(), "Every Tuesday at 9");
    ran_with(dir.path(), &workflow_name(dir.path()), "");
    assert!(matches!(
        session.turn("activate"),
        TurnOutcome::Question { .. }
    ));
    assert!(matches!(
        session.turn("Europe/Paris"),
        TurnOutcome::Question { .. }
    ));
    assert!(matches!(session.turn("1"), TurnOutcome::Question { .. }));
    let (_, _, options, _) = session.waiting_activation_choices().expect("rows");
    let recommended: Vec<&str> = (options.iter())
        .filter(|o| o.recommended)
        .map(|o| o.label.as_str())
        .collect();
    assert_eq!(recommended, ["$0.10 · the last run cost nothing priced"]);
}

/// A choice left to Nika on the activation's questions takes the run-once policy and the
/// recommended ceiling, both shown in the declaration the person consents to (live gauntlet,
/// 2026-10-16: « choisis pour moi » was dropped on these two questions).
#[test]
fn a_delegated_activation_answer_takes_the_shown_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = at_the_missed_policy(dir.path());
    let TurnOutcome::Question { key, .. } = session.turn("choisis pour moi") else {
        panic!("the ceiling is asked next");
    };
    assert_eq!(key, "project.ceiling");
    let out = session.turn("fais au mieux, ça coûte presque rien");
    let TurnOutcome::Proposal { preview, .. } = out else {
        panic!("{out:?}")
    };
    assert!(preview.contains("if missed · catch up once"), "{preview}");
    assert!(
        preview.contains("ceiling · $0.25 per scheduled run"),
        "{preview}"
    );
}

/// « Active-le » with its cadence declares that cadence, the person's words read as they speak
/// (live gauntlet r02/r05/r08, 2026-10-16): a city for the zone, a sentence for the missed run,
/// a delegation after another clause for the ceiling.
#[test]
fn an_activation_that_states_its_cadence_declares_it_in_the_persons_words() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("notes/brief.md"), "unchanged input\n").unwrap();
    let mut session = open(root);
    save(
        &mut session,
        root,
        "Read ./notes/brief.md and write it to ./out/copy.md",
    );
    for (line, expected) in [
        (
            "Maintenant active-le tous les matins à 7h stp",
            "project.timezone",
        ),
        ("heure de Paris", "project.missed"),
        (
            "si l'ordinateur est éteint, le faire une fois quand il se rallume",
            "project.ceiling",
        ),
    ] {
        let out = session.turn(line);
        assert!(
            matches!(out, TurnOutcome::Question { ref key, .. } if key == expected),
            "{line}: {out:?}"
        );
    }
    let out = session.turn("je ne comprends pas, choisissez pour moi");
    let TurnOutcome::Proposal { preview, .. } = out else {
        panic!("{out:?}")
    };
    assert!(preview.contains("TZ=Europe/Paris 0 7 * * *"), "{preview}");
    assert!(
        preview.contains("si manqué · le rattraper une fois"),
        "a French activation is declared in French: {preview}"
    );
    assert!(
        !root.join("nika.yaml").exists(),
        "nothing declared before consent"
    );
}
