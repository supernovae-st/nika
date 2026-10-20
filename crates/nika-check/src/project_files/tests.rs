// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::raw::RawAction;
use nika_schema::source::FileId;

fn wf(yaml: &str) -> RawWorkflow {
    parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse")
}

/// A project of in-memory files, every read recorded.
fn disk<'a>(
    files: &'a [(&'a str, &'a [u8])],
    opened: &'a mut Vec<String>,
) -> impl FnMut(&str) -> Result<Vec<u8>, ProjectFileRead> + 'a {
    move |path| {
        opened.push(path.to_owned());
        (files.iter())
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| bytes.to_vec())
            .ok_or(ProjectFileRead::Missing)
    }
}

const ROOT: &str = "nika: t\npermits:\n  fs: { read: [\"prompts/**\"] }\ntasks:\n  brief:\n    infer:\n      prompt: { file: \"prompts/brief.md\" }\n";

#[test]
fn a_project_file_composes_into_the_workflow_the_check_judges() {
    let mut opened = Vec::new();
    let files = [("prompts/brief.md", &b"Summarize the notes"[..])];
    let mut read_file = disk(&files, &mut opened);
    let (composed, report) = check_project(
        &wf(ROOT),
        "t.nika",
        &mut |_| Err("no child".to_owned()),
        &mut read_file,
    );
    assert!(
        report.project_files.is_empty(),
        "{:?}",
        report.project_files
    );
    assert!(!report.findings.iter().any(|f| f.kind == "project_file"));
    let RawAction::Infer(action) = &composed.tasks[0].value.action else {
        panic!("infer");
    };
    assert_eq!(action.prompt.value, "Summarize the notes");
    drop(read_file);
    assert_eq!(opened, ["prompts/brief.md"]);
}

#[test]
fn a_refused_project_file_dirties_the_report_with_its_code() {
    let mut opened = Vec::new();
    let mut read_file = disk(&[], &mut opened);
    let (_, report) = check_project(
        &wf(ROOT),
        "t.nika",
        &mut |_| Err("no child".to_owned()),
        &mut read_file,
    );
    assert_eq!(report.project_files.len(), 1);
    assert_eq!(
        report.project_files[0].code.as_deref(),
        Some("NIKA-FILE-002")
    );
    assert!(!report.is_clean());
    let row = (report.findings.iter())
        .find(|f| f.kind == "project_file")
        .expect("the refusal joins findings[]");
    assert_eq!(row.gate, "PROJECT FILES");
    assert_eq!(row.code.as_deref(), Some("NIKA-FILE-002"));
    let codes: Vec<String> = (report.extra_conformance_codes().iter())
        .map(ToString::to_string)
        .collect();
    assert!(codes.contains(&"NIKA-FILE-002".to_owned()), "{codes:?}");
}

#[test]
fn a_plain_check_reads_no_project_file() {
    let report = crate::check(&wf(ROOT));
    assert!(report.project_files.is_empty());
}

const PARENT: &str =
    "nika: p\ntasks:\n  call:\n    invoke:\n      workflow: \"./workflows/brief.nika\"\n";

fn child(path: &str) -> String {
    format!(
        "nika: c\npermits:\n  fs: {{ read: [\"../prompts/**\", \"../../prompts/**\"] }}\ntasks:\n  brief:\n    infer:\n      prompt: {{ file: \"{path}\" }}\n"
    )
}

#[test]
fn a_child_composes_its_files_from_its_own_folder() {
    for (path, code) in [
        ("../prompts/brief.md", None),
        ("../../prompts/brief.md", Some("NIKA-FILE-001")),
    ] {
        let child = child(path);
        let mut opened = Vec::new();
        let files = [("prompts/brief.md", &b"hi"[..])];
        let mut read_file = disk(&files, &mut opened);
        let mut read = |id: &str| {
            if id == "proj/workflows/brief.nika" {
                Ok(child.clone())
            } else {
                Err(format!("no unit `{id}`"))
            }
        };
        let (_, report) = check_project(&wf(PARENT), "proj/parent.nika", &mut read, &mut read_file);
        let found: Vec<&str> = (report.project_files.iter())
            .filter_map(|f| f.code.as_deref())
            .collect();
        match code {
            None => {
                assert!(found.is_empty(), "{:?}", report.project_files);
                drop(read_file);
                assert_eq!(opened, ["prompts/brief.md"]);
            }
            Some(code) => {
                assert_eq!(found, [code]);
                assert_eq!(
                    report.project_files[0].task,
                    "proj/workflows/brief.nika · brief"
                );
            }
        }
    }
}

