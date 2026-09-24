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
    assert!(preview.contains("Declared is not active"), "{preview}");
    (id, preview)
}

fn declared(session: &mut SessionRuntime, root: &Path, id: &ProposalId, expected: &str) {
    let out = session.consent_to(id, "yes");
    assert!(
        matches!(out, TurnOutcome::Facts(ref text) if text.contains("Declared in `nika.yaml` · not active")),
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
        assert!(preview.contains("interval steps start at zero on the local clock"));
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
        "Every 2 days at 9",
        "Toutes les 2 semaines à 9h",
        "Every 5 hours",
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
            matches!(out, TurnOutcome::Refusal(ref r) if r.text.contains("incomplete, conflicting or unsupported")),
            "{phrase}: {out:?}"
        );
        assert!(session.pending_activation().is_none());
        assert!(session.pending_proposal().is_none());
        assert!(matches!(session.consent("yes"), TurnOutcome::Refusal(_)));
        assert!(!dir.path().join("nika.yaml").exists());
        assert!(!dir.path().join("out/copy.md").exists());
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
