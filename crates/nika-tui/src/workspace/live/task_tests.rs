// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A task of the run in view is picked by its id, never by the line it is
//! painted on, and read in detail from what the stream folded: its state,
//! its failure, what was measured and its output, each absence named. The
//! graph lends order and static facts only when the run names its bytes.
//! These drive the desk the shell keeps, key by key, as the human would.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika_display::run_story::RunFrame;

use crate::model::demo_project;
use crate::session::feed::Observed;
use crate::workspace::desk::{Desk, Route};
use crate::workspace::focus::Region;
use crate::workspace::inspect::Inspected;
use crate::workspace::object::Object;

pub(super) const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
const NEXT: &str = "01a0ef11-0212-70de-a8b3-99de94271111";
pub(super) const WIDE: (u16, u16) = (120, 40);
pub(super) const SMALL: (u16, u16) = (80, 24);
const NARROW: (u16, u16) = (60, 18);
const HUGE: (u16, u16) = (240, 60);

const TWO: &str = "nika: two\npermits: {}\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n  second:\n    with: { x: \"${{ tasks.first.output }}\" }\n    invoke: { tool: \"nika:log\", args: { message: two } }\n";

const THREE: &str = "nika: three\npermits: {}\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: one } }\n  second:\n    with: { x: \"${{ tasks.first.output }}\" }\n    invoke: { tool: \"nika:log\", args: { message: two } }\n  tidy:\n    after: { first: unwind }\n    invoke: { tool: \"nika:log\", args: { message: tidy } }\n";

/// The glyphs of a state the fold reports, in both glyph columns' Unicode.
const STATES: [&str; 8] = ["○", "◐", "✔", "✖", "↻", "↷", "⊘", "◇"];

pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// One `{"key":…,"value":…}` field, its value already JSON.
pub(super) fn field(key: &str, value: &str) -> String {
    format!(r#"{{"key":"{key}","value":{value}}}"#)
}

/// A JSON string holding `text`.
pub(super) fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("json")
}

/// A runtime event of `kind` in execution `exec`, with id `n` and `fields`.
pub(super) fn event(exec: &str, n: u32, kind: &str, fields: &[String]) -> Observed {
    let line = format!(
        r#"{{"correlation":null,"execution":{{"uuid":"{exec}"}},"fields":[{}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#,
        fields.join(",")
    );
    Observed::Frame(RunFrame::decode(&line).expect("a runtime event"))
}

/// A task event of `kind` naming `id`, with `extra` fields.
pub(super) fn task(exec: &str, n: u32, kind: &str, id: &str, extra: &[String]) -> Observed {
    let mut fields = vec![field("task", &quoted(id))];
    fields.extend_from_slice(extra);
    event(exec, n, kind, &fields)
}

/// The run's start, naming the sha256 of `bytes`.
pub(super) fn start(exec: &str, bytes: &str) -> Observed {
    use sha2::{Digest as _, Sha256};
    let hash = Sha256::digest(bytes.as_bytes())
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{b:02x}"));
            hex
        });
    event(
        exec,
        1,
        "workflow_started",
        &[
            field("workflow", &quoted("two")),
            field("workflow_sha256", &quoted(&hash)),
        ],
    )
}

pub(super) fn settled(exec: &str, status: &str) -> Observed {
    let line = format!(
        r#"{{"kind":"run_settled","status":"{status}","cause":"normal","elapsed_ms":2,"execution":{{"uuid":"{exec}"}},"evidence":"unsealed","spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"outputs":{{}}}}"#
    );
    Observed::Frame(RunFrame::decode(&line).expect("a settlement"))
}

/// A run of `two.nika` asked over `look`.
pub(super) fn asked(look: Option<Inspected>, resume: bool) -> Observed {
    Observed::Asked {
        workflow: "two.nika".to_owned(),
        resume,
        typed: true,
        look: look.map(Box::new),
    }
}

fn look(source: &str) -> Inspected {
    crate::session::judge_for_tests("two.nika", "w".repeat(64), source)
}

/// The desk of the demo project after `seen`, the object holding the keys.
pub(super) fn desk(seen: Vec<Observed>) -> Desk {
    let mut desk = Desk::new();
    desk.view = Some(demo_project());
    desk.observe(seen.into_iter());
    desk.focus.region = Region::Object;
    desk
}

/// The object in view on a terminal of `size`, as its title and body rows.
pub(super) fn object(desk: &mut Desk, size: (u16, u16), ascii: bool) -> (String, Vec<String>) {
    desk.prepare(size, ascii, false);
    let Object::Workflow { title, body } = desk.screen(ascii).object else {
        panic!("the run is in view");
    };
    (
        title.to_string(),
        body.iter().map(ToString::to_string).collect(),
    )
}