const CALLER: &str =
    "nika: p\ntasks:\n  call:\n    invoke:\n      workflow: \"./workflows/extract.nika\"\n";

/// The parent's check over the child `proj/workflows/extract.nika` holding `child`, the project
/// files on `files`.
fn parent_check(child: &str, files: &[(&str, &[u8])]) -> CheckReport {
    let mut opened = Vec::new();
    let mut read_file = disk(files, &mut opened);
    let mut read = |id: &str| {
        if id == "proj/workflows/extract.nika" {
            Ok(child.to_owned())
        } else {
            Err(format!("no unit `{id}`"))
        }
    };
    check_project(&wf(CALLER), "proj/parent.nika", &mut read, &mut read_file).1
}

/// The codes a workflow's own check raises: the analyzer tier and the check-only lanes.
fn own_codes(yaml: &str) -> Vec<SpecCode> {
    let wf = wf(yaml);
    let analyzed = crate::analyzer::analyze(&wf).err().unwrap_or_default();
    let mut codes: Vec<SpecCode> = (analyzed.iter())
        .map(nika_schema::error::SchemaError::spec_code)
        .collect();
    codes.extend(crate::check(&wf).extra_conformance_codes());
    codes
}

const SCHEMA_CHILD: &str = "nika: c\npermits:\n  fs: { read: [\"../schemas/**\"] }\ntasks:\n  extract:\n    infer:\n      prompt: \"Find the order\"\n      schema: { file: \"../schemas/order.json\" }\n";

#[test]
fn a_child_schema_file_meets_the_inline_schema_laws_at_the_parent() {
    let parse_019 = SpecCode::new("PARSE", 19, SpecCategory::ValidationError);
    let report = parent_check(
        SCHEMA_CHILD,
        &[("schemas/order.json", &b"{\"type\": 5}"[..])],
    );
    let finding = (report.project_files.iter())
        .find(|f| f.code.as_deref() == Some("NIKA-PARSE-019"))
        .unwrap_or_else(|| panic!("{:?}", report.project_files));
    assert_eq!(finding.task, "proj/workflows/extract.nika · extract");
    assert_eq!(finding.field, "schema");
    assert_eq!(finding.path, "../schemas/order.json");
    assert!(!report.is_clean());
    assert!(report.extra_conformance_codes().contains(&parse_019));
    let inline = SCHEMA_CHILD.replace("{ file: \"../schemas/order.json\" }", "{ type: 5 }");
    assert!(
        own_codes(&inline).contains(&parse_019),
        "the inline form raises it too"
    );
    let valid = parent_check(
        SCHEMA_CHILD,
        &[("schemas/order.json", &b"{\"type\": \"object\"}"[..])],
    );
    assert!(valid.project_files.is_empty(), "{:?}", valid.project_files);
}

const PROMPT_CHILD: &str = "nika: c\npermits:\n  fs: { read: [\"../prompts/**\"] }\ntasks:\n  extract:\n    infer:\n      prompt: { file: \"../prompts/brief.md\" }\n";

#[test]
fn a_child_prompt_file_meets_the_inline_templating_laws_at_the_parent() {
    let var_001 = SpecCode::new("VAR", 1, SpecCategory::VariableError);
    let text = &b"Summarize ${{ with.missing }}"[..];
    let report = parent_check(PROMPT_CHILD, &[("prompts/brief.md", text)]);
    let finding = (report.project_files.iter())
        .find(|f| f.code.as_deref() == Some("NIKA-VAR-001"))
        .unwrap_or_else(|| panic!("{:?}", report.project_files));
    assert_eq!(finding.task, "proj/workflows/extract.nika · extract");
    assert_eq!(finding.field, "prompt");
    assert_eq!(finding.path, "../prompts/brief.md");
    assert!(report.extra_conformance_codes().contains(&var_001));
    let inline = PROMPT_CHILD.replace(
        "{ file: \"../prompts/brief.md\" }",
        "\"Summarize ${{ with.missing }}\"",
    );
    assert!(
        own_codes(&inline).contains(&var_001),
        "the inline form raises it too"
    );
    let plain = parent_check(PROMPT_CHILD, &[("prompts/brief.md", &b"Summarize"[..])]);
    assert!(plain.project_files.is_empty(), "{:?}", plain.project_files);
}

