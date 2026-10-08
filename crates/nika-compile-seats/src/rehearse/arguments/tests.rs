// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The law refuses the values a run would build before the room's write budget sees them.
//! Pure: every case is a few lines of candidate source read in memory; no room, runtime, file
//! or provider is reached, and nothing here says a candidate copies what its request asks. The
//! host's screen around it (its order, its refusal class) is tested where it lives.

use nika_schema::{FileId, ParseMode};

use super::{evaluated, text};

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

/// The law over `candidate`, parsed as a host's screen parses it.
fn screened(candidate: &str) -> Result<(), String> {
    let workflow = nika_schema::parse(candidate, FileId::new(0), ParseMode::Strict)
        .expect("a candidate the strict parser reads");
    evaluated(&workflow)
}

/// The law's refusal of `candidate`, in the words of `field`, for `cause`.
fn bounded(candidate: &str, field: &str, cause: &str) {
    let reason = screened(candidate).expect_err(field);
    assert!(
        reason.starts_with(field) && reason.contains(cause),
        "{field}, {cause}: {reason}"
    );
}

/// The words of each refused form.
const TWO: &str = "joins 2 template islands";
const LIST: &str = "builds a list of values";
const HELD: &str = "builds an array or an object of values";

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
    assert!(refused.contains("cannot read"), "{refused}");
    assert!(text("a value", "", false).is_ok());
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
