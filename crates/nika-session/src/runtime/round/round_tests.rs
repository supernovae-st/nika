// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The kept round (C7 · ORACLE-r2 R/V/I/K/M, crate level) over the runtime's public doors: a
//! no-intelligence session (the reasoner, named `none`, counts any prompt it is handed), local
//! files, the deterministic compiler. No provider, no workflow execution, no paid request. A
//! dropped runtime is a close without `/quit` — the crate-level KILL: nothing runs at drop, and
//! the history keeps what its last completed record wrote.

use std::path::Path;
use std::sync::{Arc, Mutex};

use nika_onboard::compile::CompileRequest;
use nika_onboard::compile::round::{Capture, change_money};
use serde_json::{Value, json};

use super::KeptRound;
use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::outcome::{QuestionId, Refusal, RefusalClass};
use crate::reasoner::{ReasonError, Reply, SessionReasoner};
use crate::runtime::{SessionRuntime, TurnOutcome};

/// A round whose first question the human answers (the model), then closes at the endpoint.
const BRIEF: &str =
    "Read ./notes/brief.md, draft a 3-bullet summary of it and post the summary to a webhook";
/// A round whose first question asks a clause in words (the human's words restate the
/// request), then closes at the endpoint.
const ORDERS: &str = "Read ./data/orders.csv, compute the total amount per customer and post the result to a webhook";
const RULE: &str = "the total of the amount column for each customer";
const URL: &str = "https://hooks.example.com/orders";
/// A round whose only question is the model.
const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
/// A public synthetic sentinel the broker names (`sk-`), never a real credential.
const SENTINEL: &str = "sk-c7ROUNDSENTINEL000000000000";
/// Where the session lands a candidate in a root without `workflows/`.
const LANDED: &str = "compiled-workflow.nika";

type Seen = Arc<Mutex<Vec<String>>>;

/// The no-intelligence reasoner, counted: named `none` like [`crate::reasoner::NoReasoner`] (so
/// nothing reads replies) and refusing every prompt it is handed, after recording it.
struct Player {
    seen: Seen,
}

impl SessionReasoner for Player {
    fn name(&self) -> String {
        "none".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.seen
            .lock()
            .expect("prompt record")
            .push(prompt.to_owned());
        Err(ReasonError::NoIntelligence)
    }
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("project");
    std::fs::create_dir_all(dir.path().join("data")).expect("data");
    std::fs::write(
        dir.path().join("data/orders.csv"),
        "customer,amount,status\nacme,10,late\nbeta,5,ok\nacme,7,ok\n",
    )
    .expect("orders");
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(
        dir.path().join("notes/brief.md"),
        "# Brief\n\nOctober launch.\n",
    )
    .expect("brief");
    dir
}

fn open(root: &Path) -> (SessionRuntime, Seen) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let player = Player {
        seen: Arc::clone(&seen),
    };
    let intelligence = ResolvedSessionIntelligence {
        kind: IntelligenceKind::None,
        model: None,
        locus: DataLocus::None,
        ready: true,
        why: None,
    };
    (
        SessionRuntime::open(root, intelligence, Box::new(player)),
        seen,
    )
}

/// A session with history under `home`, and its notice.
fn reopen(root: &Path, home: &Path) -> (SessionRuntime, Seen, String) {
    let (mut session, seen) = open(root);
    let notice = session
        .enable_history(home)
        .expect("history")
        .unwrap_or_default();
    (session, seen, notice)
}

fn question(outcome: TurnOutcome) -> (String, String) {
    match outcome {
        TurnOutcome::Question { key, question } => (key, question),
        other => panic!("expected a question, got {other:?}"),
    }
}

fn proposal(outcome: TurnOutcome) -> (crate::outcome::ProposalId, String) {
    match outcome {
        TurnOutcome::Proposal { id, preview } => (id, preview),
        other => panic!("expected a proposal, got {other:?}"),
    }
}

