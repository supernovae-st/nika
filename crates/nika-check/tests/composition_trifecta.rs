// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    reason = "fixture construction and required findings are test assertions"
)]

//! NIKA-SEC-009 across the composition wall (NIK-273): a child workflow that
//! can bring untrusted content in is an ingress for its caller, exactly as the
//! same task written inline. All sources come from the injected reader of the
//! public composed-check door: no filesystem, no network, no effect.

use nika_check::{CheckReport, check, check_composed};
use nika_schema::{ParseMode, parse, source::FileId};

const PARENT: &str = "parent.nika";

/// A child that fetches a page and returns it.
const CHILD_FETCH: &str = r#"nika: child-fetch
permits:
  tools: ["nika:fetch"]
  net: { http: ["docs.example.com"] }
tasks:
  page:
    invoke:
      tool: nika:fetch
      args: { url: "https://docs.example.com/page", mode: text }
outputs:
  text: { value: "${{ tasks.page.output }}", type: string }
"#;

/// A child whose subprocess output (a git log) is its result.
const CHILD_EXEC: &str = r#"nika: child-exec
permits:
  exec: ["git"]
  fs: { read: ["./repo/**"] }
tasks:
  log:
    exec: { command: ["git", "-C", "./repo", "log", "--oneline", "-3"] }
outputs:
  text: { value: "${{ tasks.log.output }}", type: string }
"#;

/// A child that brings nothing in: a model summary of a literal prompt.
const CHILD_PURE: &str = r#"nika: child-pure
model: mock/echo
permits: {}
tasks:
  brief:
    infer: { prompt: "two sentences on the DAG", max_tokens: 64 }
outputs:
  text: { value: "${{ tasks.brief.output }}", type: string }
"#;

/// A middle child with no ingress of its own: it calls `./leaf.nika`.
const CHILD_MIDDLE: &str = r#"nika: child-middle
permits:
  tools: ["nika:fetch"]
  net: { http: ["docs.example.com"] }
tasks:
  call:
    invoke: { workflow: "./leaf.nika" }
    returns: { object: { text: string } }
outputs:
  text: { value: "${{ tasks.call.output.text }}", type: string }
"#;

/// The parent: a private read and the child's result leave through a webhook.
/// `{grants}` completes its `permits:` for the child it calls.
const PARENT_UNGATED: &str = r#"nika: parent
permits:
  tools: ["nika:read", "nika:notify"{tools}]
  net: { http: ["hooks.example.com"{hosts}] }
  fs: { read: ["./private.txt"{reads}] }{exec}
tasks:
  call:
    invoke: { workflow: "{target}" }
    returns: { object: { text: string } }
  note:
    invoke: { tool: nika:read, args: { path: ./private.txt } }
  ping:
    with:
      page: ${{ tasks.call.output.text }}
      note: ${{ tasks.note.output }}
    invoke:
      tool: nika:notify
      args:
        channel: webhook
        target: https://hooks.example.com/hook
        message: "page: ${{ with.page }}"
        data: { note: "${{ with.note }}" }
"#;

/// The same parent with a blocking human gate upstream of both entries and
/// the egress bound to the affirmative answer.
const PARENT_GATED: &str = r#"nika: parent
permits:
  tools: ["nika:read", "nika:notify", "nika:prompt"{tools}]
  net: { http: ["hooks.example.com"{hosts}] }
  fs: { read: ["./private.txt"{reads}] }{exec}
tasks:
  ask:
    invoke: { tool: nika:prompt, args: { message: "Send the page and the note?" } }
  call:
    after: { ask: success }
    invoke: { workflow: "{target}" }
    returns: { object: { text: string } }
  note:
    after: { ask: success }
    invoke: { tool: nika:read, args: { path: ./private.txt } }
  ping:
    with:
      go: ${{ tasks.ask.output }}
      page: ${{ tasks.call.output.text }}
      note: ${{ tasks.note.output }}
    when: ${{ with.go == true }}
    invoke:
      tool: nika:notify
      args:
        channel: webhook
        target: https://hooks.example.com/hook
        message: "page: ${{ with.page }}"
        data: { note: "${{ with.note }}" }
"#;

/// Which grants the parent holds for its child (spec 14 law 3: the child runs
/// inside the parent's boundary, so COMP-002 requires them here).
#[derive(Clone, Copy)]
enum Holds {
    Fetch,
    Git,
    Nothing,
}

