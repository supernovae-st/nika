// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native acts still use the real Session review and one-use child response.
use super::*;
use crate::model::{Asked, Presentation, Shape, UiState};
use crate::session::feed::Gap;

fn action(beats: &[Beat]) -> (String, Asked) {
    match waits(beats).expect("waiting") {
        Waiting::Action { key, asked } => (key, asked),
        other => panic!("expected native action, got {other:?}"),
    }
}

fn answer(live: &mut Live, key: &str, witness: &str) -> Turn {
    let (busy, _) = std::sync::mpsc::channel();
    let (seen, _) = std::sync::mpsc::sync_channel(8);
    live.answer_bound(
        key,
        witness,
        &busy,
        &Seen::new(seen, Arc::new(Gap::default())),
    )
}

fn keys(asked: &Asked) -> Vec<&str> {
    let Shape::Choice(offers) = &asked.shape else {
        panic!("finite choice")
    };
    offers.iter().map(|offer| offer.key.as_str()).collect()
}

#[test]
fn a_native_run_review_reads_details_without_answering_then_runs_exactly_once() {
    let room = Room::new("native-review");
    let mut live = run_live(&room);
    let (key, asked) = action(&live.submit("run one.nika").beats);
    assert_eq!(key, "run_cost");
    assert_eq!(keys(&asked), ["yes", "no", "details"]);
    assert!(!asked.free_text);
    assert!(live.fresh_input_required());
    assert!(reply_of(&room).is_empty());

    let details = answer(&mut live, "details", &asked.witness);
    assert!(question(&details.beats).contains("challenge nonce-s90"));
    let (_, current) = action(&details.beats);
    assert_ne!(current.witness, asked.witness);
    assert!(reply_of(&room).is_empty());
    let stale = answer(&mut live, "yes", &asked.witness);
    assert!(joined(&stale.beats).contains("Nothing was sent"));
    assert!(reply_of(&room).is_empty());
    // A refused stale gesture repaints the decision and lends a fresh identity.
    let (_, fresh) = action(&stale.beats);
    let result = answer(&mut live, "yes", &fresh.witness);
    assert!(result.handoff.is_none());
    let sent = String::from_utf8(reply_of(&room)).expect("reply");
    assert!(sent.ends_with(r#""yes":true}"#), "{sent}");
    assert_eq!(sent.matches("nika/run-cost-response@1").count(), 1);
    let _ = answer(&mut live, "yes", &fresh.witness);
    assert_eq!(
        String::from_utf8(reply_of(&room)).expect("same reply"),
        sent
    );
}

#[test]
fn native_review_witness_cannot_approve_a_replacement_challenge() {
    let room = Room::new("native-review-replaced");
    let mut live = run_live(&room);
    let (_, asked) = action(&live.submit("run one.nika").beats);
    let replacement = live
        .runtime
        .as_mut()
        .expect("runtime")
        .run_review_asked("another request", "different evidence");
    let refused = answer(&mut live, "yes", &asked.witness);
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert!(reply_of(&room).is_empty());
    assert_eq!(
        live.runtime.as_ref().expect("runtime").waiting(),
        work::Waiting::RunReview {
            review: replacement
        }
    );
}

#[test]
fn native_review_cancel_sends_no_response_and_unknown_keys_cannot_approve() {
    let room = Room::new("native-review-cancel");
    let mut live = run_live(&room);
    let (_, asked) = action(&live.submit("run one.nika").beats);
    let refused = answer(&mut live, "yes, please", &asked.witness);
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert!(reply_of(&room).is_empty());
    let (_, fresh) = action(&refused.beats);
    let _ = answer(&mut live, "no", &fresh.witness);
    assert!(reply_of(&room).is_empty());
    assert_eq!(
        live.runtime.as_ref().expect("runtime").waiting(),
        work::Waiting::Free
    );
}

fn proposal_live(room: &Room) -> Live {
    let (census, pref) = super::super::intelligence::fixture();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        runners(),
    );
    let _ = live.open();
    let _ = live.submit("aggregate-by-key");
    let _ = live.submit("EUR");
    live
}

#[test]
fn a_native_save_names_the_visible_candidate_and_never_runs_it() {
    let room = Room::new("native-save");
    let mut live = proposal_live(&room);
    let (key, asked) = action(&[live.wait()]);
    assert_eq!(key, "save");
    assert_eq!(keys(&asked), ["yes", "save & run", "no", "/show"]);
    assert!(!room.0.join("aggregate-by-key.nika").exists());
    let inspected = answer(&mut live, "/show", &asked.witness);
    assert!(!room.0.join("aggregate-by-key.nika").exists());
    let (_, fresh) = action(&inspected.beats);
    let saved = answer(&mut live, "yes", &fresh.witness);
    assert!(saved.handoff.is_none(), "Save is not Run");
    assert!(room.0.join("aggregate-by-key.nika").exists());
    assert_eq!(live.runtime.as_ref().expect("runtime").work().run, None);
    let repeated = answer(&mut live, "yes", &fresh.witness);
    assert!(joined(&repeated.beats).contains("Nothing was sent"));
}

#[test]
fn a_native_save_without_the_visible_candidate_is_refused() {
    let room = Room::new("native-save-hidden");
    let mut live = proposal_live(&room);
    let (_, asked) = action(&[live.wait()]);
    live.candidate = None;
    let refused = answer(&mut live, "yes", &asked.witness);
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert!(!room.0.join("aggregate-by-key.nika").exists());
    assert!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .pending_proposal()
            .is_some()
    );
}

