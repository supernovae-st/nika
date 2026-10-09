// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The live conversation over the actual Session runtime: preparation is
//! continuous (no cost question; cost is observed, unknown stays unknown),
//! and a retained Run review keeps its own fresh cost question and contract.
//! Hermetic mechanics only: the stub route never answers and no socket opens,
//! so no model, billing or UX is qualified here.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use nika_cli_host::lane::{ChildSlot, drive_reviewed_child};
use nika_session::intelligence::IntelligenceKind;
use nika_session::{CostHostEvidence, ReasonError, Reply, ScriptedReasoner, SessionReasoner};
use std::sync::atomic::{AtomicUsize, Ordering};

const MODEL: &str = "deepseek/s90-unpriced-fixture";
const SOURCE: &str = "nika: fresh-run\nmodel: deepseek/s90-unpriced-fixture\ntasks:\n  answer:\n    infer: { prompt: Say OK, max_tokens: 32 }\noutputs:\n  result: ${{ tasks.answer.output }}\n";
/// One challenge frame as the child's host review writes it (every identity
/// is a token no sentence of the copy contains).
const FRAME: &str = r#"{"schema":"nika/run-cost-challenge@1","nonce":"nonce-s90","candidate":"cand-s90","invocation":"inv-s90","route":{"provider":"deepseek","model":"s90-unpriced-fixture","endpoint":"https://api.deepseek.com/v1"},"source_sha256":"src-s90","inputs_sha256":"in-s90","question":"USD cost is unknown; a charge is possible on deepseek/s90-unpriced-fixture.\nAt most 1 requests; each at most 8192 output tokens and 120 seconds (at most 120 seconds of model wait). No automatic retry.\nOverrides only the shown defaults (invocation: $0.250000; project: none); no hard cap is overridden.\nContinue once? yes / no","native_price":"price and invoice unknown","review_details":"candidate cand-s90 · invocation inv-s90 · endpoint https://api.deepseek.com/v1 · host fixture"}"#;

/// A temporary project root, removed when the test ends.
struct Room(PathBuf);

impl Room {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path =
            std::env::temp_dir().join(format!("nika-tui-s90-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).expect("room");
        Self(path)
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The selected unpriced route. Continuous preparation calls it with no cost
/// question; it refuses every call and counts each one, so a test can tell a
/// turn that reached it from one answered locally.
struct Route(Arc<AtomicUsize>);

impl SessionReasoner for Route {
    fn name(&self) -> String {
        "s90 route".to_owned()
    }
    fn reason(&mut self, _prompt: &str) -> Result<Reply, ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(ReasonError::Provider(
            "unmetered call on an unknown-cost route".to_owned(),
        ))
    }
    fn supports_admission(&self) -> bool {
        true
    }
    fn authoring_model(&self) -> Option<String> {
        Some(MODEL.to_owned())
    }
}

fn runners() -> Runners {
    Runners {
        run_once: Box::new(|_, _| panic!("no plain Run in this fixture")),
        run_resume: Box::new(|_, _, _, _| panic!("no resume in this fixture")),
        run_tapped: None,
    }
}

fn session_live(room: &Room, unmetered: &Arc<AtomicUsize>) -> Live {
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("deepseek".to_owned());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Api {
            provider: "deepseek".to_owned(),
        },
        Some(MODEL.to_owned()),
    );
    let calls = Arc::clone(unmetered);
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(move |_| Box::new(Route(Arc::clone(&calls)))),
        runners(),
    )
    .with_cost_host_evidence(CostHostEvidence::unmanaged_interactive_local());
    let _ = live.open();
    live
}

/// A Run review whose child asks `FRAME` and copies its one-use reply pipe to
/// `reply.json` until EOF.
fn run_live(room: &Room) -> Live {
    std::fs::write(room.0.join("one.nika"), SOURCE).expect("workflow");
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".to_owned());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        None,
    );
    let slot: ChildSlot = Arc::default();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
        runners(),
    )
    .with_run_review(Box::new(move |root, _, busy| {
        let args = vec![
            "-c".to_owned(),
            "printf '%s\\n' \"$1\"; exec /bin/cat > reply.json".to_owned(),
            "fixture".to_owned(),
            FRAME.to_owned(),
        ];
        // The story-only alias as the base consumers wrote it: it still compiles.
        drive_reviewed_child(Path::new("/bin/sh"), &args, root, busy, &slot)
    }));
    let _ = live.open();
    live
}

fn said(beats: &[Beat]) -> Vec<(Kind, String)> {
    beats
        .iter()
        .filter_map(|beat| match beat {
            Beat::Say(block) => Some((block.kind, block.text.clone())),
            _ => None,
        })
        .collect()
}

fn question(beats: &[Beat]) -> String {
    said(beats)
        .into_iter()
        .find_map(|(kind, text)| (kind == Kind::Question).then_some(text))
        .unwrap_or_else(|| panic!("no question in {beats:?}"))
}

