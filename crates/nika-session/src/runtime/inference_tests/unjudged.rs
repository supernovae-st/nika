// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real Session → compiler → loopback: no outcome, plan or candidate injected into Session.
use super::*;
struct Work;
impl TurnClassifier for Work {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        TurnDecision::new(TurnAct::NewWork, crate::turn::RoutingMethod::Model)
    }
}
fn live(root: &Path, home: &Path) -> SessionRuntime {
    let mut s = open(root);
    s.enable_continuous_preparation();
    s.with_classifier(Box::new(Work));
    s.enable_history(home).unwrap();
    s
}
fn first_script() -> Vec<(u16, Value)> {
    let mut script: Vec<_> = semantic_create()
        .iter()
        .map(|s| (200, response(s)))
        .collect();
    script.push((
        400,
        json!({"error":{"message":"synthetic judge unavailable"}}),
    ));
    script
}
fn kept(s: &SessionRuntime) -> Value {
    assert!(s.pending_proposal().is_none());
    assert!(s.pending_question().is_none(), "no invented clarification");
    assert!(s.status_line().contains("Not ready"));
    s.authoring.as_ref().unwrap().continuation.clone().unwrap()
}
/// The digest of the bytes the first round's judge was asked (its call failed): the ones its
/// kept record replays.
fn first_judged(peer: &Peer) -> String {
    judged_sha(&peer.bodies()[CREATE_CALLS - 1]).expect("the first round asked its judge")
}
#[test]
fn same_intent_retries_only_the_judge_before_any_save_or_run() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = live(dir.path(), home.path());
    assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
    let plan = kept(&s);
    assert_eq!(peer.bodies().len(), CREATE_CALLS);
    for line in ["save", "Save", "SAVE", "yes", "run"] {
        assert!(matches!(s.turn(line), TurnOutcome::Facts(_)));
        assert!(
            kept(&s) == plan,
            "Save and Run must preserve the complete unjudged plan"
        );
    }
    assert_eq!(peer.bodies().len(), CREATE_CALLS);
    let judge = Peer::start(vec![(200, response(JUDGE_APPROVES))]);
    let _again = test_transport::install(&judge.url);
    assert!(matches!(s.turn("RePrEnD"), TurnOutcome::Proposal { .. }));
    assert_eq!(judge.bodies().len(), 1, "no regenerated author calls");
    assert!(judge.bodies()[0].to_string().contains("unfaithful"));
    let source = s.candidate().unwrap().set.changes[0].content();
    assert_eq!(
        first_judged(&peer),
        nika_event::source_id::sha256_hex(source.as_bytes())
    );
    assert!(!dir.path().join("compiled-workflow.nika").exists());
    assert!(!dir.path().join("sortie.txt").exists());
}
#[test]
fn close_restore_and_retry_preserve_exact_candidate_and_obligations() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let plan = {
        let mut s = live(dir.path(), home.path());
        assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
        kept(&s)
    };
    let mut s = live(dir.path(), home.path());
    assert!(s.pending_proposal().is_none());
    assert!(matches!(s.restore_round(), TurnOutcome::Facts(_)));
    assert!(
        kept(&s) == plan,
        "the exact record and obligations must survive close"
    );
    assert_eq!(
        peer.bodies().len(),
        CREATE_CALLS,
        "restore never calls a model"
    );
    let judge = Peer::start(vec![(200, response(JUDGE_APPROVES))]);
    let _again = test_transport::install(&judge.url);
    assert!(matches!(s.turn("Réessaie"), TurnOutcome::Proposal { .. }));
    assert_eq!(judge.bodies().len(), 1);
    let source = s.candidate().unwrap().set.changes[0].content();
    assert_eq!(
        first_judged(&peer),
        nika_event::source_id::sha256_hex(source.as_bytes())
    );
}
#[test]
fn a_correction_discards_the_old_candidate_and_authors_the_changed_intent() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = live(dir.path(), home.path());
    assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
    let _old = kept(&s);
    let failed = Peer::start(vec![(
        400,
        json!({"error":{"message":"new author unavailable"}}),
    )]);
    let _new = test_transport::install(&failed.url);
    let out = s.turn("Utilise revised.txt au lieu de sortie.txt.");
    assert!(!matches!(
        out,
        TurnOutcome::Proposal { .. } | TurnOutcome::RunRequested { .. }
    ));
    assert!(!s.judgment_waits());
    assert!(s.pending_proposal().is_none());
    let bodies = failed.bodies();
    assert!(!bodies.is_empty());
    assert!(bodies[0].to_string().contains("revised.txt"));
    assert!(
        !bodies[0].to_string().contains("unfaithful"),
        "fresh author, not old judge"
    );
}
/// [`WORK`]'s one part, as the verifier asks it alone: the request cut where its final period
/// ends the phrase. It restricts nothing, and a request of one part is offered neither
/// `no_operation` nor `superseded` (only a later part supersedes an earlier one).
const PART: &str =
    "Je veux que sortie.txt contienne exactement les octets présents dans entree.txt";
