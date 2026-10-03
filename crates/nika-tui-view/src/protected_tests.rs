// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A protected object never shows its secret. First the four leaks an
//! independent review traced (the values of a patch, a multi-line value, a
//! workflow's source, the facts and notes of the head), then every format
//! the view knows with its secret at the top, two levels deep, inside an
//! array, in a multi-line value and under a parent key naming a
//! credential: the secret's bytes reach no line, fact, note or head row,
//! and the view says it masked something.

#![allow(clippy::expect_used)]

use nika_display::theme::Role;
use ratatui::text::Line;

use super::{Canvas, Content, Format, Limits, Meta, Note, Rendered, artifact};

/// A secret no shape betrays (short, lower case, no known prefix): only
/// its place in the structure can say it is one.
const SECRET: &str = "hunter2hunter2";

/// Where a sample puts its secret.
const PLACES: [&str; 5] = ["top", "nested", "array", "multi-line", "parent"];

fn text(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// An object named `name`, declared `mime`, protected or not.
fn meta(name: &str, mime: Option<&str>, protected: bool) -> Meta {
    let mut meta = Meta::new(name);
    meta.mime_type = mime.map(str::to_owned);
    meta.protected = protected;
    meta
}

/// Every word a rendering holds: its title, facts, body and notes, and the
/// rows its head draws.
fn everything(rendered: &Rendered, canvas: Canvas) -> String {
    let mut all = vec![rendered.title.clone()];
    all.extend(rendered.facts.iter().cloned());
    all.extend(rendered.lines.iter().map(text));
    all.extend(rendered.notes.iter().map(|n| n.text(canvas.ascii)));
    all.extend(rendered.head(canvas).iter().map(text));
    all.join("\n")
}

fn masked(rendered: &Rendered) -> bool {
    rendered
        .notes
        .iter()
        .any(|n| matches!(n, Note::Masked { .. }))
}

/// Whether `text` holds a control character or a bidirectional mark as is.
fn raw_marks(text: &str) -> bool {
    text.chars()
        .any(|c| c.is_control() || matches!(u32::from(c), 0x202A..=0x202E | 0x2066..=0x2069))
}

/// `body` rendered protected: its `secret` shows nowhere and a note says
/// what was masked.
fn assert_masked(name: &str, mime: Option<&str>, body: &str, secret: &str) -> Rendered {
    let canvas = Canvas::new(120, false, true);
    let rendered = artifact(Content::Text(body), &meta(name, mime, true), canvas);
    let all = everything(&rendered, canvas);
    assert!(!all.contains(secret), "{name}: the secret shows:\n{all}");
    assert!(
        masked(&rendered),
        "{name}: no masked note: {:?}",
        rendered.notes
    );
    rendered
}

#[test]
fn a_protected_json_patch_masks_what_its_values_hold() {
    let body = r#"[{"op":"add","path":"/config","value":{"password":"hunter2"}}]"#;
    let patch = Some("application/json-patch+json");
    let rendered = assert_masked("ops.json", patch, body, "hunter2");
    assert_eq!(rendered.format, Format::JsonPatch);
    let row = text(&rendered.lines[0]);
    assert!(
        row.starts_with("+ add") && row.contains("/config") && row.contains("\"password\""),
        "the operation, its path and the key stay readable: {row}"
    );
    let document = assert_masked("ops.json", Some("application/json"), body, "hunter2");
    assert_eq!(
        document.format,
        Format::Json,
        "the same bytes as a document"
    );
}

#[test]
fn a_protected_merge_patch_walks_arrays() {
    let merge = Some("application/merge-patch+json");
    let users = r#"{"users":[{"password":"hunter2"}]}"#;
    let rendered = assert_masked("merge.json", merge, users, "hunter2");
    assert_eq!(rendered.format, Format::MergePatch);
}

#[test]
fn a_protected_merge_patch_masks_what_a_secret_parent_holds() {
    let merge = Some("application/merge-patch+json");
    assert_masked(
        "merge.json",
        merge,
        r#"{"auth":{"pass":"hunter2"}}"#,
        "hunter2",
    );
}

#[test]
fn a_yaml_block_scalar_under_a_secret_key_stays_masked() {
    let yaml = "password: |\n  hunter2hunter2\nuser: ann\n";
    let rendered = assert_masked("conf.yaml", None, yaml, SECRET);
    let rows: Vec<String> = rendered.lines.iter().map(text).collect();
    assert_eq!(rows[0], "password: |", "the indicator stays");
    assert_eq!(rows[2], "user: ann", "the block ends where its indent does");
}

#[test]
fn a_toml_multi_line_string_under_a_secret_key_stays_masked() {
    let toml = "password = \"\"\"\nhunter2hunter2\n\"\"\"\nnext = 1\n";
    let rendered = assert_masked("conf.toml", None, toml, SECRET);
    let last = rendered.lines.last().expect("a last row");
    let accent = crate::role::style(Role::Accent, true);
    assert!(
        last.spans
            .iter()
            .any(|s| s.content.as_ref() == "next" && s.style == accent),
        "the closing fence closes the string, the key after it is a key: {last:?}"
    );
}

#[test]
fn a_protected_workflow_source_is_masked() {
    let body = "nika: draft\nconst:\n  api_key: sk-live-0123456789abcdefXYZ\n";
    let rendered = assert_masked("draft.nika", None, body, "0123456789abcdef");
    assert_eq!(rendered.format, Format::Workflow);
}

#[test]
fn a_protected_shape_fact_is_withheld() {
    let one = Canvas::new(80, false, false).with_limits(Limits::new(1 << 18, 1, 4096));
    let key = "\"sk-proj-0123456789abcdef\"";
    let rendered = artifact(Content::Text(key), &meta("key.json", None, true), one);
    let all = everything(&rendered, one);
    assert!(!all.contains("0123456789abcdef"), "the shape fact: {all}");
    assert!(
        rendered.facts.iter().any(|f| f.contains("withheld")),
        "{:?}",
        rendered.facts
    );
}

#[test]
fn a_key_the_shape_fact_decodes_is_cleaned_in_the_head() {
    let two = Canvas::new(80, false, false).with_limits(Limits::new(1 << 18, 2, 4096));
    let doc = r#"{"\u202eevil": 1, "b": 2, "c": 3}"#;
    let rendered = artifact(Content::Text(doc), &meta("doc.json", None, false), two);
    assert!(
        rendered.facts.iter().any(|f| f.starts_with("shape ")),
        "{:?}",
        rendered.facts
    );
    let head: Vec<String> = rendered.head(two).iter().map(text).collect();
    assert!(
        !head.iter().any(|row| raw_marks(row)),
        "a raw mark in the head: {head:?}"
    );
    assert!(head.iter().any(|row| row.contains("<U+202E>")), "{head:?}");
}

#[test]
fn a_declared_type_in_a_note_is_cleaned_in_the_head() {
    let canvas = Canvas::new(80, false, false);
    let lying = meta("x.png", Some("image/png; x=\u{202E}gnp.exe"), false);
    let rendered = artifact(Content::Bytes(b"just words"), &lying, canvas);
    assert!(
        rendered
            .notes
            .iter()
            .any(|n| matches!(n, Note::Mismatch { .. })),
        "{:?}",
        rendered.notes
    );
    let head: Vec<String> = rendered.head(canvas).iter().map(text).collect();
    assert!(
        !head.iter().any(|row| raw_marks(row)),
        "a raw mark in the head: {head:?}"
    );
}

/// One sample: the viewer it reaches, its name and declared type, where
/// its secret sits, and its bytes (`%S%` stands for the secret).
struct Sample {
    format: Format,
    name: &'static str,
    mime: Option<&'static str>,
    place: &'static str,
    body: &'static str,
}

const fn sample(
    format: Format,
    name: &'static str,
    mime: Option<&'static str>,
    place: &'static str,
    body: &'static str,
) -> Sample {
    Sample {
        format,
        name,
        mime,
        place,
        body,
    }
}

const PATCH: Option<&str> = Some("application/json-patch+json");
const MERGE: Option<&str> = Some("application/merge-patch+json");

/// The JSON family: a document, a JSON Patch, a merge patch (one row per
/// sample, so the table stays a table).
#[rustfmt::skip]
fn json_samples() -> Vec<Sample> {
    use Format::{Json, JsonPatch, MergePatch};
    vec![
        sample(Json, "data.json", None, "top", r#"{"password": "%S%", "user": "ann"}"#),
        sample(Json, "data.json", None, "nested", r#"{"db": {"conn": {"password": "%S%"}}}"#),
        sample(Json, "data.json", None, "array", r#"{"users": [{"name": "ann", "password": "%S%"}]}"#),
        sample(Json, "data.json", None, "array", r#"{"api_keys": ["%S%"]}"#),
        sample(Json, "data.json", None, "multi-line", r#"{"password": "first line\n%S%\nlast line"}"#),
        sample(Json, "data.json", None, "parent", r#"{"auth": {"pass": "%S%", "user": "ann"}}"#),
        sample(JsonPatch, "ops.json", PATCH, "top", r#"[{"op": "add", "path": "/password", "value": "%S%"}]"#),
        sample(JsonPatch, "ops.json", PATCH, "nested", r#"[{"op": "add", "path": "/config", "value": {"db": {"password": "%S%"}}}]"#),
        sample(JsonPatch, "ops.json", PATCH, "array", r#"[{"op": "add", "path": "/users", "value": [{"password": "%S%"}]}]"#),
        sample(JsonPatch, "ops.json", PATCH, "multi-line", r#"[{"op": "replace", "path": "/password", "value": "first\n%S%"}]"#),
        sample(JsonPatch, "ops.json", PATCH, "parent", r#"[{"op": "add", "path": "/auth", "value": {"pass": "%S%"}}]"#),
        sample(JsonPatch, "ops.json", PATCH, "parent", r#"[{"op": "test", "path": "/auth/pass", "value": "%S%"}]"#),
        sample(MergePatch, "merge.json", MERGE, "top", r#"{"password": "%S%", "draft": null}"#),
        sample(MergePatch, "merge.json", MERGE, "nested", r#"{"db": {"conn": {"password": "%S%"}}}"#),
        sample(MergePatch, "merge.json", MERGE, "array", r#"{"users": [{"password": "%S%"}]}"#),
        sample(MergePatch, "merge.json", MERGE, "multi-line", r#"{"password": "first\n%S%"}"#),
        sample(MergePatch, "merge.json", MERGE, "parent", r#"{"auth": {"pass": "%S%"}}"#),
    ]
}

/// YAML, TOML and the two table formats.
#[rustfmt::skip]
fn data_samples() -> Vec<Sample> {
    use Format::{Csv, Toml, Yaml};
    vec![
        sample(Yaml, "conf.yaml", None, "top", "user: ann\npassword: %S%\n"),
        sample(Yaml, "conf.yaml", None, "nested", "db:\n  conn:\n    password: %S%\n"),
        sample(Yaml, "conf.yaml", None, "array", "users:\n  - name: ann\n    password: %S%\n"),
        sample(Yaml, "conf.yaml", None, "array", "api_keys:\n- %S%\n- second\n"),
        sample(Yaml, "conf.yaml", None, "array", "tokens: [%S%, second]\n"),
        sample(Yaml, "conf.yaml", None, "multi-line", "password: |\n  %S%\n  second line\nuser: ann\n"),
        sample(Yaml, "conf.yaml", None, "multi-line", "private_key: >-\n  %S%\n"),
        sample(Yaml, "conf.yaml", None, "parent", "auth:\n  pass: %S%\n  user: ann\n"),
        sample(Toml, "conf.toml", None, "top", "password = \"%S%\"\n"),
        sample(Toml, "conf.toml", None, "nested", "[db.conn]\npassword = \"%S%\"\n"),
        sample(Toml, "conf.toml", None, "nested", "db = { conn = { password = \"%S%\" } }\n"),
        sample(Toml, "conf.toml", None, "array", "tokens = [\"%S%\"]\n"),
        sample(Toml, "conf.toml", None, "array", "users = [{ name = \"ann\", password = \"%S%\" }]\n"),
        sample(Toml, "conf.toml", None, "array", "[[users]]\npassword = \"%S%\"\n"),
        sample(Toml, "conf.toml", None, "multi-line", "password = \"\"\"\n%S%\n\"\"\"\nport = 8080\n"),
        sample(Toml, "conf.toml", None, "multi-line", "password = '''\n%S%\n'''\n"),
        sample(Toml, "conf.toml", None, "parent", "[auth]\npass = \"%S%\"\n"),
        sample(Toml, "conf.toml", None, "parent", "auth = { pass = \"%S%\" }\n"),
        sample(Csv, "table.csv", None, "top", "user,password\nann,%S%\n"),
        sample(Csv, "table.csv", None, "nested", "user,db.conn.password\nann,%S%\n"),
        sample(Csv, "table.csv", None, "nested", "user,payload\nann,\"{\"\"db\"\": {\"\"password\"\": \"\"%S%\"\"}}\"\n"),
        sample(Csv, "table.csv", None, "array", "user,tokens[0]\nann,%S%\n"),
        sample(Csv, "table.csv", None, "multi-line", "user,password\nann,\"%S%\nsecond\"\n"),
        sample(Csv, "table.csv", None, "parent", "user,auth.pass\nann,%S%\n"),
        sample(Csv, "table.tsv", None, "top", "user\tpassword\nann\t%S%\n"),
        sample(Csv, "table.tsv", None, "nested", "user\tdb.conn.password\nann\t%S%\n"),
        sample(Csv, "table.tsv", None, "array", "user\ttokens[0]\nann\t%S%\n"),
        sample(Csv, "table.tsv", None, "multi-line", "user\tpassword\nann\t\"%S%\nsecond\"\n"),
        sample(Csv, "table.tsv", None, "parent", "user\tauth.pass\nann\t%S%\n"),
    ]
}

/// Markdown, a diff, plain text, code and a workflow's source (by its
/// extension and by its first words).
#[rustfmt::skip]
fn text_samples() -> Vec<Sample> {
    use Format::{Code, Diff, Markdown, Text, Workflow};
    vec![
        sample(Markdown, "notes.md", None, "top", "# Setup\n\npassword: %S%\n"),
        sample(Markdown, "notes.md", None, "nested", "```yaml\ndb:\n  conn:\n    password: %S%\n```\n"),
        sample(Markdown, "notes.md", None, "array", "```json\n{\"users\": [{\"password\": \"%S%\"}]}\n```\n"),
        sample(Markdown, "notes.md", None, "array", "- tokens:\n  - %S%\n"),
        sample(Markdown, "notes.md", None, "multi-line", "```yaml\npassword: |\n  %S%\n```\n"),
        sample(Markdown, "notes.md", None, "parent", "```yaml\nauth:\n  pass: %S%\n```\n"),
        sample(Diff, "change.diff", None, "top", "--- a/.env\n+++ b/.env\n@@ -1 +1 @@\n-PASSWORD=old\n+PASSWORD=%S%\n"),
        sample(Diff, "change.diff", None, "nested", "@@ -1,3 +1,3 @@\n db:\n   conn:\n-    password: old\n+    password: %S%\n"),
        sample(Diff, "change.diff", None, "array", "@@ -1,2 +1,2 @@\n tokens:\n-  - old\n+  - %S%\n"),
        sample(Diff, "change.diff", None, "multi-line", "@@ -1,2 +1,2 @@\n password: |\n-  old\n+  %S%\n"),
        sample(Diff, "change.diff", None, "parent", "@@ -1,2 +1,2 @@\n auth:\n-  pass: old\n+  pass: %S%\n"),
        sample(Text, "notes.txt", None, "top", "The demo password: %S%\n"),
        sample(Text, "notes.txt", None, "top", "curl -H \"Authorization: Bearer %S%\" https://api.example.com\n"),
        sample(Text, "notes.txt", None, "nested", "db:\n  conn:\n    password: %S%\n"),
        sample(Text, "notes.txt", None, "array", "tokens:\n  - %S%\n"),
        sample(Text, "notes.txt", None, "multi-line", "password: |\n  %S%\n"),
        sample(Text, "notes.txt", None, "parent", "auth:\n  pass: %S%\n"),
        sample(Code, "main.rs", None, "top", "let password = \"%S%\";\n"),
        sample(Code, ".env", None, "top", "DB_PASSWORD=%S%\n"),
        sample(Code, "main.py", None, "nested", "config = {\n    \"db\": {\n        \"password\": \"%S%\",\n    },\n}\n"),
        sample(Code, "config.js", None, "array", "const users = [\n  { name: \"ann\", password: \"%S%\" },\n];\n"),
        sample(Code, "main.py", None, "multi-line", "PASSWORD = \"\"\"\n%S%\n\"\"\"\n"),
        sample(Code, "main.py", None, "parent", "auth = {\n    \"pass\": \"%S%\",\n}\n"),
        sample(Workflow, "draft.nika", None, "top", "nika: draft\npassword: %S%\n"),
        sample(Workflow, "draft.nika", None, "nested", "nika: draft\ntasks:\n  call:\n    invoke:\n      tool: \"nika:fetch\"\n      args:\n        password: %S%\n"),
        sample(Workflow, "draft.nika", None, "array", "nika: draft\ntasks:\n  call:\n    invoke:\n      args:\n        tokens:\n          - %S%\n"),
        sample(Workflow, "draft.nika", None, "multi-line", "nika: draft\nconst:\n  private_key: |\n    %S%\n    more\n"),
        sample(Workflow, "draft.nika", None, "parent", "nika: draft\nconst:\n  auth:\n    pass: %S%\n"),
        sample(Workflow, "proposal", None, "top", "nika: proposal\npassword: %S%\n"),
        sample(Workflow, "proposal", None, "multi-line", "nika: proposal\nconst:\n  token: |\n    %S%\n"),
        sample(Workflow, "proposal", None, "parent", "nika: proposal\nconst:\n  auth:\n    pass: %S%\n"),
    ]
}

#[test]
fn no_protected_secret_survives_any_format_or_place() {
    let mut samples = json_samples();
    samples.extend(data_samples());
    samples.extend(text_samples());
    let formats = [
        Format::Json,
        Format::JsonPatch,
        Format::MergePatch,
        Format::Yaml,
        Format::Toml,
        Format::Csv,
        Format::Markdown,
        Format::Diff,
        Format::Text,
        Format::Code,
        Format::Workflow,
    ];
    for format in formats {
        for place in PLACES {
            assert!(
                samples
                    .iter()
                    .any(|s| s.format == format && s.place == place),
                "{format:?} has no sample with its secret {place}"
            );
        }
    }
    let canvases = [Canvas::new(160, false, false), Canvas::new(160, true, true)];
    let mut leaks = Vec::new();
    for s in &samples {
        let body = s.body.replace("%S%", SECRET);
        for canvas in canvases {
            let context = format!("{} ({}, ascii {})", s.name, s.place, canvas.ascii);
            let open = artifact(Content::Text(&body), &meta(s.name, s.mime, false), canvas);
            assert_eq!(open.format, s.format, "{context}");
            assert!(
                everything(&open, canvas).contains(SECRET),
                "{context}: the sample shows no secret even unprotected"
            );
            let shut = artifact(Content::Text(&body), &meta(s.name, s.mime, true), canvas);
            let all = everything(&shut, canvas);
            if all.contains(SECRET) || !masked(&shut) {
                leaks.push(format!("{context}, masked note {}:\n{all}", masked(&shut)));
            }
        }
    }
    assert!(
        leaks.is_empty(),
        "{} of {} renderings leak:\n\n{}",
        leaks.len(),
        samples.len() * canvases.len(),
        leaks.join("\n\n")
    );
}