fn refused(outcome: TurnOutcome) -> Refusal {
    match outcome {
        TurnOutcome::Refusal(why) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn facts(outcome: TurnOutcome) -> String {
    match outcome {
        TurnOutcome::Facts(text) => text,
        other => panic!("expected facts, got {other:?}"),
    }
}

fn journal(home: &Path, root: &Path) -> String {
    let canonical = root.canonicalize().expect("canonical");
    let identity = blake3::hash(canonical.to_str().expect("UTF-8").as_bytes());
    let log = home
        .join(".nika/sessions")
        .join(identity.to_hex().as_str())
        .join("events.ndjson");
    std::fs::read_to_string(log).expect("history")
}

/// The round the history's last completed record keeps.
fn kept_round(home: &Path, root: &Path) -> Option<Value> {
    journal(home, root)
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|record| record["event"]["kind"] == "completed")
        .and_then(|record| {
            let round = &record["event"]["state"]["round"];
            (!round.is_null()).then(|| round.clone())
        })
}

/// The BRIEF round closed (no `/quit`) at its second question, and the identity that waited.
fn brief_closed_at_the_endpoint(root: &Path, home: &Path) -> QuestionId {
    let (mut first, _, _) = reopen(root, home);
    assert_eq!(question(first.turn(BRIEF)).0, "model");
    assert_eq!(
        question(first.turn("mock/echo")).0,
        "const.publish_endpoint"
    );
    let id = first.pending_question_id().expect("a question waits");
    drop(first);
    id
}

/// The kept-round slot of a runtime, as a fresh open would fill it from `raw`.
fn keep(session: &mut SessionRuntime, raw: Value) {
    session.restored_round = Some(KeptRound::new(raw, Vec::new()));
}

/// Every file under `root`, sorted.
fn tree(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(
                    path.strip_prefix(root)
                        .expect("inside")
                        .display()
                        .to_string(),
                );
            }
        }
    }
    out.sort();
    out
}

/// A kept record over `request`, with the questions the deterministic core asks for `skeleton`.
fn crafted(request: &str, skeleton: &str) -> Capture<'static> {
    let questions = crate::authoring::compile_deterministic(&CompileRequest::create(skeleton))
        .expect("compiles")
        .questions;
    Capture::new(request, &|t: &str| crate::broker::redact(t).0).questions(&questions)
}

/// R1 · R2 · INV-LIVE-EQUIV · a round closed after one settled answer is named at open, kept
/// until `/restore`, asked again under a fresh identity (the closed session's is refused), and
/// its next answer reaches the live session's proposal, byte for byte, with no model and no
/// write.
#[test]
fn a_round_closed_after_an_answer_is_kept_and_continued_to_the_live_proposal() {
    let live_root = project();
    let (mut live, _) = open(live_root.path());
    question(live.turn(BRIEF));
    question(live.turn("mock/echo"));
    let (live_id, live_preview) = proposal(live.turn(URL));

    let root = project();
    let home = tempfile::tempdir().expect("home");
    let old_id = brief_closed_at_the_endpoint(root.path(), home.path());
    let kept = kept_round(home.path(), root.path()).expect("the round is kept");
    assert_eq!(kept["request"]["text"], BRIEF);
    assert_eq!(kept["answers"][0]["key"], "model");
    assert_eq!(kept["answers"][0]["literal"]["text"], "\"mock/echo\"");
    assert_eq!(kept["questions"][0]["key"], "const.publish_endpoint");
    for field in ["money", "observed", "consent", "account", "question_id"] {
        assert!(kept.get(field).is_none(), "{field} in {kept}");
    }

    let (mut again, seen, notice) = reopen(root.path(), home.path());
    assert!(
        notice.contains("restored round: « Read ./notes/brief.md"),
        "{notice}"
    );
    assert!(notice.contains("settled model → mock/echo"), "{notice}");
    assert!(notice.contains("/restore"), "{notice}");
    assert!(
        !notice.contains("expired"),
        "a restorable round is never announced as expired: {notice}"
    );
    assert!(
        again.pending_question().is_none(),
        "kept, never live before /restore"
    );
    assert!(again.intent.unresolved.is_empty());
    assert!(again.slash_commands().contains(&"/restore"));
    assert!(again.help_card().contains(super::ROUND_HELP));

    let (key, text) = question(again.turn("/restore"));
    assert_eq!(
        key, "const.publish_endpoint",
        "the same question, asked again"
    );
    assert!(
        text.contains("your request from the last session"),
        "{text}"
    );
    assert!(text.contains("no AI asked"), "{text}");
    let new_id = again.pending_question_id().expect("asked again");
    assert_ne!(new_id, old_id, "a fresh identity in this session");
    let stale = refused(again.answer_question_for(&old_id, URL));
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{}", stale.text);
    assert_eq!(
        again.pending_question_id(),
        Some(new_id),
        "the stale token bound nothing"
    );
    let (id, preview) = proposal(again.turn(URL));
    assert_eq!(id, live_id, "the restored round reaches the live proposal");
    assert_eq!(preview, live_preview, "the same review, byte for byte");
    assert!(seen.lock().expect("record").is_empty(), "no model, ever");
    assert!(
        !root.path().join(LANDED).exists(),
        "a proposal writes nothing"
    );
}

