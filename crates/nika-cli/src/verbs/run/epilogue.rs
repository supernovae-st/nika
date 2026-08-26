// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's LAST WORDS — the human final frame (flow epilogue · resume
//! summary) + the machine envelopes (`{"error":…}` · `{"paused":…}` ·
//! the `outputs:` line) — one seam, so display features land here,
//! never in the driver (`mod.rs` composes and drives; this module
//! speaks the ending).

// The epilogue IS a printing surface (the same sanctioned exemption the
// parent module carries): the render is the run's final frame, it
// cannot be deferred to a `VerbOutput`.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;

use serde_json::Value;

use nika_runtime::{RunOutcome, WorkflowPause};

use crate::Theme;
use crate::verbs::exit;
use nika_dap::resume;

/// The `--resume` post-run summary (`resumed · N skipped · M ran live`) —
/// printed ONLY when a resume was requested (a fresh run's surfaces stay
/// byte-identical). Machine modes route it to stderr (stdout is the
/// contract surface); human modes print it under the final frame.
pub(super) fn print_resume_summary(outcome: &RunOutcome, resumed: bool, to_stderr: bool) {
    if !resumed {
        return;
    }
    let ran_live = outcome
        .records
        .values()
        .filter(|r| r.started_at.is_some())
        .count();
    let line = resume::summary_line(outcome.cache_hits.len(), ran_live);
    if to_stderr {
        eprintln!("{line}");
    } else {
        println!("\n  {line}");
    }
}

/// The TTY final-frame epilogue: the post-run waterfall (real durations ·
/// real overlap · pure fold of the run's own event stream) then the
/// shareable verdict card, its outputs note naming what left the run —
/// closed by the explore hint. SEAM (stated, not faked): a live run
/// writes no trace file today, so the hint teaches the two-step that
/// works NOW (record with `--json`, then browse); when auto-trace
/// recording ships, this collapses to the recorded path.
pub(super) fn print_flow_epilogue(
    view: &crate::RunView,
    outputs: &BTreeMap<String, Value>,
    theme: Theme,
    file: &str,
    trace_recorded: bool,
) {
    for line in crate::display::flow::waterfall(view, &theme) {
        println!("{line}");
    }
    let mut notes = fruit_notes(view, trace_recorded);
    notes.extend(outputs_note(outputs));
    for line in crate::display::flow::verdict_card(view, &theme, &notes) {
        println!("{line}");
    }
    // The workflow path is CLICKABLE on link-capable terminals (OSC-8 ·
    // file:// — the one real file in the hint; the ndjson names are the
    // suggested two-step, not files that exist yet).
    let file_cell = crate::verbs::linked_path(theme, file);
    let record =
        format!("nika run {file_cell} --json > run.ndjson · nika trace outputs run.ndjson");
    println!(
        "  {}",
        crate::display::vocab::hint(theme, "explore", &record)
    );
}

/// The FRUIT block (A-2 · user gauntlet 2026-07-31 · "the run wrote
/// `output.md` — and never said so"): `wrote <path> (<size>)` per file
/// the view's write rows materialized + the model's last word, bounded.
/// Byte sizes are stat'd HERE — the one I/O the display crate refuses —
/// and a path that stat fails (deleted since · sandbox) simply drops
/// its size cell, never the line: the fruit is the run's claim, the
/// size is today's disk.
pub(super) fn fruit_notes(view: &crate::RunView, trace_recorded: bool) -> Vec<String> {
    use crate::display::{fruit, shape};
    let mut notes = Vec::new();
    let files = fruit::written_files(view);
    for f in files.iter().take(3) {
        let size = std::fs::metadata(&f.path)
            .ok()
            .and_then(|m| usize::try_from(m.len()).ok());
        notes.push(match size {
            Some(n) => format!("{} {} ({})", f.verb, f.path, shape::fmt_bytes(n)),
            None => format!("{} {}", f.verb, f.path),
        });
    }
    if files.len() > 3 {
        notes.push(format!("… +{} more files", files.len() - 3));
    }
    // The model's last word — the answer SEEN, not narrated (bounded by
    // the shape law: head only, never a data dump; ADR-099 §1 already
    // guarantees no secret can reach the stream this reads).
    if let Some((_task, text)) = fruit::last_said(view)
        && let Some(quote) = shape::summarize(text, SAID_CELLS)
    {
        notes.push(format!("said {quote}"));
        // The word was CUT — teach the zero-arg door to the whole answer
        // (gauntlet wave 2 · Marta: the card truncated the said and
        // pointed at a raw .ndjson path; `nika trace <path>` is not a
        // command, and `trace outputs` surfaced only after a two-screen
        // help detour). Zero-arg on purpose: it auto-selects the
        // workspace's latest trace — nothing to copy. ONLY where the run
        // actually recorded one: `examples run` stages to a temp file
        // with its journal deliberately off, and the taught door failed
        // exit 3 in the exact context where it was taught (Elliot ·
        // wave 3 — the same law, one recursion deeper).
        let cut = shape::summarize(text, usize::MAX)
            .is_some_and(|full| full.chars().count() > SAID_CELLS);
        if cut && trace_recorded {
            notes.push("see it whole: nika trace outputs".to_owned());
        }
    }
    notes
}

