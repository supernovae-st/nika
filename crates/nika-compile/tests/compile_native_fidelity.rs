// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Three authoring failures measured on real seats (2026-09-23), judged end to end through the
//! native door with a hermetic scripted seat:
//! - a final approval the request states in words the reader cannot bind to an effect
//!   (« prépare son envoi à … · demande-moi avant de l'envoyer »): the seat's gate was refused as
//!   invented, and an ungated send met no law (Law 3b); a bound approval (Law 3) took any
//!   dependency on a prompt for a yes, a defaulted one included;
//! - a `nika:jq` that treats the text of a `nika:read` as records passed every law and Check,
//!   then failed at Run with NIKA-BUILTIN-JQ-001 (Law 23);
//! - line files lost their final newline and named shapes were rewrapped: the seat now reads
//!   the engine's output conventions beside the card, and the receipt names their digest;
//! - a whole source name the human typed for the assembler's source question
//!   (`const.source_paths`) is read whole by the native and sketch judges exactly as by the
//!   assembler's emission, and a file the request also names on its own stays owed.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode, intent_sha256,
};
use nika_compile_cognition::compile_with_provider;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, Role, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::Duration;

mod common;
use common::{Judged, Rotating};

/// The reader sees the gate and no effect word it knows in this request (its plan carries
/// `lexicon::GATE_WITHOUT_EFFECT`): the Law 3b case, asserted below before it is used.
const UNBOUND: &str = "Lis ./note.txt et prépare son envoi à https://hooks.example.test/in. Demande-moi explicitement avant de l’envoyer.";

fn policy(repairs: u32) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(repairs)
}

fn native(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["native"].clone()
}

