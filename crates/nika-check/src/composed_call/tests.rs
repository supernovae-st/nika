// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

const FETCH: &str = "nika: leaf\npermits: { tools: [\"nika:fetch\"], net: { http: [\"docs.example.com\"] } }\ntasks:\n  page:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://docs.example.com/page\" } }\noutputs:\n  text: { value: \"${{ tasks.page.output }}\", type: string }\n";
const GIT: &str = "nika: log\npermits: { exec: [\"git\"], fs: { read: [\"./repo/**\"] } }\ntasks:\n  log:\n    exec: { command: [\"git\", \"-C\", \"./repo\", \"log\"] }\noutputs:\n  text: { value: \"${{ tasks.log.output }}\", type: string }\n";
const PURE: &str = "nika: pure\nmodel: mock/echo\npermits: {}\ntasks:\n  brief:\n    infer: { prompt: \"hi\", max_tokens: 8 }\noutputs:\n  text: { value: \"${{ tasks.brief.output }}\", type: string }\n";

/// A caller holding a wide boundary and calling `target` once.
fn caller(target: &str) -> String {
    format!(
        "nika: caller\npermits:\n  exec: [\"git\", \"jq\"]\n  tools: [\"nika:fetch\", \"nika:read\", \"nika:glob\"]\n  net: {{ http: [\"docs.example.com\", \"hooks.example.com\"] }}\n  fs: {{ read: [\"./repo/**\", \"./notes/**\"], write: [\"./out/**\"] }}\ntasks:\n  call:\n    invoke: {{ workflow: \"{target}\" }}\n"
    )
}

fn calls_of(caller: &str, files: &[(&str, &str)]) -> Vec<ComposedCall> {
    let wf = nika_schema::parse(
        caller,
        nika_schema::source::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    crate::check_composed(&wf, "caller.nika", &mut |path| {
        files
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, text)| (*text).to_owned())
            .ok_or_else(|| format!("no such file: {path}"))
    })
    .composed_calls
}

fn calls(target: &str, files: &[(&str, &str)]) -> Vec<ComposedCall> {
    calls_of(&caller(target), files)
}

fn only(calls: &[ComposedCall]) -> &ComposedCall {
    assert_eq!(calls.len(), 1, "{calls:?}");
    &calls[0]
}

#[test]
fn a_fetching_child_is_ingress_and_holds_exactly_its_fetch_grants() {
    let found = calls("./leaf.nika", &[("leaf.nika", FETCH)]);
    let call = only(&found);
    assert_eq!(call.task, "call");
    assert_eq!(call.target, "./leaf.nika");
    assert_eq!(
        call.ingress.as_deref(),
        Some("`./leaf.nika` · task `page` invokes `nika:fetch`")
    );
    assert_eq!(
        call.born.output.as_deref(),
        Some("`./leaf.nika` · task `page` invokes `nika:fetch`"),
        "the fetched page is the child's output"
    );
    assert_eq!(call.born.egress, None, "a fetch is not an egress task");
    assert_eq!(call.born.file, None);
    assert!(call.args.is_empty() && call.files.is_empty(), "{call:?}");
    assert_eq!(call.grants.tools, Some(vec!["nika:fetch".to_owned()]));
    assert_eq!(
        call.grants.net,
        Some(NetPermits::new(vec!["docs.example.com".to_owned()]))
    );
    assert_eq!(call.grants.exec, None, "the child runs no program");
    assert_eq!(call.grants.fs, None, "the child touches no file");
}

/// The subprocess's reach is opaque, but it runs inside the child's own
/// declared block: only the caller entries that block meets are held.
#[test]
fn an_exec_child_holds_the_entries_its_own_boundary_meets() {
    let found = calls("./log.nika", &[("log.nika", GIT)]);
    let call = only(&found);
    assert_eq!(
        call.ingress.as_deref(),
        Some("`./log.nika` · task `log` is an `exec:` (its output is untrusted content)")
    );
    assert_eq!(
        call.grants.exec,
        Some(ExecPermit::Programs(vec!["git".to_owned()])),
        "only the program the child runs, `jq` stays judged"
    );
    assert_eq!(
        call.grants.fs,
        Some(FsPermits::new(vec!["./repo/**".to_owned()], Vec::new())),
        "the child declares no `./notes/**` read and no write: its subprocess reaches neither"
    );
    assert_eq!(call.grants.net, None, "the child declares no host");
    assert_eq!(call.grants.tools, None);
    assert!(call.born.output.is_some(), "the log is the child's output");
    assert_eq!(
        call.born.egress, None,
        "no untrusted content reaches the subprocess itself (inline: a clean exec is no sink)"
    );
}