/// Widest the `said` quote grows (display cells) — fits the verdict
/// card's inner width beside its `said ` label.
const SAID_CELLS: usize = 46;

/// The card's outputs note: `outputs → key (type) · key2 (type)` — the
/// export contract's shape at a glance (types only, never a data dump).
/// Two keys shown, the rest counted.
pub(super) fn outputs_note(outputs: &BTreeMap<String, Value>) -> Option<String> {
    if outputs.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = outputs
        .iter()
        .take(2)
        .map(|(key, value)| format!("{key} ({})", json_type_name(value)))
        .collect();
    if outputs.len() > 2 {
        parts.push(format!("+{} more", outputs.len() - 2));
    }
    Some(format!("outputs → {}", parts.join(" · ")))
}

/// The JSON type vocabulary for the outputs pointer — names only, never
/// values (a summary line, not a data leak into the scrollback).
pub(super) fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The export contract's stdout payload (spec 01 §"What leaves a run"): the
/// resolved workflow `outputs:` as ONE JSON object on a single line. An
/// empty map (no `outputs:` declared · or references that no longer
/// resolve) renders `{}` — stdout is ALWAYS a single JSON object in
/// `--output json` mode, a stable machine contract for the composition
/// path (`exec: nika run sub --output json` + `capture: stdout`).
pub(super) fn outputs_json_line(outputs: &BTreeMap<String, Value>) -> String {
    serde_json::to_string(outputs).unwrap_or_else(|_| "{}".to_owned())
}

/// Route a human-readable diagnostic to the spec-correct stream: stderr in
/// `--output json` mode (stdout MUST stay a clean JSON object · the export
/// contract · `capture: stdout` composition), stdout in the human modes.
/// In machine mode the failure ALSO lands on stdout as the `{"error":{…}}`
/// envelope (F6) — the machine surface is self-sufficient, success or not.
pub(super) fn emit_diagnostic(text: &str, output_json: bool) {
    // Terminal-newline law (gauntlet 08-01, Marc): the red pre-run
    // diagnostic ended flush against the next shell prompt and dirtied
    // concatenated CI logs — every diagnostic ends its own line, and a
    // text already carrying one is not doubled.
    let text = text.strip_suffix('\n').unwrap_or(text);
    if output_json {
        eprintln!("{text}");
        println!("{}", error_envelope_line(envelope_message(text)));
    } else {
        println!("{text}");
    }
}

/// Print the machine failure envelope when in `--output json` mode (the
/// ENV-class exits inside `run` share this one seam).
pub(super) fn emit_error_envelope(message: &str, output_json: bool) {
    if output_json {
        println!("{}", error_envelope_line(message));
    }
}

/// The ENV-class refusal surface — the message rides stderr + the machine
/// error envelope (one voice for every pre-run refusal).
pub(super) fn env_refusal(message: &str, output_json: bool) -> u8 {
    eprintln!("nika run: {message}");
    emit_error_envelope(message, output_json);
    exit::ENV
}

