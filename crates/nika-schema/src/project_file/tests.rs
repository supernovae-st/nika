// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::SchemaError;
use crate::parser::{ParseMode, parse};
use crate::source::FileId;

fn workflow(yaml: &str) -> RawWorkflow {
    parse(yaml, FileId::new(0), ParseMode::Strict).expect("the workflow parses")
}

fn refused(yaml: &str) -> SchemaError {
    parse(yaml, FileId::new(0), ParseMode::Strict).expect_err("the workflow is refused")
}

/// An in-memory project: files by project-relative path, the paths behind a link, every read.
#[derive(Default)]
struct Disk {
    files: Vec<(&'static str, Vec<u8>)>,
    linked: Vec<&'static str>,
    reads: Vec<String>,
}

impl Disk {
    fn with(mut self, path: &'static str, bytes: &[u8]) -> Self {
        self.files.push((path, bytes.to_vec()));
        self
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, ProjectFileRead> {
        self.reads.push(path.to_owned());
        if self.linked.contains(&path) {
            return Err(ProjectFileRead::Linked);
        }
        (self.files.iter())
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| bytes.clone())
            .ok_or(ProjectFileRead::Missing)
    }
}

fn compose(yaml: &str, naming: &str, disk: &mut Disk) -> ComposedProjectFiles {
    compose_project_files(&workflow(yaml), naming, &mut |path| disk.read(path))
}

fn codes(composed: &ComposedProjectFiles) -> Vec<&'static str> {
    composed.findings.iter().map(|f| f.code).collect()
}

fn infer(composed: &ComposedProjectFiles) -> &crate::raw::RawInferAction {
    let RawAction::Infer(action) = &composed.workflow.tasks[0].value.action else {
        panic!("the first task infers");
    };
    action
}

const GRANTED: &str = "nika: t\npermits:\n  fs: { read: [\"prompts/**\", \"schemas/**\"] }\ntasks:\n  brief:\n    infer:\n";

fn granted(fields: &str) -> String {
    format!("{GRANTED}{fields}")
}

#[test]
fn the_file_form_parses_and_keeps_its_path_unread() {
    let wf = workflow(&granted("      prompt: { file: \"prompts/brief.md\" }\n"));
    let refs = project_file_refs(&wf);
    assert_eq!(refs.len(), 1);
    let (task, field, reference) = refs[0];
    assert_eq!((task, field), ("brief", "prompt"));
    assert_eq!(reference.path.value, "prompts/brief.md");
    assert!(!reference.composed, "the parser reads no file");
}

#[test]
fn the_file_form_is_closed_for_infer_and_agent() {
    for yaml in [
        granted("      prompt: { file: \"prompts/brief.md\", text: \"x\" }\n"),
        granted("      prompt: hi\n      system: { file: \"prompts/editor.md\", x: 1 }\n"),
        granted("      prompt: hi\n      schema: { file: \"schemas/order.json\", type: object }\n"),
        "nika: t\ntasks:\n  go:\n    agent:\n      prompt: { file: \"prompts/a.md\", model: x }\n"
            .to_owned(),
    ] {
        let err = refused(&yaml);
        assert!(matches!(err, SchemaError::UnknownField { .. }), "{err:?}");
        assert_eq!(err.spec_code().to_string(), "NIKA-PARSE-005", "{yaml}");
    }
}

#[test]
fn a_schema_mapping_without_file_stays_an_inline_schema() {
    let wf = workflow(&granted(
        "      prompt: hi\n      schema: { type: object }\n",
    ));
    let RawAction::Infer(action) = &wf.tasks[0].value.action else {
        panic!("infer");
    };
    assert!(action.schema.is_some() && action.schema_file.is_none());
}

