// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The source-only hold on an `mcp:` step and the way it lifts: the host's pins. The shapes are
//! the steps a Session drafts for connected apps: a literal read, a read chained on an upstream
//! output, a list without arguments, and a write. Outcomes come from the core's own `finish`.

use super::{
    Attempt, EffectCounts, PinnedTool, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse,
    judged_tools,
};
use nika_compile::surface::{UNJUDGED_DEPENDENCY, finding, finish, initial, sha256};
use nika_compile::{CompileOutcome, CompileStatus};
use serde_json::{Value, json};
use std::time::Duration;

const FIND: &str = "\
nika: invoice-email
permits:
  tools: [\"mcp:abench/gmail_find_email\"]
tasks:
  find_email:
    invoke:
      tool: mcp:abench/gmail_find_email
      args: { query: invoice }
";

const CHAIN: &str = "\
nika: invoice-email
permits:
  tools: [\"mcp:abench/gmail_find_email\", \"mcp:abench/gmail_get_email_by_id\"]
tasks:
  find_email:
    invoke:
      tool: mcp:abench/gmail_find_email
      args: { query: invoice }
  read_email:
    with:
      found: ${{ tasks.find_email.output }}
    invoke:
      tool: mcp:abench/gmail_get_email_by_id
      args:
        id: ${{ with.found }}
";

const SLACK: &str = "\
nika: closed-deal
permits:
  tools: [\"mcp:abench/slack_list_channels\", \"mcp:abench/slack_send_channel_message\"]
tasks:
  list_channels:
    invoke:
      tool: mcp:abench/slack_list_channels
  post:
    with:
      channels: ${{ tasks.list_channels.output }}
    invoke:
      tool: mcp:abench/slack_send_channel_message
      args:
        channel: ${{ with.channels }}
        text: Deal closed
";

/// The pinned input schemas of the connected apps the shapes reach.
fn schema(tool: &str) -> Option<Value> {
    Some(match tool {
        "gmail_find_email" => json!({"type": "object", "additionalProperties": false,
            "properties": {"query": {"type": "string"}, "max_results": {"type": "integer"}},
            "required": ["query"]}),
        "gmail_get_email_by_id" => json!({"type": "object",
            "properties": {"id": {"type": "string"}}, "required": ["id"]}),
        "slack_list_channels" => json!({"type": "object", "properties": {}}),
        "slack_send_channel_message" => json!({"type": "object",
            "properties": {"channel": {"type": "string"}, "text": {"type": "string"}},
            "required": ["channel", "text"]}),
        _ => return None,
    })
}

/// A project that registered and approved `abench`, whose pins hold every tool above.
fn approved(server: &str, tool: &str) -> PinnedTool {
    match (server, schema(tool)) {
        ("abench", Some(input_schema)) => PinnedTool::Pinned {
            input_schema,
            output_schema: None,
            approved_at: "2026-10-13T08:00:00Z".to_owned(),
        },
        ("abench", None) => PinnedTool::NotPinned {
            pinned: vec!["gmail_find_email".to_owned()],
        },
        _ => PinnedTool::Unregistered,
    }
}

/// A host whose only answer is its pins.
struct Pins(fn(&str, &str) -> PinnedTool);

impl Rehearse for Pins {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        not_run(candidate)
    }
    fn bound(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn pinned(&self, server: &str, tool: &str) -> PinnedTool {
        (self.0)(server, tool)
    }
}

/// A host that lends only its pins: it rehearses nothing, and the runs it answers for make no
/// MCP call (a server whose runs have no MCP plane).
struct PinsOnly(fn(&str, &str) -> PinnedTool);

impl Rehearse for PinsOnly {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        not_run(candidate)
    }
    fn bound(&self) -> Duration {
        Duration::ZERO
    }
    fn pinned(&self, server: &str, tool: &str) -> PinnedTool {
        (self.0)(server, tool)
    }
    fn offers_rehearsal(&self) -> bool {
        false
    }
    fn runs_mcp_steps(&self) -> bool {
        false
    }
}

/// The default port reads no pins.
struct Unaware;

impl Rehearse for Unaware {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        not_run(candidate)
    }
    fn bound(&self) -> Duration {
        Duration::from_secs(1)
    }
}

/// The report of a host that runs nothing.
fn not_run(candidate: &str) -> RehearsalFuture<'_> {
    Box::pin(async move {
        let outcome = Rehearsal::NotRun {
            reason: "no room in this test".to_owned(),
        };
        let none = EffectCounts::none();
        RehearsalReport::new(outcome, Attempt::NeverAttempted, none, sha256(candidate))
    })
}

