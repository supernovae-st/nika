// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's FRUIT and its form-sanity reads — pure functions of the
//! folded [`RunView`] (the fold law · zero new instrumentation).
//!
//! Two laws, one module (user gauntlet 2026-07-31 · 19 grounded
//! personas):
//!
//! 1. **The fruit is named** — a run that wrote a file says so on its
//!    closing surface (`wrote output.md (412B)`): the Aha is "I made a
//!    thing", not "the plumbing ran". The paths fold out of the
//!    `nika:write`/`nika:edit` rows' own outputs (the builtins return
//!    the path they wrote — spec stdlib §write/§edit); byte sizes are
//!    the CALLER's to stat (no I/O lives in this crate).
//! 2. **A green run never lies in silence** — when the answer's FORM
//!    contradicts the verdict (the model asked for its inputs back ·
//!    every input settled through `on_error.recover` fallbacks · an
//!    empty answer), the closing surface says it. A caution is a
//!    reading aid, never a verdict: the exit code does not change.

use crate::state::{RunView, TaskRow, TaskState};

/// One file the run materialized, folded from a write-shaped row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WroteFile {
    /// The verb the row performed — `wrote` (nika:write) · `edited`
    /// (nika:edit): the closing line speaks the builtin's own action.
    pub verb: &'static str,
    /// The path the builtin returned (its output value · verbatim).
    pub path: String,
}

/// The dispatch-note prefixes that mark an inference row (the runtime's
/// verb vocabulary — `infer · <model>` · `agent · <model>`).
fn is_infer(row: &TaskRow) -> bool {
    row.model.is_some()
        || row
            .started_note
            .as_deref()
            .is_some_and(|n| n.starts_with("infer · ") || n.starts_with("agent · "))
}

/// The write-shaped builtins whose OUTPUT is the path they materialized.
/// Deliberately the taught pair only (`nika:write` · `nika:edit`) — an
/// `exec` side-effect or an mcp tool write has no path contract to read.
fn write_verb(row: &TaskRow) -> Option<&'static str> {
    match row.started_note.as_deref() {
        Some("invoke · nika:write") => Some("wrote"),
        Some("invoke · nika:edit") => Some("edited"),
        _ => None,
    }
}

/// Every file the run's settled write rows materialized, in row order,
/// de-duplicated (a re-written path reads once). A `for_each` write
/// settles as ONE row carrying an array of paths — each fans out here.
#[must_use]
pub fn written_files(view: &RunView) -> Vec<WroteFile> {
    let mut seen: Vec<WroteFile> = Vec::new();
    for row in view.rows() {
        if row.state != TaskState::Ok {
            continue;
        }
        // A recovered write is Ok because the recover arm supplied a
        // substitute, not because the effect landed. Prefixing `wrote`
        // to that substitute is the lie (issue 1045).
        if row.recovered {
            continue;
        }
        let Some(verb) = write_verb(row) else {
            continue;
        };
        let Some(text) = row.output_json.as_deref() else {
            continue;
        };
        // The builtin's contract: ONE path string · a fan-out's array of
        // them. Anything else is not a path read — render nothing rather
        // than a guess (the hand-edited-trace stance shape.rs holds).
        let mut push = |path: String| {
            if !seen.iter().any(|f| f.path == path) {
                seen.push(WroteFile { verb, path });
            }
        };
        match serde_json::from_str(text) {
            Ok(serde_json::Value::String(path)) => push(path),
            Ok(serde_json::Value::Array(items)) => {
                for item in items {
                    if let serde_json::Value::String(path) = item {
                        push(path);
                    }
                }
            }
            _ => {}
        }
    }
    seen
}

/// The last word the model spoke: the LAST settled inference row whose
/// output is a non-empty string — `(task id, the compact JSON text)`.
/// The caller renders it bounded (`shape::summarize`) so the card shows
/// the model SPEAKING, never a data dump.
#[must_use]
pub fn last_said(view: &RunView) -> Option<(&str, &str)> {
    view.rows().iter().rev().find_map(|row| {
        if row.state != TaskState::Ok || !is_infer(row) {
            return None;
        }
        let text = row.output_json.as_deref()?;
        // A JSON string with content — `""` is the empty-answer caution's
        // territory, not a quote worth printing.
        (text.len() > 2 && text.starts_with('"')).then_some((row.id.as_str(), text))
    })
}