/// R2 · a round whose clause was restated in words keeps the restated request and its plan;
/// continued, it reaches the live proposal's exact review. Its consent identity differs from
/// the live one by the monetary input it binds: the gate read the kept (restated) request at
/// `/restore`, where the live gate read the first wording — each identity names what its own
/// session's gate admitted, and none grants anything across sessions.
#[test]
fn a_restated_round_is_kept_with_its_plan_and_reaches_the_live_review() {
    let live_root = project();
    let (mut live, _) = open(live_root.path());
    question(live.turn(ORDERS));
    question(live.turn(RULE));
    let (live_id, live_preview) = proposal(live.turn(URL));

    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut first, _, _) = reopen(root.path(), home.path());
    assert_eq!(question(first.turn(ORDERS)).0, "const.rule_expression");
    assert_eq!(question(first.turn(RULE)).0, "const.publish_endpoint");
    drop(first);
    let kept = kept_round(home.path(), root.path()).expect("kept");
    let restated = ORDERS.replacen("the total amount per customer", RULE, 1);
    assert_eq!(kept["request"]["text"], restated.as_str());
    assert!(
        kept["continuation"]["value"]["rules"]
            .as_array()
            .is_some_and(|r| !r.is_empty())
    );

    let (mut again, seen, _) = reopen(root.path(), home.path());
    question(again.turn("/restore"));
    let (id, preview) = proposal(again.turn(URL));
    let review = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|line| !line.contains(" identity "))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(review(&preview), review(&live_preview), "the same review");
    assert_ne!(
        id, live_id,
        "the identity binds this session's monetary input"
    );
    assert!(preview.contains(&format!("identity {}", &id.to_string()[..12])));
    assert!(seen.lock().expect("record").is_empty());
}

/// V3 · a source whose header changed while the session was closed: `/restore` replays the
/// recorded observation (zero reads), and the next answer round observes the project now — the
/// compiler asks again which field the rule means, the original rule words kept, nothing bound
/// stale and nothing proposed from the old header.
#[test]
fn a_source_changed_while_closed_is_asked_again_never_bound_stale() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut first, _, _) = reopen(root.path(), home.path());
    question(first.turn(ORDERS));
    question(first.turn(RULE));
    drop(first);
    std::fs::write(
        root.path().join("data/orders.csv"),
        "customer,price,status\nacme,10,late\nbeta,5,ok\nacme,7,ok\n",
    )
    .expect("the header changed");
    let (mut again, seen, _) = reopen(root.path(), home.path());
    let (key, _) = question(again.turn("/restore"));
    assert_eq!(
        key, "const.publish_endpoint",
        "the recorded observation asks the same"
    );
    let (key, text) = question(again.turn(URL));
    assert_ne!(key, "const.publish_endpoint");
    assert!(
        text.contains("`amount`") && text.contains("price"),
        "{text}"
    );
    assert!(
        again.pending_proposal().is_none(),
        "nothing proposed over the old header"
    );
    assert!(
        again
            .authoring
            .as_ref()
            .is_some_and(|r| r.intent.contains(RULE)),
        "the original business words stay"
    );
    assert!(seen.lock().expect("record").is_empty());
}