#[test]
fn the_finite_activation_questions_use_native_actions() {
    let room = Room::new("native-schedule");
    std::fs::create_dir_all(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "brief").expect("brief");
    let (census, pref) = super::super::intelligence::fixture();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        runners(),
    );
    let _ = live.open();
    // An English request: the Session says its own questions in the person's language.
    let _ = live.submit("Every morning at 8, read ./notes/brief.md and write it to ./out/copie.md");
    let _ = live.submit("yes");
    let timezone = live.submit("activate");
    assert!(
        matches!(waits(&timezone.beats), Some(Waiting::Question { key }) if key == "project.timezone")
    );
    let policy = live.submit("Europe/Paris");
    let (key, asked) = action(&policy.beats);
    assert_eq!(key, "project.missed");
    assert_eq!(keys(&asked), ["1", "2", "3", "cancel"]);
    assert!(
        asked.free_text,
        "a typed answer, a delegation too, reaches the Session"
    );
    assert!(
        asked
            .label
            .starts_with("If this machine is off at 08:00 (Europe/Paris)")
    );
    // The transcript reads the sentence and one short line: the rows are never repeated.
    let said = joined(&policy.beats);
    assert!(said.contains("If this machine is off at 08:00"), "{said}");
    assert!(!said.contains("run once when it comes back"), "{said}");
    let ceiling = answer(&mut live, "2", &asked.witness);
    let (key, amounts) = action(&ceiling.beats);
    assert_eq!(key, "project.ceiling");
    assert_eq!(keys(&amounts), ["0.10", "0.25", "0.50", "1.00", "cancel"]);
    assert!(amounts.free_text, "another amount is typed");
    let Shape::Choice(offers) = &amounts.shape else {
        panic!("finite choice")
    };
    let recommended: Vec<&str> = (offers.iter())
        .filter(|offer| offer.recommended)
        .map(|offer| offer.label.as_str())
        .collect();
    assert_eq!(
        recommended,
        ["$0.25 · no run of this workflow observed yet"]
    );
    let said = joined(&ceiling.beats);
    assert!(!said.contains("or type another amount"), "{said}");
    assert!(!room.0.join("nika.yaml").exists());
    let stale = answer(&mut live, "3", &asked.witness);
    assert!(joined(&stale.beats).contains("Nothing was sent"));
    assert_eq!(
        live.runtime.as_ref().expect("runtime").pending_activation(),
        Some("project.ceiling")
    );
    // Another amount is typed beside the rows: the declaration is proposed, not written.
    let (_, amounts) = action(&stale.beats);
    let declared = answer(&mut live, "0.20", &amounts.witness);
    assert!(joined(&declared.beats).contains("ceiling · $0.2 per scheduled run"));
    assert!(!room.0.join("nika.yaml").exists());
}

/// A delegation typed at an activation's list question (« choisis pour moi » at the missed-run
/// policy) reaches the Session, which honours it with its own choice and asks the next question;
/// it is never refused as an act the list does not offer.
#[test]
fn a_delegation_typed_at_an_activation_question_reaches_the_session() {
    let room = Room::new("native-schedule-delegated");
    std::fs::create_dir_all(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "brief").expect("brief");
    let (census, pref) = super::super::intelligence::fixture();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        runners(),
    );
    let _ = live.open();
    let _ = live.submit("Chaque matin à 8h, lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let _ = live.submit("yes");
    let _ = live.submit("activate");
    let policy = live.submit("Europe/Paris");
    let (key, asked) = action(&policy.beats);
    assert_eq!(key, "project.missed");
    let delegated = answer(&mut live, "choisis pour moi", &asked.witness);
    let said = joined(&delegated.beats);
    assert!(!said.contains("Nothing was sent"), "{said}");
    let (next, _) = action(&delegated.beats);
    assert_eq!(next, "project.ceiling", "{said}");
}

/// A project record whose inference exposure was budgeted: the Session reopened on it has no
/// saved decision to stand for a typed request's Run ceiling.
const BUDGETED_RECORD: &str = r#"{"version":1,"updated_at":"2026-10-15T09:00:00Z","inference_observations":[{"schema":"nika/session-decision-seat@2","state":"Closed","attempts":[],"cost":"unknown"}]}"#;

/// A run whose ceiling cannot be proved asks it as rows (NIK-170), as the activation's ceiling
/// does: the Session's amounts, its recommendation with why, another typed amount admitted, the
/// way out; the transcript keeps the save report and the one short question. Picking an amount
/// hands the run over under exactly that ceiling; nothing was handed over before.
#[test]
fn an_unprovable_run_ceiling_is_asked_as_native_rows() {
    let room = Room::new("native-run-ceiling");
    std::fs::create_dir_all(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "brief").expect("brief");
    std::fs::create_dir_all(room.0.join(".nika")).expect("project store");
    std::fs::write(room.0.join(".nika/session-state.json"), BUDGETED_RECORD).expect("record");
    let (census, pref) = super::super::intelligence::fixture();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        runners(),
    );
    let _ = live.open();
    let proposed = live.submit("Lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let (key, consent) = action(&proposed.beats);
    assert_eq!(key, "save");
    assert!(
        keys(&consent).contains(&"save & run"),
        "{:?}",
        keys(&consent)
    );
    let asked = answer(&mut live, "save & run", &consent.witness);
    let (key, ceiling) = action(&asked.beats);
    assert_eq!(key, "run.ceiling");
    assert_eq!(keys(&ceiling), ["0.10", "0.25", "0.50", "1.00", "cancel"]);
    assert!(ceiling.free_text, "another amount is typed");
    assert_eq!(
        ceiling.label,
        "Je n'ai pas lancé : quel plafond pour ce lancement ?"
    );
    let Shape::Choice(offers) = &ceiling.shape else {
        panic!("finite choice")
    };
    let recommended: Vec<&str> = (offers.iter())
        .filter(|offer| offer.recommended)
        .map(|offer| offer.label.as_str())
        .collect();
    assert_eq!(
        recommended,
        ["$0.25 · aucun run de ce workflow observé pour l'instant"]
    );
    let said = joined(&asked.beats);
    assert!(said.contains("appliqué · écrit"), "the save stands: {said}");
    assert!(
        said.contains("Je n'ai pas lancé : quel plafond pour ce lancement ? (par ex. 0.25)"),
        "{said}"
    );
    assert!(!said.contains("restored inference exposure"), "{said}");
    assert!(
        live.pending.is_none(),
        "nothing handed over before the ceiling"
    );
    let run = answer(&mut live, "0.25", &ceiling.witness);
    assert!(run.handoff.is_some(), "the pick requests the run");
    let Some((_, Work::Run(request))) = &live.pending else {
        panic!("the run handed over: {:?}", run.beats)
    };
    assert_eq!(request.max_cost_usd.to_bits(), 0.25_f64.to_bits());
}