/// An absent child block is the declared zero: an opaque face reaches
/// nothing of the caller's.
#[test]
fn an_exec_child_with_no_permits_block_holds_no_opaque_entry() {
    let bare = "nika: bare\ntasks:\n  log:\n    exec: { command: [\"git\", \"log\"] }\n";
    let found = calls("./bare.nika", &[("bare.nika", bare)]);
    let call = only(&found);
    assert_eq!(call.grants.fs, None);
    assert_eq!(call.grants.net, None);
    assert_eq!(
        call.grants.exec,
        Some(ExecPermit::Programs(vec!["git".to_owned()])),
        "its program stays a need (COMP-002 judges it)"
    );
}

/// Bounded at every level: a grandchild's subprocess runs inside the
/// middle's block too.
#[test]
fn a_grandchild_subprocess_is_bounded_by_every_block_above_it() {
    let wide = "nika: wide\npermits: { exec: [\"git\"], fs: { read: [\"./repo/**\", \"./notes/**\"] } }\ntasks:\n  log:\n    exec: { command: [\"git\", \"log\"] }\n";
    let middle = "nika: middle\npermits: { exec: [\"git\"], fs: { read: [\"./repo/**\"] } }\ntasks:\n  inner:\n    invoke: { workflow: \"./wide.nika\" }\n";
    let found = calls(
        "./middle.nika",
        &[("middle.nika", middle), ("wide.nika", wide)],
    );
    let call = only(&found);
    assert_eq!(
        call.grants.fs,
        Some(FsPermits::new(vec!["./repo/**".to_owned()], Vec::new())),
        "the middle grants no `./notes/**`: the grandchild cannot reach it"
    );
}

#[test]
fn a_pure_child_brings_nothing_in_and_holds_nothing() {
    let found = calls("./pure.nika", &[("pure.nika", PURE)]);
    let call = only(&found);
    assert_eq!(call.ingress, None);
    assert!(call.born.is_empty() && call.files.is_empty(), "{call:?}");
    assert_eq!(call.effect, None, "an inference acts on nothing");
    assert_eq!(call.grants, Permits::new());
}

/// A subprocess whose output reaches neither the outputs nor an egress
/// leaves the call's output clean, as the same tasks inline do.
#[test]
fn an_unused_subprocess_is_ingress_but_never_reaches_the_output() {
    let stamp = "nika: stamp\npermits: { exec: [\"date\"], tools: [\"nika:jq\"] }\ntasks:\n  stamp:\n    exec: { command: [\"date\"] }\n  label:\n    invoke: { tool: \"nika:jq\", args: { input: \"{\\\"a\\\": 1}\", expression: \".a\" } }\noutputs:\n  n: { value: \"${{ tasks.label.output }}\", type: integer }\n";
    let found = calls("./stamp.nika", &[("stamp.nika", stamp)]);
    let call = only(&found);
    assert!(call.ingress.is_some(), "leg ② is realized: {call:?}");
    assert!(call.born.is_empty(), "{call:?}");
}

/// The call's arguments reaching an egress task of the child: the call is
/// a sink for whatever the caller passes.
#[test]
fn a_child_writing_its_arguments_out_is_an_argument_egress() {
    let drop = "nika: drop\npermits: { tools: [\"nika:write\"], fs: { write: [\"/tmp/out/**\"] } }\ninputs:\n  page: { type: string, required: true }\ntasks:\n  leak:\n    invoke: { tool: \"nika:write\", args: { path: \"/tmp/out/leak.txt\", content: \"${{ inputs.page }}\" } }\n";
    let found = calls("./drop.nika", &[("drop.nika", drop)]);
    let call = only(&found);
    assert_eq!(call.ingress, None, "the child brings nothing in itself");
    assert_eq!(
        call.args.egress.as_deref(),
        Some("`./drop.nika` · task `leak` invokes `nika:write`")
    );
    assert_eq!(
        call.args.file.as_deref(),
        Some("`./drop.nika` · task `leak` writes a file through `nika:write`")
    );
    assert!(call.born.is_empty(), "{call:?}");
    assert_eq!(
        call.effect.as_deref(),
        Some("`./drop.nika` · task `leak` invokes `nika:write`"),
        "the write acts, whatever content reaches it"
    );
}

/// A child that saves what it brings in to a file arms its caller's file
/// channel, as the same writer inline does.
#[test]
fn a_child_saving_its_ingress_to_a_file_is_a_born_writer() {
    let dropper = "nika: dropper\npermits: { exec: [\"echo\"], tools: [\"nika:write\"], fs: { write: [\"./drop/**\"] } }\ntasks:\n  page:\n    exec: { command: [\"echo\", \"body\"] }\n  save:\n    with: { body: \"${{ tasks.page.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./drop/page.txt\", content: \"${{ with.body }}\" } }\n";
    let found = calls("./dropper.nika", &[("dropper.nika", dropper)]);
    let call = only(&found);
    assert_eq!(
        call.born.file.as_deref(),
        Some("`./dropper.nika` · task `save` writes a file through `nika:write`")
    );
    assert!(
        call.born.egress.is_some(),
        "the tainted write is an egress task"
    );
    assert_eq!(call.born.output, None, "the child returns nothing");
}

