// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as session_pty: this suite's WHOLE JOB is to drive the
// real binary through a PTY, because bare `nika` opens the session only
// on a terminal (ADR-125) — unreachable from every piped harness.
#![allow(clippy::disallowed_types)]

//! Product e2e (native P0 · one engine, first surface): a human launches
//! bare `nika`, describes work in words, and reaches a real result without
//! leaving the product — the ONE compiler reads the intent (deterministic,
//! keyless), the candidate is reviewed and accepted, the real check runs,
//! an explicit `run it` runs it through the SAME path as `nika run`, the
//! result is observed, a trace exists. Nothing here is a demo special
//! case: the intents are ordinary sentences, the worlds are ordinary
//! files, the compiler is the frozen product base.
//!
//! What this suite proves is that the organs are connected; what the
//! compiler makes of a sentence is the Arena's business.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{Eof, Expect};

type LoggedSession = Session<UnixProcess, LogStream<PtyStream, std::io::Stderr>>;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika")
}

/// A project root and a home whose kept choice is « no AI in this
/// conversation »: authoring stays deterministic, no model is contacted.
fn rig(tag: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let project = tempfile::Builder::new()
        .prefix(&format!("nika-product-{tag}-"))
        .tempdir()
        .expect("project dir");
    let home = tempfile::Builder::new()
        .prefix(&format!("nika-product-home-{tag}-"))
        .tempdir()
        .expect("home dir");
    std::fs::create_dir_all(home.path().join(".nika")).expect("home .nika");
    std::fs::write(
        home.path().join(".nika").join("session-intelligence.json"),
        "{\"kind\":{\"kind\":\"none\"},\"model\":null,\"chosen_at\":\"2026-09-20T00:00:00Z\"}",
    )
    .expect("kept choice");
    (project, home)
}

/// Bare `nika` on a PTY in `project`, with `home` as the home.
fn open_session(project: &Path, home: &Path) -> LoggedSession {
    let mut cmd = Command::new(bin());
    cmd.current_dir(project)
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("HOME", home)
        .env("NIKA_KEYCHAIN", "off");
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(120)));
    session
        .expect("What do you want to automate?")
        .expect("the session opens on the human's question");
    session.expect("nika ›").expect("the prompt");
    session
}

fn exit_code(session: &mut LoggedSession) -> i32 {
    match session.get_process_mut().wait().expect("wait") {
        WaitStatus::Exited(_, code) => code,
        other => panic!("unexpected wait status: {other:?}"),
    }
}

/// The candidate the compiler returns for this intent through its own
/// machine document — the parity oracle for the bytes the session landed.
fn compiled_candidate(project: &Path, intent: &str) -> String {
    let out = Command::new(bin())
        .current_dir(project)
        .env("NIKA_KEYCHAIN", "off")
        .args(["compile", intent, "--json", "--fresh"])
        .output()
        .expect("nika compile");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("compile document");
    assert_eq!(doc["status"], "ready", "{doc}");
    doc["candidate"].as_str().expect("candidate").to_owned()
}

