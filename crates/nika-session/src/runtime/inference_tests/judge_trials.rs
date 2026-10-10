// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Trial observations reach the configured verifier as ordinary preparation context.
//! The end-to-end compile uses a real sealed room and loopback provider.
use super::unjudged::{DOUBTED, HELD_MEANING, UNRESOLVED, UNTRIED, asked, held_findings, meaning};
use super::*;
use crate::authoring::{AuthoringContext, AuthoringRound, Reading};
use nika_cli_host::compile::config::AuthoringSettings;
use nika_event::source_id::sha256_hex;
/// What the room copies in: words no other request of the round carries.
const TRIAL_TEXT: &str = "TRIAL-ROW-7 a line only a trial run reads\n";
/// A request the sketch door authors whose source is stated as material to read and whose
/// destination follows a destination connector: the room copies the one in and reads the other
/// back.
const COPY: &str = "Read ./entree.txt and write its bytes unchanged to ./sortie.txt.";
/// Its one part, as each question asks it alone.
const PART: &str = "Read ./entree.txt and write its bytes unchanged to ./sortie.txt";
/// The author's calls before the judge's: the sketch, then its fills.
const AUTHOR_CALLS: usize = 2;
fn session(root: &Path) -> SessionRuntime {
    let mut s = open(root);
    std::fs::write(root.join("entree.txt"), TRIAL_TEXT).expect("input");
    let sketch = AuthoringSettings::none().with_strategy("sketch");
    let context = AuthoringContext::from_settings(&sketch, &AuthoringSettings::none());
    s.set_authoring_context(context);
    s.admit_money("budget 2 USD", false, false)
        .expect("admitted");
    s
}
/// The sketch door's answers for [`COPY`] (read the stated source, write its text to the stated
/// destination; the compiler writes the source), then a judge that doubts the whole request,
/// carries its one part (the read and the write are the request's own: the engine's facts leave
/// no extra question), and finds a trial run it is shown consistent.
fn doubted() -> Vec<(u16, Value)> {
    let sketch = json!({"name": "compiled-workflow", "tasks": [
        {"id": "read_source", "verb": "invoke", "tool": "nika:read", "purpose": "read",
         "reads": ["./entree.txt"]},
        {"id": "write_output", "verb": "invoke", "tool": "nika:write", "purpose": "write",
         "writes": ["./sortie.txt"], "with": [{"name": "content", "from": "read_source"}]},
    ], "outputs": [], "questions": [], "gaps": [], "notes": "copy the bytes"});
    let fills = json!({"fills": [], "notes": "the edge carries the bytes"});
    let mut script = vec![
        (200, response(&sketch.to_string())),
        (200, response(&fills.to_string())),
    ];
    for choice in ["unfaithful", "carried", "consistent"] {
        script.push((200, response(&json!({ "choice": choice }).to_string())));
    }
    script
}
/// The last verification attempt of the outcome the session keeps.
fn attempt(s: &SessionRuntime) -> Value {
    let out = s.last_outcome.as_ref().expect("the outcome");
    let attempts = &out.provenance.decision.as_ref().expect("decision")["semantic_verification"];
    assert_eq!(attempts.as_array().map(Vec::len), Some(1), "{attempts:#}");
    attempts[0].clone()
}
/// The judge's two questions before any run is consulted: the whole request and its one part
/// (the engine's facts settle the extra operation with no call).
fn assert_localized(judged: &[(Value, Vec<String>)]) {
    assert_eq!(judged[0].1, ["faithful", "unfaithful", "none"]);
    assert_eq!(judged[1].0["clause"], json!({ "text": PART }));
    assert_eq!(judged[1].1, ["carried", "missing", "none"]);
}
/// A trial observation is available without extra configuration. Its consistent verdict
/// resolves the doubt, while the original project remains untouched.
#[test]
fn a_doubted_trial_run_reaches_the_configured_verifier_by_default() {
    let peer = Peer::start(doubted());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = session(dir.path());
    let round = AuthoringRound::new(COPY);
    let seat = s.seat.clone();
    let out = s.compile_round(&round, &seat).expect("a seated compile");
    let said = s.settle(round, Reading::of(out));
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), AUTHOR_CALLS + 3);
    let judged: Vec<_> = bodies[AUTHOR_CALLS..]
        .iter()
        .map(|body| asked(body).expect("a closed choice"))
        .collect();
    assert_localized(&judged);
    assert!(
        judged[..2]
            .iter()
            .all(|(state, _)| state["observation"].is_null())
    );
    // The run's texts reach the verifier in the observed question alone: no authoring call and
    // no earlier question carries them.
    let before = &bodies[..AUTHOR_CALLS + 2];
    assert!(
        before
            .iter()
            .all(|b| !b.to_string().contains("TRIAL-ROW-7"))
    );
    assert!(bodies[AUTHOR_CALLS + 2].to_string().contains("TRIAL-ROW-7"));
    assert!(
        !dir.path().join("sortie.txt").exists(),
        "preparation never runs on the project"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("entree.txt")).unwrap(),
        TRIAL_TEXT
    );
    assert!(matches!(said, TurnOutcome::Proposal { .. }), "{said:?}");
    assert_observed(&s, &judged[2]);
}
/// The shared run: the verifier is asked over exactly what the room copied in and read back of
/// these bytes, every text whole; the record keeps each text's digest, never the text. Beside
/// `consistent` it is offered `unexercised` (the run's inputs never exercise some part, so it
/// proves no whole output), each part and each task the engine's facts leave open (none here).
fn assert_observed(s: &SessionRuntime, (state, options): &(Value, Vec<String>)) {
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    let offered = ["consistent", "unexercised", "part-0", "none"];
    assert_eq!(options, offered);
    let candidate = s.candidate().expect("proposed").set.changes[0]
        .content()
        .to_owned();
    let sha = sha256_hex(candidate.as_bytes());
    let observation = json!({
        "candidate_sha256": sha,
        "inputs": [{"path": "./entree.txt", "text": TRIAL_TEXT, "read_whole": true}],
        "outputs": [{"path": "./sortie.txt", "text": TRIAL_TEXT, "written": true, "read_whole": true}],
    });
    assert_eq!(state["observation"], observation);
    let attempt = attempt(s);
    assert_eq!(attempt["doubt"], json!(["unfaithful"]));
    assert_eq!(attempt["settled_by"], "verify-observed");
    assert_eq!(attempt["candidate_sha256"], sha);
    for empty in ["defects", "unknown", "contested", "unsettled"] {
        assert_eq!(attempt[empty], json!([]), "{empty}");
    }
    let asked = &attempt["questions"][2];
    assert_eq!(asked["question"], "verify-observed");
    assert_eq!(asked["role"], "judge_observed");
    assert_eq!(asked["choice"], "consistent");
    let text = |role: &str, path: &str, written: Value| {
        json!({"role": role, "path": path, "bytes": TRIAL_TEXT.len(),
            "sha256": sha256_hex(TRIAL_TEXT.as_bytes()), "read_whole": true, "written": written})
    };
    let receipt = json!({
        "candidate_sha256": sha,
        "sha256": sha256_hex(observation.to_string().as_bytes()),
        "texts": [
            text("input", "./entree.txt", Value::Null),
            text("output", "./sortie.txt", json!(true)),
        ],
    });
    assert_eq!(asked["observation"], receipt);
    assert!(
        !asked.to_string().contains("TRIAL-ROW-7"),
        "a digest, never the text"
    );
}
/// Every line is new work.
struct Work;
impl TurnClassifier for Work {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        TurnDecision::new(TurnAct::NewWork, crate::turn::RoutingMethod::Model)
    }
}
/// Why a doubt with no defect stays undecided when no run of the doubted bytes exists.
const NO_TRIAL: &str = "no trial run of these exact bytes exists in this compile";
/// A doubted creation no trial run can decide ([`WORK`] states no file the room may copy in, so
/// the room refuses it before any run): the judge doubts the whole request, carries its one part
/// (no task is left open to name) and, asked where its doubt is, names nothing: the same judge
/// asked again would decide nothing. The candidate is held as the preview, never proposed; its
/// record is kept with the judge's rejection, so a later line that replays it asks that judge
/// nothing, and nothing waits for a judge; the session says so.
#[test]
fn a_doubt_no_trial_run_can_decide_is_held_never_proposed_nor_replayed() {
    let mut script: Vec<_> = semantic_create()
        .iter()
        .map(|t| (200, response(t)))
        .collect();
    for choice in ["unfaithful", "carried", "unlocated"] {
        script.push((200, response(&json!({ "choice": choice }).to_string())));
    }
    let peer = Peer::start(script);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.enable_continuous_preparation();
    s.with_classifier(Box::new(Work));
    let said = s.turn(WORK);
    assert!(
        matches!(&said, TurnOutcome::Facts(words) if words == DOUBTED),
        "{said:?}"
    );
    // The document, then the whole request, its part, and where the doubt is.
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 2);
    let held = s.last_outcome.as_ref().expect("the held outcome");
    assert!(held.candidate.is_some(), "shown as the preview");
    assert_eq!(
        held.status,
        nika_onboard::compile::CompileStatus::Incomplete
    );
    assert!(held.questions.is_empty());
    let declined = (held.provenance.plan.as_ref()).and_then(|plan| plan["declined"].as_array());
    assert!(
        declined.is_some_and(|declined| !declined.is_empty()),
        "the record keeps the judge's rejection: replayed, that judge is asked nothing"
    );
    assert!(s.pending_proposal().is_none());
    assert!(!s.judgment_waits());
    let attempt = attempt(&s);
    assert_eq!(attempt["doubt"], json!(["unfaithful"]));
    assert_eq!(attempt["contested"], json!([WORK]));
    assert_eq!(attempt["unsettled"], json!([NO_TRIAL]));
    for empty in ["defects", "unknown", "notes"] {
        assert_eq!(attempt[empty], json!([]), "{empty}");
    }
    assert_eq!(attempt["settled_by"], Value::Null);
    let roles: Vec<&Value> = (attempt["questions"].as_array().into_iter().flatten())
        .map(|question| &question["role"])
        .collect();
    assert_eq!(roles, ["judge_request", "judge_part", "judge_doubt"]);
    let contested = format!(
        "The judge did not accept the request as carried (unfaithful) and located no defect a repair could start from; the same judge asked again decides nothing ({NO_TRIAL}). Nothing is READY on it. Next: a correction of the request, or another verifier."
    );
    let findings: Vec<&str> = (held.diagnostics.iter())
        .filter(|d| d.target == "semantic_verification")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(findings, [contested.as_str()]);
    let untried = format!("{UNRESOLVED}{UNTRIED}");
    assert_eq!(held_findings(held), [untried.as_str()]);
    let route = &held.provenance.decision.as_ref().expect("decision")["route"];
    let last = route.as_array().and_then(|steps| steps.last());
    assert_eq!(last, Some(&json!("verify: not ready, candidate held")));
    assert!(!dir.path().join("sortie.txt").exists(), "never run");
    assert!(!dir.path().join("compiled-workflow.nika").exists());
    assert_eq!(meaning(&mut s), HELD_MEANING);
}