/// R6 · I6 · I7 · INV-READONLY · opening and every read-only line leave the kept round
/// byte-identical: `/meaning`, `/why` and `/status` read it, an unknown command and an unknown
/// utterance bind nothing, no model is asked and no project file is written.
#[test]
fn opening_and_reading_leave_the_kept_round_unchanged_and_ask_nothing() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    brief_closed_at_the_endpoint(root.path(), home.path());
    let before = kept_round(home.path(), root.path()).expect("kept");
    let files = tree(root.path());
    let (mut again, seen, _) = reopen(root.path(), home.path());
    let meaning = facts(again.turn("/meaning"));
    assert!(meaning.contains("not continued yet"), "{meaning}");
    assert!(meaning.contains("settled model → mock/echo"), "{meaning}");
    assert!(!meaning.contains("nothing to read"), "R4 73: {meaning}");
    let why = facts(again.turn("/why"));
    assert!(
        why.contains("has not continued") && why.contains("why it was asked"),
        "{why}"
    );
    assert!(why.contains("/restore continues it"), "{why}");
    let TurnOutcome::Facts(status) = again.turn("/status") else {
        panic!("status is a fact");
    };
    assert!(status.contains("kept round (not continued)"), "{status}");
    let bogus = again.turn("/bogus");
    assert!(
        !matches!(
            bogus,
            TurnOutcome::Question { .. } | TurnOutcome::Proposal { .. }
        ),
        "{bogus:?}"
    );
    assert!(
        seen.lock().expect("record").is_empty(),
        "no model read any of it"
    );
    // An unknown utterance is the conversation's (a no-intelligence refusal here): it binds
    // nothing to the kept question and leaves the kept round as it was.
    let unknown = again.turn("hmm, not sure");
    assert!(
        !matches!(
            unknown,
            TurnOutcome::Question { .. } | TurnOutcome::Proposal { .. }
        ),
        "{unknown:?}"
    );
    assert!(
        again.pending_question().is_none(),
        "nothing bound to the kept question"
    );
    assert_eq!(
        kept_round(home.path(), root.path()),
        Some(before),
        "byte-identical"
    );
    assert_eq!(tree(root.path()), files, "no project file written");
    // The kept round still continues after all of it.
    assert_eq!(question(again.turn("/restore")).0, "const.publish_endpoint");
}

/// R3 · I3 · a kept round survives repeated restarts byte for byte, and new work replaces it.
#[test]
fn a_round_survives_repeated_restarts_and_new_work_replaces_it() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    brief_closed_at_the_endpoint(root.path(), home.path());
    let first = kept_round(home.path(), root.path()).expect("kept");
    for _ in 0..2 {
        let (again, _, notice) = reopen(root.path(), home.path());
        assert!(notice.contains("restored round"), "{notice}");
        drop(again);
        assert_eq!(kept_round(home.path(), root.path()).as_ref(), Some(&first));
    }
    let (mut again, _, _) = reopen(root.path(), home.path());
    question(again.turn(DRAFT));
    drop(again);
    let replaced = kept_round(home.path(), root.path()).expect("the new round is kept");
    assert_eq!(
        replaced["request"]["text"], DRAFT,
        "new work replaces the kept round"
    );
}

/// I4 · a round continued and then cancelled is gone from the record and the next open.
#[test]
fn cancel_after_restore_drops_the_round() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    brief_closed_at_the_endpoint(root.path(), home.path());
    let (mut again, _, _) = reopen(root.path(), home.path());
    question(again.turn("/restore"));
    let dropped = facts(again.turn("cancel"));
    assert!(dropped.contains("discarded"), "{dropped}");
    drop(again);
    assert_eq!(kept_round(home.path(), root.path()), None);
    let (_, _, notice) = reopen(root.path(), home.path());
    assert!(!notice.contains("restored round"), "{notice}");
}