/// The picked task's row: the one the pick mark leads.
fn picked(body: &[String], ascii: bool) -> Option<(usize, String)> {
    let mark = if ascii { "* " } else { "› " };
    let mut rows = body.iter().enumerate().filter(|(_, r)| r.starts_with(mark));
    let found = rows.next().map(|(at, row)| (at, row.clone()));
    assert!(rows.next().is_none(), "one task is picked: {body:#?}");
    found
}

/// The task list: the rows under its header.
fn list(body: &[String]) -> &[String] {
    let at = (body.iter().position(|r| r.starts_with("tasks")))
        .unwrap_or_else(|| panic!("a task list: {body:#?}"));
    &body[at + 1..]
}

/// The row of the task list naming `id` (after the pick mark's column).
fn row<'a>(body: &'a [String], id: &str) -> &'a String {
    let named = |r: &&String| {
        let rest = (r.strip_prefix("› ").or_else(|| r.strip_prefix("  "))).unwrap_or("");
        rest.split_whitespace().any(|word| word == id)
    };
    (list(body).iter().find(named)).unwrap_or_else(|| panic!("a row names {id}: {body:#?}"))
}

/// Two tasks of the bound bytes: `first` succeeded with an output, `second`
/// failed and said why; the run settled failed.
fn two_tasks() -> Vec<Observed> {
    vec![
        asked(Some(look(TWO)), false),
        start(EXEC, TWO),
        task(EXEC, 2, "task_scheduled", "first", &[]),
        task(EXEC, 3, "task_scheduled", "second", &[]),
        task(EXEC, 4, "task_started", "first", &[]),
        task(
            EXEC,
            5,
            "task_completed",
            "first",
            &[
                field("output", &quoted("\"one\"")),
                field("duration_ms", "3"),
            ],
        ),
        task(EXEC, 6, "task_started", "second", &[]),
        task(
            EXEC,
            7,
            "task_failed",
            "second",
            &[
                field("detail", &quoted("the log refused: boom")),
                field("duration_ms", "5"),
            ],
        ),
        settled(EXEC, "failed"),
    ]
}

#[test]
fn the_picked_task_is_kept_by_its_id_through_wrap_resize_and_faces() {
    let mut desk = desk(two_tasks());
    let (_, body) = object(&mut desk, WIDE, false);
    let (_, first) = picked(&body, false).expect("the first listed task is picked");
    assert!(first.contains("✔ first"), "{first}");
    assert_eq!(desk.route(key(KeyCode::Down), WIDE), Route::Repaint);
    let (_, body) = object(&mut desk, WIDE, false);
    let (_, second) = picked(&body, false).expect("a task is picked");
    assert!(second.contains("✖ second"), "{second}");
    // A narrower terminal wraps the facts above the list onto more rows:
    // the same task stays picked on another painted line.
    let (_, huge) = object(&mut desk, HUGE, false);
    let (huge_at, wide) = picked(&huge, false).expect("still picked");
    assert!(wide.contains("second"), "{huge:#?}");
    let (_, narrow) = object(&mut desk, NARROW, false);
    let (narrow_at, still) = picked(&narrow, false).expect("still picked");
    assert!(still.contains("second"), "{narrow:#?}");
    assert_ne!(huge_at, narrow_at, "the pick is not a painted line");
    let (_, small) = object(&mut desk, SMALL, false);
    assert!(picked(&small, false).is_some_and(|(_, r)| r.contains("second")));
    // A face turned away and back keeps the pick.
    assert_eq!(desk.route(key(KeyCode::Right), WIDE), Route::Repaint);
    assert_eq!(desk.route(key(KeyCode::Left), WIDE), Route::Repaint);
    let (_, body) = object(&mut desk, WIDE, false);
    assert!(picked(&body, false).is_some_and(|(_, r)| r.contains("second")));
    // The last task is the last one: Down moves no further.
    assert_eq!(desk.route(key(KeyCode::Down), WIDE), Route::Nothing);
    // Enter opens the picked task's detail, read from the fold.
    let scroll = desk.focus.scroll;
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
    let (title, detail) = object(&mut desk, WIDE, false);
    assert!(
        title.contains("task second") && title.contains("Backspace"),
        "{title}"
    );
    let text = detail.join("\n");
    assert!(text.contains("failed"), "{text}");
    assert!(text.contains("the log refused: boom"), "{text}");
    assert!(text.contains("5 ms"), "{text}");
    assert!(
        !text.contains("\"one\""),
        "never another task's output: {text}"
    );
    assert!(desk.wanted().is_none(), "a detail acquires nothing");
    // Backspace returns to the list: the same task, the same scroll.
    assert_eq!(desk.route(key(KeyCode::Backspace), WIDE), Route::Repaint);
    let (title, body) = object(&mut desk, WIDE, false);
    assert!(!title.contains("task second"), "{title}");
    assert!(picked(&body, false).is_some_and(|(_, r)| r.contains("second")));
    assert_eq!(desk.focus.scroll, scroll);
    assert_eq!(
        desk.route(key(KeyCode::Backspace), WIDE),
        Route::Nothing,
        "no detail is open"
    );
    assert_eq!(desk.route(key(KeyCode::Up), WIDE), Route::Repaint);
    let (_, body) = object(&mut desk, WIDE, false);
    assert!(picked(&body, false).is_some_and(|(_, r)| r.contains("first")));
}