fn traces(project: &Path) -> Vec<PathBuf> {
    let dir = project.join(".nika").join("traces");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ndjson"))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// How many times `task` completed across every trace of the project.
fn completions(project: &Path, task: &str) -> usize {
    traces(project)
        .iter()
        .map(|trace| {
            std::fs::read_to_string(trace)
                .expect("trace")
                .lines()
                .filter(|line| {
                    let event: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
                    // The engine's trace format: `kind` at the top, the task id
                    // among the event's `fields` (`{key: "task", value: id}`).
                    event["kind"] == "task_completed"
                        && event["fields"].as_array().is_some_and(|fields| {
                            fields
                                .iter()
                                .any(|f| f["key"] == "task" && f["value"] == task)
                        })
                })
                .count()
        })
        .sum()
}

/// Describe, review, accept, check, run, observe — one workflow from words
/// to a file, the session never leaving the product.
fn author_accept_run(session: &mut LoggedSession, intent: &str, yes: &str) {
    session.send_line(intent).expect("the intent");
    session
        .expect("Nika proposes `compiled-workflow.nika`:")
        .expect("the review opens with what Nika proposes");
    session
        .expect("human approval at run · none")
        .expect("the review states the approval fact");
    session
        .expect("check of these bytes · `compiled-workflow.nika` · clean")
        .expect("the review carries the check of the exact bytes");
    session.expect("apply? ›").expect("the consent prompt");
    session.send_line(yes).expect("consent");
    session
        .expect("applied · wrote `compiled-workflow.nika`")
        .expect("the exact bytes landed");
    session
        .expect("check · `compiled-workflow.nika` · clean")
        .expect("the real check ran on disk");
    session
        .expect("nika ›")
        .expect("back at the prompt: consent is never a run");
    session.send_line("run it").expect("the explicit run line");
    session
        .expect("running `compiled-workflow.nika` once · ceiling $0.25")
        .expect("the ceiling is announced");
    session
        .expect("run observed · exit 0")
        .expect("the actual result is observed");
    session.expect("nika ›").expect("the prompt again");
}

/// A · a simple fresh file transformation, asked in messy French.
#[test]
fn a_french_copy_goes_from_words_to_a_file_without_leaving_the_product() {
    let (project, home) = rig("copy-fr");
    let brief = "# Brief\n\nLe lancement passe en octobre. Budget : 12k.\n";
    std::fs::create_dir_all(project.path().join("notes")).expect("notes");
    std::fs::write(project.path().join("notes/brief.md"), brief).expect("brief");
    let intent = "Lis ./notes/brief.md et écris-le dans ./out/copie.md";

    let mut session = open_session(project.path(), home.path());
    author_accept_run(&mut session, intent, "oui");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);

    assert_eq!(
        std::fs::read_to_string(project.path().join("out/copie.md")).expect("the artefact"),
        brief,
        "the expected final artefact"
    );
    let landed = std::fs::read_to_string(project.path().join("compiled-workflow.nika"))
        .expect("the accepted workflow");
    assert_eq!(
        landed,
        compiled_candidate(project.path(), intent),
        "the bytes the session landed are the compiler's own candidate"
    );
    let tree: Vec<String> = walkdir(project.path());
    assert_eq!(
        traces(project.path()).len(),
        1,
        "one run, one trace · the project holds {tree:?}"
    );
    assert_eq!(completions(project.path(), "write_output"), 1);
}

/// Every file under `root`, relative, sorted — the whole world when an
/// assertion needs to name what is there.
fn walkdir(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// B · a graph-shaped multi-file task: fan out over a folder, fold, write.
#[test]
fn a_fan_out_over_a_folder_lands_one_combined_file() {
    let (project, home) = rig("fanout");
    std::fs::create_dir_all(project.path().join("rfc")).expect("rfc");
    std::fs::write(project.path().join("rfc/0001-alpha.md"), "ALPHA-BODY\n").expect("a");
    std::fs::write(project.path().join("rfc/0002-beta.md"), "BETA-BODY\n").expect("b");
    let intent = "Read every file in ./rfc/*.md and write them combined into ./all.md";

    let mut session = open_session(project.path(), home.path());
    session.send_line(intent).expect("the intent");
    session
        .expect("· nika:read · for each item")
        .expect("the review shows the fan-out from the parser");
    session.expect("apply? ›").expect("the consent prompt");
    session.send_line("yes").expect("consent");
    session
        .expect("applied · wrote `compiled-workflow.nika`")
        .expect("landed");
    session.expect("nika ›").expect("prompt");
    session.send_line("run it").expect("run");
    session
        .expect("run observed · exit 0")
        .expect("the run succeeded");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);

    let all = std::fs::read_to_string(project.path().join("all.md")).expect("the artefact");
    assert!(all.contains("ALPHA-BODY"), "{all}");
    assert!(all.contains("BETA-BODY"), "{all}");
    assert!(all.contains("## "), "one heading per source file: {all}");
    assert!(
        all.find("ALPHA-BODY") < all.find("BETA-BODY"),
        "sources fold in order: {all}"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("compiled-workflow.nika")).expect("landed"),
        compiled_candidate(project.path(), intent)
    );
}