fn gate_live(room: &Room, values: &[&str]) -> (Live, std::path::PathBuf) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(room, &calls);
    let source = "nika: choose\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  pick:\n    invoke: { tool: \"nika:prompt\", args: { mode: choice, message: Choose one, choices: [a, b] } }\noutputs:\n  result: ${{ tasks.pick.output }}\n";
    let source = source.replace(
        "choices: [a, b]",
        &format!(
            "choices: {}",
            serde_json::to_string(values).expect("choices")
        ),
    );
    std::fs::write(room.0.join("gate.nika"), source).expect("workflow");
    let runtime = live.runtime.as_mut().expect("runtime");
    assert!(matches!(
        runtime.turn("run gate.nika"),
        TurnOutcome::RunRequested { .. }
    ));
    let trace = room.0.join("paused.ndjson");
    let frame = serde_json::json!({"kind":"workflow_paused","fields":[
        {"key":"task","value":"pick"}, {"key":"mode","value":"choice"},
        {"key":"message","value":"Choose one"},
        {"key":"choices","value":serde_json::to_string(values).expect("options")}
    ]});
    std::fs::write(&trace, frame.to_string()).expect("trace");
    assert!(matches!(
        runtime.observe_run(4, Some(&trace)),
        TurnOutcome::GateAsk { .. }
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    (live, trace)
}

#[test]
fn native_gate_choices_preserve_strings_and_never_interpret_them_as_commands() {
    for value in ["yes", "true", "1", "/quit", "why?", "  spaced  "] {
        let room = Room::new(&format!("native-choice-{}", value.len()));
        let (mut live, trace) = gate_live(&room, &[value, "another"]);
        let (key, asked) = action(&[live.wait()]);
        assert_eq!(key, "gate_choice");
        assert_eq!(asked.label, "Choose one");
        assert!(!asked.free_text);
        let Shape::Choice(offers) = &asked.shape else {
            panic!("choice")
        };
        assert_eq!(offers[0].label, value);
        let wire = serde_json::to_string(value).expect("wire");
        assert_eq!(offers[0].key, wire);
        let result = answer(&mut live, &wire, &asked.witness);
        assert!(
            result.handoff.is_some(),
            "the gesture only requests the resume"
        );
        let Some((
            _,
            Work::Resume {
                trace: sent_trace,
                answer: sent,
                ..
            },
        )) = &live.pending
        else {
            panic!("exact resume request: {:?}", result.beats)
        };
        assert_eq!(sent_trace, &trace);
        assert_eq!(sent, &format!("pick={wire}"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(sent.split_once('=').expect("pair").1)
                .expect("JSON"),
            serde_json::json!(value)
        );
        assert!(
            live.runtime
                .as_ref()
                .expect("runtime")
                .waiting_gate()
                .is_none()
        );
        let repeated = answer(&mut live, &wire, &asked.witness);
        assert!(joined(&repeated.beats).contains("Nothing was sent"));
    }
}

#[test]
fn a_changed_trace_or_an_unlisted_gate_value_never_requests_a_resume() {
    let room = Room::new("native-choice-stale");
    let (mut live, trace) = gate_live(&room, &["a", "b"]);
    let (_, asked) = action(&[live.wait()]);
    let refused = answer(&mut live, "a", &asked.witness);
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert!(live.pending.is_none());
    let (_, fresh) = action(&refused.beats);
    let raw = std::fs::read_to_string(&trace).expect("trace");
    std::fs::write(&trace, format!("{raw}\n")).expect("same choices, changed bytes");
    let stale = answer(&mut live, "\"a\"", &fresh.witness);
    assert!(joined(&stale.beats).contains("Nothing was sent"));
    assert!(live.pending.is_none());
    assert!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .waiting_gate()
            .is_some()
    );
}

#[test]
fn an_expired_native_gate_explains_the_extra_answer_without_changing_literal_choices() {
    let room = Room::new("expired-native-choice");
    let (mut live, trace) = gate_live(&room, &["true", "/quit"]);
    let raw = std::fs::read_to_string(&trace).expect("trace");
    let mut frame: serde_json::Value = serde_json::from_str(&raw).expect("frame");
    frame["fields"].as_array_mut().expect("fields").extend([
        serde_json::json!({"key":"approval_shown_hash","value":"shown"}),
        serde_json::json!({"key":"approval_nonce","value":"run"}),
        serde_json::json!({"key":"approval_minted_at_ms","value":0}),
        serde_json::json!({"key":"approval_ttl_seconds","value":900}),
    ]);
    let raw = frame.to_string();
    std::fs::write(&trace, &raw).expect("timed trace");
    let (key, asked) = action(&[live.wait()]);
    assert_eq!(key, "gate_choice");
    assert_eq!(asked.label, "Choose one");
    assert_eq!(
        asked.why,
        "Expired: a fresh question is needed before approval."
    );
    assert_eq!(keys(&asked), ["\"true\"", "\"/quit\""]);
    assert!(live.pending.is_none(), "projection dispatches no resume");
    assert_eq!(std::fs::read_to_string(&trace).expect("same trace"), raw);
}

/// The acts and the question this host builds beside the Session's follow the
/// language the person writes, and every act keeps its key: a French line read
/// first, the run's cost decision asks « Continuer ce run une fois ? » over
/// French acts, and `yes`, `no` and `details` still answer it. The same request
/// in English keeps the English words.
#[test]
fn the_acts_this_host_builds_follow_the_persons_language_and_keep_their_keys() {
    let labels = |asked: &Asked| {
        let Shape::Choice(offers) = &asked.shape else {
            panic!("finite choice")
        };
        offers
            .iter()
            .map(|offer| offer.label.clone())
            .collect::<Vec<_>>()
    };
    let room = Room::new("native-review-english");
    let mut live = run_live(&room);
    let (_, asked) = action(&live.submit("run one.nika").beats);
    assert_eq!(asked.label, "Continue this run once?");
    assert_eq!(
        labels(&asked),
        ["Run once", "Cancel this run", "Read the full cost evidence"]
    );
    let room = Room::new("native-review-french");
    let mut live = run_live(&room);
    let _ = live.submit("Bonjour, je voudrais lancer le fichier de mes notes");
    let (key, asked) = action(&live.submit("run one.nika").beats);
    assert_eq!(key, "run_cost");
    assert_eq!(keys(&asked), ["yes", "no", "details"]);
    assert_eq!(asked.label, "Continuer ce run une fois ?");
    assert_eq!(
        labels(&asked),
        [
            "Lancer une fois",
            "Annuler ce run",
            "Lire toute la preuve de coût"
        ]
    );
    let result = answer(&mut live, "yes", &asked.witness);
    assert!(result.handoff.is_none());
    let sent = String::from_utf8(reply_of(&room)).expect("reply");
    assert!(
        sent.ends_with(r#""yes":true}"#),
        "the key answers it: {sent}"
    );
}

/// Told the ASCII glyph column, the acts this host builds in French lose their
/// accents; their keys and the Session's words stay.
#[test]
fn the_acts_built_in_french_take_the_ascii_glyph_column() {
    let room = Room::new("native-review-ascii");
    let mut live = run_live(&room);
    live.glyphs(true);
    let _ = live.submit("Bonjour, je voudrais lancer le fichier de mes notes");
    let (_, asked) = action(&live.submit("run one.nika").beats);
    let Shape::Choice(offers) = &asked.shape else {
        panic!("finite choice")
    };
    let labels: Vec<&str> = offers.iter().map(|offer| offer.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Lancer une fois",
            "Annuler ce run",
            "Lire toute la preuve de cout"
        ]
    );
    assert_eq!(keys(&asked), ["yes", "no", "details"]);
}

/// A host that installs nothing on this machine: it records each gesture and names one unit
/// per schedule as `arm --emit` names it, under a directory of no machine, never touching
/// launchd. A unit it loaded and has not removed since is the one it holds.
#[derive(Clone, Default)]
struct Recorded(Arc<std::sync::Mutex<Vec<String>>>);

/// The unit `arm --emit` names for `beat` of the project at `root`, under `/units`: the file
/// name the Session journals a disarm by.
fn unit(root: &Path, beat: &str) -> PathBuf {
    let fingerprint = nika_runtime::project_root_fingerprint(root).expect("the root resolves");
    let key = nika_cadence::emit::project_key(&fingerprint);
    let name = nika_cadence::emit::beat_unit(&key, beat);
    PathBuf::from("/units").join(format!("{name}.plist"))
}

impl Recorded {
    fn seen(&self) -> Vec<String> {
        self.0.lock().expect("record").clone()
    }

    fn push(&self, said: String) {
        self.0.lock().expect("record").push(said);
    }
}

impl nika_session::TriggerHost for Recorded {
    fn emit(&self, root: &Path, program: &Path, beat: &str) -> Result<Vec<PathBuf>, String> {
        assert!(
            root.join("nika.yaml").is_file(),
            "emitted from the declared project"
        );
        assert!(program.is_absolute(), "{}", program.display());
        self.push(format!("emit {beat}"));
        Ok(vec![unit(root, beat)])
    }

    fn load(&self, unit: &Path) -> Result<(), String> {
        self.push(format!("load {}", unit.display()));
        Ok(())
    }

    fn remove(&self, unit: &Path) -> Result<(), String> {
        self.push(format!("remove {}", unit.display()));
        Ok(())
    }

    fn installed(&self, root: &Path, beat: &str) -> bool {
        let named = unit(root, beat);
        let (loaded, removed) = (
            format!("load {}", named.display()),
            format!("remove {}", named.display()),
        );
        let seen = self.seen();
        let last_load = seen.iter().rposition(|said| *said == loaded);
        let last_removal = seen.iter().rposition(|said| *said == removed);
        last_load.is_some_and(|at| last_removal.is_none_or(|gone| gone < at))
    }
}

/// The French daily copy saved, then its schedule declared through the Session's own
/// questions (the zone, the missed policy, the ceiling) and its yes, in a Live that lends
/// `host` or none: the Live and the turn of that yes.
fn declared(room: &Room, host: Option<Recorded>) -> (Live, Turn) {
    declared_with(room, host, |_| {})
}

/// [`declared`], with `saved` changing the project once the daily copy is saved and before its
/// schedule is asked for.
fn declared_with(room: &Room, host: Option<Recorded>, saved: fn(&Room)) -> (Live, Turn) {
    let (mut live, proposed) = declaring(room, host, saved);
    let (key, consent) = action(&proposed.beats);
    assert_eq!(key, "save", "the declaration is proposed");
    let landed = answer(&mut live, "yes", &consent.witness);
    (live, landed)
}

/// The daily copy saved and its schedule asked for, in French, up to the declaration's proposal:
/// the Live and the turn that proposes it.
fn declaring(room: &Room, host: Option<Recorded>, saved: fn(&Room)) -> (Live, Turn) {
    std::fs::create_dir_all(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "brief").expect("brief");
    let (census, pref) = super::super::intelligence::fixture();
    let live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        runners(),
    );
    let mut live = match host {
        Some(host) => live.with_trigger_host(Box::new(host)),
        None => live,
    };
    let _ = live.open();
    let _ = live.submit("Chaque matin à 8h, lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let _ = live.submit("oui");
    saved(room);
    let _ = live.submit("active-le");
    let zone = live.submit("Europe/Paris");
    let (_, missed) = action(&zone.beats);
    let ceiling = answer(&mut live, "1", &missed.witness);
    let (_, ceiling) = action(&ceiling.beats);
    let proposed = answer(&mut live, "0.20", &ceiling.witness);
    (live, proposed)
}

/// The declaration's card says its schedule in the person's words, as the Session gives them:
/// when it runs, what a missed run does, the most each run may cost, never the cron (live
/// gauntlet 5, r01 B6 and r08 B14: « cadence: "TZ=Europe/Paris 0 8 1 * *" » was its only word
/// of it); the file view keeps the exact bytes.
#[test]
fn a_declarations_card_says_its_schedule_in_words_and_keeps_the_bytes_in_its_file() {
    let room = Room::new("native-declaration-words");
    let (live, proposed) = declaring(&room, None, |_| {});
    let (key, _) = action(&proposed.beats);
    assert_eq!(key, "save", "the declaration is proposed");
    let shown = live.candidate().expect("the declaration's fold");
    assert_eq!(shown.path(), "nika.yaml");
    let review = shown
        .review(false)
        .expect("a proposal the conversation reviews");
    let rows: Vec<String> = (review.lines(false, false, 120).iter())
        .map(ToString::to_string)
        .collect();
    let said = rows.join("\n");
    for words in [
        "planification · compiled-workflow.nika · tous les jours à 8h (heure de Paris)",
        "si le Mac est éteint : le rattraper une fois",
        "plafond 0,20 $ par run",
    ] {
        assert!(said.contains(words), "{words}\n{said}");
    }
    assert!(
        !said.contains("TZ=") && !said.contains("0 8 * * *"),
        "never the cron:\n{said}"
    );
    assert!(!said.contains("pas un workflow audité"), "{said}");
    let (_, body) = shown.face_lines(nika_tui_view::Face::Source, 120, false, false);
    let bytes: Vec<String> = body.iter().map(ToString::to_string).collect();
    assert!(
        (bytes.iter()).any(|row| row.contains("TZ=Europe/Paris 0 8 * * *")),
        "the file view keeps the bytes: {bytes:#?}"
    );
}

/// D1: a door that lends a trigger host (a person's own terminal on a Mac lends launchd) asks,
/// once the declaration landed, whether to install its trigger: two rows, installing first and
/// recommended, the Session's report of what it wrote above its question, the acts said once
/// (as rows), nothing installed before the yes. « oui » writes and loads the unit through the
/// host and says when it fires next; « désactive-le » removes it. A door that lends none leaves
/// the schedule declared and asks nothing.
#[test]
fn a_declared_schedule_installs_its_trigger_through_the_lent_host_on_the_yes() {
    let plain = Room::new("native-trigger-declared");
    let (_, landed) = declared(&plain, None);
    let said = joined(&landed.beats);
    assert!(
        said.contains("Déclaré dans `nika.yaml` · pas actif"),
        "{said}"
    );
    assert!(
        matches!(waits(&landed.beats), Some(Waiting::Free)),
        "nothing asked: {said}"
    );

    let room = Room::new("native-trigger-install");
    let host = Recorded::default();
    let (mut live, landed) = declared(&room, Some(host.clone()));
    let (key, asked) = action(&landed.beats);
    assert_eq!(key, "trigger.install");
    assert_eq!(keys(&asked), ["install", "declared"]);
    let Shape::Choice(offers) = &asked.shape else {
        panic!("finite choice")
    };
    let labels: Vec<&str> = offers.iter().map(|offer| offer.label.as_str()).collect();
    assert_eq!(labels, ["Installer et activer", "Garder déclaré seulement"]);
    assert!(offers[0].recommended && !offers[1].recommended);
    let said = joined(&landed.beats);
    for words in [
        "appliqué · écrit `nika.yaml`",
        "Déclaré dans `nika.yaml`",
        "Installer le déclencheur sur ce Mac ?",
        "une unité launchd",
    ] {
        assert!(said.contains(words), "{words}\n{said}");
    }
    assert!(
        !said.contains("1 Installer et activer · 2 Garder déclaré seulement"),
        "the rows say the acts: {said}"
    );
    assert!(host.seen().is_empty(), "nothing installed before the yes");
    let installed = answer(&mut live, "oui", &asked.witness);
    let said = joined(&installed.beats);
    assert!(
        said.contains(
            "Installé et actif · `compiled-workflow.nika` tournera tout seul sur ce Mac · prochain run : "
        ),
        "{said}"
    );
    let seen = host.seen();
    let [emit, load] = seen.as_slice() else {
        panic!("one unit written and loaded: {seen:?}");
    };
    let beat = emit.strip_prefix("emit ").expect("the schedule's label");
    let named = unit(&room.0, beat);
    assert_eq!(load, &format!("load {}", named.display()));
    assert!(matches!(waits(&installed.beats), Some(Waiting::Free)));
    let removed = joined(&live.submit("désactive-le").beats);
    assert!(removed.contains("désactivé"), "{removed}");
    assert_eq!(
        host.seen().last(),
        Some(&format!("remove {}", named.display()))
    );
}

/// The rail line the beats paint, as their last rail says it.
fn rail(beats: &[Beat]) -> String {
    beats
        .iter()
        .rev()
        .find_map(|beat| match beat {
            Beat::Rail(rail) => Some(rail.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no rail in {beats:?}"))
}

/// Item 6: what a schedule's trigger fired reaches the conversation and the list, read only
/// through the Session's history of its arm ledger, and only a proved fire makes the rail
/// active. A fired run journaled while the person was away (a catch-up of one missed slot, 90
/// minutes late, one skipped before it) is said at the next turn, once, with its trace and the
/// catch-up the ledger counts; the list counts it
/// under its workflow. The rail stays declared until then; the unit the lent host holds (its
/// `installed`, as the Session asks it) proves the fire, « Actif ● »; « désactive-le »
/// removes the unit and the rail is declared again.
#[test]
fn a_fired_run_is_said_listed_and_only_a_proved_fire_makes_the_rail_active() {
    use nika_arm::state::{ArmState, FireKind, HistoryEntry};
    let ts = |text: &str| text.parse::<jiff::Timestamp>().expect("an instant");
    let room = Room::new("native-trigger-fired");
    let (mut live, label) = armed(&room);
    assert!(
        rail(&live.footer()).contains("Actif ◐"),
        "installed, no fire yet"
    );
    let state = ArmState::at_project(&room.0);
    let mut skipped = HistoryEntry::new(
        Some(ts("2026-10-15T06:00:00Z")),
        ts("2026-10-15T07:30:00Z"),
        FireKind::Skipped,
    );
    skipped.reason = Some("missed:1".to_owned());
    state.record_fixture(&label, &skipped).expect("skipped");
    let mut fired = HistoryEntry::new(
        Some(ts("2026-10-16T06:00:00Z")),
        ts("2026-10-16T07:30:00Z"),
        FireKind::Fired,
    );
    fired.exit = Some(0);
    fired.trace = Some(".nika/traces/fired.ndjson".to_owned());
    fired.slots = Some(1);
    state.record_fixture(&label, &fired).expect("fired");

    let turn = live.submit("merci");
    let said = joined(&turn.beats);
    for words in [
        "`compiled-workflow.nika` a tourné tout seul : 1 run par son déclencheur",
        "dernier créneau 16/10 08:00, heure de Paris · sortie 0 · sa trace .nika/traces/fired.ndjson",
        "1 créneau manqué rattrapé à 16/10 07:30",
        "1 créneau sauté",
    ] {
        assert!(said.contains(words), "{words}\n{said}");
    }
    // The host holds the unit it loaded: a fire of an installed trigger is proved.
    assert!(
        rail(&turn.beats).contains("Actif ●"),
        "{}",
        rail(&turn.beats)
    );
    let again = joined(&live.submit("merci encore").beats);
    assert!(!again.contains("a tourné tout seul"), "said once: {again}");

    let view = crate::session::Conversation::project(&live).expect("the project");
    let french = Voice {
        lang: crate::lang::Lang::Fr,
        ascii: false,
    };
    let listed = crate::workspace::project::aside(
        Some(&view),
        crate::workspace::aside::Tab::Nika,
        None,
        None,
        (None, &[]),
        (french, &jiff::tz::TimeZone::UTC),
    );
    let at = (listed.entries.iter())
        .position(|entry| entry.label == "compiled-workflow.nika")
        .expect("the workflow listed");
    let under = &listed.entries[at + 1];
    assert_eq!(
        (under.label.as_str(), under.depth),
        ("1 run déclenché · 16/10 08:00 Paris ✔", 1)
    );

    // « désactive-le »: the host removes the unit, and declared is not active.
    let removed = live.submit("désactive-le");
    assert!(
        rail(&removed.beats).contains("Actif ◐"),
        "removed, declared again: {}",
        rail(&removed.beats)
    );
}

/// The daily copy declared and its trigger installed through the recording host: the Live,
/// once its yes landed, and the label the schedule's ledger is kept under.
fn armed(room: &Room) -> (Live, String) {
    let (mut live, landed) = declared(room, Some(Recorded::default()));
    let (_, asked) = action(&landed.beats);
    let _ = answer(&mut live, "oui", &asked.witness);
    let runtime = live.runtime.as_ref().expect("runtime");
    let history = runtime.schedule_history();
    let label = history
        .first()
        .expect("the declared schedule")
        .label
        .clone();
    (live, label)
}

/// A run its trigger fired for the slot of 16 October at 06:00 UTC, two seconds after it,
/// journaled in the ledger of `label` as a firer journals it: exit 0, its trace kept.
fn fired_on_time(room: &Room, label: &str) {
    use nika_arm::state::{ArmState, FireKind, HistoryEntry};
    let ts = |text: &str| text.parse::<jiff::Timestamp>().expect("an instant");
    let mut fired = HistoryEntry::new(
        Some(ts("2026-10-16T06:00:00Z")),
        ts("2026-10-16T06:00:02Z"),
        FireKind::Fired,
    );
    fired.exit = Some(0);
    fired.trace = Some(".nika/traces/fired.ndjson".to_owned());
    let state = ArmState::at_project(&room.0);
    state.record_fixture(label, &fired).expect("fired");
}

/// Item 7: a fire journaled while the door stands open, no turn under way, is said when the
/// shell's idle ask reads the Session's history again: its line once, with the rail; an ask
/// with nothing new says nothing, and none adds a wait or a question.
#[test]
fn a_fire_while_the_door_stands_open_is_said_on_the_idle_ask() {
    let room = Room::new("native-trigger-idle");
    let (mut live, label) = armed(&room);
    let quiet = crate::session::Conversation::idle(&mut live);
    assert_eq!(quiet, Vec::<Beat>::new(), "nothing fired yet");
    fired_on_time(&room, &label);
    let news = crate::session::Conversation::idle(&mut live);
    let said = joined(&news);
    assert_eq!(
        said,
        "`compiled-workflow.nika` a tourné tout seul : 1 run par son déclencheur\n  dernier créneau 16/10 08:00, heure de Paris · sortie 0 · sa trace .nika/traces/fired.ndjson"
    );
    // The rail comes with it: the host holds the unit, so the fire is proved.
    assert!(
        rail(&news).contains("Actif ●"),
        "the rail with it: {news:?}"
    );
    assert_eq!(waits(&news), None, "the news asks nothing");
    let again = crate::session::Conversation::idle(&mut live);
    assert_eq!(again, Vec::<Beat>::new(), "said once");
}

/// Item 8: a ledger the Session cannot read is said once, though nothing is counted, so the
/// person knows its fires cannot be proved; the rail stays declared and the list adds nothing.
#[test]
fn an_unreadable_ledger_is_said_once_though_nothing_is_counted() {
    let room = Room::new("native-trigger-unreadable");
    let (mut live, label) = armed(&room);
    fired_on_time(&room, &label);
    // A line its chain never recorded: the ledger no longer reads, and counts nothing.
    let state = nika_arm::state::ArmState::at_project(&room.0);
    let history = state.root().join(&label).join("history.ndjson");
    let mut text = std::fs::read_to_string(&history).expect("the ledger");
    text.push_str("{\"tampered\":true}\n");
    std::fs::write(&history, text).expect("tampered");
    let news = crate::session::Conversation::idle(&mut live);
    let said = joined(&news);
    assert!(
        said.starts_with(
            "`compiled-workflow.nika` : le journal de son déclencheur est illisible, ses runs ne peuvent pas être prouvés : "
        ) && !said.contains('\n'),
        "{said}"
    );
    assert!(rail(&news).contains('◐'), "declared: {news:?}");
    let again = crate::session::Conversation::idle(&mut live);
    assert_eq!(again, Vec::<Beat>::new(), "said once");
    let view = crate::session::Conversation::project(&live).expect("the project");
    let listed = crate::workspace::project::aside(
        Some(&view),
        crate::workspace::aside::Tab::Nika,
        None,
        None,
        (None, &[]),
        (Voice::default(), &jiff::tz::TimeZone::UTC),
    );
    let at = (listed.entries.iter())
        .position(|entry| entry.label == "compiled-workflow.nika")
        .expect("the workflow listed");
    assert!(
        listed
            .entries
            .get(at + 1)
            .is_none_or(|next| next.depth == 0),
        "no fired row under it"
    );
}

/// Item 6, re-install: a fire from before the last disarm proves nothing. « désactive-le »
/// journals the disarm and takes the rail back; « active-le » installs the trigger again and the
/// rail stays declared, though the ledger counts the old fire; the next fire proves it, « Actif
/// ● ».
#[test]
fn a_trigger_installed_again_is_active_only_once_it_fires_again() {
    let room = Room::new("native-trigger-reinstalled");
    let (mut live, label) = armed(&room);
    fired_on_time(&room, &label);
    let turn = live.submit("merci");
    assert!(rail(&turn.beats).contains("Actif ●"), "a proved fire");
    let removed = live.submit("désactive-le");
    let said = joined(&removed.beats);
    assert!(said.contains("journalisé : disarmed"), "{said}");
    assert!(rail(&removed.beats).contains("Actif ◐"), "removed");
    let again = live.submit("active-le");
    let (key, asked) = action(&again.beats);
    assert_eq!(key, "trigger.install");
    let installed = answer(&mut live, "oui", &asked.witness);
    let said = joined(&installed.beats);
    assert!(said.contains("Installé et actif"), "{said}");
    let runtime = live.runtime.as_ref().expect("runtime");
    let history = runtime.schedule_history();
    let history = history.first().expect("the declared schedule");
    assert_eq!(
        (
            history.unit_installed,
            history.fired,
            history.fired_since_disarm
        ),
        (true, 1, 0),
        "installed again, the old fire counted before its disarm"
    );
    assert!(
        rail(&installed.beats).contains("Actif ◐"),
        "an old fire proves nothing: {}",
        rail(&installed.beats)
    );
    fired_on_time(&room, &label);
    let news = crate::session::Conversation::idle(&mut live);
    assert!(rail(&news).contains("Actif ●"), "fired again: {news:?}");
}

/// The focus view the decision `beats` of `live` leave waiting paints at 200×50, in the language
/// its person writes, row by row, and the act its composer opens on.
fn painted(live: &Live, beats: &[Beat]) -> (Vec<String>, Option<usize>) {
    let mut state = UiState::new(Presentation::Focus, false, (200, 50));
    state.lang = crate::session::Conversation::language(live).unwrap_or_default();
    for beat in beats.iter().cloned() {
        state.apply(beat);
    }
    let mut composer = crate::composer::Composer::new();
    composer.follow(&state.waiting);
    let opens = composer.offer_selected(&state.waiting);
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 50)).expect("terminal");
    terminal
        .draw(|frame| crate::render::draw_focus(frame, &state, &composer))
        .expect("draw the decision");
    let buffer = terminal.backend().buffer();
    let rows = (0..50)
        .map(|y| {
            let row: String = (0..200).map(|x| buffer[(x, y)].symbol()).collect();
            row.trim_end().to_owned()
        })
        .collect();
    (rows, opens)
}

/// The painted row of the act `label`.
fn act<'a>(rows: &'a [String], label: &str) -> &'a str {
    (rows.iter())
        .find(|row| row.contains(&format!("( ) {label}")))
        .unwrap_or_else(|| panic!("{label}:\n{}", rows.join("\n")))
}