/// Every round's diagnostic messages, in round order.
fn rounds(out: &CompileOutcome) -> Vec<Vec<String>> {
    native(out)["rounds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|round| {
            round["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|d| d["message"].as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}

#[test]
fn the_approval_request_reads_as_a_final_gate_the_reader_cannot_bind() {
    // The premise of the two tests below: if the reader learns this effect, Law 3 judges it and
    // these tests must move to a phrasing the reader still leaves unbound.
    let plan = nika_compile_reader::lexicon::read(UNBOUND).plan;
    assert!(
        nika_compile_fidelity::fidelity::unbound_final_gate(&plan),
        "{:?} · {:?}",
        plan.unknowns,
        plan.effects
    );
}

#[tokio::test]
async fn a_stated_final_gate_the_seat_writes_is_accepted_not_refused_as_invented() {
    // Red at 4a06aa3a: round 0 « INVENTED GATE » for `send`, the same answer again, no progress.
    let provider = Rotating::new(send_graph(Some(&json!({"gated_by": "review"}))));
    let req = CompileRequest::create(UNBOUND).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(native(&out)["accepted"], true, "{out:#?}");
    let rounds = rounds(&out);
    assert_eq!(rounds.len(), 2, "the sketch and its fills: {rounds:?}");
    assert!(rounds.iter().all(Vec::is_empty), "{rounds:?}");
    assert_ne!(out.status, CompileStatus::Refused, "{out:#?}");
}

#[tokio::test]
async fn a_final_send_without_the_stated_approval_never_passes() {
    // Red at 4a06aa3a: no law fires and the ungated send is accepted.
    let provider = Rotating::new(send_graph(None));
    let req = CompileRequest::create(UNBOUND).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(native(&out)["accepted"], true, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        rounds(&out)[0]
            .iter()
            .any(|m| m.starts_with("MISSING APPROVAL")),
        "{:?}",
        rounds(&out)
    );
}

#[tokio::test]
async fn a_final_send_that_only_waits_for_the_prompt_is_the_wrong_order() {
    // Bypass attempt: the prompt exists, the send waits for it (`after`) but ignores the answer.
    let provider = Rotating::new(send_graph(Some(&json!({"after": ["review"]}))));
    let req = CompileRequest::create(UNBOUND).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(native(&out)["accepted"], true, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let first = &rounds(&out)[0];
    assert!(
        first.iter().any(|m| m.contains("`send`")
            && (m.starts_with("APPROVAL ORDER") || m.starts_with("MISSING APPROVAL"))),
        "the unguarded send is named: {first:?}"
    );
    assert!(
        !first.iter().any(|m| m.starts_with("INVENTED GATE")),
        "{first:?}"
    );
}

/// The reader binds this approval to the write (a human-first `write`): Law 3, bound.
const DRAFT: &str =
    "Lis ./draft.md et écris-le dans ./out/final.md, mais demande-moi avant d'écrire.";

#[tokio::test]
async fn a_bound_write_approved_by_a_defaulted_gate_is_refused_and_the_human_confirm_passes() {
    // Red at 4a06aa3a (native door): a `default: true` said yes with nobody there; `default:
    // false` answered no unattended. A sketch's prompt states its message only: a fill that adds
    // a default is refused by its name, and the repaired fill asks a human.
    let draft = json!([{"name": "draft", "from": "read_draft"}]);
    let graph = sketch_of(&json!([
        node("read_draft", "nika:read", &json!({"reads": ["./draft.md"]})),
        node("review", "nika:prompt", &json!({"with": draft})),
        node(
            "write_final",
            "nika:write",
            &json!({"writes": ["./out/final.md"], "with": draft, "gated_by": "review"})
        ),
    ]));
    let message = json!({"task": "review", "field": "args.message", "value": "Écrire ce brouillon ? ${{ with.draft }}"});
    for defaulted in [true, false] {
        let hostile =
            json!([message, {"task": "review", "field": "args.default", "value": defaulted}]);
        let provider = Rotating::new(vec![
            graph.clone(),
            fills_of(&hostile),
            fills_of(&json!([message])),
        ]);
        let req = CompileRequest::create(DRAFT).with_authoring_policy(policy(1));
        let out = compile_with_provider(&req, &provider).await.unwrap();
        let rounds = rounds(&out);
        assert_eq!(rounds.len(), 3, "{defaulted}: {rounds:?}");
        assert!(
            rounds[1]
                .iter()
                .any(|m| m.contains("names no hole of `review`")),
            "the defaulted gate is refused by name: {defaulted}: {rounds:?}"
        );
        assert!(rounds[2].is_empty(), "{defaulted}: {rounds:?}");
        assert_eq!(native(&out)["accepted"], true, "{defaulted}: {out:#?}");
        let source =
            native(&out)["rounds"][2].to_string() + out.candidate.as_deref().unwrap_or_default();
        assert!(!source.contains("default:"), "{defaulted}: {out:#?}");
    }
}

const TICKETS: &str = "Lis ./tickets.json et écris le ticket 42 dans ./out/ticket.json.";

#[tokio::test]
async fn records_read_from_raw_text_are_refused_before_ready_and_the_parsed_repair_passes() {
    // Red at 4a06aa3a: round 0 is accepted (Check-clean) and the Run fails NIKA-BUILTIN-JQ-001.
    let raw = find_graph("[.[] | select(.id == 42)] | first | tojson");
    let parsed = find_graph("fromjson | [.[] | select(.id == 42)] | first | tojson");
    let provider = Rotating::new(vec![raw[0].clone(), raw[1].clone(), parsed[1].clone()]);
    let req = CompileRequest::create(TICKETS).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let rounds = rounds(&out);
    assert_eq!(rounds.len(), 3, "{rounds:?}");
    assert!(rounds[0].is_empty(), "the sketch: {rounds:?}");
    assert!(
        rounds[1]
            .iter()
            .any(|m| m.starts_with("RAW TEXT AS RECORDS")
                && m.contains("`find_ticket`")
                && m.contains("`read_tickets`")),
        "{rounds:?}"
    );
    assert!(rounds[2].is_empty(), "{rounds:?}");
    assert_eq!(native(&out)["accepted"], true, "{out:#?}");
}

#[tokio::test]
async fn string_operations_on_a_reads_text_stay_admissible() {
    // Control: a line transform over the read's text is the proper use of the text.
    let provider = Rotating::new(find_graph(
        r#"split("\n") | map(rtrimstr("\r")) | if .[-1] == "" then .[:-1] else . end | map(select(test("42"))) | first"#,
    ));
    let req = CompileRequest::create(TICKETS).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let rounds = rounds(&out);
    assert_eq!(rounds.len(), 2, "{rounds:?}");
    assert!(
        !rounds
            .iter()
            .flatten()
            .any(|m| m.starts_with("RAW TEXT AS RECORDS")),
        "{rounds:?}"
    );
}

#[tokio::test]
async fn a_wrapped_read_is_not_json_text_and_native_repair_can_select_the_field() {
    // The native door refused `fromjson` over an object-wrapped read (NON-TEXT AS JSON). The sketch
    // binds a jq task's input itself, so a fill that wraps it names no hole and is refused by
    // name; the non-text shape never forms, and the repaired fill parses the read's text.
    let [graph, parsed] = find_graph("fromjson | [.[] | select(.id == 42)] | first")
        .try_into()
        .unwrap();
    let wrapped = fills_of(&json!([
        {"task": "find_ticket", "field": "args", "value": {"input": {"content": "${{ with.text }}"}}},
        {"task": "find_ticket", "field": "expression", "value": ".content | fromjson | [.[] | select(.id == 42)] | first"}
    ]));
    let provider = Rotating::new(vec![graph.clone(), wrapped.clone(), parsed]);
    let req = CompileRequest::create(TICKETS).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let judged = rounds(&out);
    assert_eq!(judged.len(), 3, "{out:#?}");
    assert!(
        judged[1]
            .iter()
            .any(|m| m.contains("`find_ticket.args` is owned by the sketch")),
        "{judged:?}"
    );
    assert!(judged[2].is_empty(), "{judged:?}");
    assert_eq!(native(&out)["accepted"], true, "{out:#?}");

    let provider = Rotating::new(vec![graph, wrapped]);
    let req = CompileRequest::create(TICKETS).with_authoring_policy(policy(0));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(native(&out)["accepted"], true, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
}

#[tokio::test]
async fn native_fidelity_keeps_relative_multiword_paths_whole_before_ready() {
    for (intent, source, destination, shortened_source, shortened_destination) in [
        (
            "Copie le fichier dossier source/notes.txt vers dossier sortie/notes.txt.",
            "dossier source/notes.txt",
            "dossier sortie/notes.txt",
            "source/notes.txt",
            "sortie/notes.txt",
        ),
        (
            "Copy the file team notes/input.json to team notes/output.json",
            "team notes/input.json",
            "team notes/output.json",
            "notes/input.json",
            "notes/output.json",
        ),
    ] {
        let bad = copy_graph(shortened_source, shortened_destination);
        let fixed = copy_graph(source, destination);
        let provider = Rotating::new(vec![bad.clone(), fixed, NO_FILLS.to_owned()]);
        let req = CompileRequest::create(intent).with_authoring_policy(policy(1));
        // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
        let out = compile_with_provider(&req, &Judged::approving(&provider))
            .await
            .unwrap();
        let judged = rounds(&out);
        assert_eq!(judged.len(), 3, "{intent}: {out:#?}");
        for path in [source, destination] {
            assert!(
                judged[0]
                    .iter()
                    .any(|m| m.starts_with("UNREALIZED PATH") && m.contains(path)),
                "{judged:?}"
            );
        }
        assert!(judged[1].is_empty() && judged[2].is_empty(), "{judged:?}");
        assert_eq!(native(&out)["accepted"], true, "{out:#?}");
        let doc = document(&out);
        assert_eq!(doc["permits"]["fs"]["read"], json!([source]));
        assert_eq!(doc["permits"]["fs"]["write"], json!([destination]));

        let provider = Rotating::new(vec![bad]);
        let req = CompileRequest::create(intent).with_authoring_policy(policy(0));
        let out = compile_with_provider(&req, &provider).await.unwrap();
        assert_ne!(native(&out)["accepted"], true, "{out:#?}");
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        assert!(out.candidate.is_none(), "{out:#?}");
    }
}

/// A seat that records the system message it is sent.
struct Recording {
    answer: String,
    systems: Mutex<Vec<String>>,
}

impl ProviderInferDyn for Recording {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let system: String = request
            .messages
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .flat_map(|m| m.content.iter())
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        self.systems.lock().unwrap().push(system);
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: self.answer.clone(),
            }],
            TokenUsage::new(100, 50),
            StopReason::EndTurn,
        ))
    }
}