#[test]
fn the_keys_keep_their_other_roles_and_the_hint_promises_only_the_available() {
    let mut desk = desk(two_tasks());
    let (_, body) = object(&mut desk, WIDE, false);
    assert!(
        body.iter()
            .any(|r| r.contains("↑↓ pick") && r.contains("Enter details")),
        "{body:#?}"
    );
    let (_, ascii) = object(&mut desk, WIDE, true);
    assert!(
        ascii
            .iter()
            .any(|r| r.contains("Up/Down pick") && r.contains("Enter details")),
        "{ascii:#?}"
    );
    assert!(picked(&ascii, true).is_some_and(|(_, r)| r.contains("first")));
    // The page keys still scroll; Esc still returns to the composer and the
    // detail it leaves is found again.
    assert_eq!(desk.route(key(KeyCode::Enter), SMALL), Route::Repaint);
    assert_eq!(desk.route(key(KeyCode::Esc), SMALL), Route::Repaint);
    assert_eq!(desk.focus.region, Region::Conversation);
    assert_eq!(desk.route(key(KeyCode::Backspace), SMALL), Route::Compose);
    let (title, _) = object(&mut desk, SMALL, false);
    assert!(title.contains("task first"), "{title}");
    // A leg that kept no task lists none and promises nothing.
    let mut kept = Desk::new();
    kept.view = Some(demo_project());
    let mut record = nika_session::KeptRun::new();
    record.workflow = Some("two.nika".to_owned());
    record.execution = Some(EXEC.to_owned());
    kept.kept(Some(Ok(record)));
    kept.focus.region = Region::Object;
    let (_, body) = object(&mut kept, WIDE, false);
    assert!(
        !body.iter().any(|r| r.contains("Enter details")),
        "{body:#?}"
    );
    assert!(
        body.iter().any(|r| r.contains("the record keeps no task")),
        "{body:#?}"
    );
    assert_eq!(kept.route(key(KeyCode::Enter), WIDE), Route::Nothing);
}

#[test]
fn the_bound_graph_orders_the_list_and_a_static_task_is_never_shown_run() {
    let mut desk = desk(vec![
        asked(Some(look(THREE)), false),
        start(EXEC, THREE),
        task(EXEC, 2, "task_scheduled", "first", &[]),
        task(EXEC, 3, "task_started", "first", &[]),
        task(
            EXEC,
            4,
            "task_completed",
            "first",
            &[field("duration_ms", "2")],
        ),
    ]);
    let (_, body) = object(&mut desk, WIDE, false);
    let (first, second, tidy) = (
        row(&body, "first"),
        row(&body, "second"),
        row(&body, "tidy"),
    );
    let at = |r: &String| list(&body).iter().position(|b| b == r).expect("listed");
    assert!(at(first) < at(second) && at(second) < at(tidy), "{body:#?}");
    assert!(first.contains("✔"), "{first}");
    assert!(second.contains("not observed"), "{second}");
    assert!(!STATES.iter().any(|g| second.contains(g)), "{second}");
    assert!(tidy.contains("cleanup"), "{tidy}");
    assert!(!STATES.iter().any(|g| tidy.contains(g)), "{tidy}");
    desk.route(key(KeyCode::Down), WIDE);
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
    let (_, detail) = object(&mut desk, WIDE, false);
    let text = detail.join("\n");
    assert!(text.contains("not observed in this run"), "{text}");
    assert!(
        text.contains("nika:log"),
        "the bound bytes' static facts: {text}"
    );
    assert!(!text.contains("succeeded"), "{text}");
    desk.route(key(KeyCode::Backspace), WIDE);
    desk.route(key(KeyCode::Down), WIDE);
    desk.route(key(KeyCode::Enter), WIDE);
    let (_, detail) = object(&mut desk, WIDE, false);
    let text = detail.join("\n");
    assert!(text.contains("cleanup") && text.contains("first"), "{text}");
}