#[test]
fn the_lexical_law_refuses_at_parse_with_file_001() {
    for path in [
        "\"\"",
        "\"prompts/${{ inputs.name }}.md\"",
        "\"prompts/*.md\"",
        "\"/etc/brief.md\"",
        "'prompts\\brief.md'",
        "\"https://example.com/brief.md\"",
        "\"c:/brief.md\"",
        "\"prompts//brief.md\"",
        "\"prompts/.env\"",
        "\"./././brief.md\"",
    ] {
        let err = refused(&granted(&format!("      prompt: {{ file: {path} }}\n")));
        assert!(
            matches!(err, SchemaError::ProjectFilePath { .. }),
            "{path}: {err:?}"
        );
        assert_eq!(err.spec_code().to_string(), "NIKA-FILE-001", "{path}");
    }
    for path in [
        "./prompts/brief.md",
        "prompts/brief.v2.md",
        "../prompts/brief.md",
    ] {
        let wf = workflow(&granted(&format!("      prompt: {{ file: \"{path}\" }}\n")));
        assert_eq!(project_file_refs(&wf).len(), 1, "{path}");
    }
}

#[test]
fn containment_follows_the_naming_folder_inside_the_root() {
    assert_eq!(
        contained(".", "prompts/drafts/../brief.md").as_deref(),
        Some("prompts/brief.md")
    );
    assert_eq!(
        contained("workflows", "../prompts/brief.md").as_deref(),
        Some("prompts/brief.md")
    );
    assert_eq!(contained(".", "../brief.md"), None);
    assert_eq!(contained("workflows", "../../prompts/brief.md"), None);
}

#[test]
fn a_prompt_file_composes_into_the_field() {
    let mut disk = Disk::default().with("prompts/brief.md", b"Summarize ${{ inputs.topic }}");
    let composed = compose(
        &granted("      prompt: { file: \"prompts/brief.md\" }\n"),
        ".",
        &mut disk,
    );
    assert!(composed.findings.is_empty(), "{:?}", composed.findings);
    let action = infer(&composed);
    assert_eq!(action.prompt.value, "Summarize ${{ inputs.topic }}");
    assert!(action.prompt_file.as_ref().is_some_and(|r| r.composed));
    assert_eq!(composed.files.len(), 1);
    assert_eq!(composed.files[0].resolved, "prompts/brief.md");
    assert_eq!(composed.files[0].bytes, 29);
}

#[test]
fn the_boundary_refuses_before_any_read() {
    let absent =
        "nika: t\ntasks:\n  brief:\n    infer:\n      prompt: { file: \"prompts/brief.md\" }\n";
    let outside = "nika: t\npermits:\n  fs: { read: [\"other/**\"] }\ntasks:\n  brief:\n    infer:\n      prompt: { file: \"prompts/brief.md\" }\n";
    for (yaml, code) in [(absent, "NIKA-AUTH-006"), (outside, "NIKA-SEC-004")] {
        let mut disk = Disk::default().with("prompts/brief.md", b"hi");
        let composed = compose(yaml, ".", &mut disk);
        assert_eq!(codes(&composed), [code]);
        assert!(
            disk.reads.is_empty(),
            "an ungranted path is never opened: {:?}",
            disk.reads
        );
        assert!(
            !infer(&composed)
                .prompt_file
                .as_ref()
                .is_some_and(|r| r.composed)
        );
    }
}

#[test]
fn a_path_past_the_root_is_refused_unread_and_a_child_folder_reaches_the_root() {
    let mut disk = Disk::default().with("prompts/brief.md", b"hi");
    let escape = compose(
        &granted("      prompt: { file: \"../brief.md\" }\n"),
        ".",
        &mut disk,
    );
    assert_eq!(codes(&escape), ["NIKA-FILE-001"]);
    assert!(disk.reads.is_empty());
    let child = "nika: c\npermits:\n  fs: { read: [\"../prompts/**\"] }\ntasks:\n  brief:\n    infer:\n      prompt: { file: \"../prompts/brief.md\" }\n";
    let composed = compose(child, "workflows", &mut disk);
    assert!(composed.findings.is_empty(), "{:?}", composed.findings);
    assert_eq!(disk.reads, ["prompts/brief.md"]);
}

