// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/traces")
        .join(name)
}

/// A scratch directory removed on drop: the `tempfile::TempDir` shape
/// these tests were written against, without a dev-dependency this
/// crate does not carry (its other suites use `std::env::temp_dir()`
/// the same way). One name per test, one directory per process.
struct Scratch(PathBuf);

impl Scratch {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tempdir(test: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("nika-run-view-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp");
    Scratch(dir)
}

/// A real trace of the deterministic copy (engine 0.120.3): two
/// invoke tasks, four permit decisions, a seal — the result names what
/// was produced and read from the permit frames, and the cost is
/// honestly « nothing metered ».
#[test]
fn the_copy_trace_reads_as_a_result() {
    let facts = RunFacts::read(&fixture("copy.ndjson")).expect("frames");
    assert_eq!(facts.status.as_deref(), Some("succeeded"));
    assert_eq!(facts.events, 13);
    assert_eq!(facts.tasks.len(), 2);
    assert!(facts.tasks.iter().all(|t| t.state == TaskState::Ok));
    assert_eq!(facts.permits.len(), 4);
    assert!(facts.permits.iter().all(|p| p.decision == "allow"));
    let seal = facts.seal.as_ref().expect("sealed");
    assert!(seal.head.starts_with("5b8e2720"), "{seal:?}");
    assert_eq!(
        (seal.declared, seal.exercised, seal.escapes),
        (Some(2), Some(2), Some(0))
    );
    let root = tempdir("copy-result");
    std::fs::create_dir_all(root.path().join("out")).expect("out");
    std::fs::write(
        root.path().join("out/copie.md"),
        "# Brief\n\nLe lancement passe en octobre.\n",
    )
    .expect("artefact");
    let view = facts.result(root.path(), Path::new("copy.nika"));
    assert!(
        view.starts_with("Done · `copy.nika` · 11 ms · 2 tasks ran"),
        "{view}"
    );
    assert!(
        view.contains("\n  produced · ./out/copie.md (40 B)"),
        "{view}"
    );
    assert!(view.contains("\n  read · ./notes/brief.md"), "{view}");
    assert!(view.contains("cost · no model usage recorded"), "{view}");
    assert!(
        view.contains("`/proof` shows the records and their limits"),
        "{view}"
    );
    assert!(!view.contains("approved"), "no approval happened: {view}");
}

/// A real paused trace: one task ran, the gate frame carries the
/// question; the gate view says what ran, asks, and names what a yes
/// lets happen from the list the workflow's bytes gave.
#[test]
fn a_paused_trace_reads_as_a_gate() {
    let facts = RunFacts::read(&fixture("paused.ndjson")).expect("frames");
    let pause = facts.pause.as_ref().expect("a pause");
    assert_eq!(pause.task, "approve");
    assert_eq!(pause.mode, "confirm");
    assert_eq!(pause.message, "Write the copy to ./out/copie.md?");
    assert!(facts.status.is_none(), "a paused run has no terminal word");
    let gated = vec!["write_output · nika:write".to_owned()];
    let view = facts.gate(Path::new("gated.nika"), &pause.message, &pause.mode, &gated);
    assert!(
        view.starts_with("Paused · `gated.nika` asks you before it goes on"),
        "{view}"
    );
    assert!(
        view.contains("\n  so far · read_source · invoke · nika:read (3 ms)"),
        "{view}"
    );
    assert!(
        view.contains("\n  « Write the copy to ./out/copie.md? »"),
        "{view}"
    );
    assert!(
        view.contains("\n  a yes lets happen · write_output · nika:write"),
        "{view}"
    );
    assert!(
        view.contains("a no ends the run there") && view.contains("answer yes or no"),
        "{view}"
    );
    assert!(view.contains("nothing answers for you"), "{view}");
}

/// The real resume of that pause: the pre-gate task replays from the
/// cache, the approval frame says the answer came through the resume,
/// the write ran — the result says approved, counts the cache hit.
#[test]
fn a_resumed_trace_names_the_approval_and_the_cache() {
    let facts = RunFacts::read(&fixture("resumed.ndjson")).expect("frames");
    assert_eq!(facts.status.as_deref(), Some("succeeded"));
    assert_eq!(facts.approvals.len(), 1);
    assert_eq!(facts.approvals[0].decision, "allow");
    assert_eq!(facts.approvals[0].source, "resume");
    let read = facts
        .tasks
        .iter()
        .find(|t| t.id == "read_source")
        .expect("read_source");
    assert_eq!(read.state, TaskState::CacheHit);
    let root = tempdir("resumed-result");
    let view = facts.result(root.path(), Path::new("gated.nika"));
    assert!(
        view.starts_with("Done · `gated.nika` · 49 ms · 2 tasks ran (1 from cache)"),
        "{view}"
    );
    assert!(
        view.contains(
            "\n  approved · your answer let it go on · `approve` · answered in this session"
        ),
        "{view}"
    );
    assert!(
        view.contains("\n  produced · ./out/copie.md\n"),
        "the file is not on this disk: no size · {view}"
    );
}

/// `/proof` judges the chain through the verify door (never a second
/// walker), names the workflow's two identities, the boundary the seal
/// covers, and says what it does not prove. The seal tier depends on
/// the machine's key custody and is not asserted.
#[test]
fn the_proof_reads_the_chain_through_the_verify_door() {
    let facts = RunFacts::read(&fixture("resumed.ndjson")).expect("frames");
    let root = tempdir("proof");
    std::fs::create_dir_all(root.path().join("out")).expect("out");
    std::fs::write(
        root.path().join("out/copie.md"),
        "# Brief\n\nLe lancement passe en octobre.\n",
    )
    .expect("artefact");
    let view = facts.proof(root.path());
    assert!(view.starts_with("Proof · "), "{view}");
    assert!(view.contains("what this journal records"), "{view}");
    assert!(
        view.contains("\n  records · task outcomes")
            && view.contains("not a file checksum")
            && view.contains("\n  trust · ")
            && !view.contains("proves ·")
            && !view.contains("the digests of every input and output"),
        "records, never a claim the journal does not carry: {view}"
    );
    assert!(
        view.contains(
            "\n  workflow · gated-copy · bytes sha256 00448c33…6530 · meaning bbbd59cf…9cbd"
        ),
        "{view}"
    );
    assert!(
        view.contains("\n  chain · OK — 15 events · chain intact · head 09ea39d4…2a50"),
        "the judge's own words, the head shortened for the eye: {view}"
    );
    // Three, not five: the resumed run replayed `read_source` from the
    // cache, so its two permit checks were never re-decided.
    assert!(
        view.contains("\n  boundary · 3 effect(s) declared · 3 exercised · 0 escaped · 3 permit check(s) · 3 allowed · 0 denied"),
        "{view}"
    );
    assert!(
        view.contains("\n  written · ./out/copie.md · 40 B · sha256 ") && view.contains("RE-READ"),
        "{view}"
    );
    assert!(
        view.contains("\n  approval · `approve` · allow · source resume"),
        "{view}"
    );
    assert!(
        view.contains("engine · 0.120.3 · sandbox seatbelt"),
        "{view}"
    );
    assert!(
        view.contains("does not prove · that the content is right"),
        "{view}"
    );
}

/// The copy journal staged under a project root, rewritten by `edit`:
/// the proof names it from the root, and its trust follows the verify
/// door's typed verdict. Unsealed, it says the chain alone cannot tell
/// who wrote it; broken, it attests nothing.
fn staged_proof(test: &str, edit: &dyn Fn(Vec<&str>) -> Vec<String>) -> String {
    let root = tempdir(test);
    let traces = root.path().join(".nika/traces");
    std::fs::create_dir_all(&traces).expect("traces");
    let copy = std::fs::read_to_string(fixture("copy.ndjson")).expect("fixture");
    let lines = edit(copy.lines().collect());
    let trace = traces.join("staged.ndjson");
    std::fs::write(&trace, lines.join("\n") + "\n").expect("staged");
    let facts = RunFacts::read(&trace).expect("frames");
    facts.proof(root.path())
}

#[test]
fn an_unsealed_journal_never_claims_who_wrote_it() {
    let view = staged_proof("unsealed", &|lines| {
        let keep = lines.len() - 1; // the final `run_sealed` frame is dropped
        lines[..keep].iter().map(|l| (*l).to_owned()).collect()
    });
    assert!(
        view.starts_with("Proof · `.nika/traces/staged.ndjson` · what this journal records"),
        "the path is the project's own: {view}"
    );
    assert!(
        view.contains("\n  trust · unsigned: the hash chain is internally consistent")
            && view.contains("a journal rewritten end to end would chain too")
            && view.contains("who wrote this journal (`nika sign` signs future runs)"),
        "{view}"
    );
    assert!(!view.contains("trusts the key"), "no key exists: {view}");
}

#[test]
fn a_broken_journal_attests_none_of_its_records() {
    let view = staged_proof("broken", &|lines| {
        lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                if i == 4 {
                    l.replace("\"allow\"", "\"deny\"")
                } else {
                    (*l).to_owned()
                }
            })
            .collect()
    });
    assert!(
        view.contains("\n  trust · the journal does not verify: these records are not attested")
            && view.contains("\n  does not prove · any of the records above"),
        "{view}"
    );
    assert!(!view.contains("trust · unsigned"), "{view}");
}