#[test]
fn an_unbound_graph_lends_neither_its_order_nor_its_static_facts() {
    let observed = |exec| {
        vec![
            task(exec, 2, "task_scheduled", "zeta", &[]),
            task(exec, 3, "task_started", "first", &[]),
            task(
                exec,
                4,
                "task_completed",
                "first",
                &[field("duration_ms", "1")],
            ),
        ]
    };
    let mut other = vec![
        asked(Some(look(TWO)), false),
        start(EXEC, "nika: two\n# edited after the look\n"),
    ];
    other.extend(observed(EXEC));
    let mut unnamed = vec![asked(None, false), start(EXEC, TWO)];
    unnamed.extend(observed(EXEC));
    let mut unseen = vec![asked(Some(look(TWO)), false)];
    unseen.extend(observed(EXEC));
    for (case, seen) in [("other", other), ("unnamed", unnamed), ("unseen", unseen)] {
        let mut desk = desk(seen);
        let (_, body) = object(&mut desk, WIDE, false);
        // The order is the stream's: `zeta` was seen first.
        let at = |id| list(&body).iter().position(|b| b == row(&body, id));
        assert!(at("zeta") < at("first"), "{case}: {body:#?}");
        assert!(
            !list(&body).iter().any(|r| r.contains(" second")),
            "{case}: a task only the unbound graph names is not listed: {body:#?}"
        );
        desk.route(key(KeyCode::Down), WIDE);
        assert_eq!(
            desk.route(key(KeyCode::Enter), WIDE),
            Route::Repaint,
            "{case}"
        );
        let (title, detail) = object(&mut desk, WIDE, false);
        assert!(title.contains("task first"), "{case}: {title}");
        let text = detail.join("\n");
        assert!(text.contains("graph is not bound"), "{case}: {text}");
        assert!(
            !text.contains("nika:log"),
            "{case}: no static fact lent: {text}"
        );
        assert!(text.contains("1 ms"), "{case}: {text}");
    }
}

