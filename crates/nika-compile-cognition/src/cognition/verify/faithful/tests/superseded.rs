// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A revision whose change replaces a clause of the earlier request, confirmed over a run of the
//! revised bytes (A1, R6). The request judged is the earlier one followed by the change, so the
//! clause the change replaces is still one of its parts. Over the run, an open part a later one
//! follows is offered `superseded`, as when it is asked over the bytes alone: so answered, the
//! replaced clause asks nothing of the candidate, while the change and every clause it leaves
//! untouched stay judged. The last part is never offered it, and no choice keeps a part open.

use super::*;
use nika_compile_seats::judge::REVISED_OVER_DOCUMENT;
use nika_compile_seats::judge::over_document;

/// The base the revision starts from: it keeps the tickets older than 48 hours.
const BASE: &str = r#"nika: stale-ids
permits:
  fs: { read: ["./in/tickets.json"], write: ["./out/report.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks:
  load:
    invoke:
      tool: "nika:read"
      args: { path: "./in/tickets.json" }
  select_stale:
    with: { rows: "${{ tasks.load.output }}" }
    invoke:
      tool: "nika:jq"
      args: { input: "${{ with.rows }}", expression: "map(select(.age_hours > 48) | .id)" }
  save:
    with: { ids: "${{ tasks.select_stale.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/report.json", content: "${{ with.ids }}" }
"#;

/// The request the base answers, and the change the human states.
const ORIGINAL: &str = "Read ./in/tickets.json, keep the records whose age_hours is strictly greater than 48, write their ids in input order to ./out/report.json.";
const CHANGE: &str = "Raise the age threshold to 72 hours.";

/// The parts of the revised request: the earlier request's, then the change, last.
const SPLIT: [&str; 4] = [
    "Read ./in/tickets.json",
    "keep the records whose age_hours is strictly greater than 48",
    "write their ids in input order to ./out/report.json",
    "Change: Raise the age threshold to 72 hours",
];

/// Tickets 60, 72 and 90 hours old: the earlier threshold keeps all three, the change only the
/// last; and the report the revised bytes wrote from them.
const TICKETS: &str =
    r#"[{"id":"t-60","age_hours":60},{"id":"t-72","age_hours":72},{"id":"t-90","age_hours":90}]"#;
const REPORT: &str = r#"["t-90"]"#;

/// The revised bytes: the base with the change applied.
fn candidate() -> String {
    BASE.replace("> 48", "> 72")
}

/// The run of the revised bytes over the three tickets, every text read whole.
fn run() -> Value {
    json!({
        "candidate_sha256": sha256(&candidate()),
        "inputs": [{"path": "./in/tickets.json", "text": TICKETS, "read_whole": true}],
        "outputs": [{"path": "./out/report.json", "text": REPORT, "written": true,
            "read_whole": true}],
    })
}

/// The revision as the native route judges it: the earlier request followed by the change,
/// applied over the complete base document (its record says so).
fn request() -> CompileRequest {
    CompileRequest::edit(BASE, CHANGE)
        .with_original_intent(ORIGINAL)
        .with_plan(json!({"document_revision": {}}))
}

/// The whole verdict of the revised bytes over [`run`], asked of `judge`, and its binding.
async fn confirmed(judge: &Scripted) -> (Verdict, Binding) {
    let request = request();
    let intent = nika_compile::revise_intent(&request).unwrap();
    let candidate = candidate();
    let mut base = state(&intent, &request, &candidate);
    over_document(&mut base, &request, &crate::initial());
    let binding = Binding::of(&intent, &request, &Plan::default(), &candidate);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, judge);
    let (mut verdict, mut out) = (Verdict::default(), crate::initial());
    let observation = run();
    let asked = (&base, "fixture");
    whole(
        &intent,
        asked,
        &provider,
        &binding,
        Some(&observation),
        &mut verdict,
        &mut out,
    )
    .await;
    (verdict, binding)
}

/// The earlier threshold the change replaces is offered `superseded` over the run and, so
/// answered, asks nothing of the candidate: no defect points to the task that applies the change,
/// and with the change and every untouched clause carried the faithful verdict stands. Each
/// question over the run keeps the whole revision: the request with its change, the earlier
/// request as history, the complete base, the revised bytes and their run. The change, the last
/// part, is never offered `superseded`.
#[tokio::test]
async fn an_earlier_clause_the_change_replaces_is_superseded_over_the_run() {
    let intent = nika_compile::revise_intent(&request()).unwrap();
    assert_eq!(intent, format!("{ORIGINAL}\nChange: {CHANGE}"));
    assert_eq!(parts(&intent), SPLIT);
    let judge = Scripted::new([
        (Request, Choose("faithful")),
        (ObservedPart, Choose("carried")),
        (ObservedPart, Choose("superseded")),
        (ObservedPart, Choose("carried")),
        (ObservedPart, Choose("carried")),
    ]);
    let (verdict, binding) = confirmed(&judge).await;
    let asked = [
        "verify-request",
        "verify-observed-part-0",
        "verify-observed-part-1",
        "verify-observed-part-2",
        "verify-observed-part-3",
    ];
    assert_eq!(ids(&verdict), asked);
    for k in 0..SPLIT.len() {
        let mut offered = vec!["carried", "missing", "unexercised"];
        if k + 1 < SPLIT.len() {
            offered.push("superseded");
        }
        offered.push("none");
        let over = record(&verdict, &format!("verify-observed-part-{k}"));
        assert_eq!(over["options"], json!(offered), "{k}");
    }
    assert_eq!(
        record(&verdict, "verify-observed-part-1")["choice"],
        "superseded"
    );
    let judgment = carried(&intent, "verify-request", MODEL, &binding);
    assert_eq!(verdict.judgments, [judgment]);
    assert_eq!(verdict.settled_by, Some("verify-request"));
    assert_eq!(lists(&verdict), found(&[], &[], &[], &[], &[]));
    assert!(verdict.settled() && !verdict.doubted());
    assert_eq!(counts(&verdict), (5, 5, 5));
    let revision = json!({"change": CHANGE, "base_request": ORIGINAL, "appended": true,
        "base_nika": BASE, "over_document": true});
    let sent = judge.sent.lock().unwrap();
    for (k, part) in SPLIT.iter().enumerate() {
        let shown = &sent[k + 1];
        assert_eq!(shown.state["request"], intent.as_str(), "{k}");
        assert_eq!(shown.state["revision"], revision, "{k}");
        assert_eq!(shown.state["original_request"], Value::Null, "{k}");
        assert_eq!(shown.state["candidate_nika"], candidate().as_str(), "{k}");
        assert_eq!(shown.state["observation"], run(), "{k}");
        assert_eq!(shown.state["clause"], json!({"text": part}), "{k}");
        assert!(shown.told.contains(REVISED_OVER_DOCUMENT), "{k}");
        assert_eq!(shown.told.contains(PART), k + 1 < SPLIT.len(), "{k}");
    }
    drop(sent);
    assert_eq!(judge.left(), 0);
}

/// What the change asks, and what it leaves untouched, stay required over the run: the change
/// shown missing, or the report shown missing, is a defect located on its task; the earlier
/// threshold left without a choice stays open. None of them stands.
#[tokio::test]
async fn the_change_and_untouched_clauses_stay_required_and_no_choice_stays_open() {
    let note = |task: &str| format!("in the trial run, {}", points(task));
    let (change, report) = (note("select_stale"), note("save"));
    let cases: [(Vec<(Kind, Reply)>, Value); 3] = [
        (
            vec![
                (ObservedPart, Choose("carried")),
                (ObservedPart, Choose("superseded")),
                (ObservedPart, Choose("carried")),
                (ObservedPart, Choose("missing")),
                (Point, Choose("task-select_stale")),
            ],
            found(&[(SPLIT[3], change.as_str())], &[], &[], &[], &[]),
        ),
        (
            vec![
                (ObservedPart, Choose("carried")),
                (ObservedPart, Choose("superseded")),
                (ObservedPart, Choose("missing")),
                (Point, Choose("task-save")),
                (ObservedPart, Choose("carried")),
            ],
            found(&[(SPLIT[2], report.as_str())], &[], &[], &[], &[]),
        ),
        (
            vec![
                (ObservedPart, Choose("carried")),
                (ObservedPart, Choose("none")),
                (ObservedPart, Choose("carried")),
                (ObservedPart, Choose("carried")),
            ],
            found(&[], &[SPLIT[1]], &[], &[], &[]),
        ),
    ];
    for (over_the_run, expected) in cases {
        let mut script = vec![(Request, Choose("faithful"))];
        script.extend(over_the_run);
        let judge = Scripted::new(script);
        let (verdict, _) = confirmed(&judge).await;
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert_eq!(lists(&verdict), expected);
        assert!(!verdict.settled() && verdict.doubted());
        assert_eq!(judge.left(), 0);
    }
}
