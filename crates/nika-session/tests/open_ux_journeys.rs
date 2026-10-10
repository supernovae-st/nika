// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]
#![cfg(unix)]

//! A hurried person's journeys through the public Session doors, the author scripted: an
//! ordinary request and short French replies reach a concrete candidate with no repeated
//! settled question, no value forced into machine syntax and no word taken for a value. What a
//! person accepts, delegates or already said survives corrections and a reopen with its real
//! provenance; independent questions are asked together under exact identities; a stale screen,
//! an earlier question or another Session answers nothing; a complete replacement starts over;
//! and Save and Run stay acts of the person, bound to the candidate they designate.
//!
//! The author is a loopback double on the OpenAI-compatible wire (`peer`): it scripts what an
//! author agent decides — through the author tools `ask`, `candidate_write`, `propose` and
//! `new_request` — and approves every verifier question. The scenarios therefore prove what the
//! Session does with those decisions (identities, provenance, retention, authority), never an
//! intelligence's judgment; that is measured with real models elsewhere. The environment is the
//! child's own (a test cannot set one in-process): each scenario re-runs this binary as a child
//! that drives the Session and writes a report; the parent holds the peer and judges the report
//! against the requests the peer received.

#[path = "open_ux_journeys/child.rs"]
mod child;
#[path = "open_ux_journeys/peer.rs"]
mod peer;
#[path = "open_ux_journeys/scenarios.rs"]
mod scenarios;

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use peer::{Peer, Seen};
use scenarios::{
    ACCEPT, ALREADY, DELEGATION, DIGEST, HACKER_NEWS, LE_MONDE, LE_MONDE_WORDS, OUTPUT_WORDS,
    TEAM_HOOK, TECHCRUNCH, WHY, scenario,
};
use serde_json::Value;

/// One scenario's report, and the requests the peer received while the child drove it.
struct Journey {
    report: Value,
    agent: Vec<Seen>,
    _dir: tempfile::TempDir,
}

impl Journey {
    fn step(&self, at: usize) -> &Value {
        &self.report["steps"][at]
    }
}

/// Run one scenario in a child process against its scripted peer.
fn run(name: &str) -> Journey {
    let scenario = scenario(name);
    let dir = tempfile::tempdir().unwrap();
    let (root, home) = (dir.path().join("project"), dir.path().join("home"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    for (path, text) in &scenario.files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let peer = Peer::start(scenario.script);
    let report = dir.path().join("report.json");
    let log = dir.path().join("child.log");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .env("NIKA_VLLM_BASE_URL", peer.base())
        .env("OPEN_UX_CHILD", name)
        .env("OPEN_UX_ROOT", &root)
        .env("OPEN_UX_REPORT", &report)
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(&log).unwrap()))
        .stderr(Stdio::from(
            std::fs::OpenOptions::new().append(true).open(&log).unwrap(),
        ));
    let mut process = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = process.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = process.kill();
            panic!("the child scenario `{name}` did not finish within 180 s");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.success(),
        "child `{name}` failed:\n{}",
        std::fs::read_to_string(&log).unwrap_or_default()
    );
    let report = serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
    let agent = peer.agent();
    peer.shutdown();
    Journey {
        report,
        agent,
        _dir: dir,
    }
}

/// The child: drive the scenario its parent named, write the report.
#[test]
#[ignore = "run by the parent scenarios in a child process"]
fn child() {
    let Some(name) = std::env::var_os("OPEN_UX_CHILD") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("OPEN_UX_ROOT").unwrap());
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let report = child::drive(&name.to_string_lossy(), &root, &home);
    std::fs::write(
        std::env::var_os("OPEN_UX_REPORT").unwrap(),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
}

/// What waits after a step (`question` · `questions` · `consent` …).
fn waiting(step: &Value) -> &str {
    step["shown"]["waiting"]["kind"]
        .as_str()
        .unwrap_or_default()
}

fn work(step: &Value) -> &Value {
    &step["shown"]["work"]
}

/// The workflow the candidate under review lands, exactly.
fn candidate(step: &Value) -> String {
    (work(step)["candidate"]["files"]
        .as_array()
        .into_iter()
        .flatten())
    .find(|file| file["workflow"] == true)
    .and_then(|file| file["content"].as_str())
    .unwrap_or_default()
    .to_owned()
}

