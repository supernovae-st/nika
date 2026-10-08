// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::json;

use super::{Document, Edit, Path, Refusal, Style};

/// A document beyond any sketch: typed inputs with and without defaults,
/// typed and untyped constants, permits, run, a failure edge, a business
/// condition, `retry`/`on_error`/`timeout`, a bounded fan-out, a group, `extract`,
/// comments, flow and block literals.
const RICH: &str = r#"# header comment kept byte for byte
nika: rich-revision
model: mock/echo

inputs:
  region:
    type: string
    default: "eu-west"   # trailing comment
  verbose:
    type: bool
    required: false

const:
  window_hours: 48
  label: 'it''s late'
  limits: { type: int, value: 3 }

permits:
  tools: ["nika:log", "nika:write"]
  fs:
    write:
      - out/report.md

run:
  entropy: none

tasks:
  fetch:
    exec:
      command: ["echo", "[1, 2, 3]"]
    retry:
      max_attempts: 3
      backoff_ms: 250
    timeout: "30s"
    extract:
      count: "length"
  report:
    after: { fetch: failure }
    when: ${{ inputs.verbose == true }}
    for_each:
      items: ["a", "b"]
      max_parallel: 2
      fail_fast: false
    group: reports
    invoke:
      tool: "nika:write"
      args:
        path: out/report.md
        content: |
          window ${{ const.window_hours }}h
          region ${{ inputs.region }}
        overwrite: true
    on_error:
      recover: "none"

outputs:
  total: ${{ tasks.fetch.count }}
"#;

fn rich() -> Document {
    Document::parse(RICH).expect("the rich fixture parses strictly")
}

fn path(dotted: &str) -> Path {
    Path::dotted(dotted).expect("a dotted path")
}

/// The bytes outside the single splice are the base's, and the splice spans
/// exactly `old` in the base.
fn only(base: &str, revised: &str, old: &str, new: &str) {
    assert_eq!(
        revised,
        base.replacen(old, new, 1),
        "only `{old}` → `{new}` changed"
    );
}

#[test]
fn import_keeps_the_exact_bytes_and_the_strict_ast() {
    let doc = rich();
    assert_eq!(doc.source(), RICH);
    assert_eq!(doc.workflow().tasks.len(), 2);
    assert_eq!(doc.value(&path("const.window_hours")), Some(&json!(48)));
    let identity = doc.apply(&[]).expect("an empty revision");
    assert_eq!(identity.document().source(), RICH);
    assert!(identity.bytes_preserved(RICH));
}

#[test]
fn a_constant_changes_alone_from_48_to_72() {
    let doc = rich();
    let applied = doc
        .apply(&[Edit::set(path("const.window_hours"), json!(72))])
        .expect("set");
    only(
        RICH,
        applied.document().source(),
        "window_hours: 48",
        "window_hours: 72",
    );
    assert!(applied.bytes_preserved(RICH));
    assert_eq!(applied.changed(), [path("const.window_hours")]);
    let splice = &applied.splices()[0];
    assert_eq!(&RICH[splice.start..splice.end], "48");
    assert_eq!(splice.replacement, "72");
}

#[test]
fn quoted_and_block_scalars_keep_their_presentation() {
    let doc = rich();
    let single = doc
        .apply(&[Edit::set(path("const.label"), json!("it's early"))])
        .expect("single-quoted");
    only(
        RICH,
        single.document().source(),
        "'it''s late'",
        "'it''s early'",
    );
    let double = doc
        .apply(&[Edit::set(path("inputs.region.default"), json!("us-east"))])
        .expect("double-quoted");
    only(
        RICH,
        double.document().source(),
        "\"eu-west\"",
        "\"us-east\"",
    );
    assert!(double.document().source().contains("# trailing comment"));
    let literal = doc
        .apply(&[Edit::set(
            path("tasks.report.invoke.args.content"),
            json!("window ${{ const.window_hours }}h only\n"),
        )])
        .expect("literal");
    only(
        RICH,
        literal.document().source(),
        "|\n          window ${{ const.window_hours }}h\n          region ${{ inputs.region }}",
        "|\n          window ${{ const.window_hours }}h only",
    );
}