/// C · a human-gated effect: the run reaches the gate, the human answers
/// in the product, the SAME run completes, the effect happens exactly
/// once. The gated workflow is a prepared fixture world (the frozen
/// compiler drops an approval clause from a free intent — an Arena
/// defect, reported, never hidden here).
#[test]
fn a_gated_effect_waits_for_the_human_and_happens_once() {
    let (project, home) = rig("gate");
    let draft = "the draft to publish\n";
    std::fs::write(project.path().join("draft.md"), draft).expect("draft");
    std::fs::write(
        project.path().join("approve.nika"),
        "nika: approve-then-write\npermits:\n  fs: { read: [\"./draft.md\"], write: [\"./final.md\"] }\n  tools: [\"nika:read\", \"nika:prompt\", \"nika:write\"]\ntasks:\n  read_draft:\n    invoke: { tool: \"nika:read\", args: { path: \"./draft.md\" } }\n  approve:\n    after: { read_draft: success }\n    invoke: { tool: \"nika:prompt\", args: { mode: confirm, message: \"Write final.md from the draft?\" } }\n  write_final:\n    after: { approve: success }\n    with: { go: \"${{ tasks.approve.output }}\", text: \"${{ tasks.read_draft.output }}\" }\n    when: \"${{ with.go == true }}\"\n    invoke: { tool: \"nika:write\", args: { path: \"./final.md\", content: \"${{ with.text }}\" } }\noutputs:\n  answer: ${{ tasks.approve.output }}\n",
    )
    .expect("gated workflow");

    let mut session = open_session(project.path(), home.path());
    session.send_line("run approve.nika").expect("the run line");
    session
        .expect("running `approve.nika` once · ceiling $0.25")
        .expect("the ceiling is announced");
    session
        .expect("Write final.md from the draft?")
        .expect("the gate's exact question reaches the human");
    assert!(
        !project.path().join("final.md").exists(),
        "nothing is written before the human answers"
    );
    session.send_line("y").expect("the human answers");
    session
        .expect("Done · `approve.nika`")
        .expect("the result leads with the outcome");
    session
        .expect("produced · ./final.md")
        .expect("the effect, from the permit frame");
    session
        .expect("approved · your answer let it go on · `approve`")
        .expect("the approval is a fact of the trace, never an inference");
    session
        .expect("run observed · exit 0")
        .expect("the same run completes");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);

    assert_eq!(
        std::fs::read_to_string(project.path().join("final.md")).expect("the effect"),
        draft
    );
    assert_eq!(
        completions(project.path(), "write_final"),
        1,
        "the effect happened exactly once across every leg"
    );
    assert_eq!(
        completions(project.path(), "read_draft"),
        1,
        "no earlier task ran twice"
    );
}

/// C' · the gated effect AUTHORED from words: an approval clause becomes a
/// `nika:prompt` gate the review names, consent lands it, the explicit run
/// asks the human on the terminal, the same run resumes, the effect
/// happens exactly once.
#[test]
fn an_approval_clause_in_words_becomes_a_gate_answered_in_the_product() {
    let (project, home) = rig("gate-words");
    let draft = "the draft to publish\n";
    std::fs::write(project.path().join("draft.md"), draft).expect("draft");
    let mut session = open_session(project.path(), home.path());
    session
        .send_line("Read ./draft.md, ask me to confirm before writing it to ./final.md")
        .expect("the intent");
    session
        .expect("human approval at run · `")
        .expect("the review names the gate the compiler kept");
    session.expect("apply? ›").expect("consent prompt");
    session.send_line("yes").expect("consent");
    session
        .expect("applied · wrote `compiled-workflow.nika`")
        .expect("landed");
    session.expect("nika ›").expect("prompt");
    assert!(
        !project.path().join("final.md").exists(),
        "consent is never a run"
    );
    session.send_line("run it").expect("run");
    session
        .expect("[y/N]")
        .expect("the gate asks the human on the terminal");
    assert!(
        !project.path().join("final.md").exists(),
        "nothing before the answer"
    );
    session.send_line("y").expect("the human answers");
    session
        .expect("run observed · exit 0")
        .expect("the same run completes");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert_eq!(
        std::fs::read_to_string(project.path().join("final.md")).expect("the effect"),
        draft
    );
    assert_eq!(
        completions(project.path(), "write_output"),
        1,
        "exactly once"
    );
    let landed =
        std::fs::read_to_string(project.path().join("compiled-workflow.nika")).expect("landed");
    assert!(landed.contains("nika:prompt"), "{landed}");
}

