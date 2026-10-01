// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The source read, and how an exact copy lowers it. One file is one read; several files or a
//! glob are a bounded fan-out. One plan that copies one text file to another states two
//! programs: its text read and written back (the lowering of every plan), or its bytes read as
//! an opaque envelope (`binary: true`) that the write decodes. The choice is closed and typed
//! ([`CopyLowering`]). The byte lowering holds only that exact copy, judged on the plan before
//! anything is assembled and on the emitted document before the laws; anywhere else it emits
//! no candidate. Each lowering then crosses its own ledger, fidelity laws, literal round trip
//! and Check, so each candidate is its own bytes, never the other's rewritten. Beside
//! `assemble.rs` at the 1,500-line file cap.

use std::collections::BTreeSet;

use nika_compile_fidelity::behavior::Format;
use serde_json::json;

use super::{
    Doc, Kind, emit_fan_out, emit_parse, emit_parse_lines, emit_source_columns, writes_csv,
};
use crate::bindings::{Bindings, Source};
use crate::ledger::Judgment;
use crate::paths::Structured;
use crate::plan::{EffectPolicy, EffectVerb, Op, Plan};
use crate::{CompileError, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind};

/// How the assembler lowers the source read of an exact copy of one text file to another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CopyLowering {
    /// The source read as text and written back from that text: the lowering of every plan.
    #[default]
    Text,
    /// The source read as opaque bytes (`binary: true`) and written back from that byte
    /// envelope, which the write decodes: only for an exact copy of one text file to another.
    Bytes,
}

/// The assembly under one lowering of the source read. [`CopyLowering::Text`] is the ordinary
/// assembly under the judgments made in this compile (`assemble_judged`). [`CopyLowering::Bytes`]
/// holds only an exact copy of one text file to another: one read, its one automatic write and
/// nothing else, emitted as exactly those two tasks; any other plan emits no candidate, with a
/// `lowering` refusal. Either way the candidate crosses its own ledger, laws and Check.
///
/// # Errors
/// Returns the same machinery failures as the ordinary assembly.
pub fn assemble_lowered(
    plan: &Plan,
    intent: &str,
    request: &CompileRequest,
    judgments: &[Judgment],
    whole: bool,
    lowering: CopyLowering,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    if lowering == CopyLowering::Bytes && !copy_plan(plan) {
        refuse(
            out,
            "the plan is not one read and its one automatic write with nothing else",
        );
        return Ok(());
    }
    super::assembled(plan, intent, request, judgments, whole, lowering, out)
}

/// Whether `plan` is exactly a copy: one read, its one automatic write with no policy words and
/// no value alone, path bindings, and no rule, unknown, constraint, obligation, slot or trigger.
pub(super) fn copy_plan(plan: &Plan) -> bool {
    let ([read], [write]) = (plan.steps.as_slice(), plan.effects.as_slice()) else {
        return false;
    };
    read.op == Op::Read
        && write.verb == EffectVerb::Write
        && write.policy == EffectPolicy::Automatic
        && write.policy_literal.is_none()
        && !write.alone
        && plan.rules.is_empty()
        && plan.unknowns.is_empty()
        && plan.constraints.is_empty()
        && plan.obligations.is_empty()
        && plan.slots.is_empty()
        && plan.trigger.is_none()
        && plan.bindings.iter().all(|binding| binding.role == "path")
}

/// Whether the document is exactly the copy the byte lowering holds: the binary read of one
/// text file and the one write of another fed that whole read, with their two paths, their two
/// tools and nothing else. The suffix bounds the shape the text judgment reads; it proves no
/// encoding.
pub(super) fn copy_document(d: &Doc) -> bool {
    let root = &d.root;
    let path = |key: &str| root["const"][key].as_str();
    let text =
        |key: &str| path(key).is_some_and(|file| Format::of_path(file) == Some(Format::Text));
    let file = |key: &str| path(key).map(|file| file.trim_start_matches("./"));
    let keys: BTreeSet<&str> = root
        .as_object()
        .map(|map| map.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let read = json!({"invoke": {
        "tool": "nika:read",
        "args": {"path": "${{ const.source_path }}", "binary": true},
    }});
    let write = json!({
        "after": {"read_source": "success"},
        "with": {"content": "${{ tasks.read_source.output }}"},
        "invoke": {"tool": "nika:write", "args": {
            "path": "${{ const.output_path }}",
            "content": "${{ with.content }}",
            "create_dirs": true,
            "overwrite": true,
        }},
    });
    keys == BTreeSet::from(["const", "inputs", "nika", "outputs", "permits", "tasks"])
        && root["inputs"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
        && root["const"].as_object().is_some_and(|map| map.len() == 2)
        && root["tasks"].as_object().is_some_and(|map| map.len() == 2)
        && root["tasks"]["read_source"] == read
        && root["tasks"]["write_output"] == write
        && root["outputs"] == json!({"write_status": "${{ tasks.write_output.status }}"})
        && d.tools == BTreeSet::from(["nika:read", "nika:write"])
        && d.reads == [root["const"]["source_path"].clone()]
        && d.writes == [root["const"]["output_path"].clone()]
        && d.hosts.is_empty()
        && text("source_path")
        && text("output_path")
        && file("source_path") != file("output_path")
}

/// Whether the document emitted under `lowering` may go on to the laws: the text lowering always
/// may; the byte lowering only as the exact copy it holds, else it is refused.
pub(super) fn lowered(d: &Doc, lowering: CopyLowering, out: &mut CompileOutcome) -> bool {
    if lowering == CopyLowering::Text || copy_document(d) {
        return true;
    }
    refuse(
        out,
        "the emitted workflow is not the binary read of one text file and its one write of \
         another, fed that whole read",
    );
    false
}

/// The byte lowering refused, with the reason: no candidate is emitted.
fn refuse(out: &mut CompileOutcome, why: &str) {
    crate::finding(
        out,
        DiagnosticKind::Refused,
        "lowering",
        format!(
            "The byte lowering holds only an exact copy of one text file to another: {why}; no \
             byte candidate is emitted."
        ),
    );
    out.status = CompileStatus::Refused;
}

/// One file is one read (parsed too when structured), its bytes read as an opaque envelope under
/// the byte lowering; several files or a glob are a bounded fan-out whose batch is folded into
/// one document with a heading per file, or, when the request distributes its draft, zipped into
/// `{path, text}` items.
pub(super) fn emit_read(d: &mut Doc, plan: &Plan, b: &Bindings, lowering: CopyLowering) {
    match b.read.bound() {
        Some(Source::File(path)) => {
            d.root["const"]["source_path"] = json!(path);
            d.reads.push(json!(path));
            let mut args = json!({"path": "${{ const.source_path }}"});
            if lowering == CopyLowering::Bytes {
                args["binary"] = json!(true);
            }
            d.tool("read_source", "nika:read", args, None, false);
            d.fact("document", "${{ tasks.read_source.output }}", Kind::Corpus);
            if let Some(format) = Structured::of(path)
                && b.parses()
            {
                if format == Structured::Csv && writes_csv(b) {
                    emit_source_columns(d);
                }
                emit_parse(d, format, b.guard_scope().as_deref());
            } else if b.rule_over_lines() {
                emit_parse_lines(d);
            }
        }
        Some(source @ (Source::Files(_) | Source::Glob(_))) => emit_fan_out(d, plan, b, source),
        Some(Source::Item) | None => {}
    }
}