fn joined(beats: &[Beat]) -> String {
    said(beats)
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn waits(beats: &[Beat]) -> Option<Waiting> {
    beats.iter().rev().find_map(|beat| match beat {
        Beat::Wait(waiting) => Some(waiting.clone()),
        _ => None,
    })
}

/// Continuous preparation asks no cost question: an unpriced route is reached
/// once for the line, nothing waits for a spending answer, the next line is not
/// fresh-only, and what is said keeps the Run's own budget apart.
#[test]
fn continuous_preparation_asks_no_cost_question_and_keeps_run_apart() {
    let room = Room::new("continuous");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    assert!(!live.fresh_input_required());
    let beats = live.submit("hello").beats;
    assert!(
        said(&beats).iter().all(|(kind, _)| *kind != Kind::Question),
        "{beats:?}"
    );
    assert_eq!(waits(&beats), Some(Waiting::Free));
    assert!(!live.fresh_input_required());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "reached once, without a gate"
    );
    let text = joined(&beats);
    assert!(!text.contains("Continue once?"), "{text}");
    assert!(text.contains("Run has its own budget"), "{text}");
    assert_eq!(
        live.busy_label("hello again").as_deref(),
        Some("working through your words")
    );
}

/// The welcome's example, typed in a project that holds no file of the user's, reaches the
/// Session as any request: the pack's skeleton asks its one open value, no model is called and
/// nothing is written before a proposal is consented.
#[test]
fn the_welcome_example_needs_no_file_and_asks_only_its_open_value() {
    let room = Room::new("demo");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let beats = live.submit(crate::workspace::project::DEMO).beats;
    let asked = question(&beats);
    assert!(asked.to_lowercase().contains("currency"), "{asked}");
    assert!(
        matches!(waits(&beats), Some(Waiting::QuestionDocument { .. })),
        "{beats:?}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "no model reads the example"
    );
    let written: Vec<_> = std::fs::read_dir(&room.0)
        .expect("room")
        .flatten()
        .map(|e| e.file_name())
        .filter(|name| name != ".nika")
        .collect();
    assert!(
        written.is_empty(),
        "nothing written before consent: {written:?}"
    );
}

/// With no decision waiting, an interruption after a preparation cancels
/// nothing and sends nothing.
#[test]
fn an_interruption_after_a_preparation_cancels_nothing_and_sends_nothing() {
    let room = Room::new("cancel");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let _ = live.submit("hello");
    assert!(
        live.cancel_pending().is_empty(),
        "nothing waits: nothing to cancel"
    );
    assert!(!live.fresh_input_required());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the interruption sent nothing"
    );
}

/// With nothing waiting, a bare yes (in either language) answers nothing and
/// never reaches the route: no hidden approval survives a preparation.
#[test]
fn a_bare_yes_after_a_preparation_answers_nothing_and_calls_nothing() {
    let room = Room::new("yes");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let _ = live.submit("hello");
    for line in ["yes", "oui"] {
        let late = joined(&live.submit(line).beats);
        assert!(
            late.contains("nothing waits for a yes or a no"),
            "{line}: {late}"
        );
        assert!(!live.fresh_input_required());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1, "a yes is never sent");
}

