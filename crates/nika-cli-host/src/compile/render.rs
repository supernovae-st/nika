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

/// Where the authoring authority stopped the compile, when it refused a request: the stop is
/// stated, never a silent reduction (the receipt holds the whole account).
fn authority_stop(out: &CompileOutcome) -> Option<String> {
    let account = &out.provenance.authoring.as_ref()?.backend.as_ref()?["authority"];
    let invocations = account["invocations"]["refused"].as_u64().unwrap_or(0);
    let requests = account["http_requests"]["refused"].as_u64();
    // Two units, never summed: an invocation refused sent nothing, a request refused was one of
    // an admitted invocation's own; a harness's requests are unknown, never zero.
    (invocations > 0 || requests.is_some_and(|n| n > 0)).then(|| {
        let requests = requests.map_or_else(
            || "HTTP requests unknown".to_owned(),
            |n| format!("{n} HTTP request(s)"),
        );
        format!(
            "authority · {} authoring request(s) authorized · refused before sending: {invocations} invocation(s), {requests} · authorize more with --authoring-max-calls\n",
            account["max_calls"]
        )
    })
}

/// What this invocation did with the intent's records beside the working directory: its plan
/// record, and the verdicts kept that rejected its bytes in earlier rounds.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Notes<'a> {
    pub(super) plan: Option<&'a super::sidecar::Note>,
    pub(super) declined: Option<&'a super::sidecar::Declined>,
}

pub(super) fn outcome(
    out: &CompileOutcome,
    written: Option<&str>,
    existing: Option<&str>,
    notes: Notes<'_>,
    json_output: bool,
) -> VerbOutput {
    use super::sidecar::{Declined, Note};
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
            // The caller-named destination this compile left as it was (R4 A6): never this
            // round's output, never judged a workflow.
            if let Some(path) = existing {
                object.insert("existing_destination".to_owned(), json!(path));
            }
            // A recorded, replayed or removed plan is already the core's fact (`provenance.plan`,
            // `decision.route`, the `verify_held` finding); only a failure to record or to
            // remove the record is this adapter's own.
            if let Some(Note::Failed { path, error } | Note::Unremoved { path, error }) = notes.plan
            {
                object.insert(
                    "plan_record_error".to_owned(),
                    json!({"path": path.display().to_string(), "message": error}),
                );
            }
            // A verdict carried from an earlier round is the core's fact (`carried` on its
            // attempt); only a failure to keep this round's rejections is this adapter's own.
            if let Some(Declined::Failed { path, error }) = notes.declined {
                object.insert(
                    "declined_record_error".to_owned(),
                    json!({"path": path.display().to_string(), "message": error}),
                );
            }
        }
        document.to_string()
    } else {
        human(out, written, existing, notes)
    };
    VerbOutput { text, code }
}