/// What one request to the seat asks when it is a closed choice: the state it shows and the
/// option keys its schema admits, in offered order (`none` last). `None` for an author call.
pub(super) fn asked(body: &Value) -> Option<(Value, Vec<String>)> {
    let said = (body["messages"].as_array()?.iter())
        .filter(|message| message["role"] == "user")
        .find_map(|message| message["content"].as_str()?.strip_prefix("STATE:\n"))?;
    let (state, _) = said.split_once("\n\nOPTIONS:\n")?;
    // The seat's JSON mode carries the schema on the last line of the user turn.
    let schema: Value = serde_json::from_str(said.lines().last()?).ok()?;
    let keys = (schema["properties"]["choice"]["enum"].as_array()?.iter())
        .filter_map(|key| key.as_str().map(str::to_owned))
        .collect();
    Some((serde_json::from_str(state).ok()?, keys))
}
/// The tasks of the replayed candidate, in document order, as a task question offers them.
const TASKS: [&str; 2] = ["task-read_source", "task-write_output"];
/// The replay's doubt of [`WORK`]'s kept candidate: the request's one part judged missing when
/// asked alone, the judge pointing to the task that does it differently.
fn located_defect() -> Vec<(u16, Value)> {
    vec![
        (200, response(r#"{"choice":"unfaithful"}"#)),
        // The part asked alone, judged missing, then why: the task that does it differently,
        // the located defect a new authoring round starts from.
        (200, response(r#"{"choice":"missing"}"#)),
        (200, response(r#"{"choice":"task-write_output"}"#)),
    ]
}
/// [`semantic_create`] with its tasks named otherwise: the same program in other bytes.
fn renamed_create() -> [String; 1] {
    semantic_create().map(|text| {
        text.replace("read_source", "read_entree")
            .replace("write_output", "write_sortie")
    })
}
/// A replayed candidate its judge doubts, the request's one part judged missing when asked
/// alone and the judge pointing to the task that does it differently (an answer round replays
/// and judges, never repairs), is written again under the answers already given: the authoring
/// round runs its own judgment over the bytes it writes (other bytes here), and the human never
/// types the request again. Observed in the TUI on 2026-10-06 (certificates, DeepSeek): « built
/// but not proposed ».
#[test]
fn a_replayed_candidate_the_judge_doubts_is_written_again_not_left_held() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = live(dir.path(), home.path());
    assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
    kept(&s);
    let held_sha = first_judged(&peer);
    let mut script = located_defect();
    script.extend(renamed_create().iter().map(|t| (200, response(t))));
    script.push((200, response(JUDGE_APPROVES)));
    let again = Peer::start(script);
    let _again = test_transport::install(&again.url);
    let out = s.turn("RePrEnD");
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let bodies = again.bodies();
    assert_eq!(
        bodies.len(),
        3 + CREATE_CALLS,
        "the doubt, its part and the task it points to, then one authoring round"
    );
    assert!(bodies[0].to_string().contains("unfaithful"));
    let (whole, verdicts) = asked(&bodies[0]).expect("the whole request judged");
    assert_eq!(whole["request"], WORK);
    assert_eq!(verdicts, ["faithful", "unfaithful", "none"]);
    let (part, choices) = asked(&bodies[1]).expect("its one part asked alone");
    assert_eq!(part["clause"], json!({"text": PART}));
    assert_eq!(choices, ["carried", "missing", "none"]);
    let (pointed, why) = asked(&bodies[2]).expect("the task the missing part points to");
    assert_eq!(pointed["clause"], json!({"text": PART}));
    // Both tasks, the omitted operation, then the contextual remedies the default round's lent
    // release offers: each of the bundled r2 release's 37 admitted blocks (`component-<k>`, in
    // catalogue order, none held by these bytes) closed by `no_fit`; then no task, and none.
    let mut offered: Vec<String> = [TASKS[0], TASKS[1], "omitted"].map(str::to_owned).into();
    offered.extend((0..37).map(|k| format!("component-{k}")));
    offered.extend(["no_fit", "no_task", "none"].map(str::to_owned));
    assert_eq!(why, offered);
    assert!(
        asked(&bodies[3]).is_none(),
        "an author call follows the located defect"
    );
    assert!(
        !bodies[3].to_string().contains("unfaithful"),
        "a fresh author call follows the doubt"
    );
    // The written-again bytes are judged on their own: another candidate, never the held one.
    let judged: Vec<Option<String>> = bodies.iter().map(judged_sha).collect();
    let held = Some(held_sha.clone());
    assert_eq!(judged[..3], [held.clone(), held.clone(), held]);
    let authored = 3 + CREATE_CALLS - 1;
    assert!(
        judged[3..authored].iter().all(Option::is_none),
        "authoring calls"
    );
    let fresh = judged[authored]
        .clone()
        .expect("the written-again candidate judged");
    assert_ne!(fresh, held_sha, "other bytes");
    let outcome = s.last_outcome.as_ref().expect("the proposed outcome");
    let attempts =
        &outcome.provenance.decision.as_ref().expect("decision")["semantic_verification"];
    assert_eq!(attempts.as_array().map(Vec::len), Some(1), "{attempts:#}");
    assert_eq!(attempts[0]["candidate_sha256"], json!(fresh));
    assert_eq!(attempts[0]["carried"], false);
    assert_eq!(attempts[0]["settled_by"], "verify-request");
    assert!(
        !dir.path().join("sortie.txt").exists(),
        "proposed, never run"
    );
}
/// The steps of a decision's route that start with `prefix`, in order.
fn steps<'a>(decision: &'a Value, prefix: &str) -> Vec<&'a str> {
    (decision["route"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .filter(|step| step.starts_with(prefix))
        .collect()
}
/// The written-again round after a located defect authors the held bytes again: the verdict that
/// rejected them in the replay is carried into it (R6), so their judge is not asked again. No
/// judge question follows the replay's three; the written-again attempt repeats that verdict
/// with no call (`carried`), and its located defect asks the document's repair, an authoring
/// call. The repair writes the same bytes again: a repeat within the compile, no call and no
/// progress, so the door stops and holds them: shown, never proposed, nothing written, and the
/// human is told which part stays missing.
#[test]
fn a_write_again_that_reproduces_the_held_bytes_never_asks_their_judge() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = live(dir.path(), home.path());
    assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
    kept(&s);
    let held_sha = first_judged(&peer);
    let mut script = located_defect();
    script.extend(semantic_create().iter().map(|t| (200, response(t))));
    // The repair from the carried defect: the same document, the same bytes.
    script.extend(semantic_create().iter().map(|t| (200, response(t))));
    let again = Peer::start(script);
    let _again = test_transport::install(&again.url);
    let out = s.turn("RePrEnD");
    let bodies = again.bodies();
    let judged: Vec<Option<String>> = bodies.iter().map(judged_sha).collect();
    let mut expected = vec![Some(held_sha.clone()); 3];
    expected.resize(5, None);
    let calls = "the replay's three questions, then authoring calls only";
    assert_eq!(judged, expected, "{calls}: {out:?}");
    let TurnOutcome::Facts(words) = &out else {
        panic!("nothing is proposed: {out:?}");
    };
    assert_eq!(words, NO_PROGRESS);
    let outcome = s.last_outcome.as_ref().expect("the written-again outcome");
    assert!(
        outcome.candidate.is_some(),
        "held as the preview, never offered"
    );
    let decision = outcome.provenance.decision.as_ref().expect("decision");
    let attempts = decision["semantic_verification"]
        .as_array()
        .expect("attempts");
    assert_eq!(attempts.len(), 2, "{attempts:#?}");
    // The reopened sketch's same bytes repeat the carried verdict within the compile.
    assert_eq!(attempts[1]["same_bytes_as"], json!(0), "{attempts:#?}");
    assert_eq!(attempts[1]["questions"], json!([]), "{attempts:#?}");
    let carried = &attempts[0];
    assert_eq!(carried["candidate_sha256"], json!(held_sha));
    assert_eq!(
        (
            &carried["carried"],
            &carried["same_bytes_as"],
            &carried["questions"]
        ),
        (&json!(true), &Value::Null, &json!([]))
    );
    assert_eq!(carried["defects"], json!([PART]));
    assert_eq!(
        carried["notes"],
        json!([{"defect": PART, "note": "the judge points to the task write_output"}])
    );
    // The door's own steps, in order: the document accepted, repaired from the carried defect
    // and accepted again, then stopped on no progress.
    assert_eq!(
        steps(decision, "native:"),
        [
            "native: document",
            "native: accepted",
            "native: accepted",
            "native: no progress"
        ]
    );
    // The verifier's steps stay on the route: the carried verdict, the repair it opened, its
    // repeat on the same bytes, then the held exit.
    assert_eq!(
        steps(decision, "verify:"),
        [
            "verify: same bytes, rejected in an earlier round",
            "verify: repair 1",
            "verify: same bytes, earlier verdict stands",
            "verify: not ready",
            "verify: doubted, not replayable"
        ]
    );
    assert!(s.pending_proposal().is_none());
    assert!(!dir.path().join("sortie.txt").exists(), "never run");
    assert!(!dir.path().join("compiled-workflow.nika").exists());
}
/// What the session says when the written-again bytes are the held ones: the verdict carried
/// from the replay stands, the one repair from its defect wrote them again, and they stay held.
const NO_PROGRESS: &str = "The workflow is built but not proposed: the verifier found a part missing that the repairs did not settle: « Je veux que sortie.txt contienne exactement les octets présents dans entree.txt »; nothing was written.\n  describe a correction, or `/intelligence` for another authoring model (it also judges unless a decision model is set) · `/meaning` shows what was understood";
/// What the session says of a candidate its verifier judged and rejected with no defect located
/// (`nika_onboard`'s held words): built, shown, never proposed, nothing written; a correction or
/// another authoring model, which also judges unless a decision model is set, can decide it.
pub(super) const DOUBTED: &str = "The workflow is built but not proposed: the verifier did not accept it and located no defect a repair could start from; nothing was written.\n  describe a correction, or `/intelligence` for another authoring model (it also judges unless a decision model is set) · `/meaning` shows what was understood";
/// The compiler's `verify_held` finding on a candidate its verifier rejected with no defect
/// located.
pub(super) const HELD: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it.";
/// Why no trial decided the held bytes, as the held text names it (A4): the room refused them.
pub(super) const UNTRIED: &str = " No trial of these bytes ran: the rehearsal room refused them before any attempt (./entree.txt is read but not observed: the room holds the observed inputs only).";
/// The applied `verify_held` findings of an outcome, in order.
pub(super) fn held_findings(out: &nika_onboard::compile::CompileOutcome) -> Vec<&str> {
    (out.diagnostics.iter())
        .filter(|d| d.target == "verify_held")
        .filter(|d| d.kind == nika_onboard::compile::DiagnosticKind::Applied)
        .map(|d| d.message.as_str())
        .collect()
}
/// A replayed candidate its judge doubts while its one part, asked alone, is carried and no task
/// does what the request does not ask: the verdict and its localization disagree, and agreeing
/// answers of the same judge decide nothing (R6), while an answer round has no trial run that
/// could. No defect was located, so nothing is written again from one; the candidate is held,
/// never proposed, and no record of its bytes is kept, so no later line asks the same judge
/// again on them. Nothing is written.
#[test]
fn a_replayed_doubt_no_part_settles_is_never_written_again() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = live(dir.path(), home.path());
    assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
    kept(&s);
    let held_sha = first_judged(&peer);
    // A later author request would be answered with the last reply, and counted below.
    let again = Peer::start(vec![
        (200, response(r#"{"choice":"unfaithful"}"#)),
        (200, response(r#"{"choice":"carried"}"#)),
        (200, response(r#"{"choice":"only_requested"}"#)),
    ]);
    let _again = test_transport::install(&again.url);
    let out = s.turn("RePrEnD");
    let TurnOutcome::Facts(words) = &out else {
        panic!("a doubted candidate is never proposed nor run: {out:?}");
    };
    let bodies = again.bodies();
    assert_eq!(
        bodies.len(),
        3,
        "the doubt, its one part, the extra question: no authoring round"
    );
    let (_, verdicts) = asked(&bodies[0]).expect("the whole request judged");
    assert_eq!(verdicts, ["faithful", "unfaithful", "none"]);
    let (part, choices) = asked(&bodies[1]).expect("its one part asked alone");
    assert_eq!(part["clause"], json!({"text": PART}));
    assert_eq!(choices, ["carried", "missing", "none"]);
    let (_, extra) = asked(&bodies[2]).expect("the extra-operation question");
    assert_eq!(extra, ["only_requested", TASKS[0], TASKS[1], "none"]);
    assert!(s.pending_proposal().is_none());
    assert!(!dir.path().join("compiled-workflow.nika").exists());
    assert!(!dir.path().join("sortie.txt").exists(), "never run");
    // Judged and doubted, never « not judged »: the human is told what decides it.
    assert_eq!(words, DOUBTED);
    let held = s.last_outcome.as_ref().expect("the held outcome");
    assert_eq!(
        nika_onboard::compile::reading::held_words(held, true).as_deref(),
        Some(DOUBTED)
    );
    assert_eq!(
        (held.candidate.as_deref()).map(|c| nika_event::source_id::sha256_hex(c.as_bytes())),
        Some(held_sha),
        "the same bytes, shown as the preview"
    );
    let attempts = &held.provenance.decision.as_ref().expect("decision")["semantic_verification"];
    assert_eq!(attempts.as_array().map(Vec::len), Some(1), "{attempts:#}");
    let attempt = &attempts[0];
    assert_eq!(attempt["doubt"], json!(["unfaithful"]));
    assert_eq!(attempt["contested"], json!([WORK]));
    assert_eq!(
        attempt["unsettled"],
        json!(["no trial run of these exact bytes exists in this compile"])
    );
    assert_eq!(attempt["defects"], json!([]));
    assert_eq!(
        (
            &attempt["declined"],
            &attempt["rejected"],
            &attempt["stopped"]
        ),
        (&json!(true), &json!(true), &json!(false))
    );
    let untried = format!("{HELD}{UNTRIED}");
    assert_eq!(held_findings(held), [untried.as_str()]);
    let route = &held.provenance.decision.as_ref().expect("decision")["route"];
    let last = route.as_array().and_then(|steps| steps.last());
    assert_eq!(last, Some(&json!("verify: doubted, not replayable")));
    // The doubted bytes' record is kept with the judge's rejection (A3): a later line that
    // replays them asks that judge nothing, and a kept round replays exactly that record.
    let declined = (held.provenance.plan.as_ref()).and_then(|plan| plan["declined"].as_array());
    assert!(
        declined.is_some_and(|d| !d.is_empty()),
        "{:?}",
        held.provenance.plan
    );
    assert!(!s.judgment_waits());
    assert!(
        (s.authoring.as_ref())
            .is_none_or(|r| r.continuation.as_ref() == held.provenance.plan.as_ref())
    );
    // `/meaning` says it was judged and not accepted, part by part, never « not judged yet ».
    assert_eq!(meaning(&mut s), HELD_MEANING);
}
/// The `/meaning` view of [`WORK`]'s held candidate: its one part asked alone and carried, the
/// whole request judged and not accepted.
pub(super) const HELD_MEANING: &str = "Meaning · your request as the verifier judged it\n  · « Je veux que sortie.txt contienne exactement les octets présents dans entree.txt »\n      carried\n  1 part(s) the verifier asked alone · the program was judged against the whole request: judged, not accepted; nothing was written\n  this lists the parts the verifier asked alone; a part it did not ask is not here — if something you asked is missing, say it again in its own words";
/// The `/meaning` view the session shows now, before its money lines.
pub(super) fn meaning(s: &mut SessionRuntime) -> String {
    let TurnOutcome::Aside(text) = s.turn("/meaning") else {
        panic!("`/meaning` is an aside");
    };
    text.split("\nmoney:").next().unwrap_or_default().to_owned()
}
/// The provider's unnamed default seat, and the stronger model a reading it cannot settle climbs
/// to once.
const FLASH: &str = "deepseek/deepseek-flash";
/// [`live`] under the provider's unnamed default: no model named, no account, so a reading the
/// seat cannot settle is read once more by the provider's stronger model.
fn unnamed(root: &Path, home: &Path) -> SessionRuntime {
    std::fs::write(root.join("entree.txt"), "A\n").expect("input");
    let selected = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        model: None,
        locus: DataLocus::Metered {
            provider: "deepseek".into(),
        },
        ready: true,
        why: None,
    };
    let reasoner = || ProviderReasoner {
        model: FLASH.into(),
        label: "DeepSeek".into(),
    };
    let mut s = SessionRuntime::open(root, selected, Box::new(reasoner()));
    s.factory = Some(Box::new(move |_| Box::new(reasoner())));
    s.set_authoring_context(crate::authoring::AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none(),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    ));
    s.enable_continuous_preparation();
    s.with_classifier(Box::new(Work));
    s.enable_history(home).unwrap();
    s
}
/// The sha256 of the candidate a closed choice judges, when the request is one.
fn judged_sha(body: &Value) -> Option<String> {
    let (state, _) = asked(body)?;
    let candidate = state["candidate_nika"].as_str()?;
    Some(nika_event::source_id::sha256_hex(candidate.as_bytes()))
}
/// A replayed candidate its verifier rejected with no defect located is held with that rejection
/// (A3), and the stronger seat's retry that follows decides the very same bytes with its own
/// judge (S1): no authoring call, the declining judge asked nothing more after its three
/// questions, and the held bytes, now accepted by another judge, proposed.
#[test]
fn the_stronger_retry_after_a_held_replay_judges_the_held_bytes_with_its_own_judge() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = unnamed(dir.path(), home.path());
    let stronger = crate::authoring::AuthoringSeat::Provider {
        model: "deepseek/deepseek-v4-pro".to_owned(),
    };
    assert_eq!(s.stronger_seat(), Some(stronger));
    assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
    kept(&s);
    let held_sha = first_judged(&peer);
    // The replay's verdict on the held bytes, then the stronger seat's own judge on those bytes.
    let script = vec![
        (200, response(r#"{"choice":"unfaithful"}"#)),
        (200, response(r#"{"choice":"carried"}"#)),
        (200, response(r#"{"choice":"only_requested"}"#)),
        (200, response(JUDGE_APPROVES)),
    ];
    let again = Peer::start(script);
    let _again = test_transport::install(&again.url);
    let out = s.turn("RePrEnD");
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let bodies = again.bodies();
    let models: Vec<&Value> = bodies.iter().map(|body| &body["model"]).collect();
    let (flash, pro) = (json!("deepseek-flash"), json!("deepseek-v4-pro"));
    assert_eq!(models, [&flash, &flash, &flash, &pro], "no authoring call");
    let judged: Vec<Option<String>> = bodies.iter().map(judged_sha).collect();
    let held = Some(held_sha.clone());
    assert_eq!(judged, [held.clone(), held.clone(), held.clone(), held]);
    let proposed = s.candidate().expect("proposed").set.changes[0]
        .content()
        .to_owned();
    assert_eq!(
        nika_event::source_id::sha256_hex(proposed.as_bytes()),
        held_sha,
        "the held bytes, decided by another judge"
    );
    assert!(
        !dir.path().join("sortie.txt").exists(),
        "proposed, never run"
    );
}
/// The verifier doubts the whole request, then its call over the request's one part is refused:
/// the localization stops there, nothing located a defect, and the candidate is held. The
/// refused call leaves the authoring provider's finding beside the held one, and the session
/// reads the outcome as held all the same: the held words, never the recovery card of a
/// provider failure, nothing proposed, nothing written, nothing waiting for a judge.
#[test]
fn a_judge_call_refused_while_locating_keeps_the_held_words() {
    let mut script: Vec<_> = semantic_create()
        .iter()
        .map(|t| (200, response(t)))
        .collect();
    script.push((200, response(r#"{"choice":"unfaithful"}"#)));
    script.push((400, json!({"error":{"message":"synthetic judge refused"}})));
    let peer = Peer::start(script);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = live(dir.path(), home.path());
    let out = s.turn(WORK);
    assert!(
        matches!(&out, TurnOutcome::Facts(words) if words == DOUBTED),
        "{out:?}"
    );
    assert_eq!(
        peer.bodies().len(),
        CREATE_CALLS + 1,
        "the request, then its part"
    );
    let held = s.last_outcome.as_ref().expect("the held outcome");
    let provider: Vec<&str> = (held.diagnostics.iter())
        .filter(|d| d.target == "authoring_provider")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        provider,
        ["provider API error (HTTP 400); usage and billing unknown"],
        "the refused call is named"
    );
    assert_eq!(held_findings(held), [HELD_STOPPED]);
    let attempts = &held.provenance.decision.as_ref().expect("decision")["semantic_verification"];
    assert_eq!(attempts.as_array().map(Vec::len), Some(1), "{attempts:#}");
    assert_eq!(
        (
            &attempts[0]["rejected"],
            &attempts[0]["stopped"],
            &attempts[0]["settled"]
        ),
        (&json!(true), &json!(true), &json!(false))
    );
    assert!(s.pending_proposal().is_none());
    assert!(!s.judgment_waits());
    assert!(!dir.path().join("sortie.txt").exists(), "never run");
}
/// The compiler's `verify_held` finding on a candidate its verifier rejected with no defect
/// located, the localization stopped at a call that got no answer.
const HELD_STOPPED: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it. Locating what it lacks stopped at a judge call that got no answer (refused by the call bound, or failed).";
