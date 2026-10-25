// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

/// A paused leg leaves its ceiling less what it spent before the gate; a leg admitted under none
/// leaves none (live gauntlet r04 B8: the leg after a gate ran unbounded).
#[test]
fn a_paused_leg_leaves_its_ceiling_less_its_spend() {
    let run = r#""execution":{"uuid":"01a1489f-a9d7-71f0-a44b-dc8a90d7db7c"}"#;
    let paused = [
        format!(r#"{{"kind":"workflow_started",{run},"fields":[{{"key":"workflow","value":"w"}},{{"key":"budget","value":"{{\"max_cost_usd\":0.1}}"}}]}}"#),
        format!(r#"{{"kind":"workflow_paused",{run},"fields":[{{"key":"task","value":"approve"}},{{"key":"priced_calls","value":1}},{{"key":"total_cost_usd","value":0.03}}]}}"#),
    ]
    .join("\n");
    let facts = RunFacts::of(Path::new("t.ndjson"), &paused).expect("facts");
    let left = facts.ceiling_left().expect("a ceiling");
    assert!((left - 0.07).abs() < 1e-9, "{left}");
    let unbounded = paused.replace(r#",{"key":"budget","value":"{\"max_cost_usd\":0.1}"}"#, "");
    let facts = RunFacts::of(Path::new("t.ndjson"), &unbounded).expect("facts");
    assert_eq!(facts.ceiling_left(), None);
}

/// The run card in French: what ran, what it produced, its cost and ceiling, a refused gate said
/// as such, and a failure's code said once (live gauntlet wave 5).
#[test]
fn the_run_card_reads_in_french() {
    let root = Path::new(".");
    let run = r#""execution":{"uuid":"01a1489f-a9d7-71f0-a44b-dc8a90d7db7c"}"#;
    let done = [
        format!(r#"{{"kind":"workflow_started",{run},"fields":[{{"key":"workflow","value":"w"}},{{"key":"budget","value":{{"max_cost_usd":0.1}}}}]}}"#),
        format!(r#"{{"kind":"task_completed",{run},"fields":[{{"key":"task","value":"a"}}]}}"#),
        format!(r#"{{"kind":"task_completed",{run},"fields":[{{"key":"task","value":"b"}}]}}"#),
        format!(r#"{{"kind":"approval_decided",{run},"fields":[{{"key":"task","value":"b"}},{{"key":"decision","value":"deny"}},{{"key":"source","value":"resume"}}]}}"#),
        format!(r#"{{"kind":"workflow_completed",{run},"fields":[{{"key":"status","value":"succeeded"}},{{"key":"priced_calls","value":1}},{{"key":"total_cost_usd","value":0.0004}}]}}"#),
    ]
    .join("\n");
    let facts = RunFacts::of(Path::new("t.ndjson"), &done).expect("facts");
    let fr = facts.result_in(root, Path::new("w.nika"), true);
    assert!(
        fr.starts_with("Terminé · arrêté à votre « non » · `w.nika` · 2 étapes exécutées"),
        "{fr}"
    );
    assert!(
        fr.contains("refusé · votre réponse l'a arrêté là · `b` · répondu dans cette session"),
        "{fr}"
    );
    assert!(
        fr.contains("coût · 0,0004 $ · 1 appel tarifé · estimation enregistrée"),
        "{fr}"
    );
    assert!(
        fr.contains("plafond · 0,10 $ · le plus que ce run pouvait coûter"),
        "{fr}"
    );
    assert!(
        fr.contains("`/proof` montre les preuves et leurs limites · « lance-le » le relance"),
        "{fr}"
    );
    let failed = [
        format!(r#"{{"kind":"workflow_started",{run},"fields":[{{"key":"workflow","value":"w"}}]}}"#),
        format!(r#"{{"kind":"task_failed",{run},"fields":[{{"key":"task","value":"jq"}},{{"key":"detail","value":"NIKA-BUILTIN-JQ-001 · tool `nika:jq` reported an error: NIKA-BUILTIN-JQ-001 · jq runtime error: cannot use null as iterable"}}]}}"#),
        format!(r#"{{"kind":"workflow_failed",{run},"fields":[{{"key":"status","value":"failed"}}]}}"#),
    ]
    .join("\n");
    let facts = RunFacts::of(Path::new("t.ndjson"), &failed).expect("facts");
    let fr = facts.result_in(root, Path::new("w.nika"), true);
    assert!(fr.starts_with("Échoué · `w.nika` · `jq` a échoué"), "{fr}");
    assert!(
        fr.contains("NIKA-BUILTIN-JQ-001 · tool `nika:jq` reported an error: jq runtime error"),
        "the code said once: {fr}"
    );
    assert!(fr.contains("rien n'a abouti avant"), "{fr}");
    assert!(
        fr.contains("corrigez le workflow si la cause est la sienne"),
        "{fr}"
    );
}

/// A run its ceiling stopped says so, what it spent against the ceiling and what never ran, in
/// the person's language, and never asks for a repair of the workflow (live gauntlet r09).
#[test]
fn a_run_the_ceiling_stopped_says_so_and_asks_no_repair() {
    let root = Path::new(".");
    let raw = [
        r#"{"kind":"workflow_started","execution":{"uuid":"01a1489f-a9d7-71f0-a44b-dc8a90d7db7c"},"fields":[{"key":"workflow","value":"ventes"},{"key":"budget","value":{"max_cost_usd":0.001}}]}"#,
        r#"{"kind":"task_completed","execution":{"uuid":"01a1489f-a9d7-71f0-a44b-dc8a90d7db7c"},"fields":[{"key":"task","value":"comment"}]}"#,
        r#"{"kind":"task_cancelled","execution":{"uuid":"01a1489f-a9d7-71f0-a44b-dc8a90d7db7c"},"fields":[{"key":"task","value":"write_report"},{"key":"note","value":"budget · --max-cost-usd reached"}]}"#,
        r#"{"kind":"workflow_failed","execution":{"uuid":"01a1489f-a9d7-71f0-a44b-dc8a90d7db7c"},"fields":[{"key":"status","value":"failed"},{"key":"cause","value":"budget"},{"key":"priced_calls","value":1},{"key":"total_cost_usd","value":0.003506}]}"#,
    ]
    .join("\n");
    let facts = RunFacts::of(Path::new("t.ndjson"), &raw).expect("facts");
    assert!(facts.stopped_by_ceiling());
    assert_eq!(facts.spent_usd(), Some(0.003_506));
    let fr = facts.result_in(root, Path::new("ventes.nika"), true);
    assert!(
        fr.starts_with("Arrêté · `ventes.nika` · le plafond est atteint : 0,0035 $ dépensés pour un plafond de 0,0010 $"),
        "{fr}"
    );
    assert!(fr.contains("pas lancées · write_report"), "{fr}");
    assert!(fr.contains("le workflow n'est pas en cause"), "{fr}");
    let en = facts.result(root, Path::new("ventes.nika"));
    assert!(
        en.starts_with(
            "Stopped · `ventes.nika` · the ceiling was reached: $0.0035 spent for a $0.0010 ceiling"
        ),
        "{en}"
    );
    assert!(!en.contains("repair the workflow"), "{en}");
}

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
            && view.contains(
                "who wrote this journal (a run key from `nika key init` makes later runs seal)"
            ),
        "{view}"
    );
    // `nika sign` binds a workflow file to its author; it never seals a run.
    assert!(!view.contains("nika sign"), "{view}");
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

/// An unpriced call says why its own frame names, never that a remote route is a local model.
#[test]
fn an_unpriced_call_says_its_own_reason() {
    let (_dir, trace) = journal(
        "unpriced-reason",
        &[
            "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"t\"}]}",
            "{\"kind\":\"task_completed\",\"fields\":[{\"key\":\"task\",\"value\":\"t\"},{\"key\":\"cost_unpriced\",\"value\":\"missing_catalog_price\"}]}",
            "{\"kind\":\"workflow_completed\",\"fields\":[{\"key\":\"workflow\",\"value\":\"t\"},{\"key\":\"priced_calls\",\"value\":0},{\"key\":\"unpriced_calls\",\"value\":1}]}",
        ],
    );
    let facts = RunFacts::read(&trace).expect("frames");
    let line = facts.cost_line_in(false);
    assert_eq!(
        line,
        "cost · UNKNOWN · 1 unpriced call(s): no verified tariff for its route (never free)"
    );
    assert!(!line.contains("local model"), "{line}");
}

/// A French cost line says why a call is unpriced in French (2026-10-19: « 1 appel non tarifé :
/// its provider reported no complete usage »). A reason the view does not know keeps its name.
#[test]
fn a_french_cost_line_says_the_unpriced_reason_in_french() {
    let (_dir, trace) = journal(
        "unpriced-reason-fr",
        &[
            "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"t\"}]}",
            "{\"kind\":\"task_completed\",\"fields\":[{\"key\":\"task\",\"value\":\"t\"},{\"key\":\"cost_unpriced\",\"value\":\"provider_did_not_report_usage\"}]}",
            "{\"kind\":\"task_completed\",\"fields\":[{\"key\":\"task\",\"value\":\"u\"},{\"key\":\"cost_unpriced\",\"value\":\"a_future_reason\"}]}",
            "{\"kind\":\"workflow_completed\",\"fields\":[{\"key\":\"workflow\",\"value\":\"t\"},{\"key\":\"priced_calls\",\"value\":0},{\"key\":\"unpriced_calls\",\"value\":2}]}",
        ],
    );
    let facts = RunFacts::read(&trace).expect("frames");
    let unreported = "son fournisseur n'a pas déclaré d'usage complet (dépensé, jamais gratuit)";
    assert_eq!(
        facts.cost_line_in(true),
        format!("coût · INCONNU · 2 appels non tarifés : a_future_reason · {unreported}")
    );
    assert!(
        facts
            .cost_line_in(false)
            .contains("its provider reported no complete usage")
    );
}

/// An item of a step's list that failed and was recovered is named in the report, in the
/// person's language, and a documentation host is said as the address Nika never dials (live
/// gauntlet wave 7 r11, 2026-10-19: `example.com` logged as down, the report silent).
#[test]
fn a_recovered_item_is_named_and_a_documentation_host_said() {
    let items = serde_json::json!([
        {"code": "NIKA-BUILTIN-FETCH-001", "index": 0, "item": "https://example.com",
         "message": "for_each item [0] https://example.com: tool `nika:fetch` reported an error: \
                     NIKA-BUILTIN-FETCH-001 · request failed: HTTP connection failed: documentation \
                     host `example.com` is not dialed (RFC 2606/6761)", "status": "recovered"},
        {"code": "NIKA-BUILTIN-FETCH-001", "index": 1, "item": "https://down.example.net",
         "message": "connection refused", "status": "recovered"},
        {"index": 2, "item": "https://httpbin.org/status/200", "status": "ok"}
    ]);
    // As a run journals them: the items' JSON text, kept as a JSON string.
    let field = serde_json::json!(serde_json::json!(items.to_string()).to_string());
    let completed = serde_json::json!({"kind": "task_completed", "fields": [
        {"key": "task", "value": "probe"},
        {"key": "note", "value": "for_each · 1/3 ok · 2 recovered"},
        {"key": "items", "value": field}
    ]})
    .to_string();
    let (_dir, trace) = journal(
        "recovered-items",
        &[
            "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"sites\"}]}",
            completed.as_str(),
            "{\"kind\":\"workflow_completed\",\"fields\":[{\"key\":\"workflow\",\"value\":\"sites\"}]}",
        ],
    );
    let facts = RunFacts::read(&trace).expect("frames");
    let fr = facts.reached_lines(true);
    assert!(
        fr.contains(&"pas contacté · https://example.com · une adresse de documentation (RFC 2606) que Nika ne contacte jamais · `probe` a continué sans elle".to_owned()),
        "{fr:?}"
    );
    assert!(
        fr.contains(&"en erreur, repris · https://down.example.net · `probe` a continué (NIKA-BUILTIN-FETCH-001)".to_owned()),
        "{fr:?}"
    );
    let en = facts.reached_lines(false);
    assert!(
        en.iter()
            .any(|line| line.starts_with("not dialed · https://example.com")),
        "{en:?}"
    );
    assert!(
        !en.iter().any(|line| line.contains("httpbin.org")),
        "an ok item is no line: {en:?}"
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

/// ADR-148 point 12 · the first pause's gate shows its values structured and names the keys it
/// withheld, each read as the JSON object its frame carries; a frame without them, or with text
/// that is not one object, shows none.
#[test]
fn the_gate_shows_its_values_and_names_what_it_withheld() {
    let shown = serde_json::json!({"sno": 7, "amount": 1250.5});
    let withheld = serde_json::json!({"credential": "secret"});
    let field = |key: &str, text: &str| serde_json::json!({"key": key, "value": text}).to_string();
    let rows = [
        field("task", "approve"),
        field("values", &shown.to_string()),
        field("withheld", &withheld.to_string()),
    ];
    let first = paused(&rows.join(","));
    let (_root, path) = journal("gate-shown", &[first.as_str()]);
    let facts = RunFacts::read(&path).expect("frames");
    assert_eq!(facts.pause_shown(), (Some(&shown), Some(&withheld)));
    let older = paused(r#"{"key":"task","value":"approve"},{"key":"values","value":"[1,2]"}"#);
    let (_older, path) = journal("gate-unshown", &[older.as_str()]);
    let facts = RunFacts::read(&path).expect("frames");
    assert_eq!(facts.pause_shown(), (None, None));
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

#[test]
fn native_choices_keep_the_first_pause_and_first_field_exactly() {
    let field = |key: &str, value: Value| serde_json::json!({"key":key,"value":value});
    let first = serde_json::json!({"kind":"workflow_paused","fields":[
        field("task", Value::String("pick".into())),
        field("choices", Value::String(r#"["true","  spaced  ","/quit","é"]"#.into())),
        field("choices", Value::String(r#"["replacement"]"#.into()))
    ]});
    let later = paused(r#"{"key":"task","value":"later"},{"key":"choices","value":"[\"later\"]"}"#);
    let raw = format!("{first}\n{later}");
    let facts = RunFacts::of(Path::new("choices.ndjson"), &raw).expect("facts");
    assert_eq!(
        facts.pause_choices(),
        Some(
            ["true", "  spaced  ", "/quit", "é"]
                .map(str::to_owned)
                .as_slice()
        )
    );
    for bad in [
        Value::Null,
        serde_json::json!(["a"]),
        Value::String("not JSON".into()),
        Value::String("[1,\"a\"]".into()),
    ] {
        let malformed = serde_json::json!({"kind":"workflow_paused","fields":[
            field("task", Value::String("pick".into())), field("choices", bad),
            field("choices", Value::String(r#"["replacement"]"#.into()))
        ]});
        let raw = format!("{malformed}\n{later}");
        assert_eq!(
            RunFacts::of(Path::new("choices.ndjson"), &raw)
                .expect("facts")
                .pause_choices(),
            None
        );
    }
    let no_task = paused(r#"{"key":"choices","value":"[\"a\"]"}"#);
    assert_eq!(
        RunFacts::of(Path::new("choices.ndjson"), &format!("{no_task}\n{later}"))
            .expect("facts")
            .pause_choices(),
        None
    );
}

#[test]
fn a_choice_gate_describes_a_string_answer_without_claiming_yes_or_no_semantics() {
    let facts = RunFacts::read(&fixture("paused.ndjson")).expect("frames");
    let gated = vec!["record · writes a file".to_owned()];
    let choice = facts.gate(
        Path::new("choice.nika"),
        "Choose the exact report label",
        "choice",
        &gated,
    );
    assert!(choice.contains("\n  after your choice · record · writes a file"));
    assert!(choice.contains(
        "\n  the chosen value resumes the run · nothing after the gate has happened yet"
    ));
    assert!(!choice.contains("a yes lets happen") && !choice.contains("a no ends the run"));
    let confirm = facts.gate(
        Path::new("confirm.nika"),
        "Write the report?",
        "confirm",
        &gated,
    );
    assert!(confirm.contains("\n  a yes lets happen · record · writes a file"));
    assert!(
        confirm.contains("\n  a no ends the run there · nothing after the gate has happened yet")
    );
    assert!(confirm.contains("answer yes or no"));
}

mod expiry;

/// The ceiling a run was admitted under is said beside its cost, read from its `workflow_started`
/// frame whether the budget is an object or its JSON text (live gauntlet, 2026-10-16: asked
/// whether her $0.10 ceiling held, the author answered that the run's summary did not say which
/// ceiling applied); a run that names none says nothing of it.
#[test]
fn the_result_says_the_ceiling_the_run_was_admitted_under() {
    for budget in [
        r#"{"key":"budget","value":"{\"max_cost_usd\":0.1}"}"#,
        r#"{"key":"budget","value":{"max_cost_usd":0.1}}"#,
    ] {
        let started = format!(
            r#"{{"kind":"workflow_started","fields":[{{"key":"workflow","value":"ventes"}},{budget}]}}"#
        );
        let done = r#"{"kind":"workflow_completed","fields":[{"key":"status","value":"succeeded"},{"key":"total_cost_usd","value":0.0004},{"key":"priced_calls","value":1}]}"#;
        let (root, path) = journal("ceiling", &[started.as_str(), done]);
        let facts = RunFacts::read(&path).expect("frames");
        let view = facts.result(root.path(), Path::new("ventes.nika"));
        assert!(
            view.contains("ceiling · $0.10 · the most this run was allowed to cost"),
            "{view}"
        );
    }
    let (root, path) = journal(
        "no-ceiling",
        &[r#"{"kind":"workflow_started","fields":[{"key":"workflow","value":"ventes"}]}"#],
    );
    let facts = RunFacts::read(&path).expect("frames");
    assert!(
        !facts
            .result(root.path(), Path::new("ventes.nika"))
            .contains("ceiling ·")
    );
}

/// A path the run may write that is not there now is never « produced »: the run removed it, or it
/// is simply not there (live gauntlet q05: three inbox files moved to `traites/` read « produced ·
/// ./inbox/… »).
#[test]
fn a_removed_file_reads_removed_never_produced() {
    let dir = tempdir("removed");
    std::fs::create_dir_all(dir.path().join("traites")).expect("dir");
    std::fs::write(dir.path().join("traites/a.txt"), "hello").expect("moved");
    std::fs::write(
        dir.path().join("inbox.nika"),
        "nika: inbox\npermits:\n  fs: { write: [\"./traites/*\", \"./inbox/*\", \"./tmp.txt\"] }\n  tools: [\"nika:write\", \"nika:remove_file\"]\ntasks:\n  keep:\n    invoke: { tool: \"nika:write\", args: { path: \"./traites/a.txt\", content: \"hello\" } }\n  drop:\n    after: { keep: success }\n    invoke: { tool: \"nika:remove_file\", args: { path: \"./inbox/a.txt\" } }\n",
    )
    .expect("workflow");
    let permit = |task: &str, path: &str| {
        format!(
            "{{\"kind\":\"permit_checked\",\"fields\":[{{\"key\":\"task\",\"value\":\"{task}\"}},{{\"key\":\"plane\",\"value\":\"fs\"}},{{\"key\":\"gate\",\"value\":\"permits.fs.write {path}\"}},{{\"key\":\"decision\",\"value\":\"allow\"}}]}}"
        )
    };
    let frames = [
        "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"inbox\"}]}".to_owned(),
        permit("keep", "./traites/a.txt"),
        permit("drop", "./inbox/a.txt"),
        permit("keep", "./tmp.txt"),
        "{\"kind\":\"workflow_completed\",\"fields\":[{\"key\":\"status\",\"value\":\"succeeded\"}]}".to_owned(),
    ];
    let frames: Vec<&str> = frames.iter().map(String::as_str).collect();
    let (_journal_dir, trace) = journal("removed-journal", &frames);
    let facts = RunFacts::read(&trace).expect("frames");
    let view = facts.result(dir.path(), Path::new("inbox.nika"));
    assert!(
        view.contains("\n  produced · ./traites/a.txt (5 B)"),
        "{view}"
    );
    assert!(view.contains("\n  removed · ./inbox/a.txt"), "{view}");
    assert!(view.contains("\n  not there now · ./tmp.txt"), "{view}");
    assert!(!view.contains("produced · ./inbox"), "{view}");
}

/// A file the run left empty is said beside what it removed, never left among the green ticks
/// (live gauntlet wave 7 r05, 2026-10-19: « produit · ./journal.md (0 B) » under « réussi », the
/// three messages already moved to `traites/`).
#[test]
fn an_empty_product_is_said_beside_what_the_run_removed() {
    let dir = tempdir("empty-product");
    std::fs::create_dir_all(dir.path().join("traites")).expect("dir");
    std::fs::write(dir.path().join("traites/a.txt"), "hello").expect("moved");
    std::fs::write(dir.path().join("journal.md"), "").expect("the empty journal");
    std::fs::write(
        dir.path().join("inbox.nika"),
        "nika: inbox\npermits:\n  fs: { write: [\"./traites/*\", \"./inbox/*\", \"./journal.md\"] }\n  tools: [\"nika:write\", \"nika:remove_file\"]\ntasks:\n  journal:\n    invoke: { tool: \"nika:write\", args: { path: \"./journal.md\", content: \"\" } }\n  keep:\n    invoke: { tool: \"nika:write\", args: { path: \"./traites/a.txt\", content: \"hello\" } }\n  drop:\n    after: { keep: success }\n    invoke: { tool: \"nika:remove_file\", args: { path: \"./inbox/a.txt\" } }\n",
    )
    .expect("workflow");
    let permit = |task: &str, path: &str| {
        format!(
            "{{\"kind\":\"permit_checked\",\"fields\":[{{\"key\":\"task\",\"value\":\"{task}\"}},{{\"key\":\"plane\",\"value\":\"fs\"}},{{\"key\":\"gate\",\"value\":\"permits.fs.write {path}\"}},{{\"key\":\"decision\",\"value\":\"allow\"}}]}}"
        )
    };
    let frames = [
        "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"inbox\"}]}".to_owned(),
        permit("journal", "./journal.md"),
        permit("keep", "./traites/a.txt"),
        permit("drop", "./inbox/a.txt"),
        "{\"kind\":\"workflow_completed\",\"fields\":[{\"key\":\"status\",\"value\":\"succeeded\"}]}".to_owned(),
    ];
    let frames: Vec<&str> = frames.iter().map(String::as_str).collect();
    let (_journal_dir, trace) = journal("empty-product-journal", &frames);
    let facts = RunFacts::read(&trace).expect("frames");
    let en = facts.result(dir.path(), Path::new("inbox.nika"));
    assert!(
        en.contains("\n  warning · ./journal.md is empty (0 B) while this run removed 1 file: check the result before running it again"),
        "{en}"
    );
    let fr = facts.result_in(dir.path(), Path::new("inbox.nika"), true);
    assert!(
        fr.contains("\n  attention · ./journal.md est vide (0 o) alors que ce run a retiré 1 fichier : vérifiez le résultat avant de relancer"),
        "{fr}"
    );
    // A product with bytes is no warning.
    std::fs::write(dir.path().join("journal.md"), "- a line\n").expect("a journal");
    let view = facts.result(dir.path(), Path::new("inbox.nika"));
    assert!(!view.contains("warning ·"), "{view}");
}

/// The run report says the workflow's declared outputs, and a read that failed and was
/// recovered reads « not there » (live gauntlet r08, r03 and r02, 2026-10-16).
#[test]
fn the_report_says_the_outputs_and_never_a_read_that_found_nothing() {
    let dir = tempdir("outputs");
    std::fs::write(
        dir.path().join("w.nika"),
        "nika: w\npermits:\n  fs: { read: [\"./state.txt\", \"./in.csv\"] }\n  tools: [\"nika:read\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:read\", args: { path: \"./state.txt\" } }\n",
    )
    .expect("workflow");
    let permit = |task: &str, path: &str| {
        format!(
            "{{\"kind\":\"permit_checked\",\"fields\":[{{\"key\":\"task\",\"value\":\"{task}\"}},{{\"key\":\"plane\",\"value\":\"fs\"}},{{\"key\":\"gate\",\"value\":\"permits.fs.read {path}\"}},{{\"key\":\"decision\",\"value\":\"allow\"}}]}}"
        )
    };
    let task = |kind: &str, id: &str| {
        format!("{{\"kind\":\"{kind}\",\"fields\":[{{\"key\":\"task\",\"value\":\"{id}\"}}]}}")
    };
    let outputs = serde_json::json!({"totals": {"north": 15, "south": 20}, "message": "Toutes les notes ont été traduites.\nDétail", "journal": "2026-10-16 · la première\n\n2026-10-17 · la deuxième\n2026-10-18 · la dernière\n"}).to_string();
    let completed = serde_json::json!({"kind": "workflow_completed", "fields": [
        {"key": "status", "value": "succeeded"}, {"key": "outputs", "value": outputs}]})
    .to_string();
    let frames = [
        "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"w\"}]}"
            .to_owned(),
        task("task_started", "a"),
        permit("a", "./state.txt"),
        task("task_recovered", "a"),
        task("task_started", "b"),
        permit("b", "./in.csv"),
        task("task_completed", "b"),
        completed,
    ];
    let frames: Vec<&str> = frames.iter().map(String::as_str).collect();
    let (_journal_dir, trace) = journal("outputs-journal", &frames);
    let facts = RunFacts::read(&trace).expect("frames");
    let view = facts.result(dir.path(), Path::new("w.nika"));
    assert!(view.contains("\n  read · ./in.csv"), "{view}");
    assert!(view.contains("\n  not there · ./state.txt"), "{view}");
    assert!(!view.contains("read · ./state.txt"), "{view}");
    assert!(
        view.contains("\n  output · totals · {\"north\":15,\"south\":20}"),
        "{view}"
    );
    assert!(
        view.contains(
            "\n  output · message · Toutes les notes ont été traduites. … Détail (2 lines)"
        ),
        "{view}"
    );
    // A journal a run appends to says the line this run wrote, beside its first (live gauntlet
    // wave 6 r08 B2: three runs read the first run's line).
    assert!(
        view.contains(
            "\n  output · journal · 2026-10-16 · la première … 2026-10-18 · la dernière (3 lines)"
        ),
        "{view}"
    );
}

/// One journal frame of the run `execution`, `kind` and `fields` as the runtime writes them.
fn leg_frame(execution: &str, kind: &str, fields: &serde_json::Value) -> String {
    let fields: Vec<serde_json::Value> = (fields.as_object().into_iter().flatten())
        .map(|(key, value)| serde_json::json!({"key": key, "value": value}))
        .collect();
    serde_json::json!({
        "correlation": null,
        "execution": {"uuid": execution},
        "fields": fields,
        "id": {"uuid": "01a0ef11-03a7-74fb-bba0-000000000001"},
        "kind": kind,
        "run": null,
        "timestamp": 1
    })
    .to_string()
}

/// A run a gate split (live gauntlet 4, r04 B1, W16): its first leg paused after one priced
/// call, its resumed leg finished without a model call. The resumed leg's result counts the
/// run's every leg, « $0.0438 · 1 priced call », never « no model usage recorded »; the pure
/// fold of its bytes alone reads no other journal.
#[test]
fn a_resumed_leg_counts_what_the_legs_before_its_gate_spent() {
    let store = tempdir("resumed-cost");
    let paused = "01a1453e-d896-74a2-808f-b8f8ae95a870";
    let resumed = "01a1454f-aaab-7077-bc12-87282886951b";
    let first = [
        leg_frame(
            paused,
            "workflow_started",
            &serde_json::json!({"workflow": "hn"}),
        ),
        leg_frame(
            paused,
            "task_started",
            &serde_json::json!({"task": "pick", "note": "infer · xai/grok-4.7"}),
        ),
        leg_frame(
            paused,
            "task_completed",
            &serde_json::json!({"task": "pick", "note": "infer · xai/grok-4.7", "cost_usd": 0.043_776}),
        ),
        leg_frame(
            paused,
            "workflow_paused",
            &serde_json::json!({"task": "approve", "mode": "confirm", "status": "paused", "total_cost_usd": 0.043_776, "priced_calls": 1, "unpriced_calls": 0, "cost_qualifier": "priced"}),
        ),
    ];
    let second = [
        leg_frame(
            resumed,
            "workflow_started",
            &serde_json::json!({"workflow": "hn", "resumed_from": "01a1453ed89674a2808fb8f8ae95a870"}),
        ),
        leg_frame(
            resumed,
            "task_cache_hit",
            &serde_json::json!({"task": "pick", "note": "cache hit"}),
        ),
        leg_frame(
            resumed,
            "task_completed",
            &serde_json::json!({"task": "approve"}),
        ),
        leg_frame(
            resumed,
            "workflow_completed",
            &serde_json::json!({"status": "succeeded", "priced_calls": 0, "unpriced_calls": 0, "cost_qualifier": "unmetered"}),
        ),
    ];
    let traces = store.path().join("traces");
    std::fs::create_dir_all(&traces).expect("traces");
    std::fs::write(
        traces.join("2026-10-16T15-05-10Z-a870.ndjson"),
        first.join("\n"),
    )
    .expect("paused");
    let leg = traces.join("2026-10-16T15-23-33Z-951b.ndjson");
    std::fs::write(&leg, second.join("\n")).expect("resumed");
    let facts = RunFacts::read(&leg).expect("frames");
    let view = facts.result(store.path(), Path::new("hn.nika"));
    assert!(
        view.contains("cost · $0.0438 · 1 priced call(s) · recorded estimate, invoice not verified · over its 2 legs"),
        "{view}"
    );
    assert!(!view.contains("no model usage recorded"), "{view}");
    let alone = RunFacts::of(&leg, &second.join("\n")).expect("frames");
    let view = alone.result(store.path(), Path::new("hn.nika"));
    assert!(view.contains("cost · no model usage recorded"), "{view}");
}

/// A run a gate split is reported under the ceiling it started with: its resumed leg's start
/// records only what the first leg left (live gauntlet wave 8, 2026-10-19, r04: « plafond ·
/// 0,212482 $ · le plus que ce run pouvait coûter » for a run started under 0,25 $).
#[test]
fn a_resumed_run_says_the_ceiling_it_started_under() {
    let store = tempdir("resumed-ceiling");
    let paused = "01a1453e-d896-74a2-808f-b8f8ae95a871";
    let resumed = "01a1454f-aaab-7077-bc12-87282886951c";
    let first = [
        leg_frame(
            paused,
            "workflow_started",
            &serde_json::json!({"workflow": "hn", "budget": {"max_cost_usd": 0.25}}),
        ),
        leg_frame(
            paused,
            "workflow_paused",
            &serde_json::json!({"task": "approve", "mode": "confirm", "status": "paused", "total_cost_usd": 0.037_518, "priced_calls": 1, "unpriced_calls": 0, "cost_qualifier": "priced"}),
        ),
    ];
    let second = [
        leg_frame(
            resumed,
            "workflow_started",
            &serde_json::json!({"workflow": "hn", "budget": {"max_cost_usd": 0.212_482}, "resumed_from": "01a1453ed89674a2808fb8f8ae95a871"}),
        ),
        leg_frame(
            resumed,
            "workflow_completed",
            &serde_json::json!({"status": "succeeded", "priced_calls": 0, "unpriced_calls": 0, "cost_qualifier": "unmetered"}),
        ),
    ];
    let traces = store.path().join("traces");
    std::fs::create_dir_all(&traces).expect("traces");
    std::fs::write(
        traces.join("2026-10-19T19-16-43Z-a871.ndjson"),
        first.join("\n"),
    )
    .expect("paused");
    let leg = traces.join("2026-10-19T19-19-38Z-951c.ndjson");
    std::fs::write(&leg, second.join("\n")).expect("resumed");
    let facts = RunFacts::read(&leg).expect("frames");
    let view = facts.result(store.path(), Path::new("hn.nika"));
    assert!(
        view.contains("ceiling · $0.25 · the most this run was allowed to cost"),
        "{view}"
    );
    assert!(!view.contains("0.212482"), "{view}");
    let french = facts.result_in(store.path(), Path::new("hn.nika"), true);
    assert!(
        french.contains("plafond · 0,25 $ · le plus que ce run pouvait coûter"),
        "{french}"
    );
}

/// A step that sends beyond this machine says whether it sent: completed, or not sent and why
/// (live gauntlet r02 B13, 2026-10-17: the report never said whether the webhook was pinged).
#[test]
fn each_sending_step_says_whether_it_sent() {
    let root = tempdir("sending-steps");
    std::fs::write(
        root.path().join("watch.nika"),
        "nika: watch\npermits:\n  tools: [\"nika:fetch\"]\n  net: { http: [\"hooks.example\"] }\ntasks:\n  ping:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://hooks.example/x\", method: POST, body: \"v\" } }\n  look:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://hooks.example/y\" } }\n",
    )
    .expect("workflow");
    let mut facts = RunFacts::read(&fixture("copy.ndjson")).expect("frames");
    facts.tasks.push(super::TaskFact {
        id: "ping".to_owned(),
        state: TaskState::Skipped,
        ..Default::default()
    });
    let view = facts.result(root.path(), Path::new("watch.nika"));
    assert!(
        view.contains("not sent · `ping` · its condition was false"),
        "{view}"
    );
    assert!(!view.contains("`look`"), "a GET sends nothing: {view}");
    if let Some(ping) = facts.tasks.iter_mut().find(|t| t.id == "ping") {
        ping.state = TaskState::Ok;
    }
    let view = facts.result(root.path(), Path::new("watch.nika"));
    assert!(
        view.contains("sent · `ping` · the step completed"),
        "{view}"
    );
}