/// A completed run that repaired at least one task. Exit 0 is still
/// correct (recovered is a success cause); the first-glance glyph is
/// not. Persona 14 · gauntlet g2 on 81c1138f: `--quiet` printed `✔`
/// and `trace ls` said `completed`.
#[must_use]
pub fn recovered_ok(view: &RunView) -> bool {
    view.verdict == Some(true) && view.recovered_count() > 0
}

/// A mock infer model or a mock image/tts provider — the run was a
/// REHEARSAL and the closing surface must say so (Zoe and Ben read the
/// echo as the product · C08 the mock OG card as a real render).
/// `false` when nothing mock spoke, or when any real model/provider did.
#[must_use]
pub fn rehearsal(view: &RunView) -> bool {
    let mut any = false;
    for row in view.rows() {
        if let Some(model) = row.model.as_deref() {
            if !is_mock_model(model) {
                return false;
            }
            any = true;
        }
        if is_real_media(row) {
            return false;
        }
        if is_mock_media(row) {
            any = true;
        }
    }
    any
}

fn is_mock_model(model: &str) -> bool {
    model == "mock" || model.starts_with("mock/")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    Image,
    Tts,
}

fn media_kind(row: &TaskRow) -> Option<MediaKind> {
    let note = row.started_note.as_deref().unwrap_or(row.note.as_str());
    if note.contains("nika:image_generate") {
        Some(MediaKind::Image)
    } else if note.contains("nika:tts_generate") {
        Some(MediaKind::Tts)
    } else {
        None
    }
}

fn output_provider_is_mock(row: &TaskRow) -> Option<bool> {
    let raw = row.output_json.as_deref()?;
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    Some(value.get("provider").and_then(|p| p.as_str()) == Some("mock"))
}

fn warning_names_rehearsal(row: &TaskRow) -> bool {
    row.warning.as_deref().is_some_and(|w| {
        w.contains("rehearsal")
            || w.contains("not a real image")
            || w.contains("not a real recording")
    })
}

fn is_mock_media(row: &TaskRow) -> bool {
    media_kind(row).is_some()
        && (output_provider_is_mock(row) == Some(true) || warning_names_rehearsal(row))
}

fn is_real_media(row: &TaskRow) -> bool {
    media_kind(row).is_some() && output_provider_is_mock(row) == Some(false)
}

/// The head of an answer that ASKS FOR ITS INPUTS instead of answering
/// (Priya's class: `4/4 done` over "I don't see any transcripts
/// provided"). Conservative by design — a phrase list over the answer's
/// HEAD (refusals open with it; a legit answer quoting one deep in its
/// body stays clean). A miss is acceptable; a false alarm erodes ⚠.
#[must_use]
pub fn asks_inputs_back(text: &str) -> bool {
    const PHRASES: &[&str] = &[
        "i don't see any",
        "i do not see any",
        "i don't see the",
        "i don't have access to",
        "i do not have access to",
        "could you provide",
        "could you please provide",
        "could you share",
        "could you please share",
        "please provide the",
        "please share the",
        "no input provided",
        "no inputs provided",
    ];
    let head: String = text.chars().take(300).collect::<String>().to_lowercase();
    PHRASES.iter().any(|p| head.contains(p))
}

/// The all-fallback read: every task that fed the first inference (the
/// settled non-inference rows BEFORE it, in row order — first-seen order
/// is schedule order) settled through `on_error.recover`. Returns the
/// input count when the predicate fires — `⚠ N of N inputs recovered`.
/// One clean input anywhere → `None` (the card's `N recovered` cell
/// already tells the partial story).
#[must_use]
pub fn inputs_all_recovered(view: &RunView) -> Option<usize> {
    let first_infer = view
        .rows()
        .iter()
        .position(|r| r.state == TaskState::Ok && is_infer(r))?;
    let inputs: Vec<&TaskRow> = view.rows()[..first_infer]
        .iter()
        .filter(|r| r.state == TaskState::Ok && !is_infer(r))
        .collect();
    (!inputs.is_empty() && inputs.iter().all(|r| r.recovered)).then_some(inputs.len())
}