/// The candidate as `finish` leaves it, then judged by `host`.
fn judged(host: &dyn Rehearse, source: &str) -> CompileOutcome {
    let mut out = initial();
    finish(source.to_owned(), &mut out);
    judged_tools(host, &mut out);
    out
}

fn held(out: &CompileOutcome, task: &str) -> Option<String> {
    (out.diagnostics.iter())
        .find(|d| d.target == task && d.kind == nika_compile::DiagnosticKind::Unknown)
        .map(|d| d.message.clone())
}

fn mcp_record(out: &CompileOutcome) -> Value {
    (out.provenance.decision.as_ref()).map_or(Value::Null, |d| d["mcp"].clone())
}

#[test]
fn a_pinned_mcp_step_is_judged_not_held() {
    let out = judged(&Pins(approved), FIND);
    assert_eq!(held(&out, "find_email"), None, "{:#?}", out.diagnostics);
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert_eq!(
        mcp_record(&out),
        json!([{"task": "find_email", "output": "untyped", "tools": [{
            "tool": "mcp:abench/gmail_find_email", "verdict": "judged",
            "wired": [], "approved_at": "2026-10-13T08:00:00Z"}]}])
    );
}

/// The same pinned step on a host whose runs make no MCP call: its pins judge it, the record
/// says so, and it stays held in words naming why, never READY.
#[test]
fn a_judged_step_stays_held_where_the_runs_make_no_mcp_call() {
    let out = judged(&PinsOnly(approved), FIND);
    assert_eq!(
        out.status,
        CompileStatus::Incomplete,
        "{:#?}",
        out.diagnostics
    );
    let reason = held(&out, "find_email").expect("still held");
    assert!(reason.contains("this server runs no MCP step"), "{reason}");
    assert_ne!(reason, UNJUDGED_DEPENDENCY, "the hold says why it remains");
    let record = mcp_record(&out);
    assert_eq!(record[0]["tools"][0]["verdict"], "judged", "{record}");
    assert_eq!(record[0]["held"], json!(reason), "{record}");
}

/// A registry re-pointed after approval: the approval does not carry over, and the hold names
/// the approval command.
#[test]
fn a_repointed_server_keeps_the_hold_with_the_approval_command() {
    let out = judged(&Pins(|_, _| PinnedTool::Repointed), FIND);
    assert_eq!(
        out.status,
        CompileStatus::Incomplete,
        "{:#?}",
        out.diagnostics
    );
    let reason = held(&out, "find_email").expect("still held");
    assert!(reason.contains("nika mcp approve abench"), "{reason}");
    assert!(
        reason.contains("another command, arguments or URL"),
        "{reason}"
    );
    assert_eq!(mcp_record(&out)[0]["tools"][0]["verdict"], "held");
}

#[test]
fn templated_mcp_args_are_wired_not_refused() {
    let out = judged(&Pins(approved), CHAIN);
    assert_eq!(held(&out, "read_email"), None, "{:#?}", out.diagnostics);
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert_eq!(mcp_record(&out)[1]["tools"][0]["wired"], json!(["/id"]));
}

#[test]
fn a_no_args_pinned_list_and_a_pinned_write_are_judged() {
    let out = judged(&Pins(approved), SLACK);
    assert_eq!(held(&out, "list_channels"), None, "{:#?}", out.diagnostics);
    assert_eq!(held(&out, "post"), None, "{:#?}", out.diagnostics);
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert_eq!(
        mcp_record(&out)[1]["tools"][0]["wired"],
        json!(["/channel"])
    );
}

/// Whether Check's refusal of the candidate is stated on `check_preview` with exactly `code`.
fn refused_with(out: &CompileOutcome, code: &str) -> bool {
    let stated = format!("Check refuses the candidate ({code}): ");
    (out.diagnostics.iter()).any(|d| d.target == "check_preview" && d.message.starts_with(&stated))
}

