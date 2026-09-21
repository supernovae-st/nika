// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use crate::output::{VerbOutput, exit};
use nika_onboard::compile::{
    COMPILE_WIRE_VERSION, CompileOutcome, CompileStatus, outcome_document,
};
use serde_json::json;
use std::fmt::Write as _;

pub(super) fn listing(json_output: bool) -> VerbOutput {
    let mut names = nika_pack::template_names();
    names.push("hello".to_owned());
    names.sort();
    names.dedup();
    VerbOutput::ok(if json_output {
        json!({"compile_version":COMPILE_WIRE_VERSION,"skeletons":names}).to_string()
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
            json!({"compile_version":COMPILE_WIRE_VERSION,"error":{"code":code,"message":message}})
                .to_string()
        } else {
            message.to_owned()
        },
    }
}

pub(super) fn outcome(
    out: &CompileOutcome,
    written: Option<&str>,
    note: Option<&super::sidecar::Note>,
    json_output: bool,
) -> VerbOutput {
    use super::sidecar::Note;
    let status = out.status.word();
    // FILE is the existing authoring/validation finding class (2); trace's
    // INCOMPLETE (5) judges an unfinished journal, not authoring questions.
    let code = if out.status == CompileStatus::Ready {
        exit::OK
    } else {
        exit::FILE
    };
    let text = if json_output {
        // The core owns the machine document every transport prints. Only this
        // adapter materializes files, so `written` is the one fact it adds.
        let mut document = outcome_document(out);
        if let Some(object) = document.as_object_mut() {
            object.insert("written".to_owned(), json!(written));
            // A recorded or replayed plan is already the core's fact (`provenance.plan`,
            // `decision.route`); only a failure to record is this adapter's own.
            if let Some(Note::Failed { path, error }) = note {
                object.insert(
                    "plan_record_error".to_owned(),
                    json!({"path": path.display().to_string(), "message": error}),
                );
            }
        }
        document.to_string()
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
            let _ = writeln!(text, "{} · {} · {}", d.kind.word(), d.target, d.message);
        }
        match note {
            Some(Note::Recorded(path)) => {
                let _ = writeln!(
                    text,
                    "recorded plan · {} · an --answer round replays it with zero provider calls (--fresh re-reads)",
                    path.display()
                );
            }
            Some(Note::Replayed(path)) => {
                let _ = writeln!(
                    text,
                    "replayed plan · {} · zero provider calls (--fresh re-reads)",
                    path.display()
                );
            }
            Some(Note::Failed { path, error }) => {
                let _ = writeln!(text, "plan not recorded · {} · {error}", path.display());
            }
            None => {}
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
