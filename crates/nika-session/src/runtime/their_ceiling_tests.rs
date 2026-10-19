// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The person's ceiling stays theirs (live gauntlet r09, 2026-10-17): a ceiling set alone with
//! nothing waiting is the saved workflow's next runs', said by the Session with no model, a copy
//! of a saved workflow keeps the ceiling saved with it, and a run its own last run says would
//! pass its ceiling is refused before it starts.

use super::tests::{ready, saved_daily_copy, tree};
use super::{SessionRuntime, TurnOutcome};
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::money::MonetarySource;
use crate::reasoner::NoReasoner;

/// « Mets le plafond à 0,001 $ » sets the saved workflow's next runs, and the next run starts
/// under it, saying where its ceiling comes from.
#[test]
fn a_ceiling_set_alone_is_the_saved_workflows_next_runs() {
    let dir = tree();
    let mut s = saved_daily_copy(dir.path());
    let TurnOutcome::Facts(set) = s.turn("Mets le plafond à 0,001 $") else {
        panic!("the Session says it");
    };
    assert!(
        set.starts_with("plafond de `") && set.contains("0,001 $ par run"),
        "{set}"
    );
    let TurnOutcome::RunRequested { report, run } = s.turn("lance-le") else {
        panic!("the run is requested");
    };
    assert!(
        (run.max_cost_usd - 0.001).abs() < 1e-12,
        "{}",
        run.max_cost_usd
    );
    // Said in the person's French (live gauntlet wave 6 r09).
    assert!(
        report.contains("plafond $0.001 (celui que vous avez fixé pour ses prochains runs)"),
        "{report}"
    );
    // A run line that states its own ceiling still has it.
    let TurnOutcome::RunRequested { run, .. } = s.turn("lance-le avec un plafond de 0,05 $") else {
        panic!("the run is requested");
    };
    assert!(
        (run.max_cost_usd - 0.05).abs() < 1e-12,
        "{}",
        run.max_cost_usd
    );
}

/// A line that says more than the ceiling is not this door.
#[test]
fn only_a_line_that_sets_the_ceiling_alone_is_the_door() {
    let dir = tree();
    let s = saved_daily_copy(dir.path());

    for line in [
        "Mets le plafond à 0,001 $",
        "plafond 0,05 $ par run",
        "set the ceiling to $0.05",
        "Le plafond est de 0,10 $ par run stp",
        "baisse le budget à 0,01 dollar",
    ] {
        assert!(s.sets_the_ceiling(line).is_some(), "{line}");
    }
    for line in [
        "Mets Haiku pour l'étape IA, plafond 0,10 $",
        "lis ./a.csv et garde les lignes sous 5 $",
        "combien a coûté le run ?",
        "le plafond ?",
    ] {
        assert!(s.sets_the_ceiling(line).is_none(), "{line}");
    }
}

/// A copy of a saved workflow keeps the ceiling saved with it, as a revision does; other new
/// work keeps its own default (the F-M1 law).
#[test]
fn a_copy_of_a_saved_workflow_keeps_its_ceiling_and_new_work_its_own_default() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    let TurnOutcome::Proposal { .. } =
        s.turn("Lis ./notes/brief.md et écris-le dans ./out/a.md. Budget max 0,10 $ par run.")
    else {
        panic!("proposed");
    };
    let TurnOutcome::Facts(_) = s.consent("oui") else {
        panic!("saved");
    };
    let saved = s.last_workflow.clone().expect("saved");
    let stem = saved.display().to_string();
    let stem = stem.trim_end_matches(".nika");
    let _ = s.turn(&format!(
        "Crée-moi un nouveau workflow identique à {stem}.nika mais qui écrit dans ./out/b.md"
    ));
    let money = s.monetary_decision().expect("bound");
    assert_eq!(money.source, MonetarySource::Explicit, "{money:?}");
    assert!(
        (money.effective_usd).is_some_and(|usd| (usd - 0.10).abs() < 1e-12),
        "{money:?}"
    );
    let _ = s.turn("Lis ./notes/brief.md et écris-le dans ./out/c.md");
    let money = s.monetary_decision().expect("its own");
    assert_eq!(money.source, MonetarySource::SessionDefault, "{money:?}");
}

/// A run whose own last run cost more than its ceiling is refused before it starts, with how to
/// go on; other bytes are no evidence (live gauntlet r09: $0.0035 spent under $0.001, nothing
/// written).
#[test]
fn a_run_its_last_run_says_would_exceed_the_ceiling_is_refused_before_it_starts() {
    let dir = tree();
    let mut s = saved_daily_copy(dir.path());
    let workflow = s.last_workflow.clone().expect("saved");
    let bytes = std::fs::read(dir.path().join(&workflow)).expect("workflow");
    let sha = nika_event::source_id::sha256_hex(&bytes);
    let traces = dir.path().join(".nika/traces");
    std::fs::create_dir_all(&traces).expect("traces");
    let run = "01a1489f-a9d7-71f0-a44b-dc8a90d7db7c";
    let frames = [
        format!(
            r#"{{"kind":"workflow_started","execution":{{"uuid":"{run}"}},"fields":[{{"key":"workflow","value":"copy"}},{{"key":"workflow_sha256","value":"{sha}"}}]}}"#
        ),
        format!(
            r#"{{"kind":"workflow_completed","execution":{{"uuid":"{run}"}},"fields":[{{"key":"status","value":"succeeded"}},{{"key":"priced_calls","value":1}},{{"key":"total_cost_usd","value":0.0035}}]}}"#
        ),
    ];
    std::fs::write(traces.join("t-7db7c.ndjson"), frames.join("\n")).expect("trace");
    let _ = s.observe_run(0, Some(std::path::Path::new(".nika/traces/t-7db7c.ndjson")));
    let TurnOutcome::Refusal(refused) = s.turn("lance-le avec un plafond de 0,001 $") else {
        panic!("refused before it starts");
    };
    assert!(
        refused.text.starts_with("pas lancé : le dernier run de `")
            && refused
                .text
                .contains("relevez le plafond (« lance-le avec un plafond de"),
        "{}",
        refused.text
    );
    // A ceiling above what it cost runs.
    assert!(matches!(
        s.turn("lance-le avec un plafond de 0,05 $"),
        TurnOutcome::RunRequested { .. }
    ));
    // Changed bytes are no evidence.
    std::fs::write(
        dir.path().join(&workflow),
        [bytes.as_slice(), b"\n"].concat(),
    )
    .expect("edit");
    assert!(!matches!(
        s.turn("lance-le avec un plafond de 0,001 $"),
        TurnOutcome::Refusal(ref r) if r.text.starts_with("pas lancé : le dernier run")
    ));
}