#[test]
fn a_typed_constant_binds_its_value_and_keeps_its_declaration() {
    let doc = rich();
    let at = doc.constant_path("limits").expect("declared");
    assert_eq!(at, path("const.limits.value"));
    let applied = doc.apply(&[Edit::set(at, json!(5))]).expect("typed set");
    only(
        RICH,
        applied.document().source(),
        "value: 3 }",
        "value: 5 }",
    );
}

#[test]
fn structure_is_added_and_removed_in_place() {
    let doc = rich();
    let applied = doc
        .apply(&[
            Edit::insert(
                path("tasks.fetch.retry"),
                "backoff_strategy",
                json!("exponential"),
            ),
            Edit::push(path("permits.tools"), json!("nika:assert")),
            Edit::push(path("permits.fs.write"), json!("out/extra.md")),
            Edit::insert(path("tasks.report.after"), "audit", json!("success")),
            Edit::remove(path("tasks.fetch.extract.count")),
        ])
        .expect("structural edits");
    let text = applied.document().source();
    assert!(
        text.contains("      backoff_ms: 250\n      backoff_strategy: exponential\n"),
        "{text}"
    );
    assert!(
        text.contains("tools: [\"nika:log\", \"nika:write\", \"nika:assert\"]"),
        "{text}"
    );
    assert!(
        text.contains("      - out/report.md\n      - out/extra.md\n"),
        "{text}"
    );
    assert!(
        text.contains("after: { fetch: failure, audit: success }"),
        "{text}"
    );
    assert!(text.contains("    extract:\n      {}\n"), "{text}");
    assert!(text.starts_with("# header comment kept byte for byte\n"));
    assert!(applied.bytes_preserved(RICH));
    assert_eq!(applied.changed().len(), 5);
}

#[test]
fn a_task_is_inserted_from_exact_text_and_removed_again() {
    let doc = rich();
    let text = "# reused component, kept verbatim\ninvoke: { tool: \"nika:log\", args: { message: \"done\" } }  # flow\nafter: { report: success }\n";
    let applied = doc
        .apply(&[Edit::insert_text(path("tasks"), "notify", text)])
        .expect("insert text");
    let source = applied.document().source();
    assert!(source.contains("  notify:\n    # reused component, kept verbatim\n    invoke: { tool: \"nika:log\", args: { message: \"done\" } }  # flow\n    after: { report: success }\n"), "{source}");
    assert_eq!(applied.document().workflow().tasks.len(), 3);
    let removed = applied
        .document()
        .apply(&[Edit::remove(path("tasks.notify"))])
        .expect("remove");
    assert_eq!(removed.document().source(), RICH);
}

#[test]
fn refusals_are_typed_and_keep_the_source() {
    let doc = rich();
    let cases = [
        (Edit::set(path("const.missing"), json!(1)), "unknown_path"),
        (Edit::insert(path("permits.tools"), "k", json!(1)), "shape"),
        (
            Edit::insert(path("const"), "window_hours", json!(1)),
            "shape",
        ),
        (
            Edit::set(path("tasks.fetch.retry.max_attempts"), json!("three")),
            "language",
        ),
        (
            Edit::set(path("tasks.report.for_each.max_parallel"), json!(0)),
            "language",
        ),
        (
            Edit::insert(path("tasks.fetch"), "unknown_field", json!(1)),
            "language",
        ),
        (Edit::set(Path::root(), json!({})), "shape"),
    ];
    for (edit, kind) in cases {
        let refusal = doc.apply(std::slice::from_ref(&edit)).expect_err("refused");
        assert_eq!(refusal.kind(), kind, "{edit:?}: {refusal}");
        assert!(
            refusal.to_string().contains("source is unchanged") || kind == "language",
            "{refusal}"
        );
    }
    assert_eq!(doc.source(), RICH);
}

