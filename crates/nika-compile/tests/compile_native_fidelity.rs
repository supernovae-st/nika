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
//!   the engine's output conventions beside the card, and the receipt names their digest.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode,
    compile_with_provider,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, Role, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::sync::Mutex;
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
        nika_compile_reader::fidelity::unbound_final_gate(&plan),
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