/// The saved daily copy rewritten so its model reads an API key from the terminal: a unit
/// started by launchd would not see it.
fn keyed(room: &Room) {
    let saved = room.0.join("compiled-workflow.nika");
    assert!(saved.is_file(), "the daily copy is saved");
    std::fs::write(
        &saved,
        "nika: compiled-workflow\nmodel: anthropic/claude-sonnet-5\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n",
    )
    .expect("a keyed model");
}

/// The install decision when a scheduled run would not see the workflow's keys: the Session
/// warns on its card and recommends keeping the schedule declared, the second act. The decision
/// shows its card above its two acts (what installing does, the warning), its two acts as rows
/// in the Session's order, its line of numbered acts said once (as rows), and, a decision, it
/// opens on no act: Enter installs nothing by itself.
#[test]
fn an_install_decision_shows_its_card_with_the_key_warning() {
    let room = Room::new("native-trigger-keyed");
    let (live, landed) = declared_with(&room, Some(Recorded::default()), keyed);
    let (key, asked) = action(&landed.beats);
    assert_eq!(key, "trigger.install");
    let Shape::Choice(offers) = &asked.shape else {
        panic!("finite choice")
    };
    let acts: Vec<(&str, bool)> = (offers.iter())
        .map(|offer| (offer.label.as_str(), offer.recommended))
        .collect();
    assert_eq!(
        acts,
        [
            ("Installer et activer", false),
            ("Garder déclaré seulement", true)
        ]
    );
    let (rows, opens) = painted(&live, &landed.beats);
    assert_eq!(opens, None, "no act chosen");
    let screen = rows.join("\n");
    for words in [
        "Installer le déclencheur sur ce Mac ?",
        "effet · une unité launchd lancée par",
        "attention · un run planifié ne voit pas les clés API de votre terminal",
    ] {
        assert!(screen.contains(words), "{words}\n{screen}");
    }
    let install = (rows.iter()).position(|row| row.contains("Installer et activer"));
    let keep = (rows.iter()).position(|row| row.contains("Garder déclaré seulement"));
    assert!(
        install
            .zip(keep)
            .is_some_and(|(install, keep)| install < keep),
        "two rows, installing first\n{screen}"
    );
    assert!(
        !screen.contains("1 Installer et activer · 2 Garder déclaré seulement"),
        "the acts are said once, as rows\n{screen}"
    );
}