/// An `exec:` of the child reads the files a caller's writer left: the
/// file entry reaches it.
#[test]
fn a_child_subprocess_reads_the_files_a_caller_left() {
    let found = calls("./log.nika", &[("log.nika", GIT)]);
    let call = only(&found);
    assert!(
        call.files.egress.is_some() && call.files.output.is_some(),
        "{call:?}"
    );
    assert_eq!(call.files.file, None, "the channel is already open");
    let pure = calls("./pure.nika", &[("pure.nika", PURE)]);
    assert!(only(&pure).files.is_empty(), "no subprocess reads a file");
}

/// A builtin read of the child reads the files a caller's writer left, as
/// its subprocess does (NIK-273 follow-up): the file entry reaches the
/// output and the egress that read feeds, and a child with no file read
/// stays closed to it.
#[test]
fn a_child_builtin_read_sees_the_files_a_caller_left() {
    let pick = "nika: pick\npermits: { tools: [\"nika:read\", \"nika:notify\"], net: { http: [\"hooks.example.com\"] }, fs: { read: [\"./drop/**\"] } }\ntasks:\n  pick:\n    invoke: { tool: \"nika:read\", args: { path: \"./drop/page.txt\" } }\n  ping:\n    with: { page: \"${{ tasks.pick.output }}\" }\n    invoke: { tool: \"nika:notify\", args: { channel: webhook, target: \"https://hooks.example.com/hook\", message: \"${{ with.page }}\" } }\noutputs:\n  page: { value: \"${{ tasks.pick.output }}\", type: string }\n";
    let found = calls("./pick.nika", &[("pick.nika", pick)]);
    let call = only(&found);
    assert_eq!(
        call.files.egress.as_deref(),
        Some("`./pick.nika` · task `ping` invokes `nika:notify`")
    );
    assert!(call.files.output.is_some(), "{call:?}");
    assert_eq!(call.files.file, None, "the channel is already open");
    assert_eq!(
        call.ingress, None,
        "a file read brings nothing in by itself"
    );
    let notify = "nika: tell\npermits: { tools: [\"nika:notify\"], net: { http: [\"hooks.example.com\"] } }\ntasks:\n  ping:\n    invoke: { tool: \"nika:notify\", args: { channel: webhook, target: \"https://hooks.example.com/hook\", message: \"done\" } }\n";
    let closed = calls("./tell.nika", &[("tell.nika", notify)]);
    assert!(only(&closed).files.is_empty(), "no task reads a file");
}

#[test]
fn the_witness_walks_down_to_a_grandchild() {
    let middle = "nika: middle\npermits: { tools: [\"nika:fetch\"], net: { http: [\"docs.example.com\"] } }\ntasks:\n  inner:\n    invoke: { workflow: \"./deep/leaf.nika\" }\n    returns: { object: { text: string } }\noutputs:\n  text: { value: \"${{ tasks.inner.output.text }}\", type: string }\n";
    let found = calls(
        "./middle.nika",
        &[("middle.nika", middle), ("deep/leaf.nika", FETCH)],
    );
    let call = only(&found);
    assert_eq!(
        call.ingress.as_deref(),
        Some("`./middle.nika` → `./deep/leaf.nika` · task `page` invokes `nika:fetch`")
    );
    assert_eq!(
        call.born.output.as_deref(),
        Some("`./middle.nika` → `./deep/leaf.nika` · task `page` invokes `nika:fetch`"),
        "the middle returns the grandchild's page"
    );
    assert_eq!(
        call.grants.tools,
        Some(vec!["nika:fetch".to_owned()]),
        "the grandchild's need is held through the middle"
    );
    assert_eq!(
        call.effect.as_deref(),
        Some("`./middle.nika` → `./deep/leaf.nika` · task `page` invokes `nika:fetch`"),
        "a fetch is a net effect, found below the middle"
    );
}