/// The human reading of an outcome: its status, every question and finding, the plan record,
/// then the file written, the candidate or the skeleton advice.
fn human(
    out: &CompileOutcome,
    written: Option<&str>,
    existing: Option<&str>,
    notes: Notes<'_>,
) -> String {
    use super::sidecar::{Declined, Note};
    let status = out.status.word();
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
    if let Some(line) = authority_stop(out) {
        text.push_str(&line);
    }
    match notes.plan {
        Some(Note::Recorded(path)) => {
            let _ = writeln!(
                text,
                "recorded plan · {} · an --answer round replays it with no authoring call; a judge it asks makes its own calls (--fresh re-reads)",
                path.display()
            );
        }
        Some(Note::Replayed(path)) => {
            let _ = writeln!(
                text,
                "replayed plan · {} · no authoring call; a judge this round asks makes its own calls (--fresh re-reads)",
                path.display()
            );
        }
        Some(Note::Failed { path, error }) => {
            let _ = writeln!(text, "plan not recorded · {} · {error}", path.display());
        }
        Some(Note::Removed(path)) => {
            let _ = writeln!(
                text,
                "plan record removed · {} · its judge did not accept the candidate: no later round replays it, the next compile authors again",
                path.display()
            );
        }
        Some(Note::Unremoved { path, error }) => {
            let _ = writeln!(
                text,
                "plan record not removed · {} · {error} · a later --answer round would replay the candidate its judge did not accept: remove the file, or compile with --fresh",
                path.display()
            );
        }
        None => {}
    }
    match notes.declined {
        Some(Declined::Carried(path)) => {
            let _ = writeln!(
                text,
                "declined verdicts · {} · its judge rejected these bytes in an earlier round: it was not asked again (--fresh keeps them)",
                path.display()
            );
        }
        Some(Declined::Failed { path, error }) => {
            let _ = writeln!(
                text,
                "declined verdicts not kept · {} · {error} · a later round could ask a judge again on bytes it rejected",
                path.display()
            );
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
    if let Some(path) = existing {
        let _ = writeln!(
            text,
            "existing destination remains at {path}; this compile did not write or remove it"
        );
    }
    text
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::super::sidecar::Note;
    use nika_onboard::compile::{CompileRequest, compile};
    use serde_json::Value;
    use std::path::PathBuf;

    /// The lines of `text` that name the plan record.
    fn record_lines(text: &str) -> Vec<&str> {
        text.lines()
            .filter(|line| line.starts_with("plan "))
            .collect()
    }

    /// A record removed after its candidate was held is named in words, and the machine document
    /// adds nothing (the `verify_held` finding is the core's); a record that could not be removed
    /// is named in both, the error with it.
    #[test]
    fn a_removed_or_unremovable_record_is_named() {
        let intent = "Read ./a.md and do something clever with it, then write ./b.md";
        let out = compile(&CompileRequest::create(intent)).expect("compiles");
        let path = PathBuf::from(".nika/compile/abc.plan.json");
        let removed = Note::Removed(path.clone());
        let plan = |note| super::Notes {
            plan: Some(note),
            declined: None,
        };
        let human = super::outcome(&out, None, None, plan(&removed), false);
        assert_eq!(
            record_lines(&human.text),
            [
                "plan record removed · .nika/compile/abc.plan.json · its judge did not accept the candidate: no later round replays it, the next compile authors again"
            ]
        );
        let machine = super::outcome(&out, None, None, plan(&removed), true);
        let doc: Value = serde_json::from_str(&machine.text).expect("a document");
        assert_eq!(doc.get("plan_record_error"), None);
        let error = "Operation not permitted (os error 1)".to_owned();
        let unremoved = Note::Unremoved { path, error };
        let human = super::outcome(&out, None, None, plan(&unremoved), false);
        assert_eq!(
            record_lines(&human.text),
            [
                "plan record not removed · .nika/compile/abc.plan.json · Operation not permitted (os error 1) · a later --answer round would replay the candidate its judge did not accept: remove the file, or compile with --fresh"
            ]
        );
        let machine = super::outcome(&out, None, None, plan(&unremoved), true);
        let doc: Value = serde_json::from_str(&machine.text).expect("a document");
        assert_eq!(
            doc["plan_record_error"],
            serde_json::json!({"path": ".nika/compile/abc.plan.json",
                "message": "Operation not permitted (os error 1)"})
        );
        assert_eq!((human.code, machine.code), (2, 2));
    }

    /// The lines of `text` that name the kept rejections.
    fn declined_lines(text: &str) -> Vec<&str> {
        text.lines()
            .filter(|line| line.starts_with("declined verdicts"))
            .collect()
    }

    /// A verdict kept from an earlier round that decided this round's bytes is named in words,
    /// and the machine document adds nothing (the attempt's `carried` is the core's); rejections
    /// that could not be kept are named in both, the error with them.
    #[test]
    fn a_carried_rejection_or_one_that_could_not_be_kept_is_named() {
        use super::super::sidecar::Declined;
        let intent = "Read ./a.md and do something clever with it, then write ./b.md";
        let out = compile(&CompileRequest::create(intent)).expect("compiles");
        let path = PathBuf::from(".nika/compile/abc.declined.json");
        let notes = |declined| super::Notes {
            plan: None,
            declined: Some(declined),
        };
        let carried = Declined::Carried(path.clone());
        let human = super::outcome(&out, None, None, notes(&carried), false);
        assert_eq!(
            declined_lines(&human.text),
            [
                "declined verdicts · .nika/compile/abc.declined.json · its judge rejected these bytes in an earlier round: it was not asked again (--fresh keeps them)"
            ]
        );
        let machine = super::outcome(&out, None, None, notes(&carried), true);
        let doc: Value = serde_json::from_str(&machine.text).expect("a document");
        assert_eq!(doc.get("declined_record_error"), None);
        let error = "Permission denied (os error 13)".to_owned();
        let failed = Declined::Failed { path, error };
        let human = super::outcome(&out, None, None, notes(&failed), false);
        assert_eq!(
            declined_lines(&human.text),
            [
                "declined verdicts not kept · .nika/compile/abc.declined.json · Permission denied (os error 13) · a later round could ask a judge again on bytes it rejected"
            ]
        );
        let machine = super::outcome(&out, None, None, notes(&failed), true);
        let doc: Value = serde_json::from_str(&machine.text).expect("a document");
        assert_eq!(
            doc["declined_record_error"],
            serde_json::json!({"path": ".nika/compile/abc.declined.json",
                "message": "Permission denied (os error 13)"})
        );
        // Neither note: no line, no field.
        let quiet = super::outcome(&out, None, None, super::Notes::default(), false);
        assert_eq!(declined_lines(&quiet.text), Vec::<&str>::new());
        assert_eq!((human.code, machine.code, quiet.code), (2, 2, 2));
    }
}