/// D · a draft answered with the offline mock: the same review, the same
/// consent, the same explicit run — and an honest failure observed (the
/// mock cannot satisfy the draft's anchor law), no artefact invented, the
/// trace named. A nice chat never turns a red run into a success.
#[test]
fn a_draft_with_the_mock_fails_honestly_and_invents_no_artefact() {
    let (project, home) = rig("draft-mock");
    std::fs::create_dir_all(project.path().join("notes")).expect("notes");
    std::fs::write(
        project.path().join("notes/brief.md"),
        "# Brief\n\nOctober.\n",
    )
    .expect("brief");
    let mut session = open_session(project.path(), home.path());
    session
        .send_line("Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md")
        .expect("the intent");
    session.expect("reply ›").expect("the model question");
    session.send_line("mock/echo").expect("the offline mock");
    session
        .expect("infer · mock/echo")
        .expect("the review names the model the draft runs on");
    session.expect("apply? ›").expect("consent prompt");
    session.send_line("yes").expect("consent");
    session
        .expect("applied · wrote `compiled-workflow.nika`")
        .expect("landed");
    session.expect("nika ›").expect("prompt");
    session.send_line("run it").expect("run");
    session
        .expect("run observed · exit 1 · the workflow failed")
        .expect("the failure is observed as a failure");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert!(
        !project.path().join("out/summary.md").exists(),
        "no artefact is invented on a red run"
    );
    assert_eq!(
        traces(project.path()).len(),
        1,
        "the red run left its trace"
    );
    assert_eq!(completions(project.path(), "write_output"), 0);
}

/// G · a named workflow and a ceiling in words: the run line names the
/// file and the ceiling the door announces is the one the human said.
#[test]
fn a_named_run_line_carries_its_own_ceiling() {
    let (project, home) = rig("named-run");
    std::fs::write(project.path().join("notes.md"), "hello\n").expect("notes");
    std::fs::write(
        project.path().join("copy.nika"),
        "nika: copy\npermits:\n  fs: { read: [\"./notes.md\"], write: [\"./copy.md\"] }\n  tools: [\"nika:read\", \"nika:write\"]\ntasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./notes.md\" } }\n  write:\n    with: { text: \"${{ tasks.read.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./copy.md\", content: \"${{ with.text }}\" } }\n",
    )
    .expect("workflow");
    let mut session = open_session(project.path(), home.path());
    session
        .send_line("run copy.nika with a ceiling of 0.05")
        .expect("the run line");
    session
        .expect("running `copy.nika` once · ceiling $0.05")
        .expect("the named file and the human's ceiling");
    session
        .expect("Done · `copy.nika`")
        .expect("the result leads with the outcome");
    session
        .expect("produced · ./copy.md (6 B)")
        .expect("what was produced, from the permit frame, with its size");
    session
        .expect("cost · nothing metered · no model was asked")
        .expect("the cost is honest");
    session.expect("run observed · exit 0").expect("succeeded");
    session.expect("nika ›").expect("prompt");
    session.send_line("/proof").expect("the proof door");
    session.expect("Proof · ").expect("the proof view opens");
    session
        .expect("chain · ")
        .expect("the chain verdict comes from the verify door");
    session
        .expect("written · ./copy.md · 6 B · sha256 ")
        .expect("the artefact re-read and digested");
    session
        .expect("does not prove · that the content is right")
        .expect("the limit is said");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert_eq!(
        std::fs::read_to_string(project.path().join("copy.md")).expect("artefact"),
        "hello\n"
    );
}