/// The form-sanity caution rows — RAW text (`⚠ …` · `!` under ASCII),
/// paint is the surface's job (the card fits width BEFORE painting).
/// Empty on a truthful run: ⚠ scarcity is what makes ⚠ work.
#[must_use]
pub fn cautions(view: &RunView, ascii: bool) -> Vec<String> {
    let mark = if ascii { "!" } else { "⚠" };
    let mut lines = Vec::new();
    for row in view.rows() {
        if row.state != TaskState::Ok || !is_infer(row) {
            continue;
        }
        let Some(text) = row.output_json.as_deref() else {
            continue;
        };
        if text == "\"\"" {
            // The OBS-E rider (#410) already voices a runtime-diagnosed
            // blank — this arm catches the ones the runtime had no
            // diagnosis for, without doubling the spoken row.
            if row.warning.is_none() {
                lines.push(format!("{mark} {} · answered empty", row.id));
            }
        } else if text.starts_with('"') && asks_inputs_back(text) {
            lines.push(format!(
                "{mark} {} · the answer asks for its inputs back — the run may have fed it nothing",
                row.id
            ));
        }
    }
    if let Some(n) = inputs_all_recovered(view) {
        lines.push(format!(
            "{mark} {n} of {n} inputs recovered · the model answered from fallbacks"
        ));
    }
    // C08 · a mock image/tts settle is a green that would otherwise
    // read as a real render. ⚠ here is earned: the builtin JSON used
    // to ship `warnings: []`.
    for row in view.rows() {
        if row.state != TaskState::Ok || !is_mock_media(row) {
            continue;
        }
        let kind = match media_kind(row) {
            Some(MediaKind::Image) => "not a real image",
            Some(MediaKind::Tts) => "not a real recording",
            None => continue,
        };
        lines.push(format!("{mark} {} · {kind} — mock provider", row.id));
    }
    lines
}

