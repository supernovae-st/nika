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

#[path = "open_ux_journeys/byte_change_journeys.rs"]
mod byte_change_journeys;
#[path = "open_ux_journeys/child.rs"]
mod child;
#[path = "open_ux_journeys/dropped_question_journeys.rs"]
mod dropped_question_journeys;
#[path = "open_ux_journeys/failing_check_journeys.rs"]
mod failing_check_journeys;
#[path = "open_ux_journeys/feedback_journeys.rs"]
mod feedback_journeys;
#[path = "open_ux_journeys/fragments.rs"]
mod fragments;
#[path = "open_ux_journeys/kept_default_journeys.rs"]
mod kept_default_journeys;
#[path = "open_ux_journeys/peer.rs"]
mod peer;
#[path = "open_ux_journeys/proposal_choices.rs"]
mod proposal_choices;
#[path = "open_ux_journeys/requested_file_journeys.rs"]
mod requested_file_journeys;
#[path = "open_ux_journeys/run_ceiling_journeys.rs"]
mod run_ceiling_journeys;
#[path = "open_ux_journeys/scenarios.rs"]
mod scenarios;
#[path = "open_ux_journeys/schedule_journeys.rs"]
mod schedule_journeys;
#[path = "open_ux_journeys/session_fact_journeys.rs"]
mod session_fact_journeys;
#[path = "open_ux_journeys/two_output_journeys.rs"]
mod two_output_journeys;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use peer::{Kind, Peer, Seen};
use scenarios::{
    ACCEPT, ALGOLIA, ALREADY, CODE, CODE_REQUEST, DEFLECT, DELEGATION, DIGEST, EMPTY,
    FIX_AFTER_DISCARD, FOLLOWED, HACKER_NEWS, LE_MONDE, LE_MONDE_WORDS, OUTPUT_WORDS, RENAMED,
    REPAIRING, REQUEST, SAVE_AND_RUN, STEER_B, STEERED, STILL_HERE, STILL_THERE, TEAM_HOOK,
    TECHCRUNCH, THEN_C, THEN_STOP, TRIED_ON, UNRESOLVED, WHY, YC_BLOG, scenario,
};
use serde_json::{Value, json};

/// One scenario's report, and the requests the peer received while the child drove it.
struct Journey {
    report: Value,
    agent: Vec<Seen>,
    /// Every request the peer received, in order.
    seen: Vec<Seen>,
    dir: tempfile::TempDir,
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
    let markers = dir.path().join("markers");
    std::fs::create_dir_all(&markers).unwrap();
    let peer = Peer::start(scenario.script, &markers);
    // The suite's own jq helper: this binary's `jq_helper`, its answer on descriptor 3.
    let helper = dir.path().join("jq-helper");
    let exe = std::env::current_exe().unwrap();
    let wrapper = format!(
        "#!/bin/sh\nexec /usr/bin/env OPEN_UX_JQ=1 '{}' --exact jq_helper --ignored --test-threads=1 -q 3>&1 1>/dev/null 2>/dev/null\n",
        exe.display()
    );
    std::fs::write(&helper, wrapper).unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
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
        // The person's machine is at UTC+4: the author is told its zone.
        .env("TZ", "Asia/Dubai")
        .env("NIKA_VLLM_BASE_URL", peer.base())
        .env("OPEN_UX_CHILD", name)
        .env("OPEN_UX_ROOT", &root)
        .env("OPEN_UX_REPORT", &report)
        .env("OPEN_UX_MARKERS", &markers)
        .env("OPEN_UX_JQ_HELPER", &helper)
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
    let (agent, seen) = (peer.agent(), peer.seen());
    peer.shutdown();
    Journey {
        report,
        agent,
        seen,
        dir,
    }
}

/// The suite's own jq helper, as `nika __nika-jq-eval` serves one evaluation: the engine's
/// evaluator over the framed request on stdin, its framed answer on descriptor 3 (the wrapper
/// sends libtest's own words elsewhere).
#[test]
#[ignore = "run by the journeys' jq helper wrapper"]
fn jq_helper() {
    if std::env::var_os("OPEN_UX_JQ").is_none() {
        return;
    }
    let mut answer = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/fd/3")
        .unwrap();
    let mut request = std::io::stdin().lock();
    nika_builtin::data::jq_serve(&mut request, &mut answer, 4 * 1024 * 1024).unwrap();
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
    let markers = PathBuf::from(std::env::var_os("OPEN_UX_MARKERS").unwrap());
    let report = child::drive(&name.to_string_lossy(), &root, &home, &markers);
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

/// The concrete values every option of a waiting question carries.
fn offered_values(step: &Value) -> Vec<String> {
    (rows(step, "questions").into_iter())
        .flat_map(|q| q["options"].as_array().into_iter().flatten())
        .flat_map(|option| option["values"].as_array().into_iter().flatten())
        .filter_map(|value| value["value"].as_str().map(str::to_owned))
        .collect()
}

/// The concrete values the recommended option of a waiting question, or of a choice the
/// proposal shows, carries.
fn recommended_values(step: &Value) -> Vec<String> {
    (rows(step, "questions").into_iter())
        .chain(rows(step, "open_choices"))
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
    assert_eq!(
        waiting(asked),
        "consent",
        "the request is met at once with one concrete proposal: {asked:#}"
    );
    assert!(open_questions(asked).is_empty(), "nothing asked first");
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
    // The proposal that line answered is no longer the one shown: a yes to it answers nothing.
    let again = journey.step(2);
    assert_eq!(again["outcome"]["kind"], "refusal", "{again:#}");
    assert!(
        matches!(
            again["outcome"]["class"].as_str(),
            Some("already_consumed" | "stale_revision")
        ),
        "{again:#}"
    );
}

/// `/meaning` beside a proposal an intelligence wrote reads what the Session kept of the
/// request, in the person's French: their words, each value the candidate binds with its part
/// and origin. It never shows the money card in its place (live gauntlet r08, 2026-10-16), asks
/// no model and leaves the proposal waiting for its consent.
#[test]
fn meaning_beside_an_authored_proposal_reads_the_request_and_its_values() {
    let journey = run("meaning_of_a_proposal");
    let said = journey.step(2);
    let text = said["outcome"]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("Ce que j'ai retenu de votre demande"),
        "{said:#}"
    );
    assert!(
        text.contains(REQUEST),
        "the request in the person's words: {text}"
    );
    for value in [HACKER_NEWS, TECHCRUNCH, DIGEST] {
        let row = (text.lines()).find(|line| line.contains(value));
        let row = row.unwrap_or_else(|| panic!("{value} is listed: {text}"));
        assert!(
            row.contains("de la proposition que vous avez acceptée"),
            "{value} comes from the accepted proposal: {row}"
        );
    }
    assert!(!text.contains("money:"), "never the money card: {text}");
    assert_eq!(
        waiting(said),
        "consent",
        "the proposal still waits: {said:#}"
    );
    assert!(
        !reached_the_author(&journey, "/meaning"),
        "the Session answers from its own records"
    );
}

