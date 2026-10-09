// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The screen around the argument law (`nika_compile_cognition::rehearse::arguments`, whose own
//! cases live with it): its earlier refusals come first, a value the law refuses is a data
//! bound in the words of its field, and the compiler's copies pass. Pure: candidate source
//! screened in memory; no room, runtime, file or provider is reached.

use nika_compile_cognition::rehearse::Refusal;

use super::{Refused, screen};

/// A read of `./in.txt` and its write to `./out.txt`. `@CONTENT@` is the write's content and
/// `@TAIL@` extends the workflow.
const COPY: &str = r#"nika: copy
permits:
  tools: ["nika:read", "nika:write"]
  fs: { read: ["./in.txt"], write: ["./out.txt"] }
tasks:
  read_it:
    invoke: { tool: "nika:read", args: { path: "./in.txt" } }
  write_it:
    with: { chunk: "${{ tasks.read_it.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out.txt", content: @CONTENT@ } }
@TAIL@"#;

/// The copy with one place changed: `(marker, text)` pairs, every other marker left plain.
fn copy(changes: &[(&str, &str)]) -> String {
    let mut source = COPY.to_owned();
    for (marker, change) in changes {
        source = source.replace(marker, change);
    }
    source
        .replace("@CONTENT@", r#""${{ with.chunk }}""#)
        .replace("@TAIL@", "")
}

/// The screen over the copy's one observed input.
fn screened(candidate: &str) -> Result<(), Refused> {
    screen(candidate, &["./in.txt".to_owned()]).map(|_| ())
}

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
fn a_value_the_law_refuses_is_a_data_bound_in_the_words_of_its_field() {
    let wrapped = r#"{ copy: "${{ with.chunk }}" }"#;
    let refused = screened(&copy(&[("@CONTENT@", wrapped)])).expect_err("a wrapped value");
    assert_eq!(refused.refusal, Refusal::DataBounds, "{}", refused.reason);
    let field = "task write_it args.content builds an array or an object of values";
    assert!(refused.reason.starts_with(field), "{}", refused.reason);
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