#[test]
fn an_unresolvable_child_is_ingress_and_holds_the_whole_block() {
    let found = calls("./ghost.nika", &[]);
    let call = only(&found);
    let why = call.ingress.as_deref().expect("fail-closed");
    assert!(
        why.starts_with("`./ghost.nika` · cannot read `ghost.nika`"),
        "{why}"
    );
    assert!(why.ends_with("(fail-closed)"), "{why}");
    for flow in [&call.born, &call.args] {
        assert!(
            flow.output.is_some() && flow.egress.is_some() && flow.file.is_some(),
            "{call:?}"
        );
    }
    assert!(call.files.output.is_some() && call.files.egress.is_some());
    let wide = nika_schema::parse(
        &caller("./ghost.nika"),
        nika_schema::source::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    assert_eq!(
        Some(&call.grants),
        wide.permits.as_ref().map(|p| &p.value),
        "any grant may carry an unread child"
    );

    let registry = calls("registry:acme/pages@1.0.0", &[]);
    let why = only(&registry).ingress.as_deref().expect("fail-closed");
    assert!(why.contains("cannot be resolved before the run"), "{why}");
}

#[test]
fn a_child_whose_graph_does_not_analyze_is_unresolved() {
    let broken = "nika: broken\npermits: {}\ntasks:\n  a:\n    after: { ghost: success }\n    infer: { prompt: \"x\", max_tokens: 8 }\n";
    let found = calls("./broken.nika", &[("broken.nika", broken)]);
    let call = only(&found);
    assert!(
        call.ingress
            .as_deref()
            .is_some_and(|w| w.contains("does not analyze")),
        "{call:?}"
    );
    assert!(call.args.egress.is_some());
}

#[test]
fn a_cycle_is_ingress_and_the_walk_terminates() {
    let a = "nika: a\npermits: {}\ntasks:\n  back:\n    invoke: { workflow: \"./b.nika\" }\n";
    let b = "nika: b\npermits: {}\ntasks:\n  again:\n    invoke: { workflow: \"./a.nika\" }\n";
    let found = calls("./a.nika", &[("a.nika", a), ("b.nika", b)]);
    let why = only(&found).ingress.as_deref().expect("fail-closed");
    assert!(why.contains("closes a cycle"), "{why}");
}

#[test]
fn an_agent_whitelist_holds_what_its_own_block_meets() {
    let agent = "nika: browse\nmodel: mock/echo\npermits: { tools: [\"nika:fetch\"], net: { http: [\"docs.example.com\"] } }\ntasks:\n  look:\n    agent: { prompt: \"read the page\", tools: [\"nika:fetch\"] }\noutputs:\n  text: { value: \"${{ tasks.look.output }}\", type: string }\n";
    let found = calls("./browse.nika", &[("browse.nika", agent)]);
    let call = only(&found);
    assert!(
        call.ingress
            .as_deref()
            .is_some_and(|w| w.contains("task `look` is an `agent:`")),
        "{call:?}"
    );
    assert_eq!(
        call.grants.tools,
        Some(vec!["nika:fetch".to_owned()]),
        "the whitelist dispatches inside the child's own `tools:`"
    );
    assert_eq!(
        call.grants.net,
        Some(NetPermits::new(vec!["docs.example.com".to_owned()]))
    );
    assert_eq!(call.grants.fs, None, "the child declares no file reach");
}

/// A tool-less `agent:` may run on a seat whose permission bridge judges its asks against
/// exec + fs, with or without a `schema:` (validated on the seat's final message): the
/// caller entries its own block meets are held.
#[test]
fn a_seat_servable_agent_holds_what_its_own_block_meets_with_or_without_a_schema() {
    for schema in ["", ", schema: { type: object }"] {
        let agent = format!(
            "nika: seat\nmodel: mock/echo\npermits: {{ exec: [\"git\"], fs: {{ read: [\"./repo/**\"] }} }}\ntasks:\n  look:\n    agent: {{ prompt: \"summarize the repo\"{schema} }}\n"
        );
        let found = calls("./seat.nika", &[("seat.nika", &agent)]);
        let call = only(&found);
        assert_eq!(
            call.grants.exec,
            Some(ExecPermit::Programs(vec!["git".to_owned()])),
            "{schema}"
        );
        assert_eq!(
            call.grants.fs,
            Some(FsPermits::new(vec!["./repo/**".to_owned()], Vec::new())),
            "{schema}"
        );
    }
}

#[test]
fn an_unresolved_call_is_untrusted_everywhere() {
    let call = ComposedCall::unresolved("call".to_owned(), "./x.nika".to_owned(), "unread");
    assert_eq!(call.ingress.as_deref(), Some("unread"));
    assert_eq!(call.born, CallFlow::everywhere("unread"));
    assert_eq!(call.args, CallFlow::everywhere("unread"));
    assert_eq!(call.files.file, None);
    assert!(call.files.egress.is_some() && call.files.output.is_some());
    assert_eq!(
        call.effect.as_deref(),
        Some("unread"),
        "it may act (fail-closed)"
    );
    assert!(!call.effect_conditional, "and its act is not advisory");
    assert_eq!(call.grants, Permits::new());
}

#[test]
fn a_reader_less_check_resolves_no_call() {
    let wf = nika_schema::parse(
        &caller("./leaf.nika"),
        nika_schema::source::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    assert!(crate::check(&wf).composed_calls.is_empty());
}