/// The rehearsal note — a fact, not a caution (mock is a delight: 11 of
/// 19 personas cited it) and NEVER a taught command: it states what the
/// output is, the operator decides what to do next. Media mock is the
/// C08 class (`not a real image` / recording) so the OG card cannot
/// collapse to `nika OK · $0.00`.
#[must_use]
pub fn rehearsal_note(view: &RunView) -> Option<&'static str> {
    if !rehearsal(view) {
        return None;
    }
    if view
        .rows()
        .iter()
        .any(|row| media_kind(row) == Some(MediaKind::Image) && is_mock_media(row))
    {
        return Some("rehearsal · not a real image");
    }
    if view
        .rows()
        .iter()
        .any(|row| media_kind(row) == Some(MediaKind::Tts) && is_mock_media(row))
    {
        return Some("rehearsal · not a real recording");
    }
    Some("rehearsal · a mock model echoed the prompt — not a real answer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};

    fn ev(kind: EventKind, ms: u64, fields: &[(&str, &str)]) -> nika_event::Event {
        let mut e = demo::bare_event(kind, ms);
        for (k, v) in fields {
            e = e.with_field(KeyValue::new(*k, Value::String((*v).to_owned())));
        }
        e
    }

    /// One settled write row → one named file; the fruit reads the
    /// builtin's own output, verb included (write → wrote · edit →
    /// edited), and a re-written path reads once.
    #[test]
    fn written_files_fold_from_write_rows() {
        let mut view = RunView::new();
        view.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "persist"), ("note", "invoke · nika:write")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[("task", "persist"), ("output", "\"output.md\"")],
        ));
        view.apply(&ev(
            EventKind::TaskStarted,
            2,
            &[("task", "fix"), ("note", "invoke · nika:edit")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            3,
            &[("task", "fix"), ("output", "\"notes/log.md\"")],
        ));
        // A second write to the SAME path — the fruit reads once.
        view.apply(&ev(
            EventKind::TaskStarted,
            4,
            &[("task", "again"), ("note", "invoke · nika:write")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            5,
            &[("task", "again"), ("output", "\"output.md\"")],
        ));
        let files = written_files(&view);
        assert_eq!(files.len(), 2, "{files:?}");
        assert_eq!(files[0].verb, "wrote");
        assert_eq!(files[0].path, "output.md");
        assert_eq!(files[1].verb, "edited");
        assert_eq!(files[1].path, "notes/log.md");
    }

    /// A fan-out write settles as ONE row carrying an array of paths —
    /// each path fans out into its own fruit entry.
    #[test]
    fn written_files_fan_out_from_an_array_output() {
        let mut view = RunView::new();
        view.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "translate"), ("note", "invoke · nika:write")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[("task", "translate"), ("output", r#"["a.md","b.md"]"#)],
        ));
        let files = written_files(&view);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "a.md");
        assert_eq!(files[1].path, "b.md");
    }

    /// Non-write rows, failed writes and non-path outputs fold to
    /// nothing — a fruit line is never a guess.
    #[test]
    fn written_files_ignore_non_write_shapes() {
        let mut view = RunView::new();
        // A read row (same invoke vocabulary, wrong tool).
        view.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "gather"), ("note", "invoke · nika:read")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[("task", "gather"), ("output", "\"content\"")],
        ));
        // A FAILED write (no fruit from a failure).
        view.apply(&ev(
            EventKind::TaskStarted,
            2,
            &[("task", "persist"), ("note", "invoke · nika:write")],
        ));
        view.apply(&ev(EventKind::TaskFailed, 3, &[("task", "persist")]));
        // A write row whose output is not a string (older engine · odd).
        view.apply(&ev(
            EventKind::TaskStarted,
            4,
            &[("task", "odd"), ("note", "invoke · nika:write")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            5,
            &[("task", "odd"), ("output", "42")],
        ));
        assert!(written_files(&view).is_empty());
    }

    /// A recovered write is Ok in the fold (the recover arm supplied a
    /// substitute) but nothing landed on disk. Prefixing `wrote` to that
    /// substitute is the lie issue 1045 measured: `wrote NOTHING WAS
    /// WRITTEN` · rc=0 · empty tree.
    #[test]
    fn written_files_ignore_a_recovered_write() {
        let mut view = RunView::new();
        view.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "persist"), ("note", "invoke · nika:write")],
        ));
        view.apply(&ev(EventKind::TaskRecovered, 1, &[("task", "persist")]));
        view.apply(&ev(
            EventKind::TaskCompleted,
            2,
            &[("task", "persist"), ("output", "\"NOTHING WAS WRITTEN\"")],
        ));
        assert!(
            view.rows()[0].recovered,
            "the repair fact must fold or this test is blind"
        );
        assert!(
            written_files(&view).is_empty(),
            "a recovered write is not fruit: {:?}",
            written_files(&view)
        );
    }

    /// The last word: the LAST settled inference row with a non-empty
    /// string output — skips · failures · empty answers never speak.
    #[test]
    fn last_said_is_the_last_nonempty_inference() {
        let mut view = RunView::new();
        view.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "draft"), ("note", "infer · mock/echo")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[("task", "draft"), ("output", "\"first draft\"")],
        ));
        view.apply(&ev(
            EventKind::TaskStarted,
            2,
            &[("task", "polish"), ("note", "infer · mock/echo")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            3,
            &[("task", "polish"), ("output", "\"final text\"")],
        ));
        let (task, text) = last_said(&view).expect("the model spoke");
        assert_eq!(task, "polish", "the LAST word wins");
        assert_eq!(text, "\"final text\"");

        // An empty answer is the caution's territory, not a quote.
        let mut blank = RunView::new();
        blank.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "think"), ("note", "infer · mock/echo")],
        ));
        blank.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[("task", "think"), ("output", "\"\"")],
        ));
        assert_eq!(last_said(&blank), None);
    }

    /// Rehearsal: every named model is a mock → true · one real model
    /// anywhere → false · no model at all → false (nothing to announce).
    #[test]
    fn rehearsal_requires_all_models_mock() {
        let mut view = RunView::new();
        view.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "a"), ("note", "infer · mock/echo")],
        ));
        view.apply(&ev(EventKind::TaskCompleted, 1, &[("task", "a")]));
        assert!(rehearsal(&view));
        assert_eq!(
            rehearsal_note(&view),
            Some("rehearsal · a mock model echoed the prompt — not a real answer")
        );

        view.apply(&ev(
            EventKind::TaskStarted,
            2,
            &[("task", "b"), ("note", "infer · openai/gpt-5.2")],
        ));
        assert!(!rehearsal(&view), "one real model → not a rehearsal");

        let toolless = RunView::new();
        assert!(!rehearsal(&toolless), "no model spoke → nothing to say");
    }

    /// C08 · issue 1302: a mock `nika:image_generate` run has no infer
    /// model, so the old "every named model is mock" read stayed silent
    /// and the card was only `nika OK · $0.00`. The output JSON names
    /// `provider: mock` — that IS a rehearsal, and the note must say so
    /// (`rehearsal` or `not a real image`). A real provider does not.
    #[test]
    fn mock_image_generate_is_a_rehearsal_not_a_silent_ok() {
        let mock_out = r#"{"provider":"mock","warnings":[],"images":[]}"#;
        let mut view = RunView::new();
        view.apply(&ev(
            EventKind::WorkflowStarted,
            0,
            &[("workflow", "og-images")],
        ));
        view.apply(&ev(
            EventKind::TaskStarted,
            1,
            &[("task", "hero"), ("note", "invoke · nika:image_generate")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            2,
            &[("task", "hero"), ("output", mock_out)],
        ));
        view.apply(&ev(EventKind::WorkflowCompleted, 3, &[]));

        assert!(
            rehearsal(&view),
            "provider=mock image_generate is a rehearsal, even with no infer model"
        );
        let note = rehearsal_note(&view).expect("the closing surface must name the rehearsal");
        assert!(
            note.contains("rehearsal") || note.contains("not a real image"),
            "C08 flag missing from rehearsal_note: {note}"
        );
        // The JSON the builtin returned had `warnings: []` — the card
        // still has to speak. A caution OR the rehearsal fact counts;
        // silence is the bug.
        let caution_lines = cautions(&view, true);
        assert!(
            !caution_lines.is_empty() || note.contains("rehearsal"),
            "warnings/cautions stayed empty on a mock OG run: {caution_lines:?} note={note}"
        );

        let ascii = crate::theme::Theme::new(false, true, false);
        let card = crate::flow::verdict_card(&view, &ascii, &[]).join("\n");
        assert!(
            card.contains("rehearsal") || card.contains("not a real image"),
            "the shareable card hid the mock: {card}"
        );
        // C08: a card that names the rehearsal is not the lying
        // `nika OK · $0.00` storefront, even when spend still reads $0.
        assert!(
            card.contains("rehearsal") || !card.contains("$0.00"),
            "card said only nika OK · $0.00: {card}"
        );

        // A billed provider is not a rehearsal — flipping this predicate
        // is the mutation the test must catch.
        let real_out = r#"{"provider":"xai","warnings":[],"images":[]}"#;
        let mut real = RunView::new();
        real.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "hero"), ("note", "invoke · nika:image_generate")],
        ));
        real.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[("task", "hero"), ("output", real_out)],
        ));
        assert!(
            !rehearsal(&real),
            "a real image provider must not wear the rehearsal flag"
        );
        assert_eq!(rehearsal_note(&real), None);
    }

    /// C08 · the TTS twin: mock `nika:tts_generate` is the same honesty
    /// class as mock image — the card must not read as a real recording.
    #[test]
    fn mock_tts_generate_is_a_rehearsal() {
        let mock_out = r#"{"provider":"mock","warnings":[]}"#;
        let mut view = RunView::new();
        view.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "speak"), ("note", "invoke · nika:tts_generate")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[("task", "speak"), ("output", mock_out)],
        ));
        assert!(rehearsal(&view), "mock tts is a rehearsal");
        let note = rehearsal_note(&view).expect("tts rehearsal must speak");
        assert!(
            note.contains("rehearsal") || note.contains("not a real"),
            "C08 flag missing from tts rehearsal_note: {note}"
        );
    }

    /// The ask-back head phrases fire; a legit answer quoting one deep
    /// in its body stays clean (head-bounded scan).
    #[test]
    fn asks_inputs_back_scans_the_head_only() {
        assert!(asks_inputs_back(
            "I don't see any city-council meeting transcripts provided. \
             Could you please share them?"
        ));
        assert!(asks_inputs_back("Could you provide the CSV export?"));
        assert!(!asks_inputs_back("The quarterly report is ready."));
        let deep = format!("{}please provide the...", "x".repeat(400));
        assert!(!asks_inputs_back(&deep), "a deep quote never fires");
    }

    /// The all-fallback read: every input before the first inference
    /// recovered → Some(N) · one clean input → None · no inputs → None.
    #[test]
    fn inputs_all_recovered_requires_every_input() {
        let recov = |view: &mut RunView, task: &str| {
            view.apply(&ev(
                EventKind::TaskStarted,
                0,
                &[("task", task), ("note", "invoke · nika:fetch")],
            ));
            view.apply(&ev(EventKind::TaskRecovered, 1, &[("task", task)]));
            view.apply(&ev(EventKind::TaskCompleted, 2, &[("task", task)]));
        };
        let mut view = RunView::new();
        recov(&mut view, "f1");
        recov(&mut view, "f2");
        view.apply(&ev(
            EventKind::TaskStarted,
            3,
            &[("task", "score"), ("note", "infer · openai/gpt-5.2")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            4,
            &[("task", "score"), ("output", "\"7/10\"")],
        ));
        // The write AFTER the inference never counts as an input.
        view.apply(&ev(
            EventKind::TaskStarted,
            5,
            &[("task", "persist"), ("note", "invoke · nika:write")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            6,
            &[("task", "persist"), ("output", "\"seo.md\"")],
        ));
        assert_eq!(inputs_all_recovered(&view), Some(2));
        let all = cautions(&view, false);
        assert!(
            all.iter()
                .any(|l| l == "⚠ 2 of 2 inputs recovered · the model answered from fallbacks"),
            "{all:?}"
        );

        // One CLEAN input → the partial story stays with the `N
        // recovered` cell, no all-fallback caution.
        let mut mixed = RunView::new();
        recov(&mut mixed, "f1");
        mixed.apply(&ev(
            EventKind::TaskStarted,
            3,
            &[("task", "clean"), ("note", "invoke · nika:fetch")],
        ));
        mixed.apply(&ev(EventKind::TaskCompleted, 4, &[("task", "clean")]));
        mixed.apply(&ev(
            EventKind::TaskStarted,
            5,
            &[("task", "score"), ("note", "infer · openai/gpt-5.2")],
        ));
        mixed.apply(&ev(
            EventKind::TaskCompleted,
            6,
            &[("task", "score"), ("output", "\"ok\"")],
        ));
        assert_eq!(inputs_all_recovered(&mixed), None);

        // No non-inference input at all → vacuous truth never fires.
        let mut bare = RunView::new();
        bare.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "solo"), ("note", "infer · mock/echo")],
        ));
        bare.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[("task", "solo"), ("output", "\"hi\"")],
        ));
        assert_eq!(inputs_all_recovered(&bare), None);
    }

    #[test]
    fn recovered_ok_is_completed_with_a_repair() {
        let mut view = RunView::new();
        for e in demo::recovered() {
            view.apply(&e);
        }
        assert_eq!(view.verdict, Some(true));
        assert!(recovered_ok(&view));
        let mut clean = RunView::new();
        for e in demo::success() {
            clean.apply(&e);
        }
        assert!(!recovered_ok(&clean));
        let mut failed = RunView::new();
        for e in demo::failure() {
            failed.apply(&e);
        }
        assert!(!recovered_ok(&failed));
    }

    /// The caution set: ask-back fires with the task named · an empty
    /// answer fires ONLY when the runtime did not already warn (#410
    /// stays the one voice for its own diagnosis) · a truthful run
    /// renders zero cautions (⚠ scarcity).
    #[test]
    fn cautions_cover_the_lying_green_classes() {
        let mut view = RunView::new();
        view.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "summarize"), ("note", "infer · mock/echo")],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[
                ("task", "summarize"),
                (
                    "output",
                    "\"I don't see any transcripts provided. Could you please share them?\"",
                ),
            ],
        ));
        let lines = cautions(&view, false);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("⚠ summarize · the answer asks for its inputs back"),
            "{lines:?}"
        );
        // ASCII parity — the mark degrades, the text stays.
        let ascii = cautions(&view, true);
        assert!(ascii[0].starts_with("! summarize ·"), "{ascii:?}");

        // Empty answer without a runtime warning → spoken here.
        let mut blank = RunView::new();
        blank.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "think"), ("note", "infer · mock/echo")],
        ));
        blank.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[("task", "think"), ("output", "\"\"")],
        ));
        assert_eq!(cautions(&blank, false), vec!["⚠ think · answered empty"]);

        // …and WITH a runtime warning (#410) the row already speaks —
        // this module stays silent for it.
        let mut warned = RunView::new();
        warned.apply(&ev(
            EventKind::TaskStarted,
            0,
            &[("task", "think"), ("note", "infer · mock/echo")],
        ));
        warned.apply(&ev(
            EventKind::TaskCompleted,
            1,
            &[
                ("task", "think"),
                ("output", "\"\""),
                ("warning", "thinking budget spent · answered blank"),
            ],
        ));
        assert!(cautions(&warned, false).is_empty());

        // A truthful storyboard renders ZERO cautions.
        let mut clean = RunView::new();
        for e in demo::success() {
            clean.apply(&e);
        }
        assert!(cautions(&clean, false).is_empty());
    }
}