#[test]
fn a_run_review_keeps_its_contract_and_details_leave_the_challenge_untouched() {
    let room = Room::new("run");
    let mut live = run_live(&room);
    let asked = live.submit("run one.nika").beats;
    let first = question(&asked);
    assert!(
        first.starts_with(
            "Fresh Run cost decision · deepseek/s90-unpriced-fixture at https://api.deepseek.com\n"
        ),
        "{first}"
    );
    assert!(
        first.ends_with("\nContinue once? yes / no / details"),
        "{first}"
    );
    for identity in [
        "nonce-s90",
        "cand-s90",
        "inv-s90",
        "src-s90",
        "in-s90",
        "/v1",
    ] {
        assert!(!first.contains(identity), "{identity}: {first}");
    }
    assert_eq!(
        waits(&asked),
        Some(Waiting::Question {
            key: "run_cost".to_owned(),
        })
    );
    assert!(live.fresh_input_required());
    let details = question(&live.submit("details").beats);
    assert!(
        !details.contains("Endpoint:"),
        "the route line names its origin: {details}"
    );
    for evidence in [
        "challenge nonce-s90",
        "Origin: https://api.deepseek.com:443\n",
        "Source SHA-256: src-s90\n",
        "Input SHA-256: in-s90\n",
        "Candidate: cand-s90\n",
        "Invocation: inv-s90\n",
        "Host and cap evidence: candidate cand-s90",
    ] {
        assert!(details.contains(evidence), "{evidence}: {details}");
    }
    let reply = room.0.join("reply.json");
    assert!(
        std::fs::read(&reply).unwrap_or_default().is_empty(),
        "reading the details answered the child"
    );
    assert!(live.fresh_input_required());
    let _ = live.submit("yes");
    let sent = std::fs::read_to_string(&reply).expect("the one reply");
    assert!(sent.contains(r#""nonce":"nonce-s90""#), "{sent}");
    assert!(sent.ends_with(r#""yes":true}"#), "{sent}");
    assert_eq!(
        sent.matches("nika/run-cost-response@1").count(),
        1,
        "{sent}"
    );
    assert!(!live.fresh_input_required());
}

#[test]
fn an_interruption_ends_a_run_review_without_a_reply() {
    let room = Room::new("run-cancel");
    let mut live = run_live(&room);
    let _ = live.submit("run one.nika");
    assert!(live.fresh_input_required());
    let text = joined(&live.cancel_pending());
    assert!(
        text.contains("Run cost decision cancelled; nothing sent"),
        "{text}"
    );
    assert!(!live.fresh_input_required());
    assert!(live.cancel_pending().is_empty());
    let reply = std::fs::read(room.0.join("reply.json")).unwrap_or_default();
    assert!(reply.is_empty(), "a cancelled review answered: {reply:?}");
}

fn reply_of(room: &Room) -> Vec<u8> {
    std::fs::read(room.0.join("reply.json")).unwrap_or_default()
}

fn run_cost_wait() -> Waiting {
    Waiting::Question {
        key: "run_cost".to_owned(),
    }
}

/// `oui` answers the Run review exactly as it answers
/// the Session's own choice — one reply, once, and the review is over.
#[test]
fn oui_answers_a_run_review_like_the_session_choice_once() {
    let room = Room::new("run-oui");
    let mut live = run_live(&room);
    let _ = live.submit("run one.nika");
    assert!(live.fresh_input_required());
    let _ = live.submit("oui");
    let sent = String::from_utf8(reply_of(&room)).expect("utf-8 reply");
    assert!(sent.contains(r#""nonce":"nonce-s90""#), "{sent}");
    assert!(sent.ends_with(r#""yes":true}"#), "{sent}");
    assert_eq!(
        sent.matches("nika/run-cost-response@1").count(),
        1,
        "{sent}"
    );
    assert!(!live.fresh_input_required());
}

/// An unknown line never approves and never cancels: the review is asked
/// again, the child gets nothing, the fresh decision keeps waiting.
#[test]
fn an_unknown_line_asks_the_run_review_again_and_sends_nothing() {
    let room = Room::new("run-unknown");
    let mut live = run_live(&room);
    let _ = live.submit("run one.nika");
    for line in [
        "peut-être",
        "c'est payant ?",
        "yes please",
        "oui mais pas maintenant",
        "",
    ] {
        let asked = live.submit(line).beats;
        let text = question(&asked);
        assert!(
            text.contains("is not a yes or a no · nothing was sent"),
            "{line:?}: {text}"
        );
        assert!(
            text.contains("`yes`/`oui` runs it once"),
            "{line:?}: {text}"
        );
        assert_eq!(waits(&asked), Some(run_cost_wait()), "{line:?}");
        assert!(live.fresh_input_required(), "{line:?} ended the review");
        assert!(reply_of(&room).is_empty(), "{line:?} answered the child");
    }
}

/// `/help`, `/status` and `/details` beside a Run review answer
/// from the session's own facts; the review keeps waiting, unanswered.
#[test]
fn local_commands_beside_a_run_review_answer_locally_and_keep_it() {
    let room = Room::new("run-local");
    let mut live = run_live(&room);
    let _ = live.submit("run one.nika");
    let help = live.submit("/help").beats;
    let text = joined(&help);
    assert!(
        text.contains("/details") && text.contains("/status"),
        "{text}"
    );
    assert!(text.contains("still waits"), "{text}");
    assert_eq!(waits(&help), Some(run_cost_wait()));
    let status = live.submit("/status").beats;
    assert!(
        joined(&status).starts_with("session\n"),
        "{}",
        joined(&status)
    );
    assert_eq!(waits(&status), Some(run_cost_wait()));
    let details = question(&live.submit("/details").beats);
    assert!(details.contains("challenge nonce-s90"), "{details}");
    assert!(live.fresh_input_required());
    assert!(
        reply_of(&room).is_empty(),
        "a local command answered the child"
    );
    let _ = live.submit("yes");
    assert!(
        String::from_utf8(reply_of(&room))
            .expect("utf-8 reply")
            .ends_with(r#""yes":true}"#),
        "the review still takes its own answer"
    );
}

/// A declined review is « not run », never an observed
/// exit 130 « ended with an unknown code »; no decision carries to the next
/// line or the next Run.
#[test]
fn a_declined_run_review_is_not_run_and_nothing_carries() {
    let room = Room::new("run-declined");
    let mut live = run_live(&room);
    let _ = live.submit("run one.nika");
    let text = joined(&live.submit("non, finalement pas maintenant").beats);
    assert!(
        text.contains("Run cost decision cancelled; nothing sent"),
        "{text}"
    );
    assert!(text.contains("not run · "), "{text}");
    for wrong in ["exit 130", "unknown code", "run observed"] {
        assert!(!text.contains(wrong), "{wrong}: {text}");
    }
    assert!(!live.fresh_input_required());
    let late = joined(&live.submit("yes").beats);
    assert!(late.contains("nothing waits for a yes or a no"), "{late}");
    assert!(
        reply_of(&room).is_empty(),
        "a declined review answered the child"
    );
    let interrupted = joined(&{
        let _ = live.submit("run one.nika");
        live.cancel_pending()
    });
    assert!(interrupted.contains("not run · "), "{interrupted}");
    assert!(!interrupted.contains("unknown code"), "{interrupted}");
    assert!(
        reply_of(&room).is_empty(),
        "an interruption answered the child"
    );
}

/// After a preparation, `/help` is answered locally and calls nothing, while a
/// new line of work is prepared again at once: no question stands between the
/// human and the next attempt. An ambiguous line asks its routing label and reply.
#[test]
fn help_stays_local_and_a_new_line_is_prepared_again_without_a_question() {
    let room = Room::new("again");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let _ = live.submit("hello");
    let help = joined(&live.submit("/help").beats);
    assert!(help.contains("/details"), "{help}");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "/help reached the route");
    let again = live.submit("peut-être").beats;
    assert!(
        said(&again).iter().all(|(kind, _)| *kind != Kind::Question),
        "{again:?}"
    );
    assert!(joined(&again).contains("« peut-être »"), "{again:?}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "the greeting asked a reply; the new line asks its label and reply"
    );
    assert_eq!(
        live.runtime.as_ref().expect("open runtime").routes().len(),
        1,
        "only the ambiguous line needed one routing decision"
    );
    assert!(!live.fresh_input_required());
}

/// The transcript an actual beginner produces at a Run review: a worried
/// question, the help card, the evidence, then a French yes. Every line
/// before `oui` is answered locally and the child receives exactly one reply.
#[test]
fn a_beginner_transcript_at_the_run_review() {
    let room = Room::new("run-beginner");
    let mut live = run_live(&room);
    let mut transcript = Vec::new();
    for line in [
        "run one.nika",
        "ça coûte combien ?",
        "/help",
        "details",
        "oui",
    ] {
        let beats = live.submit(line).beats;
        transcript.push(format!("› {line}\n{}", joined(&beats)));
        if line != "oui" {
            assert!(reply_of(&room).is_empty(), "{line} answered the child");
        }
    }
    let transcript = transcript.join("\n");
    for expected in [
        "Fresh Run cost decision · deepseek/s90-unpriced-fixture at https://api.deepseek.com\n",
        "« ça coûte combien ? » is not a yes or a no · nothing was sent",
        "the fresh Run cost decision still waits · `yes`/`oui` runs it once · `no`/`non` cancels · `details` shows the evidence",
        "challenge nonce-s90",
    ] {
        assert!(
            transcript.contains(expected),
            "{expected}\n---\n{transcript}"
        );
    }
    assert!(!transcript.contains("unknown code"), "{transcript}");
    let sent = String::from_utf8(reply_of(&room)).expect("utf-8 reply");
    assert_eq!(
        sent.matches("nika/run-cost-response@1").count(),
        1,
        "{sent}"
    );
    assert!(sent.ends_with(r#""yes":true}"#), "{sent}");
}

/// Local commands hold in every state, the first intelligence screen
/// included: `/help`, `/status` and `/details` answer from the session's own
/// facts, the choice keeps waiting, and nothing reaches the model.
#[test]
fn local_commands_beside_the_intelligence_choice_keep_it_waiting() {
    let room = Room::new("choice-local");
    let unmetered = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &unmetered);
    let asked = live.submit("/intelligence").beats;
    assert_eq!(waits(&asked), Some(Waiting::Choosing));
    for line in ["/help", "/status", "/details"] {
        let beats = live.submit(line).beats;
        assert!(!joined(&beats).is_empty(), "{line}: nothing said");
        assert_eq!(
            waits(&beats),
            Some(Waiting::Choosing),
            "{line}: the choice stopped waiting"
        );
    }
    let kept = joined(&live.submit("cancel").beats);
    assert!(kept.contains("the choice stands"), "{kept}");
    assert_eq!(unmetered.load(Ordering::SeqCst), 0);
}

/// Lines that start with a yes but qualify it are not a yes: each one is
/// asked again and the child receives nothing.
#[test]
fn yes_qualified_lines_never_approve_a_run_review() {
    let room = Room::new("run-qualified");
    let mut live = run_live(&room);
    let _ = live.submit("run one.nika");
    for line in [
        "yes please",
        "yes?",
        "yes, but only once",
        "yes no",
        "okay",
        "yep",
        "oui oui",
        "oui mais pas maintenant",
    ] {
        let asked = live.submit(line).beats;
        assert!(
            question(&asked).contains("is not a yes or a no · nothing was sent"),
            "{line}"
        );
        assert!(live.fresh_input_required(), "{line} ended the review");
        assert!(reply_of(&room).is_empty(), "{line} answered the child");
    }
}

/// Read-only slash commands after a preparation answer from the Session's own
/// facts: each says something, none is a spending question, and none reaches
/// the route.
#[test]
fn slash_commands_after_a_preparation_answer_locally() {
    let room = Room::new("slash");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let _ = live.submit("hello");
    for line in ["/help", "/status", "/details", "/why"] {
        let said = joined(&live.submit(line).beats);
        assert!(!said.is_empty(), "{line}: nothing said");
        assert!(
            !live.fresh_input_required(),
            "{line} opened a spending question"
        );
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a slash command reached the route"
    );
}

/// A declined review has no effect: no reply, no trace, and the status line
/// keeps the selected workflow's checked, not-run state.
#[test]
fn a_declined_run_review_has_no_effect() {
    let room = Room::new("run-no-effect");
    let mut live = run_live(&room);
    let _ = live.submit("run one.nika");
    let before = live
        .runtime
        .as_ref()
        .map(SessionRuntime::status_line)
        .unwrap_or_default();
    assert!(before.contains("no current Run result"), "{before}");
    let _ = live.submit("no");
    assert!(
        reply_of(&room).is_empty(),
        "a declined review answered the child"
    );
    assert!(
        !room.0.join(".nika").join("traces").exists(),
        "a declined review left a trace"
    );
    let after = live
        .runtime
        .as_ref()
        .map(SessionRuntime::status_line)
        .unwrap_or_default();
    assert_eq!(after, before, "a declined review changed the status");
}

/// An interrupted review (Ctrl+C) is « not run », answers nothing, and the
/// next line finds no decision to answer.
#[test]
fn an_interrupted_run_review_is_not_run_and_answers_nothing() {
    let room = Room::new("run-interrupted");
    let mut live = run_live(&room);
    let _ = live.submit("run one.nika");
    let text = joined(&live.cancel_pending());
    assert!(text.contains("not run · "), "{text}");
    assert!(!text.contains("unknown code"), "{text}");
    assert!(!live.fresh_input_required());
    assert!(
        reply_of(&room).is_empty(),
        "an interruption answered the child"
    );
    let late = joined(&live.submit("yes").beats);
    assert!(late.contains("nothing waits for a yes or a no"), "{late}");
    assert!(reply_of(&room).is_empty(), "a late yes answered the child");
}

#[test]
fn the_authoring_projection_keeps_any_other_wording_whole() {
    assert_eq!(
        authoring_cost_question("A sentence the Session wrote."),
        "Fresh authoring cost decision · this request only; approving it never saves or runs anything\nA sentence the Session wrote.\nContinue once? yes / no / details"
    );
}

/// A run asked inside the turn is observed before its child exists: the
/// request names the workflow and carries the look of its exact bytes at
/// that moment; the frames the runner tells follow it on the same queue; a
/// line that is no frame is counted, never queued. The runner here is a
/// stub that tells three frames: no process, no model.
#[test]
fn a_run_is_observed_from_its_request_to_its_frames() {
    use crate::session::feed::{Gap, Observed, Seen};
    let room = Room::new("observed");
    let source = "nika: two\npermits: {}\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n";
    std::fs::write(room.0.join("two.nika"), source).expect("workflow");
    let none = UserIntelligencePreference::new(IntelligenceKind::None, None);
    let frames = [
        r#"{"correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"workflow","value":"two"}],"id":{"uuid":"01a0ef11-03a1-73d9-a2bc-2548bdab1943"},"kind":"workflow_started","run":null,"timestamp":1}"#,
        "not a frame",
        r#"{"correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"first"}],"id":{"uuid":"01a0ef11-03a7-74fb-bba0-bfe19b901333"},"kind":"task_scheduled","run":null,"timestamp":2}"#,
    ];
    let mut live = Live::new(
        room.0.clone(),
        IntelligenceCensus::empty(),
        Some(none),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
        Runners {
            run_once: Box::new(|_, _| panic!("no plain run")),
            run_resume: Box::new(|_, _, _, _| panic!("no resume")),
            // A story-only tap lent beside the typed one: the typed one wins.
            run_tapped: Some(Box::new(|_, _, _| panic!("the typed runner goes first"))),
        },
    )
    .with_run_tapped_observed(Box::new(move |_, _, sink| {
        let mut story = nika_display::run_story::RunStory::default();
        for line in frames {
            story.tell(line, sink);
        }
        (0, None, story.lines)
    }));
    let _ = live.open();
    let (busy, _said) = std::sync::mpsc::channel();
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    let gap = Arc::new(Gap::default());
    let turn = live.submit_observed("run two.nika", &busy, &Seen::new(tx, Arc::clone(&gap)));
    let seen: Vec<Observed> = rx.try_iter().collect();
    let Some(Observed::Asked {
        workflow,
        resume,
        typed,
        look,
        world,
    }) = seen.first()
    else {
        panic!("the request comes first: {seen:?}\n{}", joined(&turn.beats));
    };
    assert_eq!(
        (workflow.as_str(), *resume, *typed),
        ("two.nika", false, true)
    );
    let world = world
        .as_ref()
        .expect("the check that cleared the request declared its reach");
    assert_eq!(
        world.summary(),
        "local only · nothing outside the process",
        "a log-only workflow reaches nothing"
    );
    let look = look
        .as_ref()
        .expect("the bytes were read when it was asked");
    assert_eq!(
        look.witness(),
        Some(
            nika_session::change::Witness::of(source.as_bytes())
                .0
                .as_str()
        )
    );
    assert_eq!(seen.len(), 3, "the request and two frames: {seen:?}");
    assert!(matches!(seen[1], Observed::Frame(_)) && matches!(seen[2], Observed::Frame(_)));
    assert_eq!((gap.dropped(), gap.unread()), (0, 1));
    assert!(
        joined(&turn.beats).contains("run observed · exit 0"),
        "{}",
        joined(&turn.beats)
    );
}

/// The story-only tap the base consumers lend (`Runners::run_tapped`) still
/// runs inside the turn: its story reaches the busy row, no frame is typed,
/// and the request says the run cannot be followed.
#[test]
fn a_story_only_tap_tells_its_story_and_no_frame() {
    use crate::session::feed::{Gap, Observed, Seen};
    let room = Room::new("story-only");
    let source = "nika: two\npermits: {}\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n";
    std::fs::write(room.0.join("two.nika"), source).expect("workflow");
    let none = UserIntelligencePreference::new(IntelligenceKind::None, None);
    let mut live = Live::new(
        room.0.clone(),
        IntelligenceCensus::empty(),
        Some(none),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
        Runners {
            run_once: Box::new(|_, _| panic!("no plain run")),
            run_resume: Box::new(|_, _, _, _| panic!("no resume")),
            run_tapped: Some(Box::new(|_, _, busy: &std::sync::mpsc::Sender<String>| {
                let _ = busy.send("a line of the story".to_owned());
                (0, None, vec!["a line of the story".to_owned()])
            })),
        },
    );
    let _ = live.open();
    let (busy, said) = std::sync::mpsc::channel();
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    let gap = Arc::new(Gap::default());
    let turn = live.submit_observed("run two.nika", &busy, &Seen::new(tx, Arc::clone(&gap)));
    let seen: Vec<Observed> = rx.try_iter().collect();
    assert!(
        matches!(seen.as_slice(), [Observed::Asked { typed: false, .. }]),
        "the request alone, untyped: {seen:?}\n{}",
        joined(&turn.beats)
    );
    let heard: Vec<String> = said.try_iter().collect();
    assert!(
        heard.iter().any(|l| l == "a line of the story"),
        "{heard:?}"
    );
    assert!(joined(&turn.beats).contains("a line of the story"));
}

/// Both review builders and both taps lent: a fresh run admitted by the
/// Session goes to the typed review alone (the story-only review and the two
/// taps panic if called), once, its request marked typed; a line the Session
/// does not admit as a run reaches no runner at all.
#[test]
fn the_typed_review_goes_first_and_only_after_the_session_admits_the_run() {
    use crate::session::feed::{Gap, Observed, Seen};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let room = Room::new("precedence");
    let source = "nika: two\npermits: {}\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n";
    std::fs::write(room.0.join("two.nika"), source).expect("workflow");
    let none = UserIntelligencePreference::new(IntelligenceKind::None, None);
    let reviewed = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&reviewed);
    let mut live = Live::new(
        room.0.clone(),
        IntelligenceCensus::empty(),
        Some(none),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
        Runners {
            run_once: Box::new(|_, _| panic!("no plain run")),
            run_resume: Box::new(|_, _, _, _| panic!("no resume")),
            run_tapped: Some(Box::new(|_, _, _| panic!("the story tap is never first"))),
        },
    )
    .with_run_tapped_observed(Box::new(|_, _, _| panic!("a fresh run is reviewed first")))
    .with_run_review(Box::new(|_, _, _| panic!("the typed review goes first")))
    .with_run_review_observed(Box::new(move |_, run, sink| {
        counted.fetch_add(1, Ordering::SeqCst);
        assert_eq!(run.workflow, Path::new("two.nika"));
        sink.said("reviewed".to_owned());
        nika_cli_host::lane::RunProgress::Complete((0, None, vec!["reviewed".to_owned()]))
    }));
    let _ = live.open();
    let (busy, _said) = std::sync::mpsc::channel();
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    let gap = Arc::new(Gap::default());
    let refused = live.submit_observed(
        "run missing.nika",
        &busy,
        &Seen::new(tx.clone(), Arc::clone(&gap)),
    );
    assert_eq!(
        reviewed.load(Ordering::SeqCst),
        0,
        "{}",
        joined(&refused.beats)
    );
    assert!(rx.try_iter().next().is_none(), "nothing was asked");
    let turn = live.submit_observed("run two.nika", &busy, &Seen::new(tx, Arc::clone(&gap)));
    assert_eq!(
        reviewed.load(Ordering::SeqCst),
        1,
        "{}",
        joined(&turn.beats)
    );
    let seen: Vec<Observed> = rx.try_iter().collect();
    assert!(
        matches!(seen.first(), Some(Observed::Asked { typed: true, .. })),
        "{seen:?}"
    );
    assert!(
        joined(&turn.beats).contains("run observed · exit 0"),
        "{}",
        joined(&turn.beats)
    );
}

#[test]
fn an_explicit_run_pin_reaches_the_typed_cost_gate_and_cannot_be_replaced_while_waiting() {
    let room = Room::new("run-pin");
    std::fs::write(room.0.join("one.nika"), "nika: pinned\ntasks:\n  log:\n    invoke: { tool: \"nika:log\", args: { message: test } }\n").expect("workflow");
    let reviewed = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&reviewed);
    let slot: ChildSlot = Arc::default();
    let mut live = Live::new(
        room.0.clone(),
        IntelligenceCensus::empty(),
        Some(UserIntelligencePreference::new(
            IntelligenceKind::None,
            None,
        )),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
        runners(),
    )
    .with_run_review_observed(Box::new(move |root, run, sink| {
        counted.fetch_add(1, Ordering::SeqCst);
        assert_eq!(run.access_pin.as_deref(), Some("mock"));
        let args = vec![
            "-c".into(),
            "printf '%s\\n' \"$1\"; exec /bin/cat > reply.json".into(),
            "fixture".into(),
            FRAME.into(),
        ];
        nika_cli_host::lane::drive_reviewed_child_observed(
            Path::new("/bin/sh"),
            &args,
            root,
            sink,
            &slot,
        )
    }));
    let _ = live.open();
    let refused = live.submit("run one.nika --access mock --access api");
    assert_eq!(
        reviewed.load(Ordering::SeqCst),
        0,
        "{}",
        joined(&refused.beats)
    );
    let asked = live.submit("run one.nika --access mock --max-cost-usd 0.1");
    assert_eq!(
        reviewed.load(Ordering::SeqCst),
        1,
        "{}",
        joined(&asked.beats)
    );
    assert!(joined(&asked.beats).contains("access mock (explicit)"));
    assert!(live.fresh_input_required());
    let _ = live.submit("run one.nika --access api");
    assert_eq!(reviewed.load(Ordering::SeqCst), 1);
    assert!(live.fresh_input_required());
    assert!(
        std::fs::read(room.0.join("reply.json"))
            .unwrap_or_default()
            .is_empty()
    );
    let text = joined(&live.submit("no").beats);
    assert!(
        text.contains("Run cost decision cancelled; nothing sent"),
        "{text}"
    );
    assert!(
        reply_of(&room).is_empty(),
        "declining closes the child without authority"
    );
    assert_eq!(reviewed.load(Ordering::SeqCst), 1);
    assert!(!live.fresh_input_required());
}

#[test]
fn a_proposal_reply_label_does_not_predict_save_or_model_work() {
    let room = Room::new("proposal-label");
    std::fs::write(room.0.join("a.md"), "the exact source").expect("source");
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".into());
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(UserIntelligencePreference::new(
            IntelligenceKind::Local {
                provider: "ollama".into(),
            },
            None,
        )),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
        runners(),
    );
    let _ = live.open();
    let proposed = live.submit("Read ./a.md and write it to ./b.md");
    assert_eq!(
        waits(&proposed.beats),
        Some(Waiting::Proposal),
        "{}",
        joined(&proposed.beats)
    );
    for line in ["no", "/show", "write it to ./c.md instead", "yes"] {
        assert_eq!(
            live.busy_label(line).as_deref(),
            Some("reviewing your reply")
        );
        assert!(!room.0.join("b.md").exists());
    }
    assert_eq!(live.busy_label(""), None);
    let declined = live.submit("no");
    assert_eq!(waits(&declined.beats), Some(Waiting::Free));
    assert!(!room.0.join("b.md").exists());
    assert!(
        !std::fs::read_dir(&room.0)
            .expect("files")
            .flatten()
            .any(|f| f.path().extension().is_some_and(|ext| ext == "nika"))
    );
}

/// A reopened project: the workflow saved, no check and no run observed in
/// this session, and the successful run an earlier session kept.
fn reopened() -> (Lifecycle, String, Option<Result<KeptRun, String>>) {
    let mut facts = LifecycleFacts::new();
    facts.saved = true;
    let kept = KeptRun::new().ended(Some(Path::new("reorder.nika")), 0, None);
    (
        Lifecycle::from_facts(&facts),
        "Saved · no current Run result · `reorder.nika`".to_owned(),
        Some(Ok(kept)),
    )
}

/// The rail and status rows of the inline live area drawn at `width`, and
/// the rows that live area takes.
fn footer_rows(beats: [Beat; 2], width: u16) -> ([String; 2], u16) {
    use ratatui::{Terminal, backend::TestBackend};
    let mut state =
        crate::model::UiState::new(crate::model::Presentation::Inline, false, (width, 12));
    for beat in beats {
        state.apply(beat);
    }
    let composer = crate::composer::Composer::new();
    let rows = crate::render::live_rows(&state, &composer, width, 12);
    let mut terminal = Terminal::new(TestBackend::new(width, 6)).expect("test terminal");
    terminal
        .draw(|frame| crate::render::draw_inline(frame, &state, &composer))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let row = |y: u16| {
        (0..width)
            .map(|x| buffer[(x, y)].symbol().to_owned())
            .collect::<String>()
            .trim_end()
            .to_owned()
    };
    ([row(0), row(1)], rows)
}

/// Reopened, the footer tells the success an earlier session kept from what
/// this session observed: Checked and Run stay ○, the earlier stage is named
/// beside Run, and the status row opens with it, so a 40-column row still
/// reads it whole; the Session's words follow unchanged and no row is added.
#[test]
fn a_reopened_footer_tells_an_earlier_success_from_this_session() {
    let (lifecycle, status, kept) = reopened();
    let plain = [Beat::Rail(lifecycle.rail()), Beat::Status(status.clone())];
    let told = footer_beats(lifecycle, status.clone(), kept.as_ref(), true);
    let rail = "Draft ✓ · Saved ✓ · Checked ○ · Active ○ · Run ○ (earlier ✓)";
    assert_eq!(told[0], Beat::Rail(rail.to_owned()));
    let note = "last run ✓ exit 0 in an earlier session";
    assert_eq!(told[1], Beat::Status(format!("{note} · {status}")));
    let (wide, wide_rows) = footer_rows(told.clone(), 75);
    assert_eq!(wide[0], rail);
    assert!(
        wide[1].starts_with(&format!("{note} · Saved · no current Run result")),
        "{wide:?}"
    );
    let (narrow, narrow_rows) = footer_rows(told, 40);
    // The complete earlier-session note stays visible; a cut mark tells that
    // both rows continue instead of silently dropping their remaining facts,
    // the rail giving way first at its head (the draft its save implies) so
    // its newest stages stay.
    assert_eq!(
        narrow,
        [
            "… Saved ✓ · Checked ○ · Active ○ · Run…",
            &format!("{note}…")
        ]
    );
    assert_eq!(wide_rows, footer_rows(plain.clone(), 75).1);
    assert_eq!(narrow_rows, footer_rows(plain, 40).1);
}

/// The earlier run is told only while this session observed no run and only
/// a choice waits; an exit other than 0 keeps its own stage and another
/// workflow is named; an unreadable or exit-less record adds nothing.
#[test]
fn the_earlier_run_gives_way_to_a_run_or_a_question_here() {
    let (lifecycle, status, kept) = reopened();
    let unchanged = [Beat::Rail(lifecycle.rail()), Beat::Status(status.clone())];
    let waits = footer_beats(lifecycle, status.clone(), kept.as_ref(), false);
    assert_eq!(waits, unchanged);
    let mut ran = LifecycleFacts::new();
    ran.saved = true;
    ran.run = RunFact::Exit(1);
    let ran = Lifecycle::from_facts(&ran);
    let done = "Done · the run failed · `reorder.nika`".to_owned();
    assert_eq!(
        footer_beats(ran, done.clone(), kept.as_ref(), true),
        [Beat::Rail(ran.rail()), Beat::Status(done)]
    );
    let records = [
        None,
        Some(Err("unreadable".to_owned())),
        Some(Ok(KeptRun::new())),
    ];
    for record in records {
        let told = footer_beats(lifecycle, status.clone(), record.as_ref(), true);
        assert_eq!(told, unchanged, "{record:?}");
    }
    let failed = KeptRun::new().ended(Some(Path::new("other.nika")), 1, None);
    let told = footer_beats(lifecycle, status.clone(), Some(&Ok(failed)), true);
    assert_eq!(
        told[0],
        Beat::Rail(format!("{} (earlier ×)", lifecycle.rail()))
    );
    assert_eq!(
        told[1],
        Beat::Status(format!(
            "last run of `other.nika` × exit 1 in an earlier session · {status}"
        ))
    );
    let alone = footer_beats(lifecycle, String::new(), kept.as_ref(), true);
    let note = "last run of `reorder.nika` ✓ exit 0 in an earlier session";
    assert_eq!(alone[1], Beat::Status(note.to_owned()));
}

/// Each turn arms its own stop from the Session's preparation token. It
/// cancels that preparation, never a Run the same turn handed to its runner,
/// and a Run of an earlier turn does not reach a later turn's stop. Arming
/// calls no model and sends nothing.
#[test]
fn a_turn_stop_cancels_its_preparation_and_never_a_run() {
    let room = Room::new("stopper");
    let mut live = run_live(&room);
    let stop = live.stopper().expect("an open runtime arms a stop");
    assert_eq!(stop(), Stopping::Requested);
    let stop = live.stopper().expect("each turn arms its own");
    live.run_started.store(true, Ordering::Release);
    assert_eq!(stop(), Stopping::RunUnderway);
    assert_eq!(
        stop(),
        Stopping::RunUnderway,
        "asked again, still not stopped"
    );
    let fresh = live.stopper().expect("a later turn");
    assert_eq!(fresh(), Stopping::Requested);
    assert!(!room.0.join("reply.json").exists(), "nothing ran");
}

mod last_turn;
