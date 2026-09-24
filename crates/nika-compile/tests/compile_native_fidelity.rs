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
use common::Rotating;

/// The reader sees the gate and no effect word it knows in this request (its plan carries
/// `lexicon::GATE_WITHOUT_EFFECT`): the Law 3b case, asserted below before it is used.
const UNBOUND: &str = "Lis ./note.txt et prépare son envoi à https://hooks.example.test/in. Demande-moi explicitement avant de l’envoyer.";

fn policy(repairs: u32) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Only)
        .with_repairs(repairs)
}

fn answer(candidate: &str) -> String {
    json!({"candidate": candidate, "questions": [], "gaps": [], "notes": "read → review → send"})
        .to_string()
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

fn send_note(guard: &str) -> String {
    format!(
        r#"nika: send-note
permits:
  tools: ["nika:read", "nika:prompt", "nika:fetch"]
  fs:
    read: ["./note.txt"]
  net:
    http: ["hooks.example.test"]
tasks:
  read_note:
    invoke:
      tool: "nika:read"
      args: {{ path: "./note.txt" }}
{guard}"#
    )
}

const GATED: &str = r#"  review:
    with: { note: "${{ tasks.read_note.output }}" }
    invoke:
      tool: "nika:prompt"
      args: { message: "Envoyer cette note ? ${{ with.note }}" }
  send:
    with: { approved: "${{ tasks.review.output }}", note: "${{ tasks.read_note.output }}" }
    when: "${{ with.approved == true }}"
    invoke:
      tool: "nika:fetch"
      args: { url: "https://hooks.example.test/in", method: POST, body: "${{ with.note }}" }
"#;

const UNGATED: &str = r#"  send:
    with: { note: "${{ tasks.read_note.output }}" }
    invoke:
      tool: "nika:fetch"
      args: { url: "https://hooks.example.test/in", method: POST, body: "${{ with.note }}" }
"#;

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
    let provider = Rotating::new(vec![answer(&send_note(GATED))]);
    let req = CompileRequest::create(UNBOUND).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(native(&out)["accepted"], true, "{out:#?}");
    let rounds = rounds(&out);
    assert_eq!(rounds.len(), 1, "{rounds:?}");
    assert!(rounds[0].is_empty(), "{rounds:?}");
    assert_ne!(out.status, CompileStatus::Refused, "{out:#?}");
}

#[tokio::test]
async fn a_final_send_without_the_stated_approval_never_passes() {
    // Red at 4a06aa3a: no law fires and the ungated send is accepted.
    let provider = Rotating::new(vec![answer(&send_note(UNGATED))]);
    let req = CompileRequest::create(UNBOUND).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(native(&out)["accepted"], false, "{out:#?}");
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
    // Bypass attempt: the prompt exists, the send waits for it (`after:`) but ignores the answer.
    let waits = GATED.replace(
        "    when: \"${{ with.approved == true }}\"\n",
        "    after: { review: success }\n",
    );
    let waits = waits.replace(
        "with: { approved: \"${{ tasks.review.output }}\", note:",
        "with: { note:",
    );
    let provider = Rotating::new(vec![answer(&send_note(&waits))]);
    let req = CompileRequest::create(UNBOUND).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(native(&out)["accepted"], false, "{out:#?}");
    let first = &rounds(&out)[0];
    assert!(
        first
            .iter()
            .any(|m| m.starts_with("APPROVAL ORDER") && m.contains("`send`")),
        "{first:?}"
    );
    assert!(
        !first.iter().any(|m| m.starts_with("INVENTED GATE")),
        "{first:?}"
    );
}

/// The reader binds this approval to the write (a human-first `write`): Law 3, bound.
const DRAFT: &str =
    "Lis ./draft.md et écris-le dans ./out/final.md, mais demande-moi avant d'écrire.";

