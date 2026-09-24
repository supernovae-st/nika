// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The live conversation over the actual Session runtime: its one-time
//! unknown-cost choice is a fresh spending question, and a retained Run review
//! keeps its contract. Hermetic mechanics only: the stub route never answers
//! and no socket opens, so no model, billing or UX is qualified here.
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

/// The selected unpriced route: it opts into admission, so the Session asks
/// its cost question, but carries no admission seam of its own. It counts
/// every unmetered call, which must never happen.
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
            "printf '%s\\n' \"$1\"; cat > reply.json".to_owned(),
            "fixture".to_owned(),
            FRAME.to_owned(),
        ];
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

#[test]
fn the_session_cost_question_is_fresh_and_its_details_answer_nothing() {
    let room = Room::new("details");
    let unmetered = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &unmetered);
    assert!(!live.fresh_input_required());
    let asked = live.submit("hello").beats;
    let first = question(&asked);
    assert!(
        first.starts_with("Fresh authoring cost decision · this request only;"),
        "{first}"
    );
    for fact in [
        "USD cost is unknown",
        "At most 3 requests",
        "8192 output tokens",
        "120 seconds",
        "no hard cap is overridden",
    ] {
        assert!(first.contains(fact), "{fact}: {first}");
    }
    assert!(
        first.ends_with("\nContinue once? yes / no / details"),
        "{first}"
    );
    assert_eq!(first.matches("Continue once?").count(), 1, "{first}");
    assert!(!first.contains("candidate"), "{first}");
    assert_eq!(
        waits(&asked),
        Some(Waiting::Question {
            key: "unknown_cost".to_owned()
        })
    );
    assert!(live.fresh_input_required());
    assert_eq!(
        live.busy_label("yes").as_deref(),
        Some("answering the fresh authoring cost question")
    );
    let details = question(&live.submit("details").beats);
    for evidence in [
        "candidate ",
        "invocation session:",
        "endpoint https://",
        "host ",
    ] {
        assert!(details.contains(evidence), "{evidence}: {details}");
    }
    assert_eq!(
        question(&live.submit(" DETAILS ").beats),
        details,
        "the same review, unchanged"
    );
    assert!(live.fresh_input_required(), "reading the details answered");
    assert_eq!(unmetered.load(Ordering::SeqCst), 0);
}

#[test]
fn an_interruption_cancels_the_session_choice_and_sends_nothing() {
    let room = Room::new("cancel");
    let unmetered = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &unmetered);
    let _ = live.submit("hello");
    assert!(live.fresh_input_required());
    let cancelled = live.cancel_pending();
    let text = joined(&cancelled);
    assert!(text.contains("cancelled; nothing sent"), "{text}");
    assert_eq!(waits(&cancelled), Some(Waiting::Free));
    assert!(!live.fresh_input_required());
    assert!(
        live.cancel_pending().is_empty(),
        "a second interruption finds nothing to cancel"
    );
    let late = joined(&live.submit("yes").beats);
    assert!(late.contains("nothing waits for a yes or a no"), "{late}");
    assert!(!live.fresh_input_required());
    assert_eq!(unmetered.load(Ordering::SeqCst), 0);
}

#[test]
fn only_a_fresh_yes_after_the_question_reaches_the_admission_seam_once() {
    let room = Room::new("yes");
    let unmetered = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &unmetered);
    let _ = live.submit("hello");
    // This route has no seam of its own: the Session's default seam refuses
    // the one admitted call, so reaching it is the evidence of the approval.
    let answered = joined(&live.submit("yes").beats);
    assert!(answered.contains("no catalog admission seam"), "{answered}");
    assert!(!live.fresh_input_required());
    let again = joined(&live.submit("yes").beats);
    assert!(again.contains("nothing waits for a yes or a no"), "{again}");
    assert_eq!(
        unmetered.load(Ordering::SeqCst),
        0,
        "an approval never falls back to an unmetered call"
    );
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
            key: "run_cost".to_owned()
        })
    );
    assert!(live.fresh_input_required());
    let details = question(&live.submit("details").beats);
    for evidence in [
        "challenge nonce-s90",
        "Endpoint: https://api.deepseek.com/v1\n",
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

/// The Session's own one-time choice reads the same grammar: an unknown line
/// is asked again (never cancelled), `/help` is local, `oui` approves once,
/// and the unknown-cost route is never called unmetered.
#[test]
fn the_session_choice_asks_an_unknown_line_again_and_keeps_help_local() {
    let room = Room::new("choice-unknown");
    let unmetered = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &unmetered);
    let _ = live.submit("hello");
    let again = question(&live.submit("peut-être").beats);
    assert!(
        again.contains("« peut-être » is not a yes or a no · nothing was sent"),
        "{again}"
    );
    assert!(
        again.ends_with("\nContinue once? yes / no / details"),
        "{again}"
    );
    assert!(live.fresh_input_required());
    let help = joined(&live.submit("/help").beats);
    assert!(help.contains("/details"), "{help}");
    assert!(live.fresh_input_required(), "/help cancelled the review");
    let answered = joined(&live.submit("oui").beats);
    assert!(answered.contains("no catalog admission seam"), "{answered}");
    assert!(!live.fresh_input_required());
    assert_eq!(unmetered.load(Ordering::SeqCst), 0);
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

/// Slash commands while the Session's own cost choice waits answer locally
/// and keep it waiting; nothing reaches the unknown-cost route.
#[test]
fn slash_commands_keep_the_session_choice_waiting() {
    let room = Room::new("choice-slash");
    let unmetered = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &unmetered);
    let _ = live.submit("hello");
    for line in ["/help", "/status", "/details", "/why"] {
        let said = joined(&live.submit(line).beats);
        assert!(!said.is_empty(), "{line}: nothing said");
        assert!(live.fresh_input_required(), "{line} ended the choice");
    }
    assert_eq!(unmetered.load(Ordering::SeqCst), 0);
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
    assert!(before.contains("nothing has run"), "{before}");
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