fn rows<'a>(step: &'a Value, key: &str) -> Vec<&'a Value> {
    work(step)[key].as_array().into_iter().flatten().collect()
}

/// The binding of `value`, if the Session holds one.
fn binding<'a>(step: &'a Value, value: &str) -> Option<&'a Value> {
    rows(step, "bindings")
        .into_iter()
        .find(|row| row["value"] == value)
}

/// The provenance of the binding of `value`: `(kind, message)`.
fn provenance(step: &Value, value: &str) -> (String, String) {
    let row = binding(step, value).unwrap_or_else(|| {
        panic!(
            "the Session binds {value} with its provenance: {:#}",
            work(step)
        )
    });
    let field = |key: &str| {
        row["provenance"][key]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    (field("kind"), field("message"))
}

/// The open questions the Session shows, by key, with their identities.
fn open_questions(step: &Value) -> Vec<(String, String)> {
    (rows(step, "questions").into_iter())
        .filter(|q| q["state"] == "open")
        .map(|q| {
            (
                q["key"].as_str().unwrap_or_default().to_owned(),
                q["id"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

/// The concrete values a waiting question's recommended option carries.
fn recommended_values(step: &Value) -> Vec<String> {
    (rows(step, "questions").into_iter())
        .flat_map(|q| q["options"].as_array().into_iter().flatten())
        .filter(|option| option["recommended"] == true)
        .flat_map(|option| option["values"].as_array().into_iter().flatten())
        .filter_map(|value| value["value"].as_str().map(str::to_owned))
        .collect()
}

/// Whether `path` names a workflow file (`.nika`, any case).
fn is_workflow(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("nika"))
}

fn outcome_kind(step: &Value) -> &str {
    step["outcome"]["kind"].as_str().unwrap_or_default()
}

fn proposal(step: &Value) -> Option<&str> {
    step["shown"]["proposal"].as_str()
}

/// Whether some author request carries the person's line `line`, word for word.
fn reached_the_author(journey: &Journey, line: &str) -> bool {
    journey.agent.iter().any(|seen| seen.text().contains(line))
}

#[test]
fn accepting_the_current_offer_binds_its_values_never_the_word_yes() {
    let journey = run("accept_offer");
    let (asked, accepted) = (journey.step(0), journey.step(1));
    assert!(
        matches!(waiting(asked), "question" | "questions"),
        "the request is met with one concrete offer: {asked:#}"
    );
    let offered = recommended_values(asked);
    for value in [HACKER_NEWS, TECHCRUNCH, DIGEST] {
        assert!(offered.iter().any(|v| v == value), "{value}: {offered:?}");
    }
    let tools = journey.agent[0].tools();
    for tool in ["ask", "candidate_write", "propose"] {
        assert!(tools.iter().any(|t| t == tool), "{tool}: {tools:?}");
    }
    assert!(
        reached_the_author(&journey, ACCEPT),
        "the reply reached the author as typed"
    );
    assert_eq!(waiting(accepted), "consent", "{accepted:#}");
    let source = candidate(accepted);
    for value in [HACKER_NEWS, TECHCRUNCH, DIGEST] {
        assert!(
            source.contains(value),
            "{value} in the candidate:\n{source}"
        );
    }
    assert!(
        !source.contains("\"oui") && !source.contains("./oui"),
        "no path or value named after the word yes:\n{source}"
    );
    for value in [HACKER_NEWS, TECHCRUNCH, DIGEST] {
        assert_eq!(
            provenance(accepted, value),
            ("offered".to_owned(), "u2".to_owned()),
            "{value}"
        );
    }
    assert!(
        rows(accepted, "bindings")
            .iter()
            .all(|row| row["value"].as_str().is_none_or(|v| !v.contains("oui"))),
        "{:#}",
        work(accepted)
    );
    // The question was answered once: its identity answers nothing again.
    let again = journey.step(2);
    assert_eq!(again["outcome"]["kind"], "refusal", "{again:#}");
    assert_eq!(again["outcome"]["class"], "already_consumed", "{again:#}");
}

#[test]
fn delegated_public_sources_and_a_derived_output_need_no_questionnaire() {
    let journey = run("delegated_sources");
    let first = journey.step(0);
    assert_ne!(outcome_kind(first), "question", "{first:#}");
    assert!(rows(first, "questions").is_empty(), "{:#}", work(first));
    assert_eq!(
        waiting(first),
        "consent",
        "one line reaches a candidate: {first:#}"
    );
    let source = candidate(first);
    for value in [HACKER_NEWS, TECHCRUNCH, LE_MONDE, DIGEST] {
        assert!(source.contains(value), "{value}:\n{source}");
    }
    for value in [HACKER_NEWS, TECHCRUNCH, LE_MONDE] {
        assert_eq!(
            provenance(first, value),
            ("delegated".to_owned(), "u1".to_owned()),
            "{value}"
        );
        assert_eq!(
            binding(first, value).unwrap()["provenance"]["excerpt"],
            DELEGATION
        );
    }
    assert_eq!(
        provenance(first, DIGEST),
        ("derived".to_owned(), "u1".to_owned())
    );
    assert_eq!(
        binding(first, DIGEST).unwrap()["provenance"]["excerpt"],
        OUTPUT_WORDS
    );
    let delegations = rows(first, "delegations");
    assert!(
        delegations
            .iter()
            .any(|d| d["excerpt"] == DELEGATION && d["message"] == "u1"),
        "the delegation is kept with the person's words: {delegations:?}"
    );
    // Delegated reading never saves, runs or writes anything.
    assert!(work(first)["saved"].is_null() && work(first)["requested"].is_null());
    let files = journey.report["files"].as_array().unwrap();
    assert!(
        files.iter().all(|f| !is_workflow(f.as_str().unwrap())),
        "{files:?}"
    );
}

#[test]
fn a_partial_correction_keeps_every_unaffected_selection_and_its_provenance() {
    let journey = run("partial_correction");
    let (before, after) = (journey.step(1), journey.step(2));
    assert_eq!(waiting(before), "consent", "{before:#}");
    assert_ne!(outcome_kind(after), "question", "{after:#}");
    assert!(rows(after, "questions").is_empty(), "{:#}", work(after));
    assert_eq!(waiting(after), "consent", "{after:#}");
    assert_ne!(
        proposal(after),
        proposal(before),
        "a new candidate is proposed"
    );
    let source = candidate(after);
    for value in [HACKER_NEWS, TECHCRUNCH, LE_MONDE, DIGEST] {
        assert!(source.contains(value), "{value}:\n{source}");
    }
    // What the person accepted stays theirs, as accepted; the addition is named by its words.
    for value in [HACKER_NEWS, TECHCRUNCH, DIGEST] {
        assert_eq!(
            provenance(after, value),
            ("offered".to_owned(), "u2".to_owned()),
            "{value}"
        );
    }
    assert_eq!(
        provenance(after, LE_MONDE),
        ("named".to_owned(), "u3".to_owned())
    );
    assert_eq!(
        binding(after, LE_MONDE).unwrap()["provenance"]["excerpt"],
        LE_MONDE_WORDS
    );
    // The author's first correction silently dropped TechCrunch: refused, with the reason.
    let refused = journey.agent[6].last();
    assert!(
        refused.contains("techcrunch.com"),
        "the refusal names the retained source the correction dropped: {refused}"
    );
}

#[test]
fn a_value_already_given_is_never_asked_again() {
    let journey = run("already_given");
    let (typed, added, frustrated) = (journey.step(1), journey.step(2), journey.step(3));
    assert_eq!(waiting(typed), "consent", "{typed:#}");
    assert_eq!(
        provenance(typed, DIGEST),
        ("answered".to_owned(), "u2".to_owned()),
        "a path typed without `./` is the output path the person gave"
    );
    // The author asked for the output again; the Session answered it with the settled value.
    assert_ne!(outcome_kind(added), "question", "{added:#}");
    assert!(open_questions(added).is_empty(), "{:#}", work(added));
    assert_eq!(waiting(added), "consent", "{added:#}");
    let settled = journey.agent[5].last();
    assert!(
        settled.contains("news/digest.md"),
        "the refused ask returns the settled value to the author: {settled}"
    );
    let source = candidate(added);
    assert!(
        source.contains(TECHCRUNCH) && source.contains(DIGEST),
        "{source}"
    );
    // « I already told you »: reconciled from what the Session holds, nothing rebound.
    assert!(reached_the_author(&journey, ALREADY));
    assert_ne!(outcome_kind(frustrated), "question", "{frustrated:#}");
    assert_eq!(proposal(frustrated), proposal(added), "{frustrated:#}");
    assert!(
        rows(frustrated, "bindings").iter().all(|row| row["value"]
            .as_str()
            .is_none_or(|v| !v.contains("deja dit"))),
        "{:#}",
        work(frustrated)
    );
}

#[test]
fn a_reopened_session_keeps_selections_and_provenance_and_renews_identities() {
    let journey = run("reopen");
    let (before, reopened, after) = (journey.step(1), journey.step(2), journey.step(3));
    assert_eq!(waiting(before), "consent", "{before:#}");
    for value in [HACKER_NEWS, TECHCRUNCH, DIGEST] {
        assert_eq!(
            provenance(reopened, value),
            ("offered".to_owned(), "u2".to_owned()),
            "{value} survives the reopen as evidence"
        );
    }
    assert!(
        proposal(reopened).is_none(),
        "a proposal never regains authority across a reopen: {reopened:#}"
    );
    assert_ne!(outcome_kind(after), "question", "{after:#}");
    assert_eq!(waiting(after), "consent", "{after:#}");
    assert_ne!(proposal(after), proposal(before), "the identity is renewed");
    let source = candidate(after);
    for value in [HACKER_NEWS, TECHCRUNCH, LE_MONDE, DIGEST] {
        assert!(source.contains(value), "{value}:\n{source}");
    }
    // The author's first request after the reopen carries the conversation it continues.
    let resumed = &journey.agent[4];
    for value in [ACCEPT, HACKER_NEWS, DIGEST] {
        assert!(
            resumed.text().contains(value),
            "{value} in the reconstructed context"
        );
    }
}

#[test]
fn a_question_about_the_question_is_answered_and_the_offer_is_renewed() {
    let journey = run("question_about_the_question");
    let (first, explained) = (journey.step(0), journey.step(1));
    assert!(
        matches!(waiting(first), "question" | "questions"),
        "{first:#}"
    );
    assert!(reached_the_author(&journey, WHY));
    assert!(
        step_text(explained).contains("ne nomme ni les sources"),
        "the explanation is shown: {explained:#}"
    );
    assert!(
        matches!(waiting(explained), "question" | "questions"),
        "{explained:#}"
    );
    let offered = recommended_values(explained);
    for value in [HACKER_NEWS, TECHCRUNCH, DIGEST] {
        assert!(offered.iter().any(|v| v == value), "{value}: {offered:?}");
    }
    assert_ne!(
        first["shown"]["question_id"], explained["shown"]["question_id"],
        "the renewed offer is another question"
    );
    let earlier = journey.step(2);
    assert_eq!(earlier["outcome"]["kind"], "refusal", "{earlier:#}");
    assert!(
        matches!(
            earlier["outcome"]["class"].as_str(),
            Some("already_consumed" | "stale_revision")
        ),
        "{earlier:#}"
    );
}

fn step_text(step: &Value) -> String {
    format!(
        "{} {}",
        step["outcome"]["text"].as_str().unwrap_or_default(),
        work(step)
    )
}

#[test]
fn independent_questions_are_asked_together_and_valid_answers_survive() {
    let journey = run("grouped_questions");
    let (asked, partly) = (journey.step(0), journey.step(1));
    let open = open_questions(asked);
    let keys: Vec<&str> = open.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        ["team_webhook", "support_webhook"],
        "the two independent questions together: {:#}",
        work(asked)
    );
    assert_ne!(open[0].1, open[1].1, "each question has its own identity");
    let held = rows(asked, "questions")
        .into_iter()
        .find(|q| q["key"] == "support_token")
        .expect("the dependent question is known");
    assert_eq!(
        held["state"], "after",
        "it waits for its prerequisite: {held:#}"
    );
    assert_eq!(held["after"][0], "support_webhook");
    // One answer given, one missing: the valid one is bound by its question, the other asked.
    assert_eq!(
        provenance(partly, TEAM_HOOK),
        ("answered".to_owned(), "u2".to_owned())
    );
    assert_eq!(binding(partly, TEAM_HOOK).unwrap()["key"], "team_webhook");
    let still = open_questions(partly);
    let keys: Vec<&str> = still.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["support_webhook"], "{:#}", work(partly));
    assert_ne!(
        still[0].1, open[1].1,
        "a question asked again is a new identity, never a positional reuse"
    );
}

#[test]
fn a_stale_screen_an_answered_question_and_another_session_answer_nothing() {
    let journey = run("stale_answers");
    let (first, corrected, stale, elsewhere) = (
        journey.step(0),
        journey.step(1),
        journey.step(2),
        journey.step(3),
    );
    assert!(
        matches!(waiting(first), "question" | "questions"),
        "{first:#}"
    );
    assert!(
        recommended_values(corrected).iter().any(|v| v == LE_MONDE),
        "the correction renews the offer: {corrected:#}"
    );
    assert_ne!(
        first["shown"]["question_id"],
        corrected["shown"]["question_id"]
    );
    assert_eq!(stale["outcome"]["kind"], "refusal", "{stale:#}");
    assert_eq!(stale["outcome"]["class"], "stale_revision", "{stale:#}");
    assert_eq!(
        stale["shown"]["question_id"], corrected["shown"]["question_id"],
        "the question that waits keeps waiting"
    );
    assert_eq!(
        journey.agent.len(),
        2,
        "the stale line never reached the author"
    );
    assert_eq!(elsewhere["outcome"]["kind"], "refusal", "{elsewhere:#}");
    assert!(
        matches!(
            elsewhere["outcome"]["class"].as_str(),
            Some("wrong_state" | "stale_revision")
        ),
        "{elsewhere:#}"
    );
}

#[test]
fn a_complete_replacement_retains_nothing_of_the_former_request() {
    let journey = run("complete_replacement");
    let (before, replaced) = (journey.step(1), journey.step(2));
    assert_eq!(waiting(before), "consent", "{before:#}");
    assert_eq!(waiting(replaced), "consent", "{replaced:#}");
    assert_ne!(proposal(replaced), proposal(before));
    let source = candidate(replaced);
    assert!(
        source.contains("./data/ventes.csv") && !source.contains("nika:fetch"),
        "{source}"
    );
    for value in [HACKER_NEWS, TECHCRUNCH, DIGEST] {
        assert!(
            binding(replaced, value).is_none(),
            "{value} belonged to the former request: {:#}",
            work(replaced)
        );
    }
    assert!(rows(replaced, "delegations").is_empty());
}

#[test]
fn save_and_run_said_in_words_cover_the_candidate_they_designate() {
    let journey = run("save_and_run_in_words");
    let said = journey.step(2);
    assert_eq!(outcome_kind(said), "run_requested", "{said:#}");
    assert_ne!(waiting(said), "consent", "no second confirmation: {said:#}");
    let saved = work(said)["saved"]["workflow"].as_str().expect("saved");
    let files = journey.report["files"].as_array().unwrap();
    assert!(
        files.iter().any(|f| f.as_str() == Some(saved)),
        "{saved} landed: {files:?}"
    );
}

#[test]
fn save_and_run_words_never_cover_a_candidate_whose_effects_changed() {
    let journey = run("save_and_run_scope_changed");
    let (before, said) = (journey.step(1), journey.step(2));
    assert_ne!(outcome_kind(said), "run_requested", "{said:#}");
    assert!(work(said)["saved"].is_null(), "nothing saved: {said:#}");
    assert!(work(said)["requested"].is_null(), "nothing run: {said:#}");
    let files = journey.report["files"].as_array().unwrap();
    assert!(
        files.iter().all(|f| !is_workflow(f.as_str().unwrap())),
        "{files:?}"
    );
    assert_eq!(
        waiting(said),
        "consent",
        "the changed candidate asks its own consent"
    );
    assert_ne!(proposal(said), proposal(before));
}