/// H · a workflow with declared inputs: the product asks for each required
/// value on its own prompt, then runs with them bound (no `--var`, no
/// launch refusal).
#[test]
fn a_run_with_declared_inputs_is_asked_for_them_in_the_product() {
    let (project, home) = rig("inputs");
    std::fs::write(
        project.path().join("greet.nika"),
        "nika: greet\ninputs:\n  name: { type: string, required: true }\npermits:\n  fs: { write: [\"./hello.txt\"] }\n  tools: [\"nika:write\"]\ntasks:\n  write:\n    invoke: { tool: \"nika:write\", args: { path: \"./hello.txt\", content: \"hello ${{ inputs.name }}\" } }\n",
    )
    .expect("workflow");
    let mut session = open_session(project.path(), home.path());
    session.send_line("run greet.nika").expect("the run line");
    session
        .expect("declares an input it needs before it runs: `name`")
        .expect("the input is asked in the product");
    session.expect("reply ›").expect("the input's own prompt");
    session.send_line("Thibaut").expect("the value");
    session
        .expect("running `greet.nika` once · ceiling $0.25")
        .expect("the run starts with the input bound");
    session.expect("run observed · exit 0").expect("succeeded");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert_eq!(
        std::fs::read_to_string(project.path().join("hello.txt")).expect("artefact"),
        "hello Thibaut"
    );
}

/// The `yes` invariant at the door: a bare `yes` with nothing pending
/// applies nothing, and a question's answer never crosses into a consent.
#[test]
fn a_bare_yes_with_nothing_pending_applies_nothing() {
    let (project, home) = rig("yes");
    let mut session = open_session(project.path(), home.path());
    session.send_line("yes").expect("a stray yes");
    session.expect("nika ›").expect("the prompt again");
    session
        .send_line("Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md")
        .expect("work that needs a model");
    session
        .expect("reply on the next line · `cancel` drops this · `why?` explains")
        .expect("the compiler's question, in its words, without its raw key");
    session
        .expect("reply ›")
        .expect("the question's own prompt");
    // A side question beside the question: answered from the machine's
    // state, the question still waits under its own prompt.
    session.send_line("why?").expect("a side question");
    session
        .expect("This answer fills `model`")
        .expect("the aside names the hole the value fills");
    session
        .expect("the question still waits")
        .expect("the aside consumed nothing");
    session
        .expect("reply ›")
        .expect("the question's prompt is back");
    session.send_line("cancel").expect("abandon");
    session
        .expect("authoring discarded")
        .expect("the round is dropped");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    let written: Vec<_> = std::fs::read_dir(project.path())
        .expect("root")
        .flatten()
        .map(|e| e.file_name())
        .filter(|n| Path::new(n).extension().is_some_and(|e| e == "nika"))
        .collect();
    assert!(written.is_empty(), "nothing was written: {written:?}");
}

/// The review reads in the sections a human decides on, `/meaning` lists
/// the request clause by clause and holds the proposal, and a `no` after
/// it discards: nothing is written.
#[test]
fn the_review_reads_in_sections_and_meaning_holds_the_proposal() {
    let (project, home) = rig("review");
    std::fs::create_dir_all(project.path().join("notes")).expect("notes");
    std::fs::write(project.path().join("notes/brief.md"), "# Brief\n").expect("brief");
    let mut session = open_session(project.path(), home.path());
    session
        .send_line("Lis ./notes/brief.md et écris-le dans ./out/copie.md")
        .expect("the intent");
    session
        .expect("Nika proposes `compiled-workflow.nika`:")
        .expect("the review opens");
    for section in [
        "Does",
        "Runs",
        "when you ask",
        "Can touch",
        "human approval at run · none",
        "Changes",
        "+ `compiled-workflow.nika`",
        "Needs",
        "nothing more from you",
        "Nothing has run yet",
    ] {
        session
            .expect(section)
            .unwrap_or_else(|e| panic!("the review names « {section} »: {e}"));
    }
    session.expect("apply? ›").expect("the consent prompt");
    session.send_line("/meaning").expect("meaning");
    session
        .expect("Meaning · your request, clause by clause")
        .expect("the meaning view");
    session
        .expect("écris-le dans ./out/copie.md")
        .expect("the clause, in the request's own words");
    session
        .expect("the proposal still waits")
        .expect("meaning holds the proposal");
    session
        .expect("apply? ›")
        .expect("still the consent prompt");
    session.send_line("no").expect("discard");
    session.expect("discarded").expect("nothing written");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert!(
        !project.path().join("compiled-workflow.nika").exists(),
        "a no writes nothing"
    );
}