#[test]
fn an_output_absent_null_empty_masked_or_cut_reads_apart() {
    let big: Vec<u32> = (0..1500).collect();
    let big = serde_json::to_string(&big).expect("json");
    let completed = |n, id: &str, output: Option<&str>| {
        let extra: Vec<String> = output
            .map(|o| field("output", &quoted(o)))
            .into_iter()
            .collect();
        [
            task(EXEC, n, "task_started", id, &[]),
            task(EXEC, n + 1, "task_completed", id, &extra),
        ]
    };
    let mut seen = vec![asked(None, false), start(EXEC, TWO)];
    seen.extend(completed(2, "absent", None));
    seen.extend(completed(4, "nothing", Some("null")));
    seen.extend(completed(6, "blank", Some("\"\"")));
    seen.extend(completed(
        8,
        "secret",
        Some(r#"{"api_key":"sk-live-0123456789abcdef0123456789"}"#),
    ));
    seen.extend(completed(10, "large", Some(&big)));
    seen.push(task(EXEC, 12, "task_started", "broken", &[]));
    seen.push(task(
        EXEC,
        13,
        "task_failed",
        "broken",
        &[field("detail", &quoted("refused"))],
    ));
    seen.push(task(EXEC, 14, "task_started", "busy", &[]));
    let mut desk = desk(seen);
    let mut details = Vec::new();
    for at in 0..7 {
        if at > 0 {
            assert_eq!(desk.route(key(KeyCode::Down), WIDE), Route::Repaint);
        }
        assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
        let (title, detail) = object(&mut desk, WIDE, false);
        details.push((title, detail.join("\n")));
        assert_eq!(desk.route(key(KeyCode::Backspace), WIDE), Route::Repaint);
    }
    let said = |id: &str| {
        let found = details
            .iter()
            .find(|(t, _)| t.contains(&format!("task {id}")));
        found.map_or_else(|| panic!("{id}: {details:#?}"), |(_, d)| d.clone())
    };
    let absent = said("absent");
    assert!(absent.contains("no output on the stream"), "{absent}");
    assert!(!absent.contains("null"), "{absent}");
    assert!(
        said("nothing").contains("the value is null"),
        "{}",
        said("nothing")
    );
    let blank = said("blank");
    assert!(
        blank.contains("\"\"") && !blank.contains("the value is null"),
        "{blank}"
    );
    assert!(!blank.contains("no output on the stream"), "{blank}");
    let secret = said("secret");
    assert!(secret.contains("masked"), "the value is shown masked");
    assert!(!secret.contains("sk-live-0123456789abcdef"), "never shown");
    let large = said("large");
    assert!(
        large.contains("more follow"),
        "the cut is announced: {large}"
    );
    assert!(
        large.contains(&format!("{} bytes on the stream", big.len())),
        "{large}"
    );
    let broken = said("broken");
    assert!(
        broken.contains("refused") && broken.contains("no output on the stream"),
        "{broken}"
    );
    let busy = said("busy");
    assert!(busy.contains("not finished"), "{busy}");
}

#[test]
fn a_new_leg_starts_its_own_pick_and_frames_keep_the_open_detail() {
    let mut seen = vec![
        asked(Some(look(TWO)), false),
        start(EXEC, TWO),
        task(EXEC, 2, "task_scheduled", "first", &[]),
        task(EXEC, 3, "task_scheduled", "second", &[]),
        task(EXEC, 4, "task_started", "first", &[]),
    ];
    let mut desk = desk(std::mem::take(&mut seen));
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
    desk.observe(
        [
            task(
                EXEC,
                5,
                "task_completed",
                "first",
                &[field("duration_ms", "4")],
            ),
            task(EXEC, 6, "task_started", "second", &[]),
        ]
        .into_iter(),
    );
    let (title, detail) = object(&mut desk, WIDE, false);
    assert!(title.contains("task first"), "{title}");
    assert!(
        detail.join("\n").contains("4 ms"),
        "the detail follows the fold"
    );
    desk.route(key(KeyCode::Backspace), WIDE);
    desk.route(key(KeyCode::Down), WIDE);
    let (_, body) = object(&mut desk, WIDE, false);
    assert!(picked(&body, false).is_some_and(|(_, r)| r.contains("second")));
    // A resume is a new leg with its own execution: the pick starts over.
    desk.observe(
        [
            asked(Some(look(TWO)), true),
            start(NEXT, TWO),
            task(NEXT, 2, "task_scheduled", "first", &[]),
            task(NEXT, 3, "task_scheduled", "second", &[]),
        ]
        .into_iter(),
    );
    let (title, body) = object(&mut desk, WIDE, false);
    assert!(!title.contains("task "), "{title}");
    assert!(picked(&body, false).is_some_and(|(_, r)| r.contains("first")));
}

#[test]
fn the_scroll_follows_a_moved_pick_and_leaves_the_page_keys_alone() {
    let mut seen = vec![asked(None, false), start(EXEC, TWO)];
    for n in 0..30 {
        let id = format!("step_{n:02}");
        seen.push(task(EXEC, 2 + n, "task_scheduled", &id, &[]));
    }
    let mut desk = desk(seen);
    let rows = usize::from(desk.extent(SMALL).expect("the workspace").object_rows);
    for n in 1..30 {
        assert_eq!(desk.route(key(KeyCode::Down), SMALL), Route::Repaint);
        let (_, body) = object(&mut desk, SMALL, false);
        let (at, row) = picked(&body, false).expect("picked");
        assert!(row.contains(&format!("step_{n:02}")), "{row}");
        let scroll = desk.focus.scroll;
        assert!(
            scroll <= at && at < scroll + rows,
            "{n}: line {at} out of {scroll}+{rows}"
        );
    }
    // The page keys scroll away from the pick, and it stays picked.
    assert_eq!(desk.route(key(KeyCode::Home), SMALL), Route::Repaint);
    let (_, body) = object(&mut desk, SMALL, false);
    assert_eq!(desk.focus.scroll, 0, "the page keys are not pulled back");
    assert!(picked(&body, false).is_some_and(|(_, r)| r.contains("step_29")));
    // A new width paints the list elsewhere: the pick comes back into view.
    let (_, narrow) = object(&mut desk, NARROW, false);
    let rows = usize::from(desk.extent(NARROW).expect("the workspace").object_rows);
    let (at, _) = picked(&narrow, false).expect("picked");
    let scroll = desk.focus.scroll;
    assert!(
        scroll <= at && at < scroll + rows,
        "line {at} out of {scroll}+{rows}"
    );
    let (title, _) = {
        desk.route(key(KeyCode::Enter), SMALL);
        object(&mut desk, SMALL, true)
    };
    assert!(
        title.contains("task step_29") && title.contains("Backspace"),
        "{title}"
    );
}