/// ONE `{"paused":{…}}` line — the machine pause contract (ADR-099 rider
/// · additive beside the success/error envelopes): the prompt payload a
/// consumer needs to deliver an answer (`--answer <task>=<value>` at
/// resume · or a serve webhook later). The F-P4 approval ticket rides
/// additively (NEP-0013): the machine consumer sees EXACTLY what an
/// answer would sign — shown-hash · digest · nonce · mint · TTL.
/// `resume_carry` (issue 772 · additive) is the run's own `--var`/
/// `--model` tail, shell-quoted verbatim — the taught line's carry, so
/// a machine consumer reconstructing the resume command drops nothing
/// (the flag-less-resume refusal stays the backstop).
pub(super) fn paused_envelope_line(pause: &WorkflowPause, carry: &str) -> String {
    let approval = pause.approval.as_ref().map(|t| {
        serde_json::json!({
            "digest": t.digest(),
            "shown_hash": t.content_hash,
            "run_nonce": t.run_nonce,
            "minted_at_ms": t.minted_at_ms,
            "ttl_seconds": t.ttl_seconds,
        })
    });
    serde_json::json!({
        "paused": {
            "task": pause.task,
            "mode": pause.mode,
            "message": pause.message,
            "choices": pause.choices,
            "approval": approval,
            "resume_carry": carry,
        }
    })
    .to_string()
}

/// The stderr resume teaching a PAUSED machine run prints beside its
/// trace anchor — the pause sibling of the failure lane's `autopsy:`
/// line (stateful gauntlet 2026-07-11: the pause had everything the
/// command needs — file · trace · task · mode — and printed none of it).
/// The taught command carries ONE concrete answer and names the
/// alternatives BESIDE it, never inside it: a `|` in the command is a
/// shell PIPE, and the pasted `--answer ask=true|false` silently bound
/// `true` (a human gate answered by the shell — `human said: true`
/// with no human) while the piped-to `false` closed stdout and leaked
/// a broken-pipe panic. A taught line must be paste-safe by
/// construction — the run's own `--var`/`--model` carry rides verbatim
/// for the same reason (a required-input workflow refuses a var-less
/// resume · seo-live-review · 2026-07-31).
pub(super) fn resume_hint_line(
    file: &str,
    trace: &std::path::Path,
    pause: &WorkflowPause,
    carry: &str,
) -> String {
    let (value, alternatives) = match pause.mode.as_str() {
        "confirm" => ("true".to_owned(), " · or false".to_owned()),
        "choice" if !pause.choices.is_empty() => {
            let rest = &pause.choices[1..];
            let alts = if rest.is_empty() {
                String::new()
            } else {
                format!(" · or {}", rest.join(" · "))
            };
            (pause.choices[0].clone(), alts)
        }
        // `input` takes free text: the quotes make the placeholder
        // paste-safe (a bare <text> would redirect).
        _ => ("\"your answer\"".to_owned(), String::new()),
    };
    format!(
        "resume: nika run {file}{carry} --resume {} --answer {}={value}{alternatives}",
        trace.display(),
        pause.task,
    )
}

/// The shell metacharacters that make a taught command unsafe to paste
/// (`|` pipes · `&` backgrounds · `;` chains · `<`/`>` redirect ·
/// backtick/`$()` substitute). The taught-line tests assert their
/// absence in the COMMAND part — a teaching surface that can be pasted
/// wrong is a teaching surface that will be.
#[cfg(test)]
pub(super) fn unsafe_to_paste(command: &str) -> Option<char> {
    // Everything after a ` · ` is prose beside the command, not part of it.
    let command = command.split(" · ").next().unwrap_or(command);
    let mut in_quotes = false;
    for c in command.chars() {
        if c == '"' || c == '\'' {
            in_quotes = !in_quotes;
        }
        if !in_quotes && matches!(c, '|' | '&' | ';' | '<' | '>' | '`' | '(' | ')') {
            return Some(c);
        }
    }
    None
}

/// The run's re-invocation carry — every `--var` the operator passed +
/// the `--model` override, shell-quoted for a paste-able line. Built
/// once per run, threaded to every taught resume line.
pub(super) fn resume_carry(vars: &[String], model_override: Option<&str>) -> String {
    use std::fmt::Write as _;
    let mut carry = String::new();
    for var in vars {
        // write! to a String is infallible.
        let _ = write!(carry, " --var {}", sh_word(var));
    }
    if let Some(model) = model_override {
        let _ = write!(carry, " --model {}", sh_word(model));
    }
    carry
}