#[test]
fn a_child_file_outside_its_own_boundary_is_never_opened_here() {
    let ungranted = PROMPT_CHILD.replace("../prompts/**", "../notes/**");
    let mut opened = Vec::new();
    let files = [("prompts/brief.md", &b"hi"[..])];
    let mut read_file = disk(&files, &mut opened);
    let mut read = |id: &str| {
        if id == "proj/workflows/extract.nika" {
            Ok(ungranted.clone())
        } else {
            Err(format!("no unit `{id}`"))
        }
    };
    let (_, report) = check_project(&wf(CALLER), "proj/parent.nika", &mut read, &mut read_file);
    assert!(
        report.project_files.is_empty(),
        "{:?}",
        report.project_files
    );
    drop(read_file);
    assert!(opened.is_empty(), "{opened:?}");
}

#[test]
fn a_child_schema_file_without_a_schema_is_type_007_at_the_parent() {
    let report = parent_check(
        SCHEMA_CHILD,
        &[("schemas/order.json", &b"[\"type\", \"object\"]"[..])],
    );
    let codes: Vec<&str> = (report.project_files.iter())
        .filter_map(|f| f.code.as_deref())
        .collect();
    assert_eq!(codes, ["NIKA-TYPE-007"], "{:?}", report.project_files);
    assert_eq!(
        report.project_files[0].task,
        "proj/workflows/extract.nika · extract"
    );
    let type_007 = SpecCode::new("TYPE", 7, SpecCategory::ValidationError);
    assert!(report.extra_conformance_codes().contains(&type_007));
}

/// A child whose task `a` loads a closed output schema declaring only `known`, and whose task `b`
/// loads a prompt; `b_with` is task `b`'s own `with:` block.
fn two_task_child(b_with: &str) -> String {
    format!(
        "nika: c\npermits:\n  fs: {{ read: [\"../schemas/**\", \"../prompts/**\"] }}\ntasks:\n  a:\n    infer:\n      prompt: \"Extract\"\n      schema: {{ file: \"../schemas/a.json\" }}\n  b:\n{b_with}    infer:\n      prompt: {{ file: \"../prompts/b.md\" }}\n"
    )
}

const CLOSED_SCHEMA: &[u8] = b"{\"type\": \"object\", \"properties\": {\"known\": {\"type\": \"string\"}}, \"required\": [\"known\"], \"additionalProperties\": false}";

fn two_task_check(b_with: &str, prompt: &[u8]) -> CheckReport {
    let files = [("schemas/a.json", CLOSED_SCHEMA), ("prompts/b.md", prompt)];
    parent_check(&two_task_child(b_with), &files)
}

#[test]
fn a_child_reference_through_with_is_judged_against_a_schema_file() {
    let var_003 = SpecCode::new("VAR", 3, SpecCategory::VariableError);
    let hoisted = "    with:\n      k: ${{ tasks.a.output.unknown_key }}\n";
    let report = two_task_check(hoisted, b"Use ${{ with.k }}");
    let finding = (report.project_files.iter())
        .find(|f| f.code.as_deref() == Some("NIKA-VAR-003"))
        .unwrap_or_else(|| panic!("{:?}", report.project_files));
    assert_eq!(finding.task, "proj/workflows/extract.nika · a");
    assert_eq!(
        (finding.field, finding.path.as_str()),
        ("schema", "../schemas/a.json")
    );
    assert!(
        !finding.detail.contains("../prompts/b.md"),
        "{}",
        finding.detail
    );
    assert!(report.extra_conformance_codes().contains(&var_003));
    let control = two_task_check(
        "    with:\n      k: ${{ tasks.a.output.known }}\n",
        b"Use ${{ with.k }}",
    );
    assert!(
        control.project_files.is_empty(),
        "{:?}",
        control.project_files
    );
}