#[tokio::test]
async fn the_seat_reads_the_output_conventions_and_the_receipt_names_them() {
    // Red at 4a06aa3a: the system message carries the card only; no conventions digest.
    let provider = Recording {
        answer: find_graph("fromjson | [.[] | select(.id == 42)] | first | tojson")[0].clone(),
        systems: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(TICKETS).with_authoring_policy(policy(0));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let systems = provider.systems.lock().unwrap().clone();
    assert!(!systems.is_empty(), "the seat was called");
    let system = &systems[0];
    assert!(system.contains("# Output conventions"), "{system}");
    assert!(
        system.contains("returns one string, not an object"),
        "{system}"
    );
    assert!(system.contains(".content | fromjson"), "{system}");
    assert!(
        system.contains("Preserve literal path spelling"),
        "{system}"
    );
    // The line law exactly as the assembler emits it, and the write-back with its terminator.
    assert!(
        system.contains(
            r#"split("\n") | map(rtrimstr("\r")) | if .[-1] == "" then .[:-1] else . end"#
        ),
        "{system}"
    );
    assert!(system.contains(r#"join("\n") + "\n""#), "{system}");
    assert!(system.contains("« par <clé> »") && system.contains("map(.<field>)"));
    assert!(system.contains("When the request names no shape, keep the source's shape"));
    let digest = native(&out)["identity"]["conventions_sha256"].clone();
    assert_eq!(digest.as_str().map(str::len), Some(64), "{digest}");
}

/// A sentence opening with its source's whole name: the reader states `équipe.txt`, the human's
/// typed answer to the assembler's source question names the whole file.
const OPENING: &str = "Notes équipe.txt doit être copié tel quel dans sortie.txt.";
/// The same whole name beside a separately stated `équipe.txt` (no word of it is a head the
/// reader turns into an effect, so Law 1 alone judges these rounds).
const BESIDE: &str =
    "Notes équipe.txt et équipe.txt doivent être copiés tels quels dans sortie.txt.";
/// The typed answer to the assembler's source question, carried by the request as a host
/// carries every answer of the conversation.
const TYPED: &str = r#"["Notes équipe.txt"]"#;

/// The copy as a sketch: the read of the whole name, the write of its text. The compiler
/// emits the document and derives its permits; the write's only hole, its content, is its edge.
fn copy_sketch() -> String {
    json!({"name": "copy-notes", "tasks": [
        {"id": "read_notes", "verb": "invoke", "tool": "nika:read", "reads": ["Notes équipe.txt"], "purpose": "the notes"},
        {"id": "write_copy", "verb": "invoke", "tool": "nika:write", "writes": ["sortie.txt"], "with": [{"name": "notes", "from": "read_notes"}], "purpose": "the copy"}
    ], "questions": [], "gaps": [], "notes": "read → write"})
    .to_string()
}

const NO_FILLS: &str = r#"{"fills": [], "notes": "the write's content is its edge"}"#;

fn sketch_policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(0)
}

/// The paths one round's `UNREALIZED PATH` refusals name.
fn unrealized(round: &[String]) -> Vec<&str> {
    round
        .iter()
        .filter(|m| m.starts_with("UNREALIZED PATH"))
        .filter_map(|m| m.split('`').nth(1))
        .collect()
}

/// The request identity the decision records.
fn intent_of(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["intent_sha256"].clone()
}

/// The Ready candidate as a document.
fn document(out: &CompileOutcome) -> Value {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

#[tokio::test]
async fn a_typed_whole_source_name_is_read_whole_at_the_native_door() {
    // The same graph each time: the human's typed answer is the only difference, never the
    // literal the seat wrote. Without it the whole name is not the request's.
    let provider = Rotating::new(vec![copy_sketch(), NO_FILLS.to_owned()]);
    let req = CompileRequest::create(OPENING)
        .with_authoring_policy(policy(0))
        .answer("const.source_paths", TYPED);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&req, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    // The sketch, its fills and the whole-request judgment.
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 3);
    assert!(rounds(&out).iter().all(Vec::is_empty), "{out:#?}");
    assert_eq!(native(&out)["accepted"], true, "{out:#?}");
    assert_eq!(intent_of(&out), intent_sha256(OPENING));
    let doc = document(&out);
    assert_eq!(doc["permits"]["fs"]["read"], json!(["Notes équipe.txt"]));
    assert_eq!(doc["permits"]["fs"]["write"], json!(["sortie.txt"]));
    let provider = Rotating::new(vec![copy_sketch(), NO_FILLS.to_owned()]);
    let req = CompileRequest::create(OPENING).with_authoring_policy(policy(0));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "no fill after a refused sketch"
    );
    assert_ne!(native(&out)["accepted"], true, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let rounds = rounds(&out);
    assert_eq!(unrealized(&rounds[0]), ["équipe.txt"], "{rounds:?}");
}

#[tokio::test]
async fn a_separately_stated_suffix_stays_owed_at_the_native_door() {
    // The typed whole name realizes only its own occurrence: the other `équipe.txt` is refused
    // with no repair left, and read by the one repair the budget allows.
    let typed = |repairs| {
        CompileRequest::create(BESIDE)
            .with_authoring_policy(policy(repairs))
            .answer("const.source_paths", TYPED)
    };
    let provider = Rotating::new(vec![copy_sketch(), NO_FILLS.to_owned()]);
    let out = compile_with_provider(&typed(0), &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(unrealized(&rounds(&out)[0]), ["équipe.txt"], "{out:#?}");
    let merge = sketch_of(&json!([
        node(
            "read_notes",
            "nika:read",
            &json!({"reads": ["Notes équipe.txt"]})
        ),
        node("read_team", "nika:read", &json!({"reads": ["équipe.txt"]})),
        node(
            "write_merge",
            "nika:write",
            &json!({"writes": ["sortie.txt"],
            "with": [{"name": "notes", "from": "read_notes"}, {"name": "team", "from": "read_team"}]})
        ),
    ]));
    let content = json!([{"task": "write_merge", "field": "args.content", "value": "${{ with.notes }}\n${{ with.team }}"}]);
    let provider = Rotating::new(vec![copy_sketch(), merge, fills_of(&content)]);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&typed(1), &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    // The sketch, its repair, its fills and the whole-request judgment.
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 4);
    let rounds = rounds(&out);
    assert_eq!(unrealized(&rounds[0]), ["équipe.txt"], "{rounds:?}");
    assert!(rounds[1].is_empty() && rounds[2].is_empty(), "{rounds:?}");
    assert_eq!(intent_of(&out), intent_sha256(BESIDE));
    let doc = document(&out);
    assert_eq!(
        doc["permits"]["fs"]["read"],
        json!(["Notes équipe.txt", "équipe.txt"])
    );
    assert_eq!(doc["permits"]["fs"]["write"], json!(["sortie.txt"]));
}

#[tokio::test]
async fn a_typed_whole_source_name_is_read_whole_at_the_sketch_door() {
    let provider = Rotating::new(vec![copy_sketch(), NO_FILLS.to_owned()]);
    let req = CompileRequest::create(OPENING)
        .with_authoring_policy(sketch_policy())
        .answer("const.source_paths", TYPED);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&req, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(receipt.calls, 3);
    assert_eq!(receipt.context[0]["call"], "sketch");
    assert_eq!(receipt.context[1]["call"], "fill");
    assert_eq!(receipt.context[2]["call"], "judge_request");
    assert_eq!(
        native(&out)["sketch"],
        json!({"accepted": true, "tasks": 2, "holes": 1}),
        "{out:#?}"
    );
    assert_eq!(native(&out)["accepted"], true, "{out:#?}");
    assert_eq!(intent_of(&out), intent_sha256(OPENING));
    let doc = document(&out);
    assert_eq!(doc["permits"]["fs"]["read"], json!(["Notes équipe.txt"]));
    assert_eq!(doc["permits"]["fs"]["write"], json!(["sortie.txt"]));
}

#[tokio::test]
async fn a_separately_stated_suffix_stays_owed_at_the_sketch_door() {
    let provider = Rotating::new(vec![copy_sketch(), NO_FILLS.to_owned()]);
    let req = CompileRequest::create(BESIDE)
        .with_authoring_policy(sketch_policy())
        .answer("const.source_paths", TYPED);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "no fill after a refused sketch"
    );
    assert_eq!(native(&out)["sketch"]["accepted"], false, "{out:#?}");
    assert_eq!(unrealized(&rounds(&out)[0]), ["équipe.txt"], "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(intent_of(&out), intent_sha256(BESIDE));
}

/// The reader glues `Copie équipe.txt` after `dans` into one destination: its `équipe.txt` is
/// a destination occurrence of the `équipe.txt` the sentence opens with.
const DESTINED: &str = "Notes équipe.txt doit aller dans Copie équipe.txt.";
/// The destination's name typed among the sources.
const BOTH_TYPED: &str = r#"["Notes équipe.txt", "Copie équipe.txt"]"#;
/// The opening name's last word also ends a separately stated rooted path.
const ARCHIVED: &str = "Notes équipe.txt doit être comparé avec ./archive/équipe.txt.";

/// A sketch of `tasks`, nothing else.
fn sketch_of(tasks: &Value) -> String {
    json!({"name": "notes-copy", "tasks": tasks, "questions": [], "gaps": [], "notes": "sketch"})
        .to_string()
}

fn route(out: &CompileOutcome) -> String {
    out.provenance.decision.as_ref().unwrap()["route"].to_string()
}

/// One sketch task, its extra fields merged.
fn node(id: &str, tool: &str, extra: &Value) -> Value {
    let mut t = json!({"id": id, "verb": "invoke", "tool": tool, "purpose": id});
    for (k, v) in extra.as_object().unwrap() {
        t[k] = v.clone();
    }
    t
}

fn fills_of(fills: &Value) -> String {
    json!({"fills": fills, "notes": "fills"}).to_string()
}

/// The note sent to the stated hook as a sketch: read, then (behind a review when `gate` names
/// how) the send; its fills.
fn send_graph(gate: Option<&Value>) -> Vec<String> {
    let note = json!([{"name": "note", "from": "read_note"}]);
    let mut tasks = vec![node(
        "read_note",
        "nika:read",
        &json!({"reads": ["./note.txt"]}),
    )];
    let mut send = node(
        "send",
        "nika:notify",
        &json!({"hosts": ["hooks.example.test"], "with": note}),
    );
    let mut fills = vec![
        json!({"task": "send", "field": "args.target", "value": "https://hooks.example.test/in"}),
        json!({"task": "send", "field": "args.message", "value": "${{ with.note }}"}),
    ];
    if let Some(gate) = gate {
        tasks.push(node("review", "nika:prompt", &json!({"with": note})));
        for (k, v) in gate.as_object().unwrap() {
            send[k] = v.clone();
        }
        fills.push(json!({"task": "review", "field": "args.message", "value": "Envoyer cette note ? ${{ with.note }}"}));
    }
    tasks.push(send);
    vec![sketch_of(&json!(tasks)), fills_of(&json!(fills))]
}

/// The ticket finder as a sketch (read, jq, write) and its expression fill.
fn find_graph(expression: &str) -> Vec<String> {
    let tasks = json!([
        node(
            "read_tickets",
            "nika:read",
            &json!({"reads": ["./tickets.json"]})
        ),
        node(
            "find_ticket",
            "nika:jq",
            &json!({"with": [{"name": "text", "from": "read_tickets"}]})
        ),
        node(
            "write_ticket",
            "nika:write",
            &json!({"writes": ["./out/ticket.json"], "with": [{"name": "ticket", "from": "find_ticket"}]})
        ),
    ]);
    let fills = json!([{"task": "find_ticket", "field": "expression", "value": expression}]);
    vec![sketch_of(&tasks), fills_of(&fills)]
}

/// A copy as a sketch: the read of `source`, the write of its text at `destination`.
fn copy_graph(source: &str, destination: &str) -> String {
    sketch_of(&json!([
        node("read_source", "nika:read", &json!({"reads": [source]})),
        node(
            "write_copy",
            "nika:write",
            &json!({"writes": [destination], "with": [{"name": "text", "from": "read_source"}]})
        ),
    ]))
}

/// A complete document that only reads both files the source answer names: no write realizes the
/// destination the request states.
const READ_BOTH: &str = "nika: notes-copy\npermits:\n  tools: [\"nika:read\"]\n  fs:\n    read: [\"Notes équipe.txt\", \"Copie équipe.txt\"]\ntasks:\n  read_notes:\n    invoke:\n      tool: \"nika:read\"\n      args:\n        path: \"Notes équipe.txt\"\n  read_copy:\n    invoke:\n      tool: \"nika:read\"\n      args:\n        path: \"Copie équipe.txt\"\n";

#[tokio::test]
async fn a_source_answer_never_stands_for_the_destination_at_the_document_door() {
    // The same law on the default route, which opens the document door: the `équipe.txt` after
    // `dans` stays owed whatever the source answer says, so a document that only reads both
    // files is refused; the document that writes the destination (the one the explicit sketch
    // door emits from the same graph) is Ready under exactly those grants.
    let escalate = |answer: &str| {
        CompileRequest::create(DESTINED)
            .with_authoring_policy(policy(1).with_native(NativeMode::Escalate))
            .answer("const.source_paths", answer)
    };
    let provider = Rotating::new(vec![common::document_answer(READ_BOTH)]);
    let out = compile_with_provider(&escalate(BOTH_TYPED), &provider)
        .await
        .unwrap();
    assert!(route(&out).contains("native: document"), "{}", route(&out));
    assert_ne!(native(&out)["accepted"], true, "{out:#?}");
    assert_eq!(unrealized(&rounds(&out)[0]), ["équipe.txt"], "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let read = |path: &str, id: &str| node(id, "nika:read", &json!({"reads": [path]}));
    let write_copie = sketch_of(&json!([
        read("Notes équipe.txt", "read_notes"),
        node(
            "write_copy",
            "nika:write",
            &json!({"writes": ["Copie équipe.txt"], "with": [{"name": "notes", "from": "read_notes"}]})
        )
    ]));
    let sketched = CompileRequest::create(DESTINED)
        .with_authoring_policy(sketch_policy())
        .answer("const.source_paths", TYPED);
    let written = common::sketched(&sketched, vec![write_copie, NO_FILLS.to_owned()]).await;
    let provider = Rotating::new(vec![common::document_answer(&written)]);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&escalate(TYPED), &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "the complete document at the first call"
    );
    assert_eq!(intent_of(&out), intent_sha256(DESTINED));
    let doc = document(&out);
    assert_eq!(doc["permits"]["fs"]["read"], json!(["Notes équipe.txt"]));
    assert_eq!(doc["permits"]["fs"]["write"], json!(["Copie équipe.txt"]));
}

#[tokio::test]
async fn a_source_answer_never_stands_for_the_destination_at_the_sketch_door() {
    let read = |path: &str, id: &str| json!({"id": id, "verb": "invoke", "tool": "nika:read", "reads": [path], "purpose": id});
    let read_both = sketch_of(&json!([
        read("Notes équipe.txt", "read_notes"),
        read("Copie équipe.txt", "read_copy")
    ]));
    let provider = Rotating::new(vec![read_both, NO_FILLS.to_owned()]);
    let req = CompileRequest::create(DESTINED)
        .with_authoring_policy(sketch_policy())
        .answer("const.source_paths", BOTH_TYPED);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "no fill after a refused sketch"
    );
    assert_eq!(native(&out)["sketch"]["accepted"], false, "{out:#?}");
    assert_eq!(unrealized(&rounds(&out)[0]), ["équipe.txt"], "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let write_copie = sketch_of(&json!([
        read("Notes équipe.txt", "read_notes"),
        {"id": "write_copy", "verb": "invoke", "tool": "nika:write", "writes": ["Copie équipe.txt"],
         "with": [{"name": "notes", "from": "read_notes"}], "purpose": "the copy"}
    ]));
    let provider = Rotating::new(vec![write_copie, NO_FILLS.to_owned()]);
    let req = CompileRequest::create(DESTINED)
        .with_authoring_policy(sketch_policy())
        .answer("const.source_paths", TYPED);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&req, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert_eq!(intent_of(&out), intent_sha256(DESTINED));
    let doc = document(&out);
    assert_eq!(doc["permits"]["fs"]["read"], json!(["Notes équipe.txt"]));
    assert_eq!(doc["permits"]["fs"]["write"], json!(["Copie équipe.txt"]));
}