/// A revision discarded over a saved workflow is gone: the author's next turn is told so, its
/// candidate is the saved workflow as it stands, and no second workflow ever appears (live
/// gauntlet r08, 2026-10-16: the discarded draft came back under another name).
#[test]
fn a_discarded_revision_leaves_the_saved_workflow_to_revise() {
    let journey = run("discard_then_fix");
    let saved = work(journey.step(2))["saved"]["workflow"]
        .as_str()
        .expect("the accepted digest was saved")
        .to_owned();
    let discarded = journey.step(4);
    assert_eq!(outcome_kind(discarded), "facts", "{discarded:#}");
    assert!(
        step_text(discarded).contains("écarté · rien n'a été écrit"),
        "said in French: {discarded:#}"
    );
    let fix = (journey.agent.iter().rev())
        .find(|seen| seen.text().contains(FIX_AFTER_DISCARD))
        .expect("the fix reached the author");
    let text = fix.text();
    assert!(
        text.contains("The person discarded the proposal of revision"),
        "the author learns of the discard: {text}"
    );
    assert!(
        text.contains("the saved workflow, as it stands"),
        "the candidate it reads is the saved workflow: {text}"
    );
    let files = journey.report["files"].as_array().unwrap();
    let workflows: Vec<&str> = (files.iter())
        .filter_map(Value::as_str)
        .filter(|f| is_workflow(f))
        .collect();
    assert_eq!(
        workflows,
        [saved.as_str()],
        "one workflow, the saved one: {files:?}"
    );
}