/// A failed task (a synthetic journal in the engine's frame shape):
/// the view names the task, its detail, what ran before, what never ran.
#[test]
fn a_failed_task_reads_as_a_failure() {
    let dir = tempdir("failed");
    let trace = dir.path().join("failed.ndjson");
    std::fs::write(
        &trace,
        concat!(
            "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"draft\"}]}\n",
            "{\"kind\":\"task_scheduled\",\"fields\":[{\"key\":\"task\",\"value\":\"read\"}]}\n",
            "{\"kind\":\"task_scheduled\",\"fields\":[{\"key\":\"task\",\"value\":\"draft\"}]}\n",
            "{\"kind\":\"task_scheduled\",\"fields\":[{\"key\":\"task\",\"value\":\"write\"}]}\n",
            "{\"kind\":\"task_started\",\"fields\":[{\"key\":\"task\",\"value\":\"read\"},{\"key\":\"note\",\"value\":\"invoke · nika:read\"}]}\n",
            "{\"kind\":\"task_completed\",\"fields\":[{\"key\":\"task\",\"value\":\"read\"},{\"key\":\"duration_ms\",\"value\":2}]}\n",
            "{\"kind\":\"task_started\",\"fields\":[{\"key\":\"task\",\"value\":\"draft\"},{\"key\":\"note\",\"value\":\"infer · mock/echo\"}]}\n",
            "{\"kind\":\"task_failed\",\"fields\":[{\"key\":\"task\",\"value\":\"draft\"},{\"key\":\"detail\",\"value\":\"NIKA-PROVIDER-002 the route refused\\nsecond line\"}]}\n",
            "{\"kind\":\"workflow_failed\",\"fields\":[{\"key\":\"workflow\",\"value\":\"draft\"},{\"key\":\"priced_calls\",\"value\":0},{\"key\":\"unpriced_calls\",\"value\":1}]}\n",
        ),
    )
    .expect("trace");
    let facts = RunFacts::read(&trace).expect("frames");
    assert_eq!(facts.status.as_deref(), Some("failed"));
    let view = facts.result(dir.path(), Path::new("draft.nika"));
    assert!(
        view.starts_with(
            "Failed · `draft.nika` · `draft` failed\n  NIKA-PROVIDER-002 the route refused\n"
        ),
        "{view}"
    );
    assert!(view.contains("\n  ran before it · read (2 ms)"), "{view}");
    assert!(view.contains("\n  never ran · write"), "{view}");
    assert!(
        view.contains("cost · UNKNOWN · 1 unpriced call(s): a route with no price table"),
        "an unpriced call is never free: {view}"
    );
}