#[tokio::test]
async fn a_rooted_path_keeps_its_own_suffix_at_the_native_door() {
    // The `équipe.txt` that ends `./archive/équipe.txt` is that path's: the opening name typed
    // and both files read, nothing more is owed and no write is granted.
    let read = |path: &str, id: &str| node(id, "nika:read", &json!({"reads": [path]}));
    let both = sketch_of(&json!([
        read("Notes équipe.txt", "read_notes"),
        read("./archive/équipe.txt", "read_archive")
    ]));
    let provider = Rotating::new(vec![both, NO_FILLS.to_owned()]);
    let req = CompileRequest::create(ARCHIVED)
        .with_authoring_policy(policy(0))
        .answer("const.source_paths", TYPED);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&req, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert!(rounds(&out).iter().all(Vec::is_empty), "{out:#?}");
    assert_eq!(intent_of(&out), intent_sha256(ARCHIVED));
    let doc = document(&out);
    assert_eq!(
        doc["permits"]["fs"]["read"],
        json!(["Notes équipe.txt", "./archive/équipe.txt"])
    );
    assert!(doc["permits"]["fs"].get("write").is_none(), "{doc:#}");
    // Unread, the rooted path is owed on its own, and only it.
    let provider = Rotating::new(vec![sketch_of(&json!([read(
        "Notes équipe.txt",
        "read_notes"
    )]))]);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(
        unrealized(&rounds(&out)[0]),
        ["./archive/équipe.txt"],
        "{out:#?}"
    );
}