/// Every decision names the act the Session recommends, as a word beside it, and opens on no
/// act, so Enter consents to nothing by itself: the trigger's install recommends installing,
/// keeping it declared once a scheduled run would not see the workflow's keys, and a save
/// decision, which recommends none, marks none.
#[test]
fn a_decision_marks_the_recommended_act_and_opens_on_none() {
    let room = Room::new("native-mark-install");
    let (live, landed) = declared(&room, Some(Recorded::default()));
    let (rows, opens) = painted(&live, &landed.beats);
    assert_eq!(opens, None, "the install opens on no act");
    assert!(act(&rows, "Installer et activer").ends_with("recommandé"));
    assert!(!act(&rows, "Garder déclaré seulement").contains("recommandé"));

    let room = Room::new("native-mark-keyed");
    let (live, landed) = declared_with(&room, Some(Recorded::default()), keyed);
    let (rows, opens) = painted(&live, &landed.beats);
    assert_eq!(opens, None, "the warned install opens on no act");
    assert!(!act(&rows, "Installer et activer").contains("recommandé"));
    assert!(act(&rows, "Garder déclaré seulement").ends_with("recommandé"));

    let room = Room::new("native-mark-save");
    std::fs::create_dir_all(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "brief").expect("brief");
    let (census, pref) = super::super::intelligence::fixture();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        runners(),
    );
    let _ = live.open();
    let proposed = live.submit("Lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let (key, _) = action(&proposed.beats);
    assert_eq!(key, "save");
    let (rows, opens) = painted(&live, &proposed.beats);
    assert_eq!(opens, None, "the save opens on no act");
    let painted = rows.join("\n");
    assert!(painted.contains("1 ( ) "), "its acts are rows\n{painted}");
    assert!(!painted.contains("recommandé"), "{painted}");
    assert!(!painted.contains("(●)"), "{painted}");
}