/// M1 · M2 · INV-MONEY-GATE · nothing admitted is restored: the money gate reads the request
/// again at `/restore`, and the continued round carries this session's admission; under a
/// restored exposure with no amount the gate refuses and nothing compiles.
#[test]
fn money_is_admitted_again_and_never_restored() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let request = format!("{DRAFT}. Budget: $0.");
    let (mut first, _, _) = reopen(root.path(), home.path());
    question(first.turn(&request));
    let admitted = first.money.admitted.clone();
    assert!(!admitted.is_empty(), "the gate admitted the directive");
    drop(first);
    let (mut again, _, _) = reopen(root.path(), home.path());
    assert!(again.money.admitted.is_empty(), "nothing admitted at open");
    assert_eq!(question(again.turn("/restore")).0, "model");
    assert_eq!(
        again.money.admitted, admitted,
        "the same request, admitted afresh"
    );
    assert_eq!(
        again.authoring.as_ref().map(|r| r.money.clone()),
        Some(admitted),
        "the continued round carries this session's admission"
    );

    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut first, _, _) = reopen(root.path(), home.path());
    question(first.turn(DRAFT));
    drop(first);
    let (mut again, _, _) = reopen(root.path(), home.path());
    again.money.reconfirm = true;
    let refusal = refused(again.turn("/restore"));
    assert_eq!(refusal.class, RefusalClass::NotAllowed, "{}", refusal.text);
    assert!(
        again.authoring.is_none() && again.last_outcome.is_none(),
        "no compile under a refusal"
    );
    drop(again);
    assert!(
        kept_round(home.path(), root.path()).is_some(),
        "the round stays kept"
    );
}

/// M5 · a kept request whose money directive the gate cannot read is refused before any
/// compile; the round stays kept.
#[test]
fn a_malformed_money_request_is_refused_by_the_gate_before_any_compile() {
    let root = project();
    let (mut session, _) = open(root.path());
    let raw = crafted(&format!("{DRAFT}. Budget: $abc."), "chain")
        .finish()
        .expect("kept");
    keep(&mut session, raw);
    let outcome = session.turn("/restore");
    assert!(matches!(outcome, TurnOutcome::Refusal(_)), "{outcome:?}");
    assert!(
        session.authoring.is_none() && session.last_outcome.is_none(),
        "no compile"
    );
    assert!(session.restored_round.is_some(), "the round stays kept");
}

/// K4 · INV-SECRETS · a secret in the request never lands in the history; a kept request the
/// redactor changed is named and never continued.
#[test]
fn a_redacted_request_is_named_and_never_continued() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut first, _, _) = reopen(root.path(), home.path());
    let _ = first.turn(&format!("{DRAFT} and include the key {SENTINEL}"));
    drop(first);
    assert!(
        !journal(home.path(), root.path()).contains(SENTINEL),
        "the sentinel never lands in history"
    );
    let (mut session, _) = open(root.path());
    let raw = crafted(&format!("{DRAFT} with the key {SENTINEL}"), "chain")
        .finish()
        .expect("kept");
    assert!(!raw.to_string().contains(SENTINEL));
    keep(&mut session, raw);
    let line = session.round_line().expect("named");
    assert!(
        line.contains("cannot be continued") && line.contains("redacted"),
        "{line}"
    );
    assert!(!session.slash_commands().contains(&"/restore"));
    let refusal = refused(session.turn("/restore"));
    assert!(refusal.text.contains("redacted"), "{}", refusal.text);
    assert!(session.authoring.is_none() && session.last_outcome.is_none());
}