fn parent(template: &str, target: &str, holds: Holds) -> String {
    let (tools, hosts, reads, exec) = match holds {
        Holds::Fetch => (r#", "nika:fetch""#, r#", "docs.example.com""#, "", ""),
        Holds::Git => ("", "", r#", "./repo/**""#, "\n  exec: [\"git\"]"),
        Holds::Nothing => ("", "", "", ""),
    };
    template
        .replace("{tools}", tools)
        .replace("{hosts}", hosts)
        .replace("{reads}", reads)
        .replace("{exec}", exec)
        .replace("{target}", target)
}

fn parsed(source: &str) -> nika_schema::raw::RawWorkflow {
    parse(source, FileId::new(0), ParseMode::Strict).expect("the fixture parses")
}

/// The composed check over `parent`, its files served from `files`.
fn composed(parent: &str, files: &[(&str, &str)]) -> CheckReport {
    check_composed(&parsed(parent), PARENT, &mut |path| {
        files
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, text)| (*text).to_owned())
            .ok_or_else(|| format!("No such file or directory: {path}"))
    })
}

/// The one SEC-009 finding on `ping`, sourced at the `call` task.
fn refused_at_ping_from_call(report: &CheckReport) {
    assert!(
        report.composition.is_empty(),
        "the call is static, typed and contained: {:?}",
        report.composition
    );
    assert_eq!(
        report.trifecta_findings.len(),
        1,
        "the child's untrusted content reaches the webhook: {:?}",
        report.trifecta_findings
    );
    let finding = &report.trifecta_findings[0];
    assert_eq!(finding.task, "ping");
    assert_eq!(finding.source.as_deref(), Some("call"));
    assert!(
        finding
            .detail
            .starts_with("lethal trifecta complete · human gate required"),
        "{}",
        finding.detail
    );
    assert!(!report.is_clean(), "the lane gates the check");
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code.as_deref() == Some("NIKA-SEC-009") && f.task.as_deref() == Some("ping")),
        "the unified list carries the coded row: {:?}",
        report.findings
    );
}

#[test]
fn a_fetching_child_feeding_a_webhook_is_refused_like_the_inline_fetch() {
    let report = composed(
        &parent(PARENT_UNGATED, "./child.nika", Holds::Fetch),
        &[("child.nika", CHILD_FETCH)],
    );
    refused_at_ping_from_call(&report);

    // The inline twin: the same fetch written in the parent is refused on the
    // same sink, so composition no longer changes the verdict.
    let inline = parent(PARENT_UNGATED, "./child.nika", Holds::Fetch).replace(
        "  call:\n    invoke: { workflow: \"./child.nika\" }\n    returns: { object: { text: string } }\n",
        "  call:\n    invoke: { tool: nika:fetch, args: { url: \"https://docs.example.com/page\", mode: text } }\n",
    );
    let inline = inline.replace("${{ tasks.call.output.text }}", "${{ tasks.call.output }}");
    let twin = check(&parsed(&inline));
    assert_eq!(
        twin.trifecta_findings.len(),
        1,
        "{:?}",
        twin.trifecta_findings
    );
    assert_eq!(twin.trifecta_findings[0].task, "ping");
    assert_eq!(twin.trifecta_findings[0].source.as_deref(), Some("call"));
}

#[test]
fn an_exec_child_feeding_a_webhook_is_refused_like_the_inline_exec() {
    let report = composed(
        &parent(PARENT_UNGATED, "./child.nika", Holds::Git),
        &[("child.nika", CHILD_EXEC)],
    );
    refused_at_ping_from_call(&report);
}

#[test]
fn a_dominating_gate_clears_a_fetching_child_and_is_credited() {
    let report = composed(
        &parent(PARENT_GATED, "./child.nika", Holds::Fetch),
        &[("child.nika", CHILD_FETCH)],
    );
    assert!(
        report.trifecta_findings.is_empty(),
        "the gate dominates every path to the webhook: {:?}",
        report.trifecta_findings
    );
    let cleared: Vec<(&str, &str)> = report
        .trifecta_mitigations
        .iter()
        .map(|m| (m.sink.as_str(), m.gate.as_str()))
        .collect();
    assert_eq!(
        cleared,
        vec![("ping", "ask")],
        "the clearance names its gate"
    );
    assert!(
        report.is_clean(),
        "the gated composition is accepted: {:?}",
        report.findings
    );
}