/// Quote one shell word for the taught line: bare when it is already a
/// safe word, single-quoted otherwise (embedded single quotes splice
/// through the POSIX `'\''` idiom — paste-able in sh/bash/zsh).
fn sh_word(word: &str) -> std::borrow::Cow<'_, str> {
    let safe = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_=./:@+-".contains(c));
    if safe {
        return std::borrow::Cow::Borrowed(word);
    }
    std::borrow::Cow::Owned(format!("'{}'", word.replace('\'', "'\\''")))
}

/// ONE `{"error":{"code":…,"message":…}}` line — the machine failure
/// contract (F6). `code` is the first NIKA wire code found in the message
/// (`null` when the failure class carries none, e.g. an unreadable file).
pub(super) fn error_envelope_line(message: &str) -> String {
    serde_json::json!({
        "error": { "code": first_nika_code(message), "message": message }
    })
    .to_string()
}

/// R4 — the witness texts a failure card speaks (workflow detail +
/// row details), the input of the census-derived seat-escape gate
/// (`nika_cli_host::probe::print_seat_escape` — printed only when THIS
/// machine has a signed-in seat).
pub(super) fn failure_witnesses(view: &crate::RunView) -> Vec<&str> {
    view.workflow_detail
        .iter()
        .map(String::as_str)
        .chain(view.rows().iter().map(|row| row.detail.as_str()))
        .collect()
}

/// Best-effort wire-code extraction: the first `NIKA-…` token in a
/// diagnostic (findings render `[NIKA-PARSE-009]` · run details lead with
/// `NIKA-431 · …`). Never invents — no token, no code.
pub(super) fn first_nika_code(text: &str) -> Option<&str> {
    let start = text.find("NIKA-")?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    let code = rest[..end].trim_end_matches('-');
    // A bare `NIKA-` prefix with no digits is prose, not a code.
    (code.len() > "NIKA-".len() && code.bytes().any(|b| b.is_ascii_digit())).then_some(code)
}

/// The one-line message for a findings-render envelope: the first line
/// carrying a wire code (the render wraps it in section noise), else the
/// first non-empty line.
pub(super) fn envelope_message(text: &str) -> &str {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next().unwrap_or(text);
    std::iter::once(first)
        .chain(lines)
        .find(|l| l.contains("NIKA-"))
        .unwrap_or(first)
        .trim()
}