#[test]
fn a_meaning_change_beyond_the_edit_is_drift() {
    let doc = rich();
    let expected = doc.literal().clone();
    let changed = RICH.replacen("backoff_ms: 250", "backoff_ms: 251", 1);
    let refusal = doc
        .judge(
            changed,
            &expected,
            &path("const.window_hours"),
            &[path("const.window_hours")],
        )
        .expect_err("drift");
    assert!(matches!(refusal, Refusal::Drift { .. }), "{refusal}");
}

#[test]
fn nodes_carry_paths_kinds_spans_and_the_parser_keysets() {
    let doc = rich();
    let nodes = doc.nodes();
    let find = |p: &str| nodes.iter().find(|n| n.path == path(p)).expect(p);
    let retry = find("tasks.fetch.retry.max_attempts");
    assert_eq!(retry.keyset, Some("retry"));
    assert_eq!(retry.to_json()["kind"], "number");
    assert_eq!(find("tasks.report.invoke").keyset, Some("verb"));
    assert_eq!(find("tasks.report.invoke.tool").keyset, Some("invoke"));
    assert_eq!(find("tasks.report.invoke.args.path").keyset, None);
    assert_eq!(find("const.window_hours").keyset, None);
    assert_eq!(find("const.limits.value").keyset, Some("const"));
    assert_eq!(find("inputs.region.default").keyset, Some("inputs"));
    assert_eq!(find("permits.fs.write").keyset, Some("permits.fs"));
    assert_eq!(find("tasks.report.after").style, Style::Flow);
    let span = find("const.window_hours").span.clone().expect("placed");
    assert_eq!(&RICH[span], "48");
}

#[test]
fn edits_apply_in_order_on_the_document_each_one_left() {
    let doc = rich();
    let applied = doc
        .apply(&[
            Edit::insert(
                path("tasks"),
                "late",
                json!({"exec": {"command": ["true"]}, "after": {"report": "terminal"}}),
            ),
            Edit::set(path("tasks.late.exec.command.0"), json!("false")),
        ])
        .expect("dependent edits");
    let late = applied.document().value(&path("tasks.late")).cloned();
    assert_eq!(
        late,
        Some(json!({"exec": {"command": ["false"]}, "after": {"report": "terminal"}}))
    );
    assert!(applied.bytes_preserved(RICH));
}

const TASK: &str = "tasks:\n  t:\n    exec:\n      command: [\"true\"]\n";

fn applied(source: &str, edits: &[Edit]) -> String {
    let doc = Document::parse(source).expect("parses");
    let applied = doc.apply(edits).expect("applied");
    assert!(applied.bytes_preserved(source));
    applied.document().source().to_owned()
}

#[test]
fn closed_blocks_tag_their_keys_where_the_vocabulary_is_unpublished() {
    let source = "nika: t\ntasks:\n  a:\n    for_each:\n      items: [1]\n      max_items: 3\n    lift:\n      - law: data-as-code\n        because: x\n    with:\n      n: 1\n    exec:\n      command: [\"true\"]\n";
    let doc = Document::parse(source).expect("parses");
    let tag = |p: Path| doc.node(&p).and_then(|n| n.keyset);
    assert_eq!(tag(path("tasks.a.for_each.max_items")), Some("for_each"));
    assert_eq!(
        tag(Path::new(["tasks", "a", "lift", "0", "law"])),
        Some("lift")
    );
    assert_eq!(tag(path("tasks.a.exec.command")), Some("exec"));
    assert_eq!(tag(path("tasks.a.with")), Some("task"));
    assert_eq!(tag(path("tasks.a.with.n")), None);
    assert_eq!(tag(path("tasks.a")), None);
}

#[test]
fn line_endings_and_a_byte_order_mark_are_kept() {
    let crlf = format!(
        "nika: crlf\r\nconst:\r\n  n: 1\r\n{}",
        TASK.replace('\n', "\r\n")
    );
    let edited = applied(
        &crlf,
        &[
            Edit::set(path("const.n"), json!(2)),
            Edit::insert(path("const"), "m", json!(3)),
        ],
    );
    assert_eq!(edited, crlf.replace("  n: 1\r\n", "  n: 2\r\n  m: 3\r\n"));
    let bom = format!("\u{feff}nika: bom\nconst:\n  n: 1\n{TASK}");
    let edited = applied(&bom, &[Edit::set(path("const.n"), json!(2))]);
    assert_eq!(edited, bom.replace("n: 1", "n: 2"));
}