fn write_draft(review_args: &str) -> String {
    format!(
        r#"nika: write-draft
permits:
  tools: ["nika:read", "nika:prompt", "nika:write"]
  fs:
    read: ["./draft.md"]
    write: ["./out/final.md"]
tasks:
  read_draft:
    invoke:
      tool: "nika:read"
      args: {{ path: "./draft.md" }}
  review:
    with: {{ draft: "${{{{ tasks.read_draft.output }}}}" }}
    invoke:
      tool: "nika:prompt"
      args: {{ message: "Écrire ce brouillon ? ${{{{ with.draft }}}}"{review_args} }}
  write_final:
    with: {{ approved: "${{{{ tasks.review.output }}}}", draft: "${{{{ tasks.read_draft.output }}}}" }}
    when: "${{{{ with.approved == true }}}}"
    invoke:
      tool: "nika:write"
      args: {{ path: "./out/final.md", content: "${{{{ with.draft }}}}", overwrite: true, create_dirs: true }}
"#
    )
}

#[tokio::test]
async fn a_bound_write_approved_by_a_defaulted_yes_is_refused_and_the_human_confirm_passes() {
    // Red at 4a06aa3a: Check is clean (the affirmative-consent lane judges the answer, not who
    // gives it) and Law 3 accepted any dependency on the prompt, so a `default: true` that says
    // yes with nobody there reached the write. The repair asks a human.
    let provider = Rotating::new(vec![
        answer(&write_draft(", default: true")),
        answer(&write_draft("")),
    ]);
    let req = CompileRequest::create(DRAFT).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let rounds = rounds(&out);
    assert_eq!(rounds.len(), 2, "{rounds:?}");
    assert!(
        rounds[0]
            .iter()
            .any(|m| m.starts_with("APPROVAL ORDER") && m.contains("`write_final`")),
        "{rounds:?}"
    );
    assert!(rounds[1].is_empty(), "{rounds:?}");
    assert_eq!(native(&out)["accepted"], true, "{out:#?}");
}

const TICKETS: &str = "Lis ./tickets.json et écris le ticket 42 dans ./out/ticket.json.";

fn find_ticket(expression: &str) -> String {
    format!(
        r#"nika: find-ticket
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs:
    read: ["./tickets.json"]
    write: ["./out/ticket.json"]
tasks:
  read_tickets:
    invoke:
      tool: "nika:read"
      args: {{ path: "./tickets.json" }}
  find_ticket:
    with: {{ text: "${{{{ tasks.read_tickets.output }}}}" }}
    invoke:
      tool: "nika:jq"
      args: {{ input: "${{{{ with.text }}}}", expression: '{expression}' }}
  write_ticket:
    with: {{ ticket: "${{{{ tasks.find_ticket.output }}}}" }}
    invoke:
      tool: "nika:write"
      args: {{ path: "./out/ticket.json", content: "${{{{ with.ticket }}}}", overwrite: true, create_dirs: true }}
"#
    )
}

#[tokio::test]
async fn records_read_from_raw_text_are_refused_before_ready_and_the_parsed_repair_passes() {
    // Red at 4a06aa3a: round 0 is accepted (Check-clean) and the Run fails NIKA-BUILTIN-JQ-001.
    let provider = Rotating::new(vec![
        answer(&find_ticket("[.[] | select(.id == 42)] | first | tojson")),
        answer(&find_ticket(
            "fromjson | [.[] | select(.id == 42)] | first | tojson",
        )),
    ]);
    let req = CompileRequest::create(TICKETS).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let rounds = rounds(&out);
    assert_eq!(rounds.len(), 2, "{rounds:?}");
    assert!(
        rounds[0]
            .iter()
            .any(|m| m.starts_with("RAW TEXT AS RECORDS")
                && m.contains("`find_ticket`")
                && m.contains("`read_tickets`")),
        "{rounds:?}"
    );
    assert!(rounds[1].is_empty(), "{rounds:?}");
    assert_eq!(native(&out)["accepted"], true, "{out:#?}");
}

