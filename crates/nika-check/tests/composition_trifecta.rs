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

/// The same parent with a blocking human gate upstream of both entries, and
/// the call and the egress bound to the affirmative answer (the child acts:
/// a refused confirm must not run it · NIKA-SEC-014).
const PARENT_GATED: &str = r#"nika: parent
permits:
  tools: ["nika:read", "nika:notify", "nika:prompt"{tools}]
  net: { http: ["hooks.example.com"{hosts}] }
  fs: { read: ["./private.txt"{reads}] }{exec}
tasks:
  ask:
    invoke: { tool: nika:prompt, args: { message: "Send the page and the note?" } }
  call:
    with: { go: "${{ tasks.ask.output }}" }
    when: ${{ with.go == true }}
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

/// Fail-closed: a child the check cannot resolve may bring anything in, and
/// send it out itself: the webhook it feeds and the call are both refused.
#[test]
fn an_unresolvable_child_is_untrusted_ingress() {
    let sinks = |report: &CheckReport| -> Vec<(String, Option<String>)> {
        report
            .trifecta_findings
            .iter()
            .map(|f| (f.task.clone(), f.source.clone()))
            .collect()
    };
    let expected = vec![
        ("call".to_owned(), Some("call".to_owned())),
        ("ping".to_owned(), Some("call".to_owned())),
    ];
    // Unreadable.
    let report = composed(&parent(PARENT_UNGATED, "./ghost.nika", Holds::Fetch), &[]);
    assert!(
        report.composition.iter().any(|f| f.code == "NIKA-COMP-001"),
        "{:?}",
        report.composition
    );
    assert_eq!(sinks(&report), expected, "{:?}", report.trifecta_findings);

    // Unparseable.
    let report = composed(
        &parent(PARENT_UNGATED, "./child.nika", Holds::Fetch),
        &[("child.nika", "nika: [unclosed\n")],
    );
    assert!(report.composition.iter().any(|f| f.code == "NIKA-COMP-001"));
    assert_eq!(sinks(&report), expected, "{:?}", report.trifecta_findings);

    // A pinned registry child, which this engine does not run.
    let report = composed(
        &parent(PARENT_UNGATED, "registry:acme/pages@1.0.0", Holds::Fetch),
        &[],
    );
    assert!(report.composition.iter().any(|f| f.code == "NIKA-COMP-001"));
    assert_eq!(sinks(&report), expected, "{:?}", report.trifecta_findings);
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

// ─── the other half: a child that sends content out (review of #1840) ────

/// A child that writes the two arguments it is given into one file.
const CHILD_DROP: &str = r#"nika: child-drop
permits:
  tools: ["nika:write"]
  fs: { write: ["./out/**"] }
inputs:
  page: { type: string, required: true }
  note: { type: string, required: true }
tasks:
  leak:
    invoke:
      tool: nika:write
      args: { path: ./out/leak.txt, content: "page=${{ inputs.page }} note=${{ inputs.note }}" }
outputs:
  done: { value: "${{ tasks.leak.output }}" }
"#;

/// A subprocess output and a private note, written out by the parent itself.
const SEND_INLINE: &str = r#"nika: send-inline
permits:
  exec: ["echo"]
  tools: ["nika:read", "nika:write"]
  fs: { read: ["./private.txt"], write: ["./out/**"] }
tasks:
  page:
    exec: { command: ["echo", "untrusted page body"] }
  secret_note:
    invoke: { tool: nika:read, args: { path: ./private.txt } }
  leak:
    with:
      page: ${{ tasks.page.output }}
      note: ${{ tasks.secret_note.output }}
    invoke:
      tool: nika:write
      args: { path: ./out/leak.txt, content: "page=${{ with.page }} note=${{ with.note }}" }
"#;

/// The same flow with the write moved into `child-drop.nika`, fed through `args:`.
const SEND_COMPOSED: &str = r#"nika: send-composed
permits:
  exec: ["echo"]
  tools: ["nika:read", "nika:write"]
  fs: { read: ["./private.txt"], write: ["./out/**"] }
tasks:
  page:
    exec: { command: ["echo", "untrusted page body"] }
  secret_note:
    invoke: { tool: nika:read, args: { path: ./private.txt } }
  leak:
    with:
      page: ${{ tasks.page.output }}
      note: ${{ tasks.secret_note.output }}
    invoke:
      workflow: ./child-drop.nika
      args: { page: "${{ with.page }}", note: "${{ with.note }}" }
"#;

/// The sink task, its source, and the one SEC-009 finding.
fn refused_at(report: &CheckReport, task: &str, source: &str) {
    let found: Vec<(&str, Option<&str>)> = report
        .trifecta_findings
        .iter()
        .map(|f| (f.task.as_str(), f.source.as_deref()))
        .collect();
    assert_eq!(
        found,
        vec![(task, Some(source))],
        "{:?}",
        report.trifecta_findings
    );
    assert!(
        report.trifecta_findings[0]
            .detail
            .starts_with("lethal trifecta complete · human gate required"),
        "{}",
        report.trifecta_findings[0].detail
    );
    assert!(!report.is_clean(), "the lane gates the check");
}

/// The egress half of NIK-273: the parent holds the untrusted content and the
/// private read and hands both to a child that writes them out. The call is
/// the sink, exactly as the inline write is.
#[test]
fn a_child_writing_its_arguments_out_is_refused_like_the_inline_write() {
    let inline = check(&parsed(SEND_INLINE));
    refused_at(&inline, "leak", "page");

    let report = composed(SEND_COMPOSED, &[("child-drop.nika", CHILD_DROP)]);
    assert!(report.composition.is_empty(), "{:?}", report.composition);
    refused_at(&report, "leak", "page");
    let detail = &report.trifecta_findings[0].detail;
    assert!(
        detail.contains(
            "`leak` calls a workflow that sends it out: `./child-drop.nika` · task `leak` invokes `nika:write`"
        ),
        "the child task behind the sink is named: {detail}"
    );
}

/// A child that only notifies, fed a fetched page and a private note.
const CHILD_SEND: &str = r#"nika: child-send
permits:
  tools: ["nika:notify"]
  net: { http: ["hooks.example.com"] }
inputs:
  page: { type: string, required: true }
  note: { type: string, required: true }
tasks:
  ping:
    invoke:
      tool: nika:notify
      args:
        channel: webhook
        target: https://hooks.example.com/hook
        message: "page says: ${{ inputs.page }}"
        data: { note: "${{ inputs.note }}" }
"#;

/// The fetch and the private read in the parent (or the fetch in a first
/// child), the webhook in a second child.
#[test]
fn a_notifying_child_is_a_sink_for_the_content_it_is_passed() {
    let parent_fetch = r#"nika: egress-in-child
permits:
  tools: ["nika:fetch", "nika:read", "nika:notify"]
  net: { http: ["docs.example.com", "hooks.example.com"] }
  fs: { read: ["./private.txt"] }
tasks:
  page:
    invoke: { tool: nika:fetch, args: { url: "https://docs.example.com/page", mode: text } }
  secret_note:
    invoke: { tool: nika:read, args: { path: ./private.txt } }
  send:
    with:
      page: ${{ tasks.page.output }}
      note: ${{ tasks.secret_note.output }}
    invoke:
      workflow: ./child-send.nika
      args: { page: "${{ with.page }}", note: "${{ with.note }}" }
"#;
    let report = composed(parent_fetch, &[("child-send.nika", CHILD_SEND)]);
    refused_at(&report, "send", "page");

    let two_children = r#"nika: two-children
permits:
  tools: ["nika:fetch", "nika:read", "nika:notify"]
  net: { http: ["docs.example.com", "hooks.example.com"] }
  fs: { read: ["./private.txt"] }
tasks:
  fetch:
    invoke: { workflow: ./child.nika }
    returns: { object: { text: string } }
  secret_note:
    invoke: { tool: nika:read, args: { path: ./private.txt } }
  send:
    with:
      page: ${{ tasks.fetch.output.text }}
      note: ${{ tasks.secret_note.output }}
    invoke:
      workflow: ./child-send.nika
      args: { page: "${{ with.page }}", note: "${{ with.note }}" }
"#;
    let report = composed(
        two_children,
        &[("child.nika", CHILD_FETCH), ("child-send.nika", CHILD_SEND)],
    );
    refused_at(&report, "send", "fetch");
    let detail = &report.trifecta_findings[0].detail;
    assert!(
        detail.contains("`fetch` calls a workflow that brings untrusted content in")
            && detail.contains("`send` calls a workflow that sends it out"),
        "both calls are named: {detail}"
    );

    // A caller's gate upstream of every entry clears it, as inline.
    let gated = parent_fetch
        .replace(
            "tasks:\n  page:\n",
            "tasks:\n  ask:\n    invoke: { tool: nika:prompt, args: { message: \"Send?\" } }\n  page:\n    after: { ask: success }\n",
        )
        .replace(
            "  secret_note:\n    invoke:",
            "  secret_note:\n    after: { ask: success }\n    invoke:",
        )
        .replace(
            "  send:\n    with:\n",
            "  send:\n    when: ${{ with.go == true }}\n    with:\n      go: ${{ tasks.ask.output }}\n",
        )
        .replace("\"nika:notify\"]", "\"nika:notify\", \"nika:prompt\"]");
    let report = composed(&gated, &[("child-send.nika", CHILD_SEND)]);
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
    assert_eq!(cleared, vec![("send", "ask")]);
}

/// A child that saves a subprocess's output to a file in the workspace.
const CHILD_DROPPER: &str = r#"nika: child-dropper
permits:
  exec: ["echo"]
  tools: ["nika:write"]
  fs: { write: ["./drop/**"] }
tasks:
  page:
    exec: { command: ["echo", "untrusted page body"] }
  save:
    with: { body: "${{ tasks.page.output }}" }
    invoke: { tool: nika:write, args: { path: ./drop/page.txt, content: "${{ with.body }}" } }
"#;

/// Inside the child, untrusted content reaches a write: under the parent's
/// legs that write is the sink it would be inline, and the call stands for it.
#[test]
fn a_child_writing_what_it_brings_in_is_judged_under_the_callers_legs() {
    let parent = r#"nika: dropped
permits:
  exec: ["echo"]
  tools: ["nika:read", "nika:write"]
  fs: { read: ["./private.txt", "./drop/**"], write: ["./drop/**", "/srv/out/**"] }
tasks:
  call:
    invoke: { workflow: ./child-dropper.nika }
  pick:
    after: { call: success }
    invoke: { tool: nika:read, args: { path: ./drop/page.txt } }
  secret_note:
    invoke: { tool: nika:read, args: { path: ./private.txt } }
  leak:
    with:
      page: ${{ tasks.pick.output }}
      note: ${{ tasks.secret_note.output }}
    invoke:
      tool: nika:write
      args: { path: /srv/out/leak.txt, content: "page=${{ with.page }} note=${{ with.note }}" }
"#;
    let report = composed(parent, &[("child-dropper.nika", CHILD_DROPPER)]);
    // The child's write is the sink the call stands for, and `pick` reads
    // the file it wrote: `leak` is the second realized sink (NIK-273
    // follow-up · a builtin read carries the file on), as inline.
    assert_eq!(
        sinks_of(&report),
        vec![("call", Some("call")), ("leak", Some("call"))]
    );
    let detail = &report.trifecta_findings[0].detail;
    assert!(
        detail.contains("task `page` is an `exec:`")
            && detail.contains("task `save` invokes `nika:write`"),
        "the child's ingress and its write are named: {detail}"
    );
    let inline = parent.replace(
        "  call:\n    invoke: { workflow: ./child-dropper.nika }\n  pick:\n    after: { call: success }\n",
        "  page:\n    exec: { command: [\"echo\", \"untrusted page body\"] }\n  save:\n    with: { body: \"${{ tasks.page.output }}\" }\n    invoke: { tool: nika:write, args: { path: ./drop/page.txt, content: \"${{ with.body }}\" } }\n  pick:\n    after: { save: success }\n",
    );
    assert_eq!(
        sinks_of(&check(&parsed(&inline))),
        vec![("save", Some("page")), ("leak", Some("page"))],
        "the inline twin"
    );
}

/// Every SEC-009 sink with its source, in declaration order.
fn sinks_of(report: &CheckReport) -> Vec<(&str, Option<&str>)> {
    report
        .trifecta_findings
        .iter()
        .map(|f| (f.task.as_str(), f.source.as_deref()))
        .collect()
}

/// The file channel crosses the wall: a parent subprocess that reads the file
/// a child wrote with untrusted content in it is tainted, as inline.
#[test]
fn a_file_a_child_writes_taints_a_parent_subprocess_like_the_inline_writer() {
    let inline = r#"nika: ship-inline
permits:
  exec: ["echo", "cp"]
  tools: ["nika:write"]
  fs: { read: ["./private.txt", "./drop/**"], write: ["./drop/**", "/srv/out/**"] }
tasks:
  page:
    exec: { command: ["echo", "untrusted page body"] }
  save:
    with: { body: "${{ tasks.page.output }}" }
    invoke: { tool: nika:write, args: { path: ./drop/page.txt, content: "${{ with.body }}" } }
  ship:
    after: { save: success }
    exec: { command: ["cp", "./drop/page.txt", "./private.txt", "/srv/out/"] }
"#;
    let twin = check(&parsed(inline));
    let sinks: Vec<(&str, Option<&str>)> = twin
        .trifecta_findings
        .iter()
        .map(|f| (f.task.as_str(), f.source.as_deref()))
        .collect();
    assert_eq!(sinks, vec![("save", Some("page")), ("ship", Some("page"))]);

    let composed_form = r#"nika: ship-composed
permits:
  exec: ["echo", "cp"]
  tools: ["nika:write"]
  fs: { read: ["./private.txt", "./drop/**"], write: ["./drop/**", "/srv/out/**"] }
tasks:
  call:
    invoke: { workflow: ./child-dropper.nika }
  ship:
    after: { call: success }
    exec: { command: ["cp", "./drop/page.txt", "./private.txt", "/srv/out/"] }
"#;
    let report = composed(composed_form, &[("child-dropper.nika", CHILD_DROPPER)]);
    let sinks: Vec<(&str, Option<&str>)> = report
        .trifecta_findings
        .iter()
        .map(|f| (f.task.as_str(), f.source.as_deref()))
        .collect();
    assert_eq!(
        sinks,
        vec![("call", Some("call")), ("ship", Some("call"))],
        "the child's write and the subprocess that reads it: {:?}",
        report.trifecta_findings
    );
}

/// No false positive: a child whose subprocess output reaches neither its
/// outputs nor an egress task leaves the call's output clean, as the same
/// tasks inline do.
#[test]
fn a_child_subprocess_that_reaches_nothing_taints_nothing() {
    let child = r#"nika: child-side
permits:
  exec: ["date"]
  tools: ["nika:jq"]
tasks:
  stamp:
    exec: { command: ["date"] }
  label:
    invoke: { tool: nika:jq, args: { input: "{\"a\": 1}", expression: ".a" } }
outputs:
  n: { value: "${{ tasks.label.output }}", type: integer }
"#;
    let parent = r#"nika: parent-side
permits:
  exec: ["date"]
  tools: ["nika:jq", "nika:read", "nika:notify"]
  net: { http: ["hooks.example.com"] }
  fs: { read: ["./private.txt"] }
tasks:
  call:
    invoke: { workflow: ./child-side.nika }
    returns: { object: { n: integer } }
  note:
    invoke: { tool: nika:read, args: { path: ./private.txt } }
  ping:
    with:
      n: ${{ tasks.call.output.n }}
      note: ${{ tasks.note.output }}
    invoke:
      tool: nika:notify
      args:
        channel: webhook
        target: https://hooks.example.com/hook
        message: "n=${{ with.n }}"
        data: { note: "${{ with.note }}" }
"#;
    let report = composed(parent, &[("child-side.nika", child)]);
    assert!(
        report.trifecta_findings.is_empty(),
        "{:?}",
        report.trifecta_findings
    );
    assert!(report.is_clean(), "{:?}", report.findings);
    // The inline twin, accepted on every engine.
    let inline = parent
        .replace(
            "  call:\n    invoke: { workflow: ./child-side.nika }\n    returns: { object: { n: integer } }\n",
            "  stamp:\n    exec: { command: [\"date\"] }\n  call:\n    invoke: { tool: nika:jq, args: { input: \"{\\\"a\\\": 1}\", expression: \".a\" } }\n",
        )
        .replace("${{ tasks.call.output.n }}", "${{ tasks.call.output }}");
    assert!(check(&parsed(&inline)).trifecta_findings.is_empty());
}

/// A blocking gate inside the child is not credited to its caller: the call
/// is one task of the caller's graph, and its gate does not dominate the
/// caller's other paths. The caller's own gate upstream of the call is.
#[test]
fn a_gate_inside_the_child_is_not_credited_to_the_caller() {
    let child = r#"nika: child-gated-fetch
permits:
  tools: ["nika:prompt", "nika:fetch"]
  net: { http: ["docs.example.com"] }
tasks:
  ask:
    invoke: { tool: nika:prompt, args: { message: "Fetch the page and send it on?" } }
  page:
    with: { go: "${{ tasks.ask.output }}" }
    when: ${{ with.go == true }}
    invoke: { tool: nika:fetch, args: { url: "https://docs.example.com/page", mode: text } }
outputs:
  text: { value: "${{ tasks.page.output }}", type: string }
"#;
    let report = composed(
        &parent(PARENT_UNGATED, "./child.nika", Holds::Fetch)
            .replace("\"nika:notify\"", "\"nika:notify\", \"nika:prompt\""),
        &[("child.nika", child)],
    );
    refused_at_ping_from_call(&report);
    assert!(report.trifecta_mitigations.is_empty());

    let gated = parent(PARENT_GATED, "./child.nika", Holds::Fetch);
    let report = composed(&gated, &[("child.nika", child)]);
    assert!(
        report.trifecta_findings.is_empty(),
        "{:?}",
        report.trifecta_findings
    );
}

/// An inline refusal in a workflow that also calls an unrelated child is
/// named as before: no child witness is appended to it.
#[test]
fn an_inline_refusal_beside_an_unrelated_call_names_no_child() {
    let mixed = r#"nika: parent-mixed
permits:
  exec: ["git"]
  tools: ["nika:fetch", "nika:read", "nika:notify"]
  net: { http: ["docs.example.com", "hooks.example.com"] }
  fs: { read: ["./private.txt", "./repo/**"] }
tasks:
  page:
    invoke: { tool: nika:fetch, args: { url: "https://docs.example.com/page", mode: text } }
  note:
    invoke: { tool: nika:read, args: { path: ./private.txt } }
  ping:
    with:
      page: ${{ tasks.page.output }}
      note: ${{ tasks.note.output }}
    invoke:
      tool: nika:notify
      args:
        channel: webhook
        target: https://hooks.example.com/hook
        message: "page: ${{ with.page }}"
        data: { note: "${{ with.note }}" }
  history:
    invoke: { workflow: ./child.nika }
    returns: { object: { text: string } }
outputs:
  log: ${{ tasks.history.output.text }}
"#;
    let report = composed(mixed, &[("child.nika", CHILD_EXEC)]);
    refused_at(&report, "ping", "page");
    // The inline twin runs the child's subprocess in place of the call.
    let twin = mixed
        .replace(
            "    invoke: { workflow: ./child.nika }\n    returns: { object: { text: string } }\n",
            "    exec: { command: [\"git\", \"-C\", \"./repo\", \"log\", \"--oneline\", \"-3\"] }\n",
        )
        .replace("${{ tasks.history.output.text }}", "${{ tasks.history.output }}");
    let inline = check(&parsed(&twin));
    refused_at(&inline, "ping", "page");
    assert_eq!(
        report.trifecta_findings[0].detail, inline.trifecta_findings[0].detail,
        "the inline message is unchanged"
    );
}

/// Fail-closed on the egress half too: an unreadable child handed untrusted
/// content and a private note may send them anywhere.
#[test]
fn an_unresolvable_child_is_a_sink_for_what_it_is_passed() {
    let report = composed(
        &SEND_COMPOSED.replace("./child-drop.nika", "./ghost.nika"),
        &[],
    );
    assert!(report.composition.iter().any(|f| f.code == "NIKA-COMP-001"));
    refused_at(&report, "leak", "page");
}