#[test]
fn an_omitted_value_and_a_flush_sequence_take_edits() {
    let source = format!("nika: t\nconst:\n  empty:\n  list:\n  - a\n  - b\n{TASK}");
    assert_eq!(
        applied(&source, &[Edit::set(path("const.empty"), json!("x"))]),
        source.replace("  empty:\n", "  empty: x\n")
    );
    assert_eq!(
        applied(&source, &[Edit::push(path("const.list"), json!("c"))]),
        source.replace("  - b\n", "  - b\n  - c\n")
    );
    assert_eq!(
        applied(&source, &[Edit::remove(Path::new(["const", "list", "0"]))]),
        source.replace("  - a\n", "")
    );
    let scalar = applied(&source, &[Edit::set(path("const.list"), json!("x"))]);
    let doc = Document::parse(scalar).expect("a scalar under its key");
    assert_eq!(doc.value(&path("const.list")), Some(&json!("x")));
}

#[test]
fn a_keep_chomped_block_is_kept_rather_than_rewritten() {
    let source = format!("nika: t\nconst:\n  note: |+\n    kept\n\n{TASK}");
    let doc = Document::parse(source.clone()).expect("parses");
    let refusal = doc
        .apply(&[Edit::set(path("const.note"), json!("new\n"))])
        .expect_err("kept");
    assert_eq!(refusal.kind(), "layout", "{refusal}");
    assert_eq!(doc.source(), source);
}

#[test]
fn flow_removals_take_their_own_comma() {
    let source = format!("nika: t\nconst:\n  m: {{ a: 1, b: 2, c: 3 }}\n  one: {{ a: 1 }}\n{TASK}");
    let remove = |p: &str| applied(&source, &[Edit::remove(Path::dotted(p).expect("path"))]);
    assert_eq!(
        remove("const.m.b"),
        source.replace("a: 1, b: 2, c: 3", "a: 1, c: 3")
    );
    assert_eq!(
        remove("const.m.c"),
        source.replace("a: 1, b: 2, c: 3", "a: 1, b: 2")
    );
    assert_eq!(
        remove("const.m.a"),
        source.replace("a: 1, b: 2, c: 3", "b: 2, c: 3")
    );
    let emptied = remove("const.one.a");
    let doc = Document::parse(emptied).expect("an empty flow mapping");
    assert_eq!(doc.value(&path("const.one")), Some(&json!({})));
}

#[test]
fn stream_markers_and_directives_are_kept_and_never_read_as_values() {
    let source = format!("%YAML 1.2\n---\nnika: marked\nconst:\n  n: 1\n{TASK}...\n");
    let edited = applied(&source, &[Edit::set(path("const.n"), json!(2))]);
    assert_eq!(edited, source.replace("n: 1", "n: 2"));
    let doc = Document::parse(source).expect("a marked document imports");
    assert_eq!(doc.value(&path("nika")), Some(&json!("marked")));
}

#[test]
fn a_new_key_that_is_not_a_simple_name_is_quoted() {
    let source = format!("nika: t\nconst:\n  headers:\n    accept: text/plain\n{TASK}");
    let edited = applied(
        &source,
        &[Edit::insert(
            path("const.headers"),
            "Content Type",
            json!("a: b"),
        )],
    );
    assert!(
        edited.contains("    accept: text/plain\n    \"Content Type\": \"a: b\"\n"),
        "{edited}"
    );
}

/// Every syntactic owner of a task reference, beside text that only spells it.
const RENAMES: &str = r#"nika: renames
inputs:
  topic:
    type: string
    default: "fetch the topic"