/// K2 · K7 · another schema, or a field this engine does not know (a crafted monetary span or
/// consent), is kept byte for byte and never continued.
#[test]
fn an_unreadable_round_is_kept_byte_for_byte_and_never_continued() {
    let root = project();
    let mut crafted_money = crafted(DRAFT, "chain").finish().expect("kept");
    crafted_money["money"] = json!([[0, 5]]);
    let mut crafted_consent = crafted(DRAFT, "chain").finish().expect("kept");
    crafted_consent["consent"] = json!(true);
    for raw in [json!({"schema": 2}), crafted_money, crafted_consent] {
        let (mut session, seen) = open(root.path());
        keep(&mut session, raw.clone());
        let line = session.round_line().expect("named");
        assert!(line.contains("cannot be read here"), "{line}");
        assert_eq!(session.round_to_keep(), Some(raw), "re-saved unchanged");
        let refusal = refused(session.turn("/restore"));
        assert!(
            refusal.text.contains("cannot be read by this engine"),
            "{}",
            refusal.text
        );
        assert!(session.money.admitted.is_empty() && session.authoring.is_none());
        assert!(seen.lock().expect("record").is_empty());
    }
}

/// V6 · V7 · a saved workflow's revision whose base moved or vanished is held: zero compile,
/// the money gate untouched, the change and its answers kept as evidence.
#[test]
fn a_revision_whose_base_moved_or_vanished_is_held_with_zero_calls() {
    let root = project();
    let base = "nika: kept\ntasks:\n  say:\n    invoke:\n      tool: nika:log\n      args: {message: hi}\n";
    std::fs::write(root.path().join("kept.nika"), base).expect("base");
    let raw = crafted("add a step that logs bye", "chain")
        .edit(Some("kept.nika"), base, "add a step that logs bye", None)
        .finish()
        .expect("kept");
    for (label, moved) in [
        ("changed since", Some("nika: moved\n")),
        ("no longer exists", None),
    ] {
        match moved {
            Some(text) => std::fs::write(root.path().join("kept.nika"), text).expect("moved"),
            None => std::fs::remove_file(root.path().join("kept.nika")).expect("gone"),
        }
        let (mut session, _) = open(root.path());
        keep(&mut session, raw.clone());
        let refusal = refused(session.turn("/restore"));
        assert!(refusal.text.contains(label), "{label}: {}", refusal.text);
        assert!(
            refusal.text.contains("kept as evidence"),
            "{}",
            refusal.text
        );
        assert!(
            refusal.text.contains("nothing was compiled"),
            "{}",
            refusal.text
        );
        assert!(session.last_outcome.is_none() && session.authoring.is_none());
        assert!(session.money.admitted.is_empty() && session.money.current.is_none());
        assert!(session.restored_round.is_some(), "the round stays kept");
        std::fs::write(root.path().join("kept.nika"), base).expect("base again");
    }
}

/// The revision unit, crafted: a kept round that revises a proposal no kept draft can propose
/// again is refused, both kept; nothing waits and nothing compiled.
#[test]
fn a_revision_of_a_proposal_that_cannot_be_proposed_again_is_refused() {
    let root = project();
    let (mut session, _) = open(root.path());
    let raw = crafted("also post it", "chain")
        .edit(
            None,
            "nika: base\n",
            "also post it",
            Some("the first request"),
        )
        .revises(Some("0bf7aa9d6449a0e5"))
        .finish()
        .expect("kept");
    keep(&mut session, raw);
    let refusal = refused(session.turn("/restore"));
    assert!(
        refusal.text.contains("can no longer be proposed"),
        "{}",
        refusal.text
    );
    assert!(session.pending_proposal().is_none() && session.revising.is_none());
    assert!(session.last_outcome.is_none() && session.restored_round.is_some());
}

/// History disabled: nothing is kept, the next session restores nothing and pretends nothing.
#[test]
fn without_history_nothing_is_kept_and_nothing_is_pretended() {
    let root = project();
    let (mut first, _) = open(root.path());
    question(first.turn(BRIEF));
    assert_eq!(
        first.round_to_keep().map(|r| r["schema"].clone()),
        Some(json!(1))
    );
    drop(first);
    let (mut again, _) = open(root.path());
    assert_eq!(again.restore_state(), None, "no record was written");
    assert!(again.round_line().is_none());
    let refusal = refused(again.turn("/restore"));
    assert!(
        refusal.text.contains("a question waiting for your answer"),
        "{}",
        refusal.text
    );
    assert!(again.pending_question().is_none());
}

