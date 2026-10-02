// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The screen refuses the values a run would build before the room's write budget sees them.
//! Pure: every case is a few lines of candidate source screened in memory; no room, runtime,
//! file or provider is reached, and nothing here says a candidate copies what its request asks.

use nika_compile_cognition::rehearse::Refusal;

use super::super::screen::{Refused, screen};
use super::text;

/// A read of `./in.txt` and its write to `./out.txt`. `@WITH@` extends the write's bindings,
/// `@FIELD@` adds lines to the write task, `@CONTENT@` is its content and `@TAIL@` extends the
/// workflow.
const COPY: &str = r#"nika: copy
permits:
  tools: ["nika:read", "nika:write", "nika:log"]
  fs: { read: ["./in.txt"], write: ["./out.txt", "./late.txt"] }
tasks:
  read_it:
    invoke: { tool: "nika:read", args: { path: "./in.txt" } }
  write_it:
    with: { chunk: "${{ tasks.read_it.output }}"@WITH@ }
@FIELD@    invoke: { tool: "nika:write", args: { path: "./out.txt", content: @CONTENT@ } }
@TAIL@"#;

/// The copy with one place changed: `(marker, text)` pairs, every other marker left plain.
fn copy(changes: &[(&str, &str)]) -> String {
    let mut source = COPY.to_owned();
    for (marker, change) in changes {
        source = source.replace(marker, change);
    }
    source
        .replace("@WITH@", "")
        .replace("@FIELD@", "")
        .replace("@CONTENT@", r#""${{ with.chunk }}""#)
        .replace("@TAIL@", "")
}

/// The screen over the copy's one observed input.
fn screened(candidate: &str) -> Result<(), Refused> {
    screen(candidate, &["./in.txt".to_owned()]).map(|_| ())
}

/// The screen's refusal of `candidate`: a data bound in the words of `field`, for `cause`.
fn bounded(candidate: &str, field: &str, cause: &str) {
    let refused = screened(candidate).expect_err(field);
    assert_eq!(refused.refusal, Refusal::DataBounds, "{}", refused.reason);
    assert!(
        refused.reason.starts_with(field) && refused.reason.contains(cause),
        "{field}, {cause}: {}",
        refused.reason
    );
}

/// The words of each refused form.
const TWO: &str = "joins 2 template islands";
const LIST: &str = "builds a list of values";
const HELD: &str = "builds an array or an object of values";

/// The compiler's two-task copy of one text file, read as text.
const TEXT_COPY: &str = r#"nika: compiled-workflow
const:
  source_path: ./in/source.txt
  output_path: ./out/copied.txt
permits:
  tools: ["nika:read", "nika:write"]
  fs: { read: ["./in/source.txt"], write: ["./out/copied.txt"] }
tasks:
  read_source:
    invoke: { tool: "nika:read", args: { path: "${{ const.source_path }}" } }
  write_output:
    after: { read_source: success }
    with: { content: "${{ tasks.read_source.output }}" }
    invoke: { tool: "nika:write", args: { path: "${{ const.output_path }}", content: "${{ with.content }}", create_dirs: true, overwrite: true } }
outputs:
  write_status: "${{ tasks.write_output.status }}"
"#;

#[test]
fn two_islands_are_refused_in_every_evaluated_field() {
    let twice = r#""${{ with.chunk }}${{ with.chunk }}""#;
    let with = r#", again: "${{ tasks.read_it.output }}${{ tasks.read_it.output }}""#;
    let when = format!("    when: {twice}\n");
    let fan = format!("    for_each: {{ items: {twice} }}\n");
    let recover = format!("    on_error: {{ recover: {twice} }}\n");
    let output =
        "outputs:\n  copied: \"${{ tasks.write_it.status }}${{ tasks.write_it.status }}\"\n";
    let model = "model: \"${{ inputs.provider }}/${{ inputs.name }}\"\n";
    for (field, candidate) in [
        ("task write_it with.again", copy(&[("@WITH@", with)])),
        ("task write_it when", copy(&[("@FIELD@", &when)])),
        ("task write_it for_each", copy(&[("@FIELD@", &fan)])),
        ("task write_it args.content", copy(&[("@CONTENT@", twice)])),
        (
            "task write_it on_error.recover",
            copy(&[("@FIELD@", &recover)]),
        ),
        ("outputs.copied", copy(&[("@TAIL@", output)])),
        ("model", copy(&[("@TAIL@", model)])),
    ] {
        bounded(&candidate, field, TWO);
    }
}

#[test]
fn a_list_built_of_references_is_refused_wherever_it_sits() {
    let list = r#""${{ [with.chunk, with.chunk] }}""#;
    let sized = "    when: \"${{ size([with.chunk]) > 0 }}\"\n";
    let member = "    when: \"${{ 'x' in [with.chunk] }}\"\n";
    let branch = r#", again: "${{ true ? [tasks.read_it.output] : [] }}""#;
    let method = "    on_error: { recover: \"${{ [with.chunk].size() }}\" }\n";
    let indexed = "    for_each: { items: \"${{ [tasks.read_it.output][0] }}\" }\n";
    for (field, candidate) in [
        ("task write_it args.content", copy(&[("@CONTENT@", list)])),
        ("task write_it when", copy(&[("@FIELD@", sized)])),
        ("task write_it when", copy(&[("@FIELD@", member)])),
        ("task write_it with.again", copy(&[("@WITH@", branch)])),
        (
            "task write_it on_error.recover",
            copy(&[("@FIELD@", method)]),
        ),
        ("task write_it for_each", copy(&[("@FIELD@", indexed)])),
    ] {
        bounded(&candidate, field, LIST);
    }
}

#[test]
fn a_json_container_holding_a_reference_is_refused() {
    let array = r#"["${{ with.chunk }}", "${{ with.chunk }}"]"#;
    let wrapper = r#"{ copy: "${{ with.chunk }}" }"#;
    let with = r#", again: ["${{ tasks.read_it.output }}"]"#;
    let fan = "    for_each: { items: [\"${{ with.chunk }}\"] }\n";
    let recover = "    on_error: { recover: { kept: \"${{ with.chunk }}\" } }\n";
    for (field, candidate) in [
        ("task write_it args.content", copy(&[("@CONTENT@", array)])),
        (
            "task write_it args.content",
            copy(&[("@CONTENT@", wrapper)]),
        ),
        ("task write_it with.again", copy(&[("@WITH@", with)])),
        ("task write_it for_each", copy(&[("@FIELD@", fan)])),
        (
            "task write_it on_error.recover",
            copy(&[("@FIELD@", recover)]),
        ),
    ] {
        bounded(&candidate, field, HELD);
    }
}

#[test]
fn the_text_and_bytes_copies_pass() {
    let bytes = TEXT_COPY.replace(
        r#"args: { path: "${{ const.source_path }}" }"#,
        r#"args: { path: "${{ const.source_path }}", binary: true }"#,
    );
    assert_ne!(bytes, TEXT_COPY);
    for candidate in [TEXT_COPY, bytes.as_str()] {
        let outcome = screen(candidate, &["./in/source.txt".to_owned()]);
        assert!(outcome.is_ok(), "{outcome:?}");
    }
    assert!(screened(&copy(&[])).is_ok());
}

#[test]
fn literal_forms_pass() {
    for candidate in [
        // Escaped openers are no islands.
        copy(&[("@CONTENT@", r"'\${{ with.chunk }}\${{ with.chunk }}'")]),
        // Static containers, and a template that reads nothing inside one.
        copy(&[("@FIELD@", "    for_each: { items: [1, 2] }\n")]),
        copy(&[("@CONTENT@", r#"["a", "b"]"#)]),
        copy(&[("@WITH@", r#", kept: { note: "plain" }"#)]),
        copy(&[("@CONTENT@", r#"["${{ 'x' }}"]"#)]),
        // A list of literals, and membership in one.
        copy(&[("@CONTENT@", r#""${{ [1, 2] }}""#)]),
        copy(&[("@FIELD@", "    when: \"${{ with.chunk in ['x', 'y'] }}\"\n")]),
        // One island in prose.
        copy(&[("@CONTENT@", r#""mcp: ${{ with.chunk }}""#)]),
    ] {
        let outcome = screened(&candidate);
        assert!(outcome.is_ok(), "{candidate}\n{outcome:?}");
    }
}

#[test]
fn constants_inputs_and_metadata_are_data() {
    let twice = "\"${{ a }}${{ b }}\"";
    let constant = format!("const:\n  note: {twice}\n");
    let input = format!("inputs:\n  greeting: {{ type: string, default: {twice} }}\n");
    let described = format!(
        "outputs:\n  copied: {{ value: \"${{{{ tasks.write_it.status }}}}\", type: string, description: {twice} }}\n"
    );
    for tail in [constant, input, described] {
        let candidate = copy(&[("@TAIL@", &tail)]);
        let outcome = screened(&candidate);
        assert!(outcome.is_ok(), "{candidate}\n{outcome:?}");
    }
}

#[test]
fn a_template_the_screen_cannot_read_is_refused() {
    let candidate = copy(&[("@CONTENT@", r#""${{ with.chunk""#)]);
    bounded(&candidate, "task write_it args.content", "cannot read");
    let refused = text("a value", "${{ with.chunk", false).expect_err("an unterminated island");
    assert!(refused.reason.contains("cannot read"), "{}", refused.reason);
    assert!(text("a value", "", false).is_ok());
}

#[test]
fn existing_refusals_come_first() {
    let twice = r#""${{ with.chunk }}${{ with.chunk }}""#;
    let jq = "  shape_it:\n    with: { text: \"${{ tasks.read_it.output }}\" }\n    invoke: { tool: \"nika:jq\", args: { input: { text: \"${{ with.text }}\" }, expression: \".\" } }\n";
    let exec = "  run_it:\n    exec: { command: [\"true\"] }\n";
    let unobserved =
        "  read_more:\n    invoke: { tool: \"nika:read\", args: { path: \"./elsewhere.txt\" } }\n";
    for (tail, refusal, words) in [
        (jq, Refusal::DataBounds, "runs nika:jq"),
        (exec, Refusal::Effect, "exec"),
        (unobserved, Refusal::Confinement, "not observed"),
    ] {
        let candidate = copy(&[("@CONTENT@", twice), ("@TAIL@", tail)]);
        let refused = screened(&candidate).expect_err(words);
        assert_eq!(refused.refusal, refusal, "{}", refused.reason);
        assert!(refused.reason.contains(words), "{}", refused.reason);
    }
}

#[test]
fn every_task_is_visited() {
    let twice = r#""${{ tasks.read_it.output }}${{ tasks.read_it.output }}""#;
    let never = format!(
        "  late:\n    when: false\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"./late.txt\", content: {twice} }} }}\n"
    );
    let unwind = format!(
        "  late:\n    after: {{ write_it: unwind }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"./late.txt\", content: {twice} }} }}\n"
    );
    let logged =
        format!("  late:\n    invoke: {{ tool: \"nika:log\", args: {{ message: {twice} }} }}\n");
    bounded(&copy(&[("@TAIL@", &never)]), "task late args.content", TWO);
    bounded(&copy(&[("@TAIL@", &unwind)]), "task late args.content", TWO);
    bounded(&copy(&[("@TAIL@", &logged)]), "task late args.message", TWO);
}