#[test]
fn a_dominating_gate_clears_an_exec_child_and_is_credited() {
    let report = composed(
        &parent(PARENT_GATED, "./child.nika", Holds::Git),
        &[("child.nika", CHILD_EXEC)],
    );
    assert!(
        report.trifecta_findings.is_empty(),
        "{:?}",
        report.trifecta_findings
    );
    let cleared: Vec<(&str, &str)> = report
        .trifecta_mitigations
        .iter()
        .map(|m| (m.sink.as_str(), m.gate.as_str()))
        .collect();
    assert_eq!(cleared, vec![("ping", "ask")]);
    assert!(report.is_clean(), "{:?}", report.findings);
}

/// No false positive: a child that brings nothing in stays clean even when
/// every leg is armed by an inline fetch on a parallel branch.
#[test]
fn a_pure_child_is_not_an_ingress_even_with_every_leg_armed() {
    let armed = parent(PARENT_UNGATED, "./child.nika", Holds::Fetch).replace(
        "  note:\n",
        "  probe:\n    invoke: { tool: nika:fetch, args: { url: \"https://docs.example.com/page\" } }\n  note:\n",
    );
    let report = composed(&armed, &[("child.nika", CHILD_PURE)]);
    assert!(
        report.trifecta_findings.is_empty(),
        "a pure child's output is not untrusted content: {:?}",
        report.trifecta_findings
    );
    assert!(report.trifecta_mitigations.is_empty());
    assert!(report.composition.is_empty(), "{:?}", report.composition);
}

/// Fail-closed: a child the check cannot resolve may bring anything in.
#[test]
fn an_unresolvable_child_is_untrusted_ingress() {
    // Unreadable.
    let report = composed(&parent(PARENT_UNGATED, "./ghost.nika", Holds::Fetch), &[]);
    assert!(
        report.composition.iter().any(|f| f.code == "NIKA-COMP-001"),
        "{:?}",
        report.composition
    );
    assert_eq!(
        report.trifecta_findings.len(),
        1,
        "{:?}",
        report.trifecta_findings
    );
    assert_eq!(report.trifecta_findings[0].task, "ping");
    assert_eq!(report.trifecta_findings[0].source.as_deref(), Some("call"));

    // Unparseable.
    let report = composed(
        &parent(PARENT_UNGATED, "./child.nika", Holds::Fetch),
        &[("child.nika", "nika: [unclosed\n")],
    );
    assert!(report.composition.iter().any(|f| f.code == "NIKA-COMP-001"));
    assert_eq!(
        report.trifecta_findings.len(),
        1,
        "{:?}",
        report.trifecta_findings
    );

    // A pinned registry child, which this engine does not run.
    let report = composed(
        &parent(PARENT_UNGATED, "registry:acme/pages@1.0.0", Holds::Fetch),
        &[],
    );
    assert!(report.composition.iter().any(|f| f.code == "NIKA-COMP-001"));
    assert_eq!(
        report.trifecta_findings.len(),
        1,
        "{:?}",
        report.trifecta_findings
    );
}

/// Transitive: the fetch sits one workflow further down.
#[test]
fn a_grandchild_fetch_reaches_the_parent_through_a_pure_middle() {
    let report = composed(
        &parent(PARENT_UNGATED, "./middle.nika", Holds::Fetch),
        &[("middle.nika", CHILD_MIDDLE), ("leaf.nika", CHILD_FETCH)],
    );
    refused_at_ping_from_call(&report);
}

/// Fail-closed treats the call's output as untrusted; the parent's own grants
/// still bound what any child can bring in at run (law 3: the child runs inside
/// them). With no ingress channel granted (no fetch, no MCP, no exec), leg ②
/// stays open, and only the composition lane speaks.
#[test]
fn an_unresolvable_child_under_a_boundary_granting_no_ingress_completes_no_trifecta() {
    let report = composed(&parent(PARENT_UNGATED, "./ghost.nika", Holds::Nothing), &[]);
    assert!(
        report.composition.iter().any(|f| f.code == "NIKA-COMP-001"),
        "{:?}",
        report.composition
    );
    assert!(
        report.trifecta_findings.is_empty(),
        "{:?}",
        report.trifecta_findings
    );
}

/// A plain `check` reads no child: its trifecta lane, like its composition
/// lane, does not judge the child. `nika check` and `nika run` compose.
#[test]
fn the_reader_less_check_stays_child_blind() {
    let wf = parsed(&parent(PARENT_UNGATED, "./child.nika", Holds::Fetch));
    assert!(check(&wf).trifecta_findings.is_empty());
}