/// C10 · a round kept after a complete replacement request keeps only the replacement: no answer
/// the earlier intent was given, no plan it settled; the kept round stays continuable and its
/// continuation reads the replacement.
#[test]
fn a_round_kept_after_a_replacement_keeps_none_of_the_earlier_intents_answers() {
    let root = project();
    let (mut s, _) = open(root.path());
    let asked = crate::authoring::compile_deterministic(&CompileRequest::create("bounded-batch"))
        .expect("compiles")
        .questions;
    let mut clarify = asked[0].clone();
    clarify.key = "intent.clarification".to_owned();
    clarify.answer_type = nika_onboard::compile::QuestionType::Text;
    let mut round =
        crate::authoring::AuthoringRound::new("keep the rows of ./data/input.csv over 250");
    round
        .answers
        .insert("const.rule_field_1".to_owned(), "\"amount\"".to_owned());
    round.continuation = Some(json!({"strategy": "hot"}));
    round.questions = vec![clarify, asked[0].clone()];
    let replacement = "read ./other.csv and keep the rows whose price is over 3";
    round.answer_current(replacement);
    s.authoring = Some(round);
    let kept = s.round_to_keep().expect("kept");
    let reading = nika_onboard::compile::round::RoundReading::from_raw(kept);
    let record = reading.record().expect("read");
    // C11: the replacement is the kept request itself, never an answer riding the old text.
    assert!(
        record.answer_map().is_empty(),
        "no earlier answer is kept: {:?}",
        record.answer_map()
    );
    assert!(record.continuation.is_none(), "no earlier plan is kept");
    assert_eq!(record.continuable(), Ok(()));
    assert!(
        record.summary().contains(replacement),
        "{}",
        record.summary()
    );
}

/// The saved workflow a revision revises, unchanged since its round was kept.
const KEPT_BASE: &str =
    "nika: kept\ntasks:\n  say:\n    invoke:\n      tool: nika:log\n      args: {message: hi}\n";

/// A session over a project whose saved `kept.nika` still holds, keeping that workflow's revision
/// as the live revision keeps one: the request is the composed goal « original — change », the
/// EDIT holds the change, and the original request stays beside it.
fn kept_revision(original: &str, change: &str) -> (tempfile::TempDir, SessionRuntime) {
    let root = project();
    std::fs::write(root.path().join("kept.nika"), KEPT_BASE).expect("base");
    let raw = crafted(&format!("{original} — {change}"), "chain")
        .edit(Some("kept.nika"), KEPT_BASE, change, Some(original))
        .finish()
        .expect("kept");
    let (mut session, _) = open(root.path());
    keep(&mut session, raw);
    (root, session)
}

/// The ceiling the money gate admitted for the continued work.
fn ceiling(session: &SessionRuntime) -> Option<f64> {
    session.money.current.as_ref().and_then(|d| d.effective_usd)
}

/// M-R1 · a saved workflow's revision is restored under the live revision's money law (B15,
/// `change_money`): the gate reads the kept change again, never the composed goal it was kept
/// with. The original request's 2 USD is never this revision's ceiling (no inherited cap), and a
/// change stating its own budget carries exactly that change's directive, the span an EDIT's
/// reader checks against the change. Nothing is written before a new yes (no inherited consent).
#[test]
fn a_restored_revision_admits_its_change_never_the_goal_it_was_kept_with() {
    let stated = "add a step that logs bye. Budget: $3.";
    for (change, cap) in [("add a step that logs bye", None), (stated, Some(3.0))] {
        let (root, mut session) = kept_revision("Log hi to the console. Budget: $2.", change);
        let before = tree(root.path());
        let outcome = session.turn("/restore");
        assert!(
            session.restored_round.is_none() && session.last_outcome.is_some(),
            "{change}: continued and compiled: {outcome:?}"
        );
        let spans = change_money(change, cap.is_some());
        assert_eq!(
            session.money.admitted, spans,
            "{change}: the change's own directives"
        );
        if let Some(round) = &session.authoring {
            assert_eq!(round.money, spans, "{change}: the continued round's spans");
        }
        assert_ne!(
            ceiling(&session),
            Some(2.0),
            "{change}: the original's 2 USD rode along"
        );
        if cap.is_some() {
            assert_eq!(ceiling(&session), cap, "{change}");
        }
        assert_eq!(
            tree(root.path()),
            before,
            "{change}: nothing written before a new yes"
        );
    }
}