#[tokio::test]
async fn string_operations_on_a_reads_text_stay_admissible() {
    // Control: a line transform over the read's text is the proper use of the text.
    let provider = Rotating::new(vec![answer(&find_ticket(
        r#"split("\n") | map(rtrimstr("\r")) | if .[-1] == "" then .[:-1] else . end | map(select(test("42"))) | first"#,
    ))]);
    let req = CompileRequest::create(TICKETS).with_authoring_policy(policy(1));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let rounds = rounds(&out);
    assert!(
        !rounds[0]
            .iter()
            .any(|m| m.starts_with("RAW TEXT AS RECORDS")),
        "{rounds:?}"
    );
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
        answer: answer(&find_ticket(
            "fromjson | [.[] | select(.id == 42)] | first | tojson",
        )),
        systems: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(TICKETS).with_authoring_policy(policy(0));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let systems = provider.systems.lock().unwrap().clone();
    assert!(!systems.is_empty(), "the seat was called");
    let system = &systems[0];
    assert!(system.contains("# Output conventions"), "{system}");
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

const COPY_WHOLE: &str = r#"nika: copy-notes
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["Notes équipe.txt"]
    write: ["sortie.txt"]
tasks:
  read_notes:
    invoke:
      tool: "nika:read"
      args: { path: "Notes équipe.txt" }
  write_copy:
    with: { notes: "${{ tasks.read_notes.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "sortie.txt", content: "${{ with.notes }}", overwrite: true, create_dirs: true }
"#;

const MERGE_BOTH: &str = r#"nika: merge-notes
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["Notes équipe.txt", "équipe.txt"]
    write: ["sortie.txt"]
tasks:
  read_notes:
    invoke:
      tool: "nika:read"
      args: { path: "Notes équipe.txt" }
  read_team:
    invoke:
      tool: "nika:read"
      args: { path: "équipe.txt" }
  write_merge:
    with: { notes: "${{ tasks.read_notes.output }}", team: "${{ tasks.read_team.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "sortie.txt", content: "${{ with.notes }}\n${{ with.team }}", overwrite: true, create_dirs: true }
"#;

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
    // One call each, the same candidate: the human's typed answer is the only difference, never
    // the literal the seat wrote.
    let provider = Rotating::new(vec![answer(COPY_WHOLE)]);
    let req = CompileRequest::create(OPENING)
        .with_authoring_policy(policy(0))
        .answer("const.source_paths", TYPED);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 1);
    assert_eq!(rounds(&out), [Vec::<String>::new()], "{out:#?}");
    assert_eq!(native(&out)["accepted"], true, "{out:#?}");
    assert_eq!(intent_of(&out), intent_sha256(OPENING));
    let doc = document(&out);
    assert_eq!(doc["permits"]["fs"]["read"], json!(["Notes équipe.txt"]));
    assert_eq!(doc["permits"]["fs"]["write"], json!(["sortie.txt"]));
    let provider = Rotating::new(vec![answer(COPY_WHOLE)]);
    let req = CompileRequest::create(OPENING).with_authoring_policy(policy(0));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(native(&out)["accepted"], false, "{out:#?}");
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
    let provider = Rotating::new(vec![answer(COPY_WHOLE)]);
    let out = compile_with_provider(&typed(0), &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(unrealized(&rounds(&out)[0]), ["équipe.txt"], "{out:#?}");
    let provider = Rotating::new(vec![answer(COPY_WHOLE), answer(MERGE_BOTH)]);
    let out = compile_with_provider(&typed(1), &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 2);
    let rounds = rounds(&out);
    assert_eq!(unrealized(&rounds[0]), ["équipe.txt"], "{rounds:?}");
    assert!(rounds[1].is_empty(), "{rounds:?}");
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
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(receipt.calls, 2);
    assert_eq!(receipt.context[0]["call"], "sketch");
    assert_eq!(receipt.context[1]["call"], "fill");
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

const READ_BOTH: &str = r#"nika: read-notes
permits:
  tools: ["nika:read"]
  fs:
    read: ["Notes équipe.txt", "Copie équipe.txt"]
tasks:
  read_notes:
    invoke:
      tool: "nika:read"
      args: { path: "Notes équipe.txt" }
  read_copy:
    invoke:
      tool: "nika:read"
      args: { path: "Copie équipe.txt" }
outputs:
  notes: ${{ tasks.read_notes.output }}
  copy: ${{ tasks.read_copy.output }}
"#;

const WRITE_COPIE: &str = r#"nika: copy-notes
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["Notes équipe.txt"]
    write: ["Copie équipe.txt"]
tasks:
  read_notes:
    invoke:
      tool: "nika:read"
      args: { path: "Notes équipe.txt" }
  write_copy:
    with: { notes: "${{ tasks.read_notes.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "Copie équipe.txt", content: "${{ with.notes }}", overwrite: true, create_dirs: true }
"#;

const READ_ARCHIVE: &str = r#"nika: read-archive
permits:
  tools: ["nika:read"]
  fs:
    read: ["Notes équipe.txt", "./archive/équipe.txt"]
tasks:
  read_notes:
    invoke:
      tool: "nika:read"
      args: { path: "Notes équipe.txt" }
  read_archive:
    invoke:
      tool: "nika:read"
      args: { path: "./archive/équipe.txt" }
outputs:
  notes: ${{ tasks.read_notes.output }}
  archive: ${{ tasks.read_archive.output }}
"#;

const READ_NOTES: &str = r#"nika: read-notes
permits:
  tools: ["nika:read"]
  fs:
    read: ["Notes équipe.txt"]
tasks:
  read_notes:
    invoke:
      tool: "nika:read"
      args: { path: "Notes équipe.txt" }
outputs:
  notes: ${{ tasks.read_notes.output }}
"#;

/// A sketch of `tasks`, nothing else.
fn sketch_of(tasks: &Value) -> String {
    json!({"name": "notes-copy", "tasks": tasks, "questions": [], "gaps": [], "notes": "sketch"})
        .to_string()
}

#[tokio::test]
async fn a_source_answer_never_stands_for_the_destination_at_the_native_door() {
    // Both names typed as sources and read, nothing written: the `équipe.txt` after `dans` is
    // owed, whatever the source answer says.
    let provider = Rotating::new(vec![answer(READ_BOTH)]);
    let req = CompileRequest::create(DESTINED)
        .with_authoring_policy(policy(0))
        .answer("const.source_paths", BOTH_TYPED);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(unrealized(&rounds(&out)[0]), ["équipe.txt"], "{out:#?}");
    assert_eq!(intent_of(&out), intent_sha256(DESTINED));
    // The source typed, the destination actually written: Ready under exactly those grants.
    let provider = Rotating::new(vec![answer(WRITE_COPIE)]);
    let req = CompileRequest::create(DESTINED)
        .with_authoring_policy(policy(0))
        .answer("const.source_paths", TYPED);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(rounds(&out), [Vec::<String>::new()], "{out:#?}");
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
    let out = compile_with_provider(&req, &provider).await.unwrap();
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
    let provider = Rotating::new(vec![answer(READ_ARCHIVE)]);
    let req = CompileRequest::create(ARCHIVED)
        .with_authoring_policy(policy(0))
        .answer("const.source_paths", TYPED);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(rounds(&out), [Vec::<String>::new()], "{out:#?}");
    assert_eq!(intent_of(&out), intent_sha256(ARCHIVED));
    let doc = document(&out);
    assert_eq!(
        doc["permits"]["fs"]["read"],
        json!(["Notes équipe.txt", "./archive/équipe.txt"])
    );
    assert!(doc["permits"]["fs"].get("write").is_none(), "{doc:#}");
    // Unread, the rooted path is owed on its own, and only it.
    let provider = Rotating::new(vec![answer(READ_NOTES)]);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(
        unrealized(&rounds(&out)[0]),
        ["./archive/équipe.txt"],
        "{out:#?}"
    );
}