#[test]
fn a_link_a_missing_file_and_the_bytes_laws_refuse() {
    let yaml = granted("      prompt: { file: \"prompts/brief.md\" }\n");
    let mut linked = Disk::default().with("prompts/brief.md", b"hi");
    linked.linked.push("prompts/brief.md");
    assert_eq!(codes(&compose(&yaml, ".", &mut linked)), ["NIKA-FILE-001"]);
    assert_eq!(
        codes(&compose(&yaml, ".", &mut Disk::default())),
        ["NIKA-FILE-002"]
    );
    let mut over = Disk::default().with("prompts/brief.md", &vec![b'a'; PROJECT_FILE_CAP + 1]);
    assert_eq!(codes(&compose(&yaml, ".", &mut over)), ["NIKA-FILE-003"]);
    let mut latin1 = Disk::default().with("prompts/brief.md", &[0x63, 0x61, 0x66, 0xe9]);
    assert_eq!(codes(&compose(&yaml, ".", &mut latin1)), ["NIKA-FILE-003"]);
    let mut at_cap = Disk::default().with("prompts/brief.md", &vec![b'a'; PROJECT_FILE_CAP]);
    let composed = compose(&yaml, ".", &mut at_cap);
    assert!(composed.findings.is_empty());
    assert_eq!(infer(&composed).prompt.value.len(), PROJECT_FILE_CAP);
}

#[test]
fn a_schema_file_meets_the_content_law_before_the_inline_law() {
    let yaml = granted("      prompt: hi\n      schema: { file: \"schemas/order.json\" }\n");
    for (bytes, valid) in [
        (
            &b"{\"type\": \"object\", \"required\": [\"order\"]}"[..],
            true,
        ),
        (&b"type: object\nrequired: [order]\n"[..], true),
        (
            &b"{\"$defs\": {\"n\": {\"type\": \"integer\"}}, \"$ref\": \"#/$defs/n\"}"[..],
            true,
        ),
        (&b"[\"type\", \"object\"]"[..], false),
        (&b"{\"$ref\": \"other.json\"}"[..], false),
        (
            &b"{\"properties\": {\"a\": {\"$ref\": \"https://example.com/a.json\"}}}"[..],
            false,
        ),
        (
            &b"base: &shape { type: object }\nitems: *shape\n"[..],
            false,
        ),
        (&b"type: [object\n"[..], false),
    ] {
        let mut disk = Disk::default().with("schemas/order.json", bytes);
        let composed = compose(&yaml, ".", &mut disk);
        let action = infer(&composed);
        if valid {
            assert!(composed.findings.is_empty(), "{:?}", composed.findings);
            assert!(action.schema.as_ref().is_some_and(|s| s.value.is_object()));
            assert!(action.schema_file.as_ref().is_some_and(|r| r.composed));
        } else {
            assert_eq!(
                codes(&composed),
                ["NIKA-TYPE-007"],
                "{}",
                String::from_utf8_lossy(bytes)
            );
            assert!(
                action.schema.is_none(),
                "a schema that is no JSON Schema never joins the task"
            );
        }
    }
}

#[test]
fn a_path_two_fields_name_is_read_once() {
    let mut disk = Disk::default().with("prompts/brief.md", b"hi");
    let composed = compose(
        &granted(
            "      prompt: { file: \"prompts/brief.md\" }\n      system: { file: \"prompts/brief.md\" }\n",
        ),
        ".",
        &mut disk,
    );
    assert!(composed.findings.is_empty());
    assert_eq!(disk.reads.len(), 1);
    assert_eq!(
        infer(&composed).system.as_ref().map(|s| s.value.as_str()),
        Some("hi")
    );
}

#[test]
fn a_reference_an_earlier_step_read_is_not_read_again() {
    let mut disk = Disk::default()
        .with("prompts/brief.md", b"Summarize")
        .with("schemas/order.json", b"{\"type\": \"object\"}");
    let wf = workflow(&granted(
        "      prompt: { file: \"prompts/brief.md\" }\n      schema: { file: \"schemas/order.json\" }\n",
    ));
    let first = compose_project_files(&wf, ".", &mut |path| {
        if path == "prompts/brief.md" {
            Err(ProjectFileRead::Missing)
        } else {
            disk.read(path)
        }
    });
    assert_eq!(codes(&first), ["NIKA-FILE-002"]);
    assert_eq!(infer(&first).prompt.value, "");
    assert!(infer(&first).schema.is_some());
    let second = compose_project_files(&first.workflow, ".", &mut |path| disk.read(path));
    assert!(second.findings.is_empty(), "{:?}", second.findings);
    assert_eq!(disk.reads, ["schemas/order.json", "prompts/brief.md"]);
    assert_eq!(infer(&second).prompt.value, "Summarize");
}