/// M-R2 · a revision whose change states money the gate cannot read is refused before any
/// compile (M5), and the round stays kept. The original request's unreadable money is not this
/// revision's: the change is read, and the round continues.
#[test]
fn a_restored_revision_is_judged_by_its_change_money_never_the_originals() {
    let (_root, mut session) = kept_revision(
        "Log hi to the console.",
        "add a step that logs bye. Budget: $abc.",
    );
    let outcome = session.turn("/restore");
    assert!(matches!(outcome, TurnOutcome::Refusal(_)), "{outcome:?}");
    assert!(
        session.authoring.is_none() && session.last_outcome.is_none(),
        "no compile"
    );
    assert!(session.restored_round.is_some(), "the round stays kept");
    let (_root, mut session) = kept_revision(
        "Log hi to the console. Budget: $abc.",
        "add a step that logs bye",
    );
    let outcome = session.turn("/restore");
    assert!(
        session.restored_round.is_none() && session.last_outcome.is_some(),
        "the original's money is not the change's: {outcome:?}"
    );
    assert!(
        session.money.admitted.is_empty(),
        "{:?}",
        session.money.admitted
    );
}

/// M-R3 · a request (no revision) is restored as before: the gate reads the kept request itself,
/// and its own budget is the ceiling (M1).
#[test]
fn a_restored_request_still_admits_its_own_budget() {
    let root = project();
    let request = format!("{DRAFT}. Budget: $2.");
    let raw = crafted(&request, "chain").finish().expect("kept");
    let (mut session, _) = open(root.path());
    keep(&mut session, raw);
    let outcome = session.turn("/restore");
    assert!(session.restored_round.is_none(), "continued: {outcome:?}");
    assert_eq!(session.money.admitted, change_money(&request, true));
    assert_eq!(ceiling(&session), Some(2.0));
}

/// M-R4 · the law itself (`rebuilt`): a kept revision carries its change's own directives
/// whenever the gate admitted money for it, whatever spans the gate reported, and none when it
/// admitted none; a request keeps the spans admitted for its own bytes. `[26..36]` is the exact
/// span the primary's EDIT reader is proven to accept for this change.
#[test]
fn a_rebuilt_revision_carries_its_changes_own_directives() {
    let stated = "add a step that logs bye. Budget: $3.";
    let (_root, session) = kept_revision("Log hi to the console. Budget: $2.", stated);
    let kept = session.restored_round.as_ref().expect("kept");
    let record = kept.reading.record().expect("readable");
    assert_eq!(change_money(stated, true), vec![26..36]);
    // Whatever spans the gate reported: here one that is no directive of the change.
    let reported: Vec<std::ops::Range<usize>> = std::iter::once(0..1).collect();
    assert_eq!(super::rebuilt(record, &reported).money, vec![26..36]);
    assert!(
        super::rebuilt(record, &[]).money.is_empty(),
        "no admission, no span"
    );
    let request = format!("{DRAFT}. Budget: $2.");
    let raw = crafted(&request, "chain").finish().expect("kept");
    let kept = KeptRound::new(raw, Vec::new());
    let record = kept.reading.record().expect("readable");
    let admitted = change_money(&request, true);
    assert_eq!(
        super::rebuilt(record, &admitted).money,
        admitted,
        "a request keeps its own spans"
    );
}