/// A file that is not a journal is no reading at all: the door's own
/// observation line stands alone.
#[test]
fn a_non_journal_is_not_read() {
    let dir = tempdir("non-journal");
    let path = dir.path().join("not.ndjson");
    std::fs::write(&path, "hello\n{\"no\":\"kind\"}\n").expect("file");
    assert!(RunFacts::read(&path).is_none());
    assert!(RunFacts::read(&dir.path().join("absent.ndjson")).is_none());
}

#[test]
fn the_human_units_read() {
    assert_eq!(human_ms(11), "11 ms");
    assert_eq!(human_ms(1234), "1.2 s");
    assert_eq!(human_ms(125_000), "2 min 05 s");
    assert_eq!(
        shorten_hex(
            "OK — 13 events · chain intact · head 1cf484e5340c918ce64f6955ed0cd3a54dcaf44572a973eab3fb6e37d0147f01"
        ),
        "OK — 13 events · chain intact · head 1cf484e5…7f01"
    );
    assert_eq!(
        short("5d1bf5915e9fbee4b3d7df30fd517302f1aeab8a25e035a85987bc5f26090730"),
        "5d1bf591…0730"
    );
    assert_eq!(short("abc"), "abc");
}

#[test]
fn model_traffic_and_a_network_permit_do_not_prove_delivery_or_an_invoice() {
    let root = tempdir("delivery-scope");
    let mut facts = RunFacts::read(&fixture("copy.ndjson")).expect("frames");
    facts.tasks.push(super::TaskFact {
        id: "summary".to_owned(),
        note: "infer · deepseek/deepseek-flash".to_owned(),
        state: TaskState::Ok,
        ..Default::default()
    });
    facts.priced_calls = Some(1);
    facts.total_cost_usd = Some(0.0001);
    let view = facts.result(root.path(), Path::new("summary.nika"));
    assert!(view.contains("asked · deepseek/deepseek-flash"), "{view}");
    assert!(!view.contains("nothing sent elsewhere"), "{view}");
    assert!(
        view.contains("recorded estimate, invoice not verified"),
        "{view}"
    );
    facts.permits.push(super::PermitFact {
        task: "send".to_owned(),
        plane: "net".to_owned(),
        gate: "example.invalid".to_owned(),
        decision: "allow".to_owned(),
    });
    let view = facts.result(root.path(), Path::new("summary.nika"));
    assert!(view.contains("permission, not delivery proof"), "{view}");
    assert!(!view.contains("\n  sent ·"), "{view}");
}