/// « je voudrais l'enregistrer et le lancer », typed at a shown proposal's save decision, goes
/// through the TUI's bound door as the person's words, never as a general line: the Session
/// saves the workflow and hands its run over, the door runs it once, and nothing ran before the
/// door performed it.
#[test]
fn a_typed_save_and_run_at_the_shown_proposal_saves_and_runs_once() {
    let room = Room::new("native-typed-save-run");
    std::fs::create_dir_all(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "brief").expect("brief");
    let runs = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&runs);
    let (census, pref) = super::super::intelligence::fixture();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        Runners {
            run_once: Box::new(move |_, _| {
                counted.fetch_add(1, Ordering::SeqCst);
                (0, None)
            }),
            run_resume: Box::new(|_, _, _, _| panic!("no resume")),
            run_tapped: None,
        },
    );
    let _ = live.open();
    let proposed = live.submit("Lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let (key, consent) = action(&proposed.beats);
    assert_eq!(key, "save");
    assert!(consent.free_text, "the decision takes the person's words");
    let turn = answer(
        &mut live,
        "je voudrais l'enregistrer et le lancer",
        &consent.witness,
    );
    let said = joined(&turn.beats);
    assert!(said.contains("appliqué · écrit"), "saved: {said}");
    assert!(
        room.0.join("compiled-workflow.nika").is_file(),
        "the workflow is saved: {said}"
    );
    let handoff = turn.handoff.clone().expect("the run is handed over");
    let Some((_, Work::Run(request))) = &live.pending else {
        panic!("the saved workflow's run: {:?}", turn.beats)
    };
    assert_eq!(request.workflow, Path::new("compiled-workflow.nika"));
    assert_eq!(
        runs.load(Ordering::SeqCst),
        0,
        "nothing ran before the door"
    );
    let _ = live.perform(&handoff);
    assert_eq!(runs.load(Ordering::SeqCst), 1, "run once");
    assert!(live.pending.is_none(), "nothing left to run");
}

