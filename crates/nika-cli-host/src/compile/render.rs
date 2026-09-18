// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use crate::output::{VerbOutput, exit};
use nika_onboard::compile::{
    AuthoringCognition, CompileOutcome, CompileStatus, DiagnosticKind, PreviewScope, QuestionType,
};
use serde_json::{Value, json};
use std::fmt::Write as _;

pub(super) fn listing(json_output: bool) -> VerbOutput {
    let mut names = nika_pack::template_names();
    names.push("hello".to_owned());
    names.sort();
    names.dedup();
    VerbOutput::ok(if json_output {
        json!({"compile_version":1,"skeletons":names}).to_string()
    } else {
        format!(
            "exact skeletons · {}\nPreview: nika compile <slug> · write: nika compile <slug> <file>.nika",
            names.join(" · ")
        )
    })
}

pub(super) fn failure(code: &str, message: &str, exit: u8, json_output: bool) -> VerbOutput {
    VerbOutput {
        code: exit,
        text: if json_output {
            json!({"compile_version":1,"error":{"code":code,"message":message}}).to_string()
        } else {
            message.to_owned()
        },
    }
}

pub(super) fn outcome(
    out: &CompileOutcome,
    written: Option<&str>,
    json_output: bool,
) -> VerbOutput {
    let status = match out.status {
        CompileStatus::Ready => "ready",
        CompileStatus::Refused => "refused",
        _ => "incomplete",
    };
    // FILE is the existing authoring/validation finding class (2); trace's
    // INCOMPLETE (5) judges an unfinished journal, not authoring questions.
    let code = if out.status == CompileStatus::Ready {
        exit::OK
    } else {
        exit::FILE
    };
    let text = if json_output {
        let questions: Vec<Value> = out.questions.iter().map(|q| json!({
            "key":q.key,"label":q.label,"type":match q.answer_type { QuestionType::Text => "text", QuestionType::Literal => "literal", _ => "unknown" },
            "why":q.why,"mandatory":q.mandatory })).collect();
        let diagnostics: Vec<Value> = out
            .diagnostics
            .iter()
            .map(|d| json!({"kind":kind(d.kind),"target":d.target,"message":d.message}))
            .collect();
        let preview = out.check_preview.as_ref().map(|p| json!({"scope":match p.scope { PreviewScope::SourceOnly => "sourceOnly", _ => "unknown" },"report":p.report}));
        json!({"compile_version":1,"status":status,"candidate":out.candidate,"questions":questions,
            "diagnostics":diagnostics,"requested_boundary":out.requested_boundary,"check_preview":preview,
            "provenance":{"compiler_version":out.provenance.compiler_version,"spec_pin":out.provenance.spec_pin,
                "skeleton":out.provenance.skeleton,"cognition":match out.provenance.cognition { AuthoringCognition::DeterministicOnly => "deterministicOnly", _ => "unknown" }},"written":written}).to_string()
    } else {
        let preview = if out.check_preview.is_some() {
            "source-only Check preview"
        } else {
            "no Check preview available"
        };
        let mut text = format!("Compile {status} · {preview}; Run checks its environment again.\n");
        for q in &out.questions {
            let _ = writeln!(
                text,
                "? {} · {}\n  {} · --answer '{}=JSON_LITERAL'",
                q.key, q.label, q.why, q.key
            );
        }
        for d in &out.diagnostics {
            let _ = writeln!(text, "{} · {} · {}", kind(d.kind), d.target, d.message);
        }
        if let Some(dest) = written {
            let run_path = if dest.starts_with('-') {
                format!("./{dest}")
            } else {
                dest.to_owned()
            };
            let _ = writeln!(
                text,
                "wrote {dest}\nnext · nika run {}",
                crate::output::sh_word(&run_path)
            );
        } else if let Some(candidate) = &out.candidate {
            let _ = writeln!(text, "\n{candidate}");
        } else {
            text.push_str("Choose an exact skeleton with nika compile --list; no substitute workflow was selected.\n");
        }
        text
    };
    VerbOutput { text, code }
}

fn kind(kind: DiagnosticKind) -> &'static str {
    match kind {
        DiagnosticKind::Applied => "applied",
        DiagnosticKind::Missed => "missed",
        DiagnosticKind::RequiresHuman => "requiresHuman",
        DiagnosticKind::Refused => "refused",
        _ => "unknown",
    }
}