/// A journal of these frames, one per line, in a scratch directory.
fn journal(test: &str, frames: &[&str]) -> (Scratch, PathBuf) {
    let root = tempdir(test);
    let path = root.path().join("run.ndjson");
    std::fs::write(&path, frames.join("\n") + "\n").expect("journal");
    (root, path)
}

fn paused(fields: &str) -> String {
    format!("{{\"kind\":\"workflow_paused\",\"fields\":[{fields}]}}")
}

/// C9 · the gate a host answers is the FIRST pause's, each key's first value; the result and
/// proof views keep the last pause they always read.
#[test]
fn the_gate_is_the_first_pause_while_the_views_keep_the_last() {
    let first = paused(
        r#"{"key":"task","value":"approve"},{"key":"message","value":"ship it?"},{"key":"mode","value":"confirm"},{"key":"task","value":"later"}"#,
    );
    let second = paused(r#"{"key":"task","value":"again"},{"key":"message","value":"sure?"}"#);
    let (_root, path) = journal("gate-two", &[first.as_str(), second.as_str()]);
    let facts = RunFacts::read(&path).expect("frames");
    assert_eq!(facts.pause_gate(), Some(("approve", "ship it?", "confirm")));
    assert_eq!(
        facts.pause.as_ref().map(|p| p.task.as_str()),
        Some("again"),
        "the views read the last pause"
    );
}

/// C9 · a first pause gives its defaults; one naming no task (absent, not text, empty) asks no
/// gate, and a later pause never stands in for it; a journal that never paused asks none.
#[test]
fn a_first_pause_without_a_task_asks_no_gate() {
    let bare = paused(r#"{"key":"task","value":"ask"}"#);
    let (_bare, path) = journal("gate-defaults", &[bare.as_str()]);
    assert_eq!(
        RunFacts::read(&path).expect("frames").pause_gate(),
        Some(("ask", "the run awaits your answer", "text"))
    );
    let later = paused(r#"{"key":"task","value":"later"}"#);
    for first in [
        paused(r#"{"key":"message","value":"no task"}"#),
        paused(r#"{"key":"task","value":""}"#),
        paused(r#"{"key":"task","value":7}"#),
    ] {
        let (_none, path) = journal("gate-none", &[first.as_str(), later.as_str()]);
        assert_eq!(
            RunFacts::read(&path).expect("frames").pause_gate(),
            None,
            "{first}"
        );
    }
    let (_run, path) = journal(
        "gate-unpaused",
        &[r#"{"kind":"workflow_started","fields":[]}"#],
    );
    assert_eq!(RunFacts::read(&path).expect("frames").pause_gate(), None);
}

/// A real journal frame of `kind` (the engine's own envelope) with these fields.
fn frame(kind: &str, fields: &str) -> String {
    format!(
        r#"{{"id":{{"uuid":"01a0e819-b68b-7649-85b2-39277be74e66"}},"timestamp":1790600394379000000,"kind":"{kind}","execution":{{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"}},"run":null,"correlation":null,"fields":[{fields}]}}"#
    )
}

/// A completion with its resume identity and the output text it journaled.
fn keyed(task: &str, output: &str) -> String {
    frame(
        "task_completed",
        &format!(
            r#"{{"key":"task","value":"{task}"}},{{"key":"def_hash","value":"{}"}},{{"key":"input_hash","value":"{}"}},{{"key":"output","value":{}}}"#,
            "d".repeat(64),
            "e".repeat(64),
            serde_json::to_string(output).expect("text")
        ),
    )
}

/// C10 · Q8 · a resume runs again, live, exactly the completions its own fold cannot serve
/// back — no resume identity, or an output that does not read back — each named once in
/// journal order, in the gate view and the line a host says before the answer; a journal
/// whose plan carries every completion says nothing.
#[test]
fn a_resume_names_the_completions_it_would_run_again_live() {
    let started = frame(
        "workflow_started",
        r#"{"key":"workflow","value":"gate-keyed"}"#,
    );
    let served = keyed("served", "\"kept\"");
    let keyless = frame("task_completed", r#"{"key":"task","value":"keyless"}"#);
    let unreadable = keyed("unreadable", "not json");
    let pause = frame(
        "workflow_paused",
        r#"{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"message","value":"Ship it?"}"#,
    );
    let frames = [&started, &served, &keyless, &unreadable, &keyless, &pause].map(String::as_str);
    let (_mixed, path) = journal("live-again", &frames);
    assert_eq!(resumed_live(&path), ["keyless", "unreadable"]);
    let line = live_again(&path).expect("named before the answer");
    assert!(
        line.contains("run again, live · keyless · unreadable"),
        "{line}"
    );
    let facts = RunFacts::read(&path).expect("frames");
    let view = facts.gate(Path::new("flow.nika"), "Ship it?", "confirm", &[]);
    assert!(view.contains(&line), "{view}");

    let frames = [&started, &served, &pause].map(String::as_str);
    let (_served, path) = journal("live-served", &frames);
    assert!(
        resumed_live(&path).is_empty(),
        "the plan carries every completion"
    );
    assert_eq!(live_again(&path), None, "no warning for zero tasks");
    let facts = RunFacts::read(&path).expect("frames");
    let view = facts.gate(Path::new("flow.nika"), "Ship it?", "confirm", &[]);
    assert!(!view.contains("run again"), "{view}");
    assert!(resumed_live(&path.with_file_name("absent.ndjson")).is_empty());
}