/// The rail tells a declared schedule's three states apart (live gauntlet 5, r01 B7, r02 B15,
/// r05, r08 B15: « Actif ◐ » read the same installed or declared only): declared with no firer,
/// « Actif ◐ déclaré »; installed, its firer standing, not fired yet, « Actif ◐ en attente »;
/// fired since the last disarm, « Actif ● », no word.
#[test]
fn the_rail_tells_declared_waiting_and_fired_apart() {
    let plain = Room::new("native-rail-declared");
    let (live, _) = declared(&plain, None);
    let said = rail(&live.footer());
    assert!(said.contains("Actif ◐ déclaré · Run"), "{said}");
    let room = Room::new("native-rail-waiting");
    let (mut live, label) = armed(&room);
    let said = rail(&live.footer());
    assert!(said.contains("Actif ◐ en attente · Run"), "{said}");
    fired_on_time(&room, &label);
    let news = crate::session::Conversation::idle(&mut live);
    let said = rail(&news);
    assert!(said.contains("Actif ● · Run"), "{said}");
}

/// An earlier session's run is told on the rail and the status only while this conversation
/// observed none (live gauntlet 5, r09 B13: « Run ○ (précédent ×) » right after a green run
/// here, once a save reset the Run field).
#[test]
fn an_earlier_sessions_run_is_not_told_once_a_run_was_observed_here() {
    let room = Room::new("native-earlier-run");
    std::fs::create_dir_all(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "brief").expect("brief");
    let (census, pref) = super::super::intelligence::fixture();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        Runners {
            run_once: Box::new(|_, _| (0, None)),
            run_resume: Box::new(|_, _, _, _| panic!("no resume")),
            run_tapped: None,
        },
    );
    let _ = live.open();
    live.kept = Some(Ok(KeptRun::new().ended(
        Some(Path::new("ancien.nika")),
        1,
        None,
    )));
    let said = rail(&live.footer());
    assert!(
        said.ends_with("(earlier ×)"),
        "nothing ran here yet: {said}"
    );
    let proposed = live.submit("Lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let (_, consent) = action(&proposed.beats);
    let turn = answer(
        &mut live,
        "je voudrais l'enregistrer et le lancer",
        &consent.witness,
    );
    let handoff = turn.handoff.clone().expect("the run is handed over");
    let _ = live.perform(&handoff);
    let again = live.submit("Lis ./notes/brief.md et écris-le dans ./out/autre.md");
    let (key, consent) = action(&again.beats);
    assert_eq!(key, "save");
    let saved = answer(&mut live, "yes", &consent.witness);
    let said = joined(&saved.beats);
    assert!(said.contains("appliqué · écrit"), "{said}");
    let (rail, status) = (rail(&saved.beats), status(&saved.beats));
    assert!(
        !rail.contains("précédent"),
        "a run was observed here: {rail}"
    );
    assert!(!status.contains("session antérieure"), "{status}");
}

/// The status row the beats paint, as their last status says it.
fn status(beats: &[Beat]) -> String {
    beats
        .iter()
        .rev()
        .find_map(|beat| match beat {
            Beat::Status(status) => Some(status.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no status in {beats:?}"))
}