/// The failure envelope for a run that EXECUTED and failed: the first
/// failed task row's detail (it carries the wire code), else the
/// workflow-level detail (run-end typed-output breaches), else a stable
/// fallback — stdout never goes silent on a machine consumer.
pub(super) fn run_failure_envelope(view: &crate::RunView) -> String {
    let failed = view
        .rows()
        .iter()
        .find(|r| r.state == crate::TaskState::Failed);
    let message = match failed {
        Some(row) if row.detail.is_empty() => format!("task `{}` failed", row.id),
        Some(row) => format!("task `{}` failed — {}", row.id, row.detail),
        None => view
            .workflow_detail
            .clone()
            .unwrap_or_else(|| "workflow failed".to_owned()),
    };
    error_envelope_line(&message)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::outputs_json_line;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn said_view(answer_json: &str) -> crate::RunView {
        use nika_types::resource::{KeyValue, Value as RValue};
        let mut view = crate::RunView::new();
        let mut started = nika_cli_display_demo_bare(nika_event::EventKind::TaskStarted, 0);
        started = started
            .with_field(KeyValue::new("task", RValue::String("think".to_owned())))
            .with_field(KeyValue::new(
                "note",
                RValue::String("infer · mock/echo".to_owned()),
            ));
        view.apply(&started);
        let mut done = nika_cli_display_demo_bare(nika_event::EventKind::TaskCompleted, 1);
        done = done
            .with_field(KeyValue::new("task", RValue::String("think".to_owned())))
            .with_field(KeyValue::new(
                "output",
                RValue::String(answer_json.to_owned()),
            ));
        view.apply(&done);
        view
    }

    fn nika_cli_display_demo_bare(kind: nika_event::EventKind, ms: u64) -> nika_event::Event {
        crate::display::demo::bare_event(kind, ms)
    }

    /// Wave 2 · Marta: a CUT `said` quote teaches the zero-arg door to
    /// the whole answer — a short word stays a quiet card (the teach
    /// line would be noise on a five-char answer).
    #[test]
    fn a_cut_said_teaches_the_trace_outputs_door() {
        let long = format!("\"{}\"", "a long answer ".repeat(12));
        let notes = super::fruit_notes(&said_view(&long), true);
        assert!(
            notes
                .iter()
                .any(|n| n == "see it whole: nika trace outputs"),
            "{notes:?}"
        );
        let notes = super::fruit_notes(&said_view("\"short.\""), true);
        assert!(
            notes.iter().all(|n| !n.contains("see it whole")),
            "a short answer keeps the card quiet: {notes:?}"
        );
        // Elliot (wave 3): the door is taught ONLY where a trace was
        // recorded — `examples run` stages to a temp file with its
        // journal deliberately off, and the taught zero-arg door failed
        // exit 3 in the exact context where it was taught.
        let long = format!("\"{}\"", "a long answer ".repeat(12));
        let notes = super::fruit_notes(&said_view(&long), false);
        assert!(
            notes.iter().all(|n| !n.contains("see it whole")),
            "no recorded trace = no taught door: {notes:?}"
        );
    }

    #[test]
    fn outputs_json_line_is_one_sorted_object() {
        let mut m: BTreeMap<String, Value> = BTreeMap::new();
        m.insert("total".to_owned(), json!(60));
        m.insert("count".to_owned(), json!(3));
        // BTreeMap key order → `count` before `total`: a single line,
        // deterministic across runs (the machine consumer can jq it).
        assert_eq!(outputs_json_line(&m), r#"{"count":3,"total":60}"#);
        assert!(!outputs_json_line(&m).contains('\n'));
    }

    #[test]
    fn outputs_json_line_empty_is_braces() {
        // No `outputs:` declared → still a JSON object on stdout.
        assert_eq!(outputs_json_line(&BTreeMap::new()), "{}");
    }

    // ── R4 · the auth-class tail gate ──

    /// Mutation pins for the witness gate: the tail is earned by an
    /// auth-class witness (NIKA-INFER-001 · NIKA-1800) on a failed row
    /// or the workflow detail — never by another class, never on a
    /// healthy view. The view→witnesses fold feeds the host predicate
    /// (`nika_cli_host::probe::auth_class_witness`); inverting either
    /// reddens this.
    #[test]
    fn the_seat_escape_gate_reads_the_auth_class_witness() {
        use nika_types::resource::{KeyValue, Value as RValue};

        let gate = |view: &crate::RunView| {
            super::failure_witnesses(view)
                .into_iter()
                .any(nika_cli_host::probe::auth_class_witness)
        };
        let mut view = crate::RunView::new();
        assert!(!gate(&view), "an empty view earns none");
        let failed = nika_cli_display_demo_bare(nika_event::EventKind::TaskFailed, 1)
            .with_field(KeyValue::new("task", RValue::String("reply".to_owned())))
            .with_field(KeyValue::new(
                "detail",
                RValue::String(
                    "NIKA-INFER-001 · no API key for 'anthropic' · set one of […]".to_owned(),
                ),
            ));
        view.apply(&failed);
        assert!(gate(&view), "the missing-credential witness earns the tail");
        // The admission refusal class rides the same gate.
        let mut view1800 = crate::RunView::new();
        view1800.workflow_detail = Some("NIKA-1800 · no access path survives admission".to_owned());
        assert!(gate(&view1800));
        // Another class earns nothing.
        let mut other = crate::RunView::new();
        other.workflow_detail = Some("NIKA-VAR-001 · unresolved reference".to_owned());
        assert!(!gate(&other));
    }

    // ── F6 · the `--output json` machine failure envelope ────────────

    /// The envelope is ONE JSON object with the `{"error":{code,message}}`
    /// shape · the code is extracted, never invented.
    #[test]
    fn error_envelope_is_one_object_with_extracted_code() {
        let line = super::error_envelope_line("task failed — NIKA-VAR-001 · unresolved reference");
        let v: Value = serde_json::from_str(&line).expect("envelope is JSON");
        assert_eq!(v["error"]["code"], json!("NIKA-VAR-001"));
        assert!(
            v["error"]["message"]
                .as_str()
                .expect("message is a string")
                .contains("unresolved"),
        );
        assert!(!line.contains('\n'), "one line — the machine contract");

        // No wire code in the failure class (unreadable file) → null, not
        // a hallucinated code.
        let env_line = super::error_envelope_line("cannot read wf.yaml: No such file");
        let v: Value = serde_json::from_str(&env_line).expect("envelope is JSON");
        assert!(v["error"]["code"].is_null());
    }

    /// Wire-code extraction: bracketed findings, leading run details,
    /// per-builtin long codes — and NO false positive on bare prose.
    #[test]
    fn first_nika_code_finds_real_codes_only() {
        assert_eq!(
            super::first_nika_code("PARSE ✗  [NIKA-PARSE-009] two verbs"),
            Some("NIKA-PARSE-009")
        );
        assert_eq!(
            super::first_nika_code("NIKA-431 · provider API error"),
            Some("NIKA-431")
        );
        assert_eq!(
            super::first_nika_code("x NIKA-BUILTIN-JQ-001 y"),
            Some("NIKA-BUILTIN-JQ-001")
        );
        assert_eq!(super::first_nika_code("the NIKA- prefix alone"), None);
        assert_eq!(super::first_nika_code("no code here"), None);
    }

    /// A findings render condenses to the line that carries the code.
    #[test]
    fn envelope_message_prefers_the_code_line() {
        let text =
            "nika check · wf.yaml\n X CONFORM  [NIKA-CEL-001] bad when\n  verdict: 1 finding\n";
        assert_eq!(
            super::envelope_message(text),
            "X CONFORM  [NIKA-CEL-001] bad when"
        );
        // No code anywhere → the first non-empty line.
        assert_eq!(
            super::envelope_message("\ncannot read x: gone\ndetail\n"),
            "cannot read x: gone"
        );
    }

    /// The run-failure envelope reads the folded view: the failed row's
    /// detail (which carries the wire code) becomes the machine message.
    #[test]
    fn run_failure_envelope_carries_the_failed_task_detail() {
        let mut view = crate::RunView::new();
        for ev in crate::demo::failure() {
            view.apply(&ev);
        }
        let line = super::run_failure_envelope(&view);
        let v: Value = serde_json::from_str(&line).expect("envelope is JSON");
        assert_eq!(v["error"]["code"], json!("NIKA-431"), "{line}");
        assert!(
            v["error"]["message"]
                .as_str()
                .expect("message present")
                .contains("task `"),
            "{line}"
        );

        // An empty view (nothing folded) still yields a stable envelope.
        let empty = super::run_failure_envelope(&crate::RunView::new());
        let v: Value = serde_json::from_str(&empty).expect("fallback is JSON");
        assert_eq!(v["error"]["message"], json!("workflow failed"));
    }

    #[test]
    fn resume_hint_speaks_each_modes_answer_shape() {
        // The pause sibling of `autopsy:` (stateful gauntlet 2026-07-11):
        // the taught command must be paste-able — file · trace · task ·
        // ONE concrete answer, alternatives named BESIDE the command.
        use nika_runtime::WorkflowPause;
        let trace = std::path::Path::new(".nika/traces/t.ndjson");
        let confirm = WorkflowPause::new("approve".into(), "confirm".into(), None, vec![]);
        assert_eq!(
            super::resume_hint_line("gate.nika.yaml", trace, &confirm, ""),
            "resume: nika run gate.nika.yaml --resume .nika/traces/t.ndjson \
             --answer approve=true · or false"
        );
        let choice = WorkflowPause::new(
            "pick".into(),
            "choice".into(),
            None,
            vec!["alpha".into(), "beta".into(), "gamma".into()],
        );
        assert_eq!(
            super::resume_hint_line("w.yaml", trace, &choice, ""),
            "resume: nika run w.yaml --resume .nika/traces/t.ndjson \
             --answer pick=alpha · or beta · gamma"
        );
        let single = WorkflowPause::new("pick".into(), "choice".into(), None, vec!["only".into()]);
        assert!(
            super::resume_hint_line("w.yaml", trace, &single, "").ends_with("--answer pick=only"),
            "a lone choice names no alternatives"
        );
        let input = WorkflowPause::new("name".into(), "input".into(), None, vec![]);
        assert!(
            super::resume_hint_line("w.yaml", trace, &input, "")
                .ends_with("--answer name=\"your answer\"")
        );
    }

    /// THE paste-safety law (2026-07-31 · found by a first-run audit):
    /// the taught `--answer ask=true|false` pasted as a shell PIPE — it
    /// silently bound `true` (a human gate answered by the shell, zero
    /// humans involved) and the piped-to `false` closed stdout, leaking
    /// a broken-pipe panic. Every taught command must be free of shell
    /// metacharacters outside quotes, in EVERY mode.
    #[test]
    fn every_taught_resume_command_is_paste_safe() {
        use nika_runtime::WorkflowPause;
        let trace = std::path::Path::new(".nika/traces/t.ndjson");
        let carry = super::resume_carry(&["page=wifi".to_owned()], Some("openai/gpt-5.2"));
        let modes = [
            WorkflowPause::new("approve".into(), "confirm".into(), None, vec![]),
            WorkflowPause::new(
                "pick".into(),
                "choice".into(),
                None,
                vec!["alpha".into(), "beta".into()],
            ),
            WorkflowPause::new("name".into(), "input".into(), None, vec![]),
            // An unknown/future mode falls to the input shape — still safe.
            WorkflowPause::new("mystery".into(), "someday".into(), None, vec![]),
        ];
        for pause in modes {
            for c in ["", carry.as_str()] {
                let line = super::resume_hint_line("w.yaml", trace, &pause, c);
                assert_eq!(
                    super::unsafe_to_paste(&line),
                    None,
                    "shell metacharacter in a taught command: {line}"
                );
            }
        }
        // The detector itself must catch the exact regression it exists for.
        assert_eq!(
            super::unsafe_to_paste("resume: nika run w.yaml --answer approve=true|false"),
            Some('|')
        );
    }

    /// The taught line carries the run's own `--var`/`--model` — a
    /// workflow with REQUIRED inputs refuses a var-less resume, so the
    /// copy-paste must re-supply them (seo-live-review · 2026-07-31).
    #[test]
    fn resume_hint_carries_the_runs_vars_and_model() {
        use nika_runtime::WorkflowPause;
        let carry = super::resume_carry(
            &[
                "page_type=wifi".to_owned(),
                r#"locales=["fr-FR","ar-SA"]"#.to_owned(),
            ],
            Some("openai/gpt-5.2"),
        );
        assert_eq!(
            carry,
            r#" --var page_type=wifi --var 'locales=["fr-FR","ar-SA"]' --model openai/gpt-5.2"#
        );
        let trace = std::path::Path::new(".nika/traces/t.ndjson");
        let confirm = WorkflowPause::new("access_gate".into(), "confirm".into(), None, vec![]);
        let line = super::resume_hint_line("seo-live-review.nika.yaml", trace, &confirm, &carry);
        assert_eq!(
            line,
            "resume: nika run seo-live-review.nika.yaml --var page_type=wifi \
             --var 'locales=[\"fr-FR\",\"ar-SA\"]' --model openai/gpt-5.2 \
             --resume .nika/traces/t.ndjson --answer access_gate=true · or false"
        );
        // No vars, no override → no carry, byte-identical to before.
        assert_eq!(super::resume_carry(&[], None), "");
        // An embedded single quote splices through the POSIX idiom.
        let quoted = super::resume_carry(&["msg=it's".to_owned()], None);
        assert_eq!(quoted, r" --var 'msg=it'\''s'");
    }

    /// Issue 772 — the machine pause contract carries the run's own
    /// carry verbatim: a JSON consumer reconstructing the resume
    /// command must not drop the `--var`/`--model` tail (the flag-less
    /// refusal at resume stays the backstop, this is the teaching lane).
    #[test]
    fn paused_envelope_carries_the_resume_carry() {
        use nika_runtime::WorkflowPause;
        let pause = WorkflowPause::new("gate".into(), "confirm".into(), None, vec![]);
        let carry = super::resume_carry(&["k=v".to_owned()], Some("mock/override"));
        let line = super::paused_envelope_line(&pause, &carry);
        let json: serde_json::Value =
            serde_json::from_str(&line).expect("the envelope is one JSON document");
        assert_eq!(
            json["paused"]["resume_carry"],
            serde_json::json!(" --var k=v --model mock/override")
        );
        // A carry-less run rides an EMPTY carry — present and honest,
        // never a missing key a consumer must special-case.
        let bare = super::paused_envelope_line(&pause, "");
        let json: serde_json::Value = serde_json::from_str(&bare).expect("one JSON document");
        assert_eq!(json["paused"]["resume_carry"], serde_json::json!(""));
    }
}