tasks:
  fetch:
    exec:
      command: ["echo", "${{ inputs.topic }}"]
    extract:
      count: "length"
  summarize:
    after: { fetch: success }
    with:
      data: ${{ tasks.fetch.output }}
      n: ${{ tasks.fetch.count }}
    infer:
      prompt: |
        Summarize ${{ with.data }} (tasks.fetch is the source; ${{ 'tasks.fetch' }})
        count ${{ with.n }}
  report:
    after:
      summarize: success
      fetch: terminal
    when: ${{ tasks.fetch.status == "success" && tasks.fetch_all == null }}
    lift:
      - law: taint
        from: tasks.fetch.output
        because: "reviewed"
    invoke:
      tool: "nika:log"
      args:
        message: "${{ tasks['fetch'].output }} vs fetch and tasks.fetch"
outputs:
  total: ${{ tasks.fetch.count }}
"#;

#[test]
fn a_task_rename_rewrites_each_reference_through_its_owner_only() {
    let renamed = applied(RENAMES, &[Edit::rename(path("tasks.fetch"), "grab")]);
    let expected = RENAMES
        .replacen("  fetch:\n    exec:", "  grab:\n    exec:", 1)
        .replacen("after: { fetch: success }", "after: { grab: success }", 1)
        .replacen(
            "data: ${{ tasks.fetch.output }}",
            "data: ${{ tasks.grab.output }}",
            1,
        )
        .replacen(
            "n: ${{ tasks.fetch.count }}",
            "n: ${{ tasks.grab.count }}",
            1,
        )
        .replacen("      fetch: terminal", "      grab: terminal", 1)
        .replacen("${{ tasks.fetch.status ==", "${{ tasks.grab.status ==", 1)
        .replacen("from: tasks.fetch.output", "from: tasks.grab.output", 1)
        .replacen(
            "${{ tasks['fetch'].output }}",
            "${{ tasks['grab'].output }}",
            1,
        )
        .replacen(
            "total: ${{ tasks.fetch.count }}",
            "total: ${{ tasks.grab.count }}",
            1,
        );
    assert_eq!(renamed, expected);
    assert!(renamed.contains("(tasks.fetch is the source; ${{ 'tasks.fetch' }})"));
    assert!(renamed.contains("tasks.fetch_all == null"));
    assert!(renamed.contains("default: \"fetch the topic\""));
}

#[test]
fn inputs_bindings_and_extracts_are_renamed_in_their_own_scope() {
    let input = applied(RENAMES, &[Edit::rename(path("inputs.topic"), "subject")]);
    assert_eq!(
        input,
        RENAMES.replacen("  topic:\n", "  subject:\n", 1).replacen(
            "${{ inputs.topic }}",
            "${{ inputs.subject }}",
            1
        )
    );
    let binding = applied(
        RENAMES,
        &[Edit::rename(path("tasks.summarize.with.data"), "payload")],
    );
    assert_eq!(
        binding,
        RENAMES
            .replacen("      data: ${{", "      payload: ${{", 1)
            .replacen("${{ with.data }}", "${{ with.payload }}", 1)
    );
    let extract = applied(
        RENAMES,
        &[Edit::rename(path("tasks.fetch.extract.count"), "items")],
    );
    assert_eq!(
        extract,
        RENAMES
            .replacen("      count: \"length\"", "      items: \"length\"", 1)
            .replace("tasks.fetch.count }}", "tasks.fetch.items }}")
    );
}

#[test]
fn a_rename_refuses_a_taken_or_illegal_name() {
    let doc = Document::parse(RENAMES).expect("parses");
    let taken = doc
        .apply(&[Edit::rename(path("tasks.fetch"), "report")])
        .expect_err("taken");
    assert_eq!(taken.kind(), "shape", "{taken}");
    let illegal = doc
        .apply(&[Edit::rename(path("tasks.fetch"), "Grab")])
        .expect_err("not snake_case");
    assert_eq!(illegal.kind(), "language", "{illegal}");
    let output = applied(RENAMES, &[Edit::rename(path("outputs.total"), "count")]);
    assert_eq!(output, RENAMES.replacen("  total: ${{", "  count: ${{", 1));
    assert_eq!(doc.source(), RENAMES);
}