/// A proposal of the saved workflow's very bytes leaves nothing to decide: the person reads that
/// nothing changed and how to run it, no proposal waits, and nothing is written again (NIK-196:
/// the saved workflow came back with Save, Save and run once, and Discard).
#[test]
fn a_proposal_of_the_saved_bytes_is_said_unchanged_never_decided() {
    let journey = run("saved_bytes_again");
    let saved = work(journey.step(2))["saved"]["workflow"]
        .as_str()
        .expect("the accepted digest was saved")
        .to_owned();
    let again = journey.step(3);
    assert_eq!(outcome_kind(again), "facts", "{again:#}");
    let text = step_text(again);
    assert!(
        text.contains(&format!(
            "Rien n'a changé : `{saved}` est enregistré tel quel"
        )),
        "{text}"
    );
    assert!(text.contains("« lance-le »"), "the next gesture: {text}");
    assert_ne!(waiting(again), "consent", "nothing to decide: {again:#}");
    assert!(work(again)["candidate"].is_null(), "no proposal: {again:#}");
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
    let settled = journey.agent[4].last();
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
    let offered = offered_values(explained);
    for value in [HACKER_NEWS, TECHCRUNCH] {
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
        offered_values(corrected).iter().any(|v| v == LE_MONDE),
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
    // The words consent to the proposal they saw: its unchanged bytes are not judged again.
    let consent = (journey.seen.iter())
        .position(|seen| seen.kind == Kind::Agent && seen.text().contains(SAVE_AND_RUN))
        .expect("the consent reached the author");
    let judged = (journey.seen[consent..].iter()).filter(|seen| seen.kind == Kind::Judge);
    assert_eq!(judged.count(), 0, "no second verdict for the bytes shown");
}

/// A real line written after the proposal is not consent because it is cited as one: the person
/// asked for an explanation, so an author claiming it authorizes saving and running gets no act,
/// and the proposal waits for the person's consent.
#[test]
fn a_request_for_an_explanation_cited_as_consent_authorizes_nothing() {
    let journey = run("explanation_cited_as_consent");
    let said = journey.step(2);
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
        "the proposal waits for the person's consent"
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

/// One line queued for the run under way, as a receipt or the work shows it.
fn queued(id: &str, mode: &str, line: &str, state: &str) -> Value {
    json!({"id": id, "mode": mode, "line": line, "state": state})
}

/// The text of a reply step (a note may follow the author's words).
fn reply(step: &Value) -> &str {
    assert_eq!(outcome_kind(step), "reply", "{step:#}");
    step["outcome"]["text"].as_str().unwrap_or_default()
}

/// Whether a request ends with the person's line `line`, cited as `cite`.
fn cites(seen: &Seen, line: &str, cite: &str) -> bool {
    let last = seen.last();
    last.contains(line) && last.contains(&format!("(cited as {cite})"))
}

#[test]
fn a_line_steered_into_the_run_enters_after_the_calls_under_way() {
    let journey = run("steer_mid_run");
    let step = journey.step(0);
    let waiting = queued("l1", "steer", STEER_B, "waiting");
    assert_eq!(step["receipt"], json!({"queued": waiting}));
    assert!(reply(step).starts_with(STEERED), "{step:#}");
    let mut entered = queued("l1", "steer", STEER_B, "entered");
    entered["cite"] = json!("u2");
    assert_eq!(work(step)["queued"], json!([entered]));
    // Both calls ran: the one answered once the line arrived hands no turn over,
    // and the author read the line, cited, right after that call's reply.
    assert_eq!(journey.agent.len(), 3, "{:#?}", journey.agent);
    assert!(
        journey.agent[..2]
            .iter()
            .all(|s| !s.text().contains(STEER_B))
    );
    let next = &journey.agent[2];
    assert!(cites(next, STEER_B, "u2"), "{}", next.last());
    let messages = next.body["messages"].as_array().unwrap();
    let ran = &messages[messages.len() - 2];
    assert_eq!(ran["role"], "tool", "{messages:#?}");
    assert_eq!(ran["tool_call_id"], "call_2", "{messages:#?}");
    let said = serde_json::to_string(&ran["content"]).unwrap();
    assert!(said.contains("offers"), "{said}");
    let steps = json!([
        ["call_1", "models", "started", false],
        ["call_1", "models", "finished", true],
        ["call_2", "models", "started", false],
        ["call_2", "models", "finished", true]
    ]);
    assert_eq!(journey.report["tools"], steps, "both calls ran");
    let late = json!({"refused": "not_reading"});
    assert_eq!(journey.report["after_turn"], late);
}

#[test]
fn a_follow_up_line_waits_for_the_end_of_the_run() {
    let journey = run("follow_up_after_run");
    let step = journey.step(0);
    let waiting = queued("l1", "follow_up", THEN_C, "waiting");
    assert_eq!(step["receipt"], json!({"queued": waiting}));
    assert!(reply(step).starts_with(FOLLOWED), "{step:#}");
    let mut entered = queued("l1", "follow_up", THEN_C, "entered");
    entered["cite"] = json!("u2");
    assert_eq!(work(step)["queued"], json!([entered]));
    // The failed call did not end the run; its answer did, and only then the line entered.
    assert_eq!(journey.agent.len(), 3, "{:#?}", journey.agent);
    assert!(!journey.agent[1].text().contains(THEN_C));
    let last = &journey.agent[2];
    assert!(cites(last, THEN_C, "u2"), "{}", last.last());
    let steps = json!([
        ["call_1", "models", "started", false],
        ["call_1", "models", "failed", true]
    ]);
    assert_eq!(journey.report["tools"], steps);
}

#[test]
fn stop_drops_the_request_under_way_and_returns_the_queued_line() {
    let journey = run("stop_mid_request");
    let step = journey.step(0);
    let waiting = queued("l1", "follow_up", THEN_STOP, "waiting");
    assert_eq!(step["receipt"], json!({"queued": waiting}));
    let returned = json!([queued("l1", "follow_up", THEN_STOP, "returned")]);
    let text = format!(
        "stopped by you · the request under way was dropped; a request already sent may still \
         be billed · the conversation and its draft are kept · not sent: « {THEN_STOP} »"
    );
    let stopped = json!({"kind": "stopped", "reach": "request_dropped", "unsent": returned,
        "candidate": null, "text": text});
    assert_eq!(step["outcome"], stopped, "{step:#}");
    assert_eq!(work(step)["queued"], returned);
    // The next line goes on, cited u2: the unsent line never became the person's line.
    let next = journey.step(1);
    assert!(reply(next).starts_with(STILL_HERE), "{next:#}");
    assert_eq!(journey.agent.len(), 2, "{:#?}", journey.agent);
    let last = &journey.agent[1];
    assert!(cites(last, STILL_THERE, "u2"), "{}", last.last());
    assert!(
        !reached_the_author(&journey, THEN_STOP),
        "the unsent line never reached the author"
    );
    assert_eq!(journey.report["tools"], json!([]));
}

/// The text of a step's outcome.
fn said(step: &Value) -> &str {
    step["outcome"]["text"].as_str().unwrap_or_default()
}

/// A refusal picks no offer, and the author's claim that the person accepted one binds
/// nothing.
#[test]
fn a_refusal_binds_no_offer_the_author_claims() {
    let journey = run("refused_offer");
    let refused = journey.step(1);
    assert_ne!(outcome_kind(refused), "proposal", "{refused:#}");
    assert!(rows(refused, "bindings").is_empty(), "{:#}", work(refused));
    let answer = &journey.agent[1];
    assert!(
        answer.text().contains("declined the offers"),
        "{}",
        answer.text()
    );
    let claim = &journey.agent[2];
    assert!(
        claim.last().contains("declined the offers"),
        "{}",
        claim.last()
    );
}

/// A deflection takes the recommended offer, delegated and visible.
#[test]
fn a_deflection_takes_the_recommendation_visibly() {
    let journey = run("deflection_takes_the_recommendation");
    assert_eq!(outcome_kind(journey.step(0)), "proposal");
    let proposed = journey.step(1);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    let hn = binding(proposed, HACKER_NEWS).expect("Hacker News is bound");
    let delegated = json!({"kind": "delegated", "message": "u2", "excerpt": DEFLECT,
        "question": "yc_source", "option": "hackernews"});
    assert_eq!(hn["provenance"], delegated, "{hn:#}");
    assert!(
        binding(proposed, YC_BLOG).is_none(),
        "never the offer not picked"
    );
    let delegations = rows(proposed, "delegations");
    assert!(
        delegations
            .iter()
            .any(|d| d["message"] == "u2" && d["scope"] == "read_source"),
        "{delegations:?}"
    );
    let read = &journey.agent[2];
    assert!(
        read.text().contains("recommended `hackernews`"),
        "{}",
        read.text()
    );
}

/// A value left to the author is chosen once and never asked again.
#[test]
fn a_name_left_to_the_author_is_never_asked_again() {
    let journey = run("delegated_name");
    assert_eq!(outcome_kind(journey.step(2)), "question");
    let named = journey.step(3);
    assert_eq!(outcome_kind(named), "proposal", "{named:#}");
    let refused =
        (journey.agent.iter()).any(|s| s.last().contains(r#""delegated":{"message":"u4"}"#));
    assert!(refused, "the second `ask` was refused, not asked");
    assert_eq!(
        provenance(named, RENAMED),
        ("derived".to_owned(), "u4".to_owned())
    );
    let delegations = rows(named, "delegations");
    assert!(
        delegations
            .iter()
            .any(|d| d["message"] == "u4" && d["scope"] == "output_path"),
        "{delegations:?}"
    );
    let changed = format!("output `{DIGEST}` replaced by `{RENAMED}`");
    assert!(said(named).contains(&changed), "{}", said(named));
}

/// One `candidate_edit` carries several edits, each read in the original candidate, and
/// lands them as one revision.
#[test]
fn two_edits_in_one_call_make_one_revision() {
    let journey = run("two_edits");
    let proposed = journey.step(0);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    let shown = candidate(proposed);
    assert!(shown.contains("nika: stories-of-the-day"), "the name edit");
    assert!(shown.contains("startswith"), "the filter edit");
    let edited = (journey.agent.iter()).any(|s| s.last().contains(r#""revision":2"#));
    assert!(edited, "two edits, one revision");
}

/// An author's own filters, programs, globs and rights state no selection and are never asked.
/// The author reads it in its instructions and in `candidate_write`, and its own jq filter,
/// stated nowhere, is proposed at once.
#[test]
fn an_authors_own_filter_is_proposed_without_a_question() {
    let journey = run("own_filter");
    let proposed = journey.step(0);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    let first = &journey.agent[0];
    let told = first.text();
    assert!(told.contains("states no selection"), "told: own code");
    assert!(!told.contains("Every value of a document"), "old rule gone");
    let write = (first.body["tools"].as_array().into_iter().flatten())
        .find(|tool| tool["function"]["name"] == "candidate_write")
        .map(|tool| tool["function"]["description"].to_string())
        .unwrap_or_default();
    assert!(write.contains("state none"), "candidate_write says it too");
}

/// « Today » removes no source, and a replacement in words
/// that name the source is said under the proposal.
#[test]
fn a_source_is_replaced_only_by_words_that_name_it_and_the_change_is_said() {
    let journey = run("source_swap");
    assert_ne!(outcome_kind(journey.step(2)), "proposal");
    let unnamed = format!("does not name `{HACKER_NEWS}`");
    let refused = (journey.agent.iter()).any(|s| s.last().contains(&unnamed));
    assert!(refused, "« today » removed nothing");
    let swapped = journey.step(3);
    assert_eq!(outcome_kind(swapped), "proposal", "{swapped:#}");
    let changed = format!(
        "Changé depuis la proposition vue : source `{HACKER_NEWS}` replaced by `{ALGOLIA}`"
    );
    assert!(said(swapped).contains(&changed), "{}", said(swapped));
}

/// The folder typed at a proposal is a revision; the card never files it as
/// money input.
#[test]
fn a_folder_line_at_a_proposal_is_never_money_input() {
    let journey = run("folder_line");
    let moved = journey.step(2);
    assert_eq!(outcome_kind(moved), "proposal", "{moved:#}");
    // Said in the person's French (live gauntlet wave 6).
    assert!(!said(moved).contains("montant indiqué"), "{}", said(moved));
    let request = format!("demande : «{REQUEST}»");
    assert!(said(moved).contains(&request), "{}", said(moved));
}

/// A failed Run goes back to its author at once with the run's facts, said in the person's
/// language; the repaired revision never runs on the words that ran the failed one, and runs on
/// new words.
#[test]
fn a_failed_run_is_repaired_and_runs_again_only_on_new_words() {
    let journey = run("failed_run_repaired");
    assert_eq!(outcome_kind(journey.step(2)), "run_requested");
    let repaired = journey.step(3);
    assert_eq!(outcome_kind(repaired), "proposal", "{repaired:#}");
    let text = said(repaired);
    assert!(
        text.starts_with("Échoué · `") && text.contains("`summarize` a échoué"),
        "{text}"
    );
    assert!(text.contains("NIKA-INFER-001"), "{text}");
    let note = (journey.agent.iter())
        .find(|s| s.text().contains("The run the person asked for failed"))
        .expect("the author read the run's facts");
    assert!(
        note.text().contains("Nika (not the person):") && note.text().contains("NIKA-INFER-001"),
        "{}",
        note.text()
    );
    // The words that ran the failed revision authorize nothing more: the repair waits for the
    // person's consent. Shown, the proposal ended the turn, and the author reads the person's
    // next line in place of the refusal.
    assert_eq!(waiting(repaired), "consent", "{repaired:#}");
    let again = journey.step(4);
    assert_eq!(outcome_kind(again), "run_requested", "{again:#}");
}

/// A complaint about a successful Run reaches the author
/// with the run's facts, and the repair runs on those words.
#[test]
fn a_complaint_about_a_run_reaches_the_author_with_its_facts() {
    let journey = run("complaint_after_success");
    assert_eq!(
        outcome_kind(journey.step(3)),
        "facts",
        "{:#}",
        journey.step(3)
    );
    let complaint = (journey.agent.iter())
        .find(|s| s.last().contains(EMPTY))
        .expect("the complaint reached the author");
    let last = complaint.last();
    assert!(
        last.contains("The last run the person asked for:") && last.contains("Terminé ·"),
        "{last}"
    );
    let again = journey.step(4);
    assert_eq!(outcome_kind(again), "run_requested", "{again:#}");
}

/// The verifier on the conversation's candidate: a judge that rejects the request as a
/// whole and locates nothing holds it, so nothing is proposed and the author repairs from the
/// findings; the same bytes verified again ask the judge nothing (R6).
#[test]
fn a_candidate_its_judge_doubts_goes_back_to_the_author_and_is_not_asked_again() {
    let journey = run("doubted_candidate");
    let step = journey.step(0);
    assert_eq!(step["outcome"]["kind"], "reply", "{step}");
    assert_eq!(step["outcome"]["text"], REPAIRING, "{step}");
    assert!(step["shown"]["proposal"].is_null(), "{step}");
    // The author read the held verdict after `propose`, then again after `verify`.
    let held: Vec<usize> = (journey.seen.iter().enumerate())
        .filter(|(_, seen)| seen.kind == Kind::Agent && seen.last().contains(UNRESOLVED))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(held.len(), 2, "{held:?}");
    let judged = |from: usize, to: usize| {
        (journey.seen[from..to].iter())
            .filter(|seen| seen.kind == Kind::Judge)
            .count()
    };
    assert!(judged(0, held[0]) > 0, "the judge was asked");
    assert_eq!(
        judged(held[0], held[1]),
        0,
        "the same bytes ask the judge nothing"
    );
}

/// The author's request after its `n`-th tool call returned: what that call answered.
fn answered(journey: &Journey, n: usize) -> String {
    journey.agent.get(n).map(Seen::last).unwrap_or_default()
}

/// A candidate is tried before it is proposed, on the pages its sources answered. A jq
/// step that refuses a date without an offset fails the trial: nothing is proposed, the author
/// reads the trial's facts (the task, its code, its message) and proposes the repaired
/// revision, whose trial passes and binds to the proposal that shows what it ran on.
#[test]
fn a_trial_catches_a_date_without_an_offset_before_any_proposal() {
    let journey = run("trial_offset");
    let step = journey.step(0);
    assert_eq!(step["outcome"]["kind"], "proposal", "{step}");
    let failed = answered(&journey, 2);
    assert!(
        failed.contains("run_failure") && failed.contains("\"today\""),
        "{failed}"
    );
    assert!(
        !failed.contains("shown to the person"),
        "nothing was proposed: {failed}"
    );
    let text = step["outcome"]["text"].as_str().unwrap();
    assert!(text.contains(TRIED_ON), "{text}");
    assert_eq!(
        step["shown"]["work"]["candidate"]["rehearsed"], true,
        "{step}"
    );
}

/// A filter that keeps nothing of the items an RFC 3339 feed gives
/// fails the trial with what came in, so the author repairs it before any proposal.
#[test]
fn a_trial_catches_a_filter_that_keeps_nothing_before_any_proposal() {
    let journey = run("trial_kept_nothing");
    let step = journey.step(0);
    assert_eq!(step["outcome"]["kind"], "proposal", "{step}");
    let failed = answered(&journey, 2);
    assert!(failed.contains("NIKA-REHEARSAL-EMPTY"), "{failed}");
    assert!(
        failed.contains("2 items in from stories.items, 0 out"),
        "{failed}"
    );
    assert!(
        failed.contains("published: string (2026-10-10T08:00:00+00:00)"),
        "{failed}"
    );
    let text = step["outcome"]["text"].as_str().unwrap();
    assert!(text.contains(TRIED_ON), "{text}");
}

/// What a trial does not run is said to the person: the model step and the POST.
#[test]
fn a_trial_names_the_steps_it_did_not_run() {
    let journey = run("trial_names_what_it_skips");
    let step = journey.step(0);
    assert_eq!(step["outcome"]["kind"], "proposal", "{step}");
    let text = step["outcome"]["text"].as_str().unwrap();
    assert!(text.contains(TRIED_ON), "{text}");
    let skipped = "pas lancé : l'étape du modèle (summarize) · une requête autre que GET (send) · tout effet hors de la copie";
    assert!(text.contains(skipped), "{text}");
    // The candidate a host draws says it in the same words (live gauntlet wave 6 r04 B6: the
    // card read the proof's English lines beside a French preview).
    let drawn = step["shown"]["rehearsed"].as_str().unwrap_or_default();
    assert!(
        drawn.contains(skipped) && !drawn.contains("not run:"),
        "{drawn}"
    );
}

/// An agent-led proposal that reads a project file is saved after the person's yes; its
/// sources are judged by the exact bytes it was shown over, never by a compile it never had.
#[test]
fn an_agent_proposal_that_reads_a_project_file_is_saved_after_the_yes() {
    let journey = run("reads_a_project_file");
    assert_eq!(journey.step(0)["outcome"]["kind"], "proposal");
    let yes = journey.step(1);
    assert_ne!(yes["outcome"]["kind"], "refusal", "{yes}");
    let text = yes["outcome"]["text"].as_str().unwrap();
    assert!(
        text.contains("les octets exacts montrés tiennent toujours pour `./data/ventes.csv`"),
        "{text}"
    );
    let files = journey.report["files"].as_array().unwrap();
    assert!(
        files.iter().any(|f| f.as_str().is_some_and(is_workflow)),
        "{files:?}"
    );
}

/// A project file changed after the proposal withdraws it at the yes, as stale.
#[test]
fn a_file_changed_after_the_proposal_withdraws_it_at_the_yes() {
    let journey = run("a_read_file_changes");
    let yes = journey.step(2);
    assert_eq!(yes["outcome"]["kind"], "refusal", "{yes}");
    let text = yes["outcome"]["text"].as_str().unwrap();
    // The trial's proof judges first (the rehearsed world moved), else the read basis, in the
    // conversation's language.
    let moved = text
        .contains("les fichiers sur lesquels cette proposition a été répétée ont changé")
        || text.contains(
            "les fichiers que lit cette proposition ont changé depuis qu'elle a été montrée",
        );
    assert!(
        moved && text.contains("ventes.csv") && text.contains("est retirée"),
        "{text}"
    );
    let files = journey.report["files"].as_array().unwrap();
    assert!(
        !files.iter().any(|f| f.as_str().is_some_and(is_workflow)),
        "{files:?}"
    );
}

/// NIK-16 change 1: a write's reply carries Nika's check findings and its laws, so the author
/// repairs an invented path at once and proposes with no `check` or `verify` round trip.
#[test]
fn a_write_reply_carries_its_findings_and_the_repaired_write_proposes() {
    let journey = run("write_carries_its_findings");
    assert_eq!(
        journey.step(0)["outcome"]["kind"],
        "proposal",
        "{}",
        journey.step(0)
    );
    let invented = answered(&journey, 1);
    assert!(invented.contains("INVENTED LITERAL"), "{invented}");
    let repaired = answered(&journey, 2);
    assert!(repaired.contains("\"findings\":[]"), "{repaired}");
    let names: Vec<String> = journey.agent.iter().flat_map(Seen::called).collect();
    assert!(
        !names.iter().any(|n| n == "check" || n == "verify"),
        "{names:?}"
    );
}

/// A model that only reasons is asked once more, in Nika's words; silent again, the turn says
/// so instead of ending in an empty reply (a reasoning model's thought-only turns).
#[test]
fn a_thought_only_answer_never_ends_a_turn_in_silence() {
    let journey = run("thought_only");
    let said = reply(journey.step(0));
    assert!(said.contains("ended this turn without a word"), "{said}");
    assert_eq!(journey.agent.len(), 2, "{:#?}", journey.agent);
    let asked = journey.agent[1].last();
    assert!(asked.contains("no words and no call"), "{asked}");
}

/// On a French news request, the first turn ends with a proposal that shows its
/// open choices with their defaults, and no question; the person's « oui » keeps every default,
/// bound as the option picked with that line, and saves the workflow.
#[test]
fn a_news_request_is_proposed_at_once_and_a_yes_keeps_the_defaults_shown() {
    let journey = run("open_defaults");
    let (proposed, saved) = (journey.step(0), journey.step(1));
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    assert!(open_questions(proposed).is_empty(), "nothing asked first");
    let card = proposed["outcome"]["text"].as_str().unwrap_or_default();
    assert!(card.contains("open choices"), "{card}");
    assert!(card.contains("`hn` Hacker News (default): https://news.ycombinator.com"));
    assert!(card.contains("`notes` notes/: ./notes/digest.md"), "{card}");
    let shown = work(proposed)["open_choices"]
        .as_array()
        .map_or(0, Vec::len);
    assert_eq!(shown, 3, "the machine door shows them");
    assert!(work(saved)["saved"]["workflow"].is_string(), "{saved:#}");
    for value in [HACKER_NEWS, DIGEST] {
        assert_eq!(provenance(saved, value), ("offered".into(), "u2".into()));
    }
    assert_eq!(journey.agent.len(), 2, "the consent reached no author");
}

/// A pick of another option binds it and the proposal shown waits for the author's
/// revision: the « oui » that follows is held, and nothing is saved.
#[test]
fn a_pick_of_another_option_holds_the_proposal_until_a_revision() {
    let journey = run("open_pick_other");
    let (picked, held) = (journey.step(1), journey.step(2));
    assert_eq!(provenance(picked, YC_BLOG), ("offered".into(), "u2".into()));
    assert_eq!(outcome_kind(held), "held", "{held:#}");
    let why = held["outcome"]["text"].as_str().unwrap_or_default();
    assert!(why.contains("picked `blog`"), "{why}");
    assert!(work(held)["saved"].is_null(), "nothing saved");
}

/// « Today » is the person's day: the author is told the machine's IANA zone (here UTC+4) in
/// the instructions a conversation starts with.
#[test]
fn the_author_is_told_the_persons_time_zone() {
    let journey = run("own_filter");
    let told = journey.agent[0].text();
    assert!(
        told.contains("The person's time zone is `Asia/Dubai`"),
        "the zone is named"
    );
}

/// The first JSON value of a tool's reply (a note may follow it).
fn reply_json(text: &str) -> Value {
    let mut values = serde_json::Deserializer::from_str(text).into_iter::<Value>();
    values.next().and_then(Result::ok).unwrap_or_default()
}

/// A question the author can recommend is never put to the person: the ask door refuses it with
/// its typed reason, and the person sees the proposal with the choice open and its default.
#[test]
fn a_choice_with_a_default_is_proposed_not_asked() {
    let journey = run("default_not_asked");
    let shown = journey.step(0);
    assert_eq!(outcome_kind(shown), "proposal", "{shown:#}");
    assert!(
        open_questions(shown).is_empty(),
        "the person sees no question"
    );
    let card = shown["outcome"]["text"].as_str().unwrap_or_default();
    assert!(card.contains("open choices"), "{card}");
    let open = recommended_values(shown);
    for value in [HACKER_NEWS, TECHCRUNCH, DIGEST] {
        assert!(open.iter().any(|v| v == value), "{value}: {open:?}");
    }
    let reply = reply_json(&answered(&journey, 1));
    let refused = reply["refused"].as_array().cloned().unwrap_or_default();
    assert_eq!(refused.len(), 1, "the ask's one question is refused");
    assert_eq!(
        (refused[0]["key"].as_str(), refused[0]["reason"].as_str()),
        (Some("plan"), Some("output"))
    );
    let why = refused[0]["why"].as_str().unwrap_or_default();
    assert!(
        why.contains("always has a name you can derive"),
        "the author reads why it is not asked"
    );
}

/// A property the person states (newest first), checked by an unconditional `nika:assert` over a
/// task's output whose message quotes their words, before the write: the trial stops at the
/// assertion the candidate breaks and names its task before any proposal, and the repaired
/// revision is proposed.
#[test]
fn a_trial_catches_a_failed_assertion_before_any_proposal() {
    let journey = run("trial_assert");
    let step = journey.step(0);
    assert_eq!(step["outcome"]["kind"], "proposal", "{step}");
    let failed = answered(&journey, 2);
    assert!(
        failed.contains("run_failure") && failed.contains("\"ordered\""),
        "{failed}"
    );
    assert!(
        !failed.contains("shown to the person"),
        "nothing was proposed: {failed}"
    );
}

/// The state the verifier's judge read: the user turn, `STATE:` before `OPTIONS:`.
fn judge_state(seen: &Seen) -> Option<Value> {
    let said = (seen.body["messages"].as_array().into_iter().flatten())
        .filter(|message| message["role"] == "user")
        .find_map(|message| message["content"].as_str()?.strip_prefix("STATE:\n"))?;
    let (state, _) = said.split_once("\n\nOPTIONS:\n")?;
    serde_json::from_str(state).ok()
}

/// The person's answer reaches the judge as theirs: a count over the customer code they typed is
/// judged with that answer and their request as first stated, so the code is no invented literal
/// and the candidate is proposed. The typed reply keeps its question: the judge reads the code they
/// bound, the line as they typed it, where it stands in their words, and the question it answers.
#[test]
fn an_answered_literal_is_judged_as_the_persons_answer() {
    let journey = run("answered_literal");
    let proposed = journey.step(1);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    let state = (journey.seen.iter())
        .filter(|seen| seen.kind == Kind::Judge)
        .find_map(judge_state)
        .expect("the judge read a state");
    let answer = &state["answers"]["client_code"];
    assert_eq!(
        answer["bound_values"], CODE,
        "the answer as the person gave it: {answer:#}"
    );
    let binding = json!({"key": "client_code", "role": "value", "value": CODE,
        "provenance": {"kind": "answered", "message": "u2", "excerpt": CODE}});
    assert_eq!(answer["bindings"], json!([binding]), "{answer:#}");
    let context = json!({"question_key": "client_code", "question": "Quel est ton code client ?",
        "human_citation": "u2", "human_lines": [1, 2], "human_reply": CODE, "role": "value",
        "selected_options": []});
    assert_eq!(answer["choice_context"], context, "{answer:#}");
    assert_eq!(
        state["original_request"], CODE_REQUEST,
        "the request as first stated"
    );
}

/// A line steered while the author prepares its proposal holds that proposal back: the person is
/// never shown the stale candidate, and the author proposes the revision the line asked for.
#[test]
fn a_line_steered_before_a_proposal_holds_it_back_and_the_revision_is_shown() {
    let journey = run("steer_before_proposal");
    let step = journey.step(0);
    assert_eq!(outcome_kind(step), "proposal", "{step:#}");
    let source = candidate(step);
    assert!(
        source.contains("./out/total.txt") && !source.contains("map(select(length > 0))"),
        "{source}"
    );
    let tools = journey.report["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let shown = (tools.iter())
        .filter(|tool| tool[1] == "propose" && tool[2] == "finished")
        .count();
    assert_eq!(shown, 1, "the held-back proposal never ran: {tools:#?}");
}

/// Words that consent to saving and running the revision the person saw never cover new bytes
/// with the same effects: the revised proposal waits for their consent, and nothing is saved or
/// run.
#[test]
fn save_and_run_words_never_cover_bytes_they_were_not_shown() {
    let journey = run("consent_bytes_changed");
    let (before, said) = (journey.step(1), journey.step(2));
    assert_ne!(outcome_kind(said), "run_requested", "{said:#}");
    assert!(work(said)["saved"].is_null(), "nothing saved: {said:#}");
    assert!(work(said)["requested"].is_null(), "nothing run: {said:#}");
    assert_eq!(
        waiting(said),
        "consent",
        "the new bytes ask their own consent"
    );
    assert_ne!(proposal(said), proposal(before));
}

/// Words that ask for a change, then the save and the run, cover the revision written in answer to
/// them: it is saved and run with no second yes, and the saved bytes are that revision's, not the
/// ones the person was shown before.
#[test]
fn a_change_then_run_line_runs_its_revision_without_a_second_yes() {
    let journey = run("change_then_run");
    let (shown, said) = (journey.step(1), journey.step(2));
    assert_eq!(outcome_kind(said), "run_requested", "{said:#}");
    assert_ne!(waiting(said), "consent", "no second yes: {said:#}");
    let saved = work(said)["saved"]["workflow"].as_str().expect("saved");
    let path = journey.dir.path().join("project").join(saved);
    let bytes = std::fs::read_to_string(path).expect("the saved workflow");
    assert!(
        bytes.contains("Résume en anglais"),
        "the revision the line asked for was saved"
    );
    let before = candidate(shown);
    assert!(
        before.contains("Résume en Markdown"),
        "the bytes shown before"
    );
    assert_ne!(bytes, before, "not the bytes shown before");
}

/// The asserted write the author's instructions show, built into a candidate as an author
/// following them writes it, is clean for `nika check` and proposed: its `after:` is the map the
/// strict parser reads, never the list it refuses.
#[test]
fn the_asserted_write_the_instructions_show_is_accepted() {
    let journey = run("taught_assert");
    let step = journey.step(0);
    assert_eq!(outcome_kind(step), "proposal", "{step:#}");
}

/// The author's instructions teach the language's own forms: every `after:` they show is a map,
/// none is a list, and each orders a workflow `nika check` accepts; no condition reads
/// `${{ tasks.… }}` outside `with:`; and the `ask` field `after` says it lists question keys,
/// unlike a task's `after:` map.
#[test]
fn the_instructions_teach_the_language_forms() {
    let text = include_str!("../assets/author_system.md");
    let lists: Vec<&str> = text.lines().filter(|l| l.contains("after: [")).collect();
    assert!(lists.is_empty(), "{lists:#?}");
    let shown: Vec<&str> = (text.split("after: { ").skip(1))
        .filter_map(|rest| rest.split(" }").next())
        .collect();
    assert!(shown.len() >= 2, "the after: map is shown: {shown:?}");
    for entry in shown {
        let (producer, outcome) = entry.split_once(": ").unwrap();
        let source = format!(
            "nika: after-form\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  {producer}:\n    invoke: {{ tool: \"nika:jq\", args: {{ input: 1, expression: \".\" }} }}\n  next:\n    after: {{ {producer}: {outcome} }}\n    invoke: {{ tool: \"nika:jq\", args: {{ input: 2, expression: \".\" }} }}\n"
        );
        let options = nika_cli_host::oracle::AuditOptions::new(None, None);
        let audit =
            nika_cli_host::oracle::audit_source(&source, "after-form.nika", None, None, options)
                .unwrap();
        assert!(
            audit.verdict.clean,
            "nika check refuses the form:\n{source}"
        );
    }
    let direct: Vec<&str> = (text.lines())
        .filter(|l| l.contains("condition") && l.contains("${{ tasks."))
        .collect();
    assert!(direct.is_empty(), "{direct:#?}");
    let tools: Value = serde_json::from_str(include_str!("../assets/author_tools.json")).unwrap();
    let after = &tools["$defs"]["question"]["properties"]["after"];
    let said = after["description"].as_str().unwrap_or_default();
    assert!(said.contains("question keys"), "{after:#}");
}

/// Choices are shaped for picking: native options among known alternatives (two to seven,
/// `multi_select` when several apply), a source labelled with its name then its address,
/// and exactly one recommended default in `open`. The assumptions-first rules stay whole: a
/// default is never asked, and `ask` recommends nothing.
#[test]
fn the_instructions_shape_choices_for_picking_and_keep_assumptions_first() {
    let text = include_str!("../assets/author_system.md");
    for taught in [
        "two to seven native `options`",
        "`multi_select: true` when several may apply",
        "« TechCrunch · techcrunch.com/feed »",
        "the choice goes in `open` with exactly one recommended option",
        "a question asked with `ask` recommends none",
        // The assumptions-first policy, unchanged.
        "You can recommend a value for it, as for most choices",
        "Ask, with `ask` and no recommended option, only what no default can stand for",
        "Nika refuses a question that recommends an option",
    ] {
        assert!(text.contains(taught), "missing: {taught}");
    }
    let tools: Value = serde_json::from_str(include_str!("../assets/author_tools.json")).unwrap();
    let ask = (tools["tools"].as_array().unwrap().iter())
        .find(|tool| tool["name"] == "ask")
        .unwrap();
    let said = ask["description"].as_str().unwrap();
    for taught in [
        "A question carries no recommended option",
        "two to seven native options",
        "(TechCrunch · techcrunch.com/feed)",
    ] {
        assert!(said.contains(taught), "ask misses {taught}: {said}");
    }
    let label = &tools["$defs"]["question"]["properties"]["options"]["items"]["properties"]["label"]
        ["description"];
    assert!(
        label
            .as_str()
            .unwrap_or_default()
            .contains("domain or feed address"),
        "{label}"
    );
}

/// An edit after a refused write names that refusal and its reason and asks for the whole
/// candidate again, since no candidate stands for the edit to change. Written again, the
/// candidate is proposed.
#[test]
fn an_edit_after_a_refused_write_names_the_refusal() {
    let journey = run("edit_after_refused_write");
    let edited = journey.agent[2].last();
    assert!(edited.contains("was refused"), "{edited}");
    assert!(edited.contains("never a list"), "{edited}");
    let step = journey.step(0);
    assert_eq!(outcome_kind(step), "proposal", "{step:#}");
}

/// A project file over the read cap: the reply says that offset and limit cannot help, since the
/// whole file is read before they apply, and what can.
#[test]
fn reading_an_oversized_file_says_offset_and_limit_cannot_help() {
    let journey = run("oversized_read");
    let refused = journey.agent[1].last();
    assert!(refused.contains("larger than 262144 bytes"), "{refused}");
    assert!(
        refused.contains("offset and limit cannot help"),
        "{refused}"
    );
}

/// A turn that would end in words over a candidate the author wrote and never proposed is asked
/// once, in Nika's words, to end with a proposal, a question or a refusal: the author proposes it.
#[test]
fn words_over_an_unproposed_candidate_are_asked_once_to_end_the_turn() {
    let journey = run("written_not_proposed");
    let step = journey.step(0);
    assert_eq!(outcome_kind(step), "proposal", "{step:#}");
    assert_eq!(journey.agent.len(), 3, "asked once: {:#?}", journey.agent);
    let asked = journey.agent[2].last();
    assert!(asked.contains("not proposed"), "{asked}");
}