/// A schedule stated in the request is saved beside the program and
/// activates nothing; « activate » asks the time zone, the missed policy
/// and the ceiling, proposes the `nika.yaml` declaration, a yes writes
/// it, and the machine's own `nika arm` reads one declared beat.
#[test]
fn a_stated_schedule_is_declared_only_when_activated() {
    let (project, home) = rig("activate");
    std::fs::create_dir_all(project.path().join("notes")).expect("notes");
    std::fs::write(project.path().join("notes/brief.md"), "# Brief\n").expect("brief");
    let mut session = open_session(project.path(), home.path());
    session
        .send_line("Chaque matin à 8h, lis ./notes/brief.md et écris-le dans ./out/copie.md")
        .expect("a scheduled intent");
    session.expect("Runs").expect("the Runs section");
    session
        .expect("a schedule to activate AFTER saving")
        .expect("saving does not activate");
    session.expect("apply? ›").expect("the consent prompt");
    session.send_line("oui").expect("save");
    session
        .expect("Saved · checked · not active · nothing has run")
        .expect("the three facts after a yes");
    session
        .expect("say « activate »")
        .expect("activation is its own gesture");
    session.expect("nika ›").expect("prompt");
    assert!(
        !project.path().join("nika.yaml").exists(),
        "saving the workflow declared nothing"
    );
    session.send_line("activate").expect("activate");
    session
        .expect("Which time zone")
        .expect("the first value the sentence did not state");
    session.expect("reply ›").expect("its own prompt");
    session.send_line("Europe/Paris").expect("zone");
    session
        .expect("If this machine is off")
        .expect("the missed policy");
    session.send_line("1").expect("run once when back");
    session
        .expect("ceiling per scheduled run")
        .expect("the ceiling");
    session.send_line("0.20").expect("ceiling");
    session
        .expect("Nika proposes to declare the schedule in `nika.yaml`")
        .expect("a proposal, not a write");
    session
        .expect("TZ=Europe/Paris 0 8 * * *")
        .expect("the cadence in the grammar's own form");
    session.expect("apply? ›").expect("consent prompt");
    assert!(
        !project.path().join("nika.yaml").exists(),
        "proposed, not written"
    );
    session.send_line("yes").expect("declare");
    session
        .expect("Declared in `nika.yaml` · not active")
        .expect("declared is not active");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    let arm = Command::new(bin())
        .arg("arm")
        .current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("NIKA_KEYCHAIN", "off")
        .output()
        .expect("nika arm");
    let text =
        String::from_utf8_lossy(&arm.stdout).into_owned() + &String::from_utf8_lossy(&arm.stderr);
    assert_eq!(
        arm.status.code(),
        Some(0),
        "the machine reads the declaration: {text}"
    );
    assert!(
        text.contains("1 beat") && text.contains("compiled-workflow.nika"),
        "one declared beat, the workflow named: {text}"
    );
}

/// I · `/quit` at the consent prompt leaves the product with nothing
/// written; the same world reopened says what it restored, and a bare
/// `yes` with nothing pending is refused, never compiled.
#[test]
fn quit_at_the_consent_prompt_leaves_and_a_stray_yes_is_refused() {
    let (project, home) = rig("quit-consent");
    std::fs::create_dir_all(project.path().join("notes")).expect("notes");
    std::fs::write(project.path().join("notes/brief.md"), "brief\n").expect("brief");
    let mut session = open_session(project.path(), home.path());
    session
        .send_line("Lis ./notes/brief.md et écris-le dans ./out/copie.md")
        .expect("the intent");
    session.expect("apply? ›").expect("the consent prompt");
    session
        .send_line("/quit")
        .expect("leave from the consent prompt");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert!(
        !project.path().join("compiled-workflow.nika").exists(),
        "leaving writes nothing"
    );
    let mut session = open_session(project.path(), home.path());
    session.send_line("yes").expect("a stray yes");
    session
        .expect("nothing waits for a yes or a no here")
        .expect("refused, never compiled");
    session.expect("nika ›").expect("prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert!(!project.path().join("compiled-workflow.nika").exists());
}