#[test]
fn a_pinned_write_is_judged_and_still_needs_its_permit() {
    // A declared boundary that leaves the write out: the escape is NIKA-SEC-004.
    let unpermitted = SLACK.replace(", \"mcp:abench/slack_send_channel_message\"", "");
    let out = judged(&Pins(approved), &unpermitted);
    assert_eq!(held(&out, "post"), None, "judging is not the refusal");
    let clean = (out.check_preview.as_ref()).is_some_and(|p| p.report.is_clean());
    assert!(!clean, "permits stay the authority over the write");
    assert_ne!(out.status, CompileStatus::Ready);
    // With the hold lifted, Check's own refusal is the reason it waits, and it is shown.
    assert!(refused_with(&out, "NIKA-SEC-004"), "{:#?}", out.diagnostics);

    // No boundary declared at all: the same writes are undeclared effects, NIKA-AUTH-006.
    let boundless: Vec<&str> = (SLACK.lines())
        .filter(|line| !line.starts_with("permits:") && !line.starts_with("  tools:"))
        .collect();
    let out = judged(&Pins(approved), &boundless.join("\n"));
    assert_eq!(held(&out, "post"), None, "judging is not the refusal");
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(
        refused_with(&out, "NIKA-AUTH-006"),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn a_literal_arg_the_pinned_schema_refuses_holds_with_its_path() {
    let wrong = FIND.replace("{ query: invoice }", "{ query: invoice, max_results: ten }");
    let out = judged(&Pins(approved), &wrong);
    let reason = held(&out, "find_email").expect("held");
    assert!(reason.contains("args/max_results"), "{reason}");
    assert!(reason.contains("pinned input schema"), "{reason}");
    assert_ne!(out.status, CompileStatus::Ready);
    assert_eq!(mcp_record(&out)[0]["tools"][0]["verdict"], "held");
}

#[test]
fn an_unpinned_mcp_server_holds_with_a_pin_question() {
    let out = judged(&Pins(|_, _| PinnedTool::Unapproved), FIND);
    let reason = held(&out, "find_email").expect("held");
    assert!(reason.contains("registered but not approved"), "{reason}");
    assert!(reason.contains("nika mcp approve abench"), "{reason}");
    assert_ne!(out.status, CompileStatus::Ready);
}

#[test]
fn a_tool_outside_the_pinned_set_holds_naming_the_pinned_tools() {
    let unpinned = CHAIN.replace("gmail_get_email_by_id", "gmail_send");
    let out = judged(&Pins(approved), &unpinned);
    let reason = held(&out, "read_email").expect("held");
    assert!(reason.contains("pinned: gmail_find_email"), "{reason}");
    assert_eq!(held(&out, "find_email"), None, "the pinned step is judged");
}

#[test]
fn an_agent_with_pinned_mcp_tools_is_judged() {
    let agent = "\
nika: triage
permits:
  tools: [\"nika:done\", \"mcp:abench/gmail_find_email\", \"mcp:abench/*\"]
tasks:
  triage:
    agent:
      prompt: Find the invoice email.
      tools: [\"nika:done\", \"mcp:abench/gmail_find_email\", \"mcp:abench/*\"]
";
    let out = judged(&Pins(approved), agent);
    assert_eq!(held(&out, "triage"), None, "{:#?}", out.diagnostics);
    let tools = &mcp_record(&out)[0]["tools"];
    assert_eq!(tools[0]["verdict"], "judged");
    assert_eq!(tools[1]["tool"], "mcp:abench/*");
    assert_eq!(tools[1]["verdict"], "judged");
}

#[test]
fn a_host_without_a_pin_lookup_keeps_the_hold() {
    let out = judged(&Unaware, FIND);
    assert_eq!(
        held(&out, "find_email").as_deref(),
        Some(UNJUDGED_DEPENDENCY)
    );
    assert_eq!(mcp_record(&out), Value::Null);
    assert_ne!(out.status, CompileStatus::Ready);
}

#[test]
fn a_task_that_calls_no_mcp_tool_is_never_lifted() {
    // A hold on a task that names no `mcp:` tool (here set by hand) has nothing to judge: it
    // stays, and nothing is recorded.
    let source = "\
nika: plain
permits:
  tools: [\"nika:done\"]
tasks:
  triage:
    agent:
      prompt: Say done.
      tools: [\"nika:done\"]
";
    let mut out = initial();
    finish(source.to_owned(), &mut out);
    finding(
        &mut out,
        nika_compile::DiagnosticKind::Unknown,
        "triage",
        UNJUDGED_DEPENDENCY,
    );
    judged_tools(&Pins(approved), &mut out);
    assert_eq!(held(&out, "triage").as_deref(), Some(UNJUDGED_DEPENDENCY));
    assert_eq!(mcp_record(&out), Value::Null);
}

/// A host whose pins hold one schema for every tool.
struct SchemaPins(Value);

impl Rehearse for SchemaPins {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        not_run(candidate)
    }
    fn bound(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn pinned(&self, _server: &str, _tool: &str) -> PinnedTool {
        PinnedTool::Pinned {
            input_schema: self.0.clone(),
            output_schema: None,
            approved_at: "2026-10-13T08:00:00Z".to_owned(),
        }
    }
}

#[test]
fn a_referenced_schema_reaches_no_server_and_reads_no_file() {
    // A loopback server that would serve a valid schema to anyone asking, counting connections,
    // and a local file holding a valid schema: a judge that fetched or read either reference
    // would see a schema that compiles and lift the hold. It does neither.
    use std::io::Write as _;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    let port = listener.local_addr().expect("loopback address").port();
    let accepted = std::sync::Arc::new(AtomicUsize::new(0));
    let counter = std::sync::Arc::clone(&accepted);
    std::thread::Builder::new()
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                let body = r#"{"type": "object"}"#;
                let _served = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\n\r\n{body}",
                    body.len()
                );
            }
        })
        .expect("loopback server");
    let dir = std::env::temp_dir().join(format!("nika-pinned-ref-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let file = dir.join("schema.json");
    std::fs::write(&file, r#"{"type": "object"}"#).expect("fixture schema");
    let references = [
        format!("http://127.0.0.1:{port}/schema.json"),
        format!("file://{}", file.display()),
    ];
    for reference in references {
        let out = judged(&SchemaPins(json!({"$ref": reference})), FIND);
        let reason = held(&out, "find_email").expect("held");
        assert!(reason.contains("does not compile"), "{reason}");
        assert_ne!(out.status, CompileStatus::Ready);
    }
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        0,
        "no connection reached the loopback"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

const MIXED: &str = "\
nika: invoice-email
permits:
  tools: [\"mcp:abench/gmail_send\"]
tasks:
  find_email:
    invoke:
      tool: mcp:abench/gmail_find_email
      args: { query: invoice }
  send:
    with:
      found: ${{ tasks.find_email.output }}
    invoke:
      tool: mcp:abench/gmail_send
      args:
        id: ${{ with.found }}
";

fn check_refusals(out: &CompileOutcome) -> usize {
    (out.diagnostics.iter())
        .filter(|d| d.target == "check_preview")
        .count()
}

#[test]
fn a_partial_lift_states_check_refusals_beside_the_hold_that_remains() {
    // One tool is pinned and lifts, the other is not pinned and stays held, and the pinned
    // step's permit is missing: both remaining causes are stated, Check's once.
    let mut out = judged(&Pins(approved), MIXED);
    assert_eq!(held(&out, "find_email"), None, "{:#?}", out.diagnostics);
    let reason = held(&out, "send").expect("the unpinned tool stays held");
    assert!(reason.contains("pinned: gmail_find_email"), "{reason}");
    let stated = check_refusals(&out);
    // The boundary is declared and leaves the found email's tool out: NIKA-SEC-004.
    assert!(
        refused_with(&out, "NIKA-SEC-004"),
        "the missing permit, by its code: {:#?}",
        out.diagnostics
    );
    assert_ne!(out.status, CompileStatus::Ready);
    // Judging again adds nothing: no duplicate, no stale finding.
    judged_tools(&Pins(approved), &mut out);
    assert_eq!(check_refusals(&out), stated);

    // The reverse order: the permit is granted first and the unpinned tool still holds, so the
    // hold is the one reason shown; once both are resolved the candidate is READY.
    let permitted = MIXED.replace(
        "tools: [\"mcp:abench/gmail_send\"]",
        "tools: [\"mcp:abench/gmail_find_email\", \"mcp:abench/gmail_send\"]",
    );
    let out = judged(&Pins(approved), &permitted);
    assert!(held(&out, "send").is_some());
    assert_eq!(check_refusals(&out), 0, "{:#?}", out.diagnostics);
    let resolved = permitted.replace("gmail_send", "gmail_get_email_by_id");
    let out = judged(&Pins(approved), &resolved);
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert_eq!(check_refusals(&out), 0);
}
