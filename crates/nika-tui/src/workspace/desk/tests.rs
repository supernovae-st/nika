// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The desk's key routing, looks, candidates and run legs, proved on the
//! crate-private state (moved beside the desk to keep its file small).

use crossterm::event::KeyModifiers;

use super::*;
use crate::model::demo_project;
use crate::workspace::aside::Tab;
use crate::workspace::object::Object;
use nika_display::run_story::RunFrame;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn demo() -> Desk {
    let mut desk = Desk::new();
    desk.view = Some(demo_project());
    desk
}

const WIDE: (u16, u16) = (120, 40);
const SMALL: (u16, u16) = (80, 24);
const TINY: (u16, u16) = (59, 20);

/// The composer's region: Esc leaves, the page keys scroll the
/// transcript, Tab and the rest are the composer's.
#[test]
fn the_composer_region_leaves_on_esc_and_scrolls_the_transcript() {
    let mut desk = demo();
    assert_eq!(desk.route(key(KeyCode::Esc), WIDE), Route::Leave);
    assert_eq!(desk.route(key(KeyCode::PageUp), WIDE), Route::Older);
    assert_eq!(desk.route(key(KeyCode::PageDown), WIDE), Route::Newer);
    for code in [
        KeyCode::Tab,
        KeyCode::Char('y'),
        KeyCode::Enter,
        KeyCode::Up,
    ] {
        assert_eq!(desk.route(key(code), WIDE), Route::Compose, "{code:?}");
    }
}

/// Esc in another region returns to the composer first; a second Esc
/// leaves. The page keys scroll the object when it has the keys.
#[test]
fn esc_climbs_the_ladder_one_region_at_a_time() {
    let mut desk = demo();
    assert_eq!(
        desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE),
        Route::Repaint
    );
    assert_eq!(desk.focus.region, Region::Aside);
    assert_eq!(desk.route(key(KeyCode::Char('x')), WIDE), Route::Nothing);
    assert_eq!(desk.route(key(KeyCode::Esc), WIDE), Route::Repaint);
    assert_eq!(desk.focus.region, Region::Conversation);
    assert_eq!(desk.route(key(KeyCode::Esc), WIDE), Route::Leave);
    // At 80 columns the aside is folded, yet reachable: Shift+F6 reaches it
    // (drawn over the object), then the object.
    desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), SMALL);
    assert_eq!(desk.focus.region, Region::Aside);
    desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), SMALL);
    assert_eq!(desk.focus.region, Region::Object);
    assert_eq!(desk.route(key(KeyCode::PageUp), SMALL), Route::Nothing);
}

/// Enter on a workflow opens it as the object and attaches nothing; Enter
/// on this conversation gives the keys back to its composer.
#[test]
fn the_aside_opens_a_workflow_without_attaching_it() {
    let mut desk = demo();
    assert!(desk.welcoming());
    desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
    desk.route(key(KeyCode::Down), WIDE);
    desk.route(key(KeyCode::Down), WIDE);
    assert_eq!(
        desk.route(key(KeyCode::Enter), WIDE),
        Route::Inspect,
        "a workflow asks its Session for a look"
    );
    assert_eq!(
        desk.opened,
        Some(Target::Workflow("enrich.nika".to_owned()))
    );
    assert!(!desk.welcoming());
    let screen = desk.screen(false);
    assert!(matches!(&screen.object, Object::Shown { name, .. } if name == "enrich"));
    assert_eq!(screen.thread.on_screen.as_deref(), Some("enrich.nika"));
    assert!(screen.thread.attached.is_empty());
    assert!(screen.aside.entries[2].open);
    assert_eq!(desk.focus.region, Region::Aside, "opening keeps the aside");
    desk.route(key(KeyCode::Home), WIDE);
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
    assert_eq!(desk.focus.region, Region::Conversation);
    assert_eq!(
        desk.opened,
        Some(Target::Workflow("enrich.nika".to_owned())),
        "the object stays"
    );
}

/// A pinned run is the aside's last entry; Enter opens it as the object,
/// and once the view no longer pins it the welcome returns.
#[test]
fn the_pinned_run_opens_as_the_object_while_it_is_pinned() {
    use crate::workspace::pinned::Pinned;
    use nika_display::state::TaskState;
    let mut desk = demo();
    let run = Pinned::new(
        "demo",
        "digest-notes.nika",
        "#1",
        TaskState::Paused,
        "waiting",
    );
    desk.view = desk.view.take().map(|view| view.pinning(run));
    desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
    desk.route(key(KeyCode::End), WIDE);
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
    assert_eq!(desk.opened, Some(Target::Run("#1".to_owned())));
    let screen = desk.screen(false);
    assert!(matches!(&screen.object, Object::Shown { name, .. } if name == "#1 digest-notes.nika"));
    assert_eq!(
        screen.thread.on_screen.as_deref(),
        Some("#1 digest-notes.nika")
    );
    assert!(screen.aside.entries.last().is_some_and(|e| e.open));
    assert!(screen.pinned.is_some());
    desk.view = Some(demo_project());
    assert!(
        desk.welcoming(),
        "the run settled: nothing claims it on screen"
    );
}

/// The Files projection lists nothing and says why; Enter opens nothing.
#[test]
fn the_files_projection_opens_nothing() {
    let mut desk = demo();
    desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
    assert_eq!(desk.route(key(KeyCode::Right), WIDE), Route::Repaint);
    assert_eq!(desk.focus.tab, Tab::Files);
    assert!(desk.screen(false).aside.entries.is_empty());
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Nothing);
    assert!(desk.welcoming());
}

/// Below the minimum the focus view stands in: keys act as in the
/// composer's region and the kept focus returns with the workspace.
#[test]
fn below_the_minimum_the_focus_view_keys_apply_and_the_focus_is_kept() {
    let mut desk = demo();
    desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
    assert_eq!(desk.focus.region, Region::Aside);
    assert_eq!(desk.extent(TINY), None);
    assert_eq!(desk.route(key(KeyCode::F(6)), TINY), Route::Compose);
    assert_eq!(desk.route(key(KeyCode::PageUp), TINY), Route::Older);
    assert_eq!(desk.route(key(KeyCode::Esc), TINY), Route::Leave);
    assert_eq!(desk.focus.region, Region::Aside, "kept for the return");
    desk.enter();
    assert_eq!(desk.focus.region, Region::Conversation);
}

/// A workflow the next view no longer lists falls back to the welcome,
/// and the thread stops naming it as on screen.
#[test]
fn a_workflow_the_view_no_longer_lists_is_not_claimed_on_screen() {
    let mut desk = demo();
    desk.opened = Some(Target::Workflow("gone.nika".to_owned()));
    assert!(desk.welcoming());
    let screen = desk.screen(false);
    assert_eq!(screen.thread.on_screen, None);
    assert!(matches!(screen.object, Object::Welcome { .. }));
}

/// A look taken of another file than the opened one is never shown; the
/// opened workflow shows its own look, face by face, from the cache the
/// shell prepares before the frame.
#[test]
fn a_look_is_shown_only_for_the_workflow_it_was_taken_of() {
    let mut desk = demo();
    desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
    let other = refused("release.nika", "aaaa");
    desk.took("release.nika", Some(other));
    assert!(desk.look.is_none(), "not the opened workflow");
    let look = refused("enrich.nika", "bbbbbbbbbbbbcccc");
    desk.took("enrich.nika", Some(look));
    assert!(desk.look.is_some());
    // Not prepared yet: the listing's facts stand in, never a stale face.
    assert!(matches!(desk.screen(false).object, Object::Shown { .. }));
    desk.prepare(WIDE, true, false);
    let Object::Workflow { title, body } = desk.screen(false).object else {
        panic!("the look is in view");
    };
    assert!(title.to_string().contains("[source]"), "{title}");
    assert!(body.iter().any(|l| l.to_string().contains("bbbbbbbbbbbb")));
    // The face turns in the object region; the cache follows it.
    desk.focus.region = Region::Object;
    assert_eq!(desk.route(key(KeyCode::Right), WIDE), Route::Repaint);
    assert_eq!(desk.face, Face::Plan);
    desk.prepare(WIDE, true, false);
    let Object::Workflow { title, .. } = desk.screen(false).object else {
        panic!("the look is in view");
    };
    assert!(title.to_string().contains("[plan]"), "{title}");
    assert_eq!(desk.route(key(KeyCode::Left), WIDE), Route::Repaint);
    assert_eq!(desk.face, Face::Source);
    assert_eq!(desk.route(key(KeyCode::Left), WIDE), Route::Repaint);
    assert_eq!(desk.face, Face::Check);
    assert_eq!(desk.route(key(KeyCode::Char('r')), WIDE), Route::Inspect);
}

/// New bytes of the opened workflow replace the look and the cache: the
/// frame never paints the old face for the new witness.
#[test]
fn new_bytes_replace_the_look_and_its_rendering() {
    let mut desk = demo();
    desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
    desk.took("enrich.nika", Some(refused("enrich.nika", "111111111111")));
    desk.prepare(WIDE, true, false);
    desk.took("enrich.nika", Some(refused("enrich.nika", "222222222222")));
    let shown = desk.screen(false).object;
    assert!(
        matches!(shown, Object::Shown { .. }),
        "the old rendering is not lent to the new bytes"
    );
    desk.prepare(WIDE, true, false);
    let Object::Workflow { body, .. } = desk.screen(false).object else {
        panic!("the new look is in view");
    };
    let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
    assert!(rows.iter().any(|r| r.contains("222222222222")), "{rows:?}");
    assert!(!rows.iter().any(|r| r.contains("111111111111")), "{rows:?}");
}

/// Opening another workflow drops the previous look and starts at its
/// source; no face turns while no look is in view.
#[test]
fn opening_another_workflow_drops_the_previous_look() {
    let mut desk = demo();
    desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
    desk.route(key(KeyCode::Down), WIDE);
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Inspect);
    desk.took("release.nika", Some(refused("release.nika", "abc")));
    desk.face = Face::Graph;
    desk.route(key(KeyCode::Down), WIDE);
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Inspect);
    assert!(desk.look.is_none());
    assert_eq!(desk.face, Face::Source);
    desk.focus.region = Region::Object;
    assert_eq!(desk.route(key(KeyCode::Right), WIDE), Route::Nothing);
}

/// Two unread looks of the same path share every key the cache reads:
/// the second one's reason is shown, never the first one's.
#[test]
fn a_new_unread_look_is_rendered_anew() {
    let mut desk = demo();
    desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
    desk.took(
        "enrich.nika",
        Some(Inspected::unread("enrich.nika", "not found")),
    );
    desk.prepare(WIDE, true, false);
    let reason = "larger than 1048576 bytes";
    desk.took(
        "enrich.nika",
        Some(Inspected::unread("enrich.nika", reason)),
    );
    desk.prepare(WIDE, true, false);
    let Object::Workflow { body, .. } = desk.screen(false).object else {
        panic!("the look is in view");
    };
    let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
    assert!(rows.iter().any(|r| r.contains(reason)), "{rows:?}");
    assert!(!rows.iter().any(|r| r.contains("not found")), "{rows:?}");
}

/// The face is rendered for the width the frame will have: a smaller
/// terminal renders it again, never paints the wider lines.
#[test]
fn the_face_is_rendered_again_for_a_new_width() {
    let mut desk = demo();
    desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
    desk.took("enrich.nika", Some(refused("enrich.nika", "abcdef")));
    desk.prepare((160, 48), true, false);
    let wide = desk.drawn.as_ref().map(|d| d.key.4);
    desk.prepare((60, 18), true, false);
    let narrow = desk.drawn.as_ref().map(|d| d.key.4);
    // The face is rendered for its body: beside the conversation the object's
    // 69 columns less the one of air after the separator; stacked at 60
    // columns, the whole region.
    assert_eq!((wide, narrow), (Some(68), Some(60)));
    let body = |size| desk.geometry(size).map(|g| screen::object_body(&g).width);
    assert_eq!((wide, narrow), (body((160, 48)), body((60, 18))));
    let Object::Workflow { title, body } = desk.screen(false).object else {
        panic!("the look is in view");
    };
    for line in std::iter::once(&title).chain(body.iter()) {
        assert!(line.width() <= 60, "{line}");
    }
}

/// Where the width folds the aside, the object it opens takes the keys,
/// so it is the object that shows, not the aside over it.
#[test]
fn opening_from_a_folded_aside_shows_the_object() {
    let mut desk = demo();
    desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), SMALL);
    assert_eq!(desk.focus.region, Region::Aside);
    desk.route(key(KeyCode::Down), SMALL);
    assert_eq!(desk.route(key(KeyCode::Enter), SMALL), Route::Inspect);
    assert_eq!(desk.focus.region, Region::Object);
    let mut wide = demo();
    wide.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
    wide.route(key(KeyCode::Down), WIDE);
    wide.route(key(KeyCode::Enter), WIDE);
    assert_eq!(
        wide.focus.region,
        Region::Aside,
        "a shown aside keeps the keys"
    );
}

/// A candidate `preview` names, over unjudged bytes landing at `path`.
fn candidate(preview: &str, path: &str, aside: bool) -> Proposed {
    let look = Inspected::unjudged(path, format!("{preview}-bytes"), "nika: x\n".to_owned());
    Proposed::new(nika_session::ProposalId::of(preview), aside, look)
}

/// A proposal becomes the object in view, listed under this conversation;
/// a revision (a new identity) replaces it in the face already in view,
/// and the same candidate lent again changes nothing.
#[test]
fn a_new_candidate_becomes_the_object_and_a_revision_replaces_it() {
    let mut desk = demo();
    desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
    desk.proposed(Some(candidate("A", "compiled-workflow.nika", false)));
    assert_eq!(desk.opened, Some(Target::Candidate));
    assert_eq!(desk.face, Face::Source);
    let screen = desk.screen(false);
    assert_eq!(
        screen.aside.entries[1].label,
        "proposal compiled-workflow.nika"
    );
    assert!(screen.aside.entries[1].open);
    assert_eq!(
        screen.thread.on_screen.as_deref(),
        Some("proposal compiled-workflow.nika")
    );
    desk.prepare(WIDE, false, false);
    let Object::Workflow { body, .. } = desk.screen(false).object else {
        panic!("the candidate's face is in view");
    };
    let a = nika_session::ProposalId::of("A").to_string();
    assert!(body.iter().any(|l| l.to_string().contains(&a)));
    // The face turns on a candidate; a revision keeps it and shows B only.
    desk.focus.region = Region::Object;
    assert_eq!(desk.route(key(KeyCode::Right), WIDE), Route::Repaint);
    assert_eq!(desk.route(key(KeyCode::Char('r')), WIDE), Route::Nothing);
    desk.proposed(Some(candidate("B", "compiled-workflow.nika", false)));
    assert_eq!(desk.face, Face::Plan, "the face in view stays");
    desk.prepare(WIDE, false, false);
    let Object::Workflow { body, .. } = desk.screen(false).object else {
        panic!("B is in view");
    };
    let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
    let b = nika_session::ProposalId::of("B").to_string();
    assert!(rows.iter().any(|r| r.contains(&b)), "{rows:?}");
    assert!(!rows.iter().any(|r| r.contains(&a)), "A is gone: {rows:?}");
    let drawn = desk.drawn.clone();
    desk.proposed(Some(candidate("B", "compiled-workflow.nika", false)));
    assert_eq!(desk.drawn, drawn, "the same candidate changes nothing");
}

/// A candidate that leaves takes the object with it: the workflow the view
/// now lists at its path is opened (and looked at), else the welcome.
#[test]
fn the_candidate_in_view_leaves_for_the_saved_workflow_or_the_welcome() {
    let mut desk = demo();
    desk.proposed(Some(candidate("A", "release.nika", false)));
    desk.proposed(None);
    assert_eq!(
        desk.opened,
        Some(Target::Workflow("release.nika".to_owned()))
    );
    assert!(desk.wants_look, "the saved file is looked at anew");
    let mut desk = demo();
    desk.proposed(Some(candidate("A", "compiled-workflow.nika", false)));
    desk.proposed(None);
    assert!(desk.welcoming(), "a discarded create leaves the welcome");
    assert!(!desk.wants_look);
    let mut desk = demo();
    desk.proposed(Some(candidate("A", "compiled-workflow.nika", false)));
    desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
    desk.proposed(None);
    assert_eq!(
        desk.opened,
        Some(Target::Workflow("enrich.nika".to_owned())),
        "another object in view stays"
    );
}

/// A run asked becomes the object in view, an aside entry and the pinned
/// row, with no identity before its first frame; a second request (a
/// resume) is a new leg and the first one is kept as history.
#[test]
fn an_asked_run_is_the_object_and_the_pinned_row() {
    let mut desk = demo();
    let asked = |resume| Observed::Asked {
        workflow: "release.nika".to_owned(),
        resume,
        typed: true,
        look: None,
        world: None,
    };
    assert!(!desk.observe(std::iter::empty()));
    assert!(desk.observe(std::iter::once(asked(false))));
    assert_eq!(desk.opened, Some(Target::Live));
    let screen = desk.screen(false);
    let pinned = screen.pinned.as_ref().expect("the run is pinned");
    assert_eq!(
        (pinned.run.as_str(), pinned.workflow.as_str()),
        ("run (starting)", "release.nika")
    );
    assert!(
        screen
            .aside
            .entries
            .iter()
            .any(|e| e.label == "run (starting) release.nika" && e.open)
    );
    desk.prepare(WIDE, false, false);
    let Object::Workflow { title, body } = desk.screen(false).object else {
        panic!("the run is in view");
    };
    assert!(title.to_string().contains("run (starting)"), "{title}");
    assert!(
        body.iter()
            .any(|l| l.to_string().contains("no run identity yet"))
    );
    desk.observe(std::iter::once(asked(true)));
    assert_eq!(desk.past.len(), 1, "the first leg is kept as history");
    desk.lost(2, 0);
    desk.prepare(WIDE, false, false);
    let Object::Workflow { body, .. } = desk.screen(false).object else {
        panic!("the resumed leg is in view");
    };
    let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
    assert!(rows.iter().any(|r| r.contains("a resumed leg")), "{rows:?}");
    assert!(rows.join(" ").contains("2 lost on the way"), "{rows:?}");
}

/// The turn's result arrives after the child already queued its last
/// frames: closing the turn folds every queued frame first, so the
/// settlement already sent settles the leg, and the losses are recorded.
#[test]
fn closing_a_turn_folds_the_settlement_already_queued() {
    use nika_display::run_story::RunFrame;
    let mut desk = demo();
    let (tx, rx) = std::sync::mpsc::sync_channel(8);
    let gap = Gap::default();
    let frames = [
        r#"{"correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"workflow","value":"release"}],"id":{"uuid":"01a0ef11-03a1-73d9-a2bc-2548bdab1943"},"kind":"workflow_started","run":null,"timestamp":1}"#,
        r#"{"kind":"run_settled","status":"succeeded","cause":"normal","execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"spend":{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0},"evidence":"none"}"#,
    ];
    tx.send(Observed::Asked {
        workflow: "release.nika".to_owned(),
        resume: false,
        typed: true,
        look: None,
        world: None,
    })
    .expect("queued");
    for frame in frames {
        tx.send(Observed::Frame(RunFrame::decode(frame).expect("a frame")))
            .expect("queued");
    }
    drop(tx);
    desk.close_turn(&rx, &gap);
    let leg = desk.live.as_ref().expect("the leg");
    assert_eq!(
        leg.reported(),
        Some(nika_display::run_story::RunState::Succeeded)
    );
    assert!(leg.whole(), "nothing lost, settled");
}

/// The same candidate identity folded again with other facts is
/// rendered anew where it is: it never takes the object back from another
/// view the human opened. A new identity does.
#[test]
fn the_same_identity_with_other_facts_never_steals_the_view() {
    let mut desk = demo();
    desk.proposed(Some(candidate("A", "compiled-workflow.nika", false)));
    desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
    let renewed =
        candidate("A", "compiled-workflow.nika", false).rehearsed(Some("rehearsed".into()));
    desk.proposed(Some(renewed.clone()));
    assert_eq!(
        desk.opened,
        Some(Target::Workflow("enrich.nika".to_owned()))
    );
    assert_eq!(
        desk.candidate.as_ref(),
        Some(&renewed),
        "the facts are kept"
    );
    desk.proposed(Some(candidate("B", "compiled-workflow.nika", false)));
    assert_eq!(desk.opened, Some(Target::Candidate), "a new identity shows");
}

/// A conversation that counts what the desk asks it to acquire.
#[derive(Default)]
struct Lender {
    fetched: usize,
    proved: usize,
}

impl Conversation for Lender {
    fn open(&mut self) -> Vec<crate::model::Beat> {
        Vec::new()
    }
    fn submit(&mut self, _line: &str) -> crate::model::Turn {
        crate::model::Turn {
            beats: Vec::new(),
            handoff: None,
        }
    }
    fn perform(&mut self, _handoff: &crate::model::Handoff) -> Vec<crate::model::Beat> {
        Vec::new()
    }
    fn fetch(&mut self, _execution: &ExecutionId, path: &str) -> Option<Fetched> {
        self.fetched += 1;
        Some(Fetched::refused(path, "lent"))
    }
    fn prove(&mut self, _execution: &ExecutionId) -> Option<Proven> {
        self.proved += 1;
        Some(Proven::refused("", "lent"))
    }
}

/// A run's face turns in the object region; what it needs is acquired
/// once, by the conversation, never while preparing the frame; `r` reads
/// it again.
#[test]
fn a_run_face_is_acquired_outside_the_frame_and_read_again_on_demand() {
    let exec = r#""execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"}"#;
    let frame = |n: u32, kind: &str, fields: &str| {
        let line = format!(
            r#"{{"correlation":null,{exec},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
        );
        Observed::Frame(RunFrame::decode(&line).expect("frame"))
    };
    let settled = format!(
        r#"{{"kind":"run_settled","status":"succeeded","cause":"normal",{exec},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"unsealed"}}"#
    );
    let starting = r#"{"key":"task","value":"save"},{"key":"note","value":"invoke · nika:write"}"#;
    let ending = r#"{"key":"task","value":"save"},{"key":"output","value":"\"./out/copy.md\""}"#;
    let mut desk = demo();
    desk.observe(
        [
            Observed::Asked {
                workflow: "copy.nika".to_owned(),
                resume: false,
                typed: true,
                look: None,
                world: None,
            },
            frame(1, "workflow_started", ""),
            frame(2, "task_started", starting),
            frame(3, "task_completed", ending),
            Observed::Frame(RunFrame::decode(&settled).expect("settled")),
        ]
        .into_iter(),
    );
    desk.focus.region = Region::Object;
    let mut lender = Lender::default();
    assert_eq!(
        desk.route(key(KeyCode::Right), WIDE),
        Route::Repaint,
        "outputs: held"
    );
    assert_eq!(
        desk.route(key(KeyCode::Right), WIDE),
        Route::Inspect,
        "files: to read"
    );
    desk.prepare(WIDE, false, false);
    assert_eq!(
        (lender.fetched, lender.proved),
        (0, 0),
        "drawing reads nothing"
    );
    assert!(acquire(&mut desk, &mut lender));
    assert!(!acquire(&mut desk, &mut lender), "read once");
    assert_eq!(desk.route(key(KeyCode::Char('r')), WIDE), Route::Inspect);
    // A reading asked before `r` arrives late: dropped, asked again.
    let (execution, wants, stale) = desk.wanted().expect("read again");
    let late = acquire_all(&mut lender, &execution, wants);
    assert_eq!(desk.route(key(KeyCode::Char('r')), WIDE), Route::Inspect);
    assert!(
        !desk.acquired(execution, stale, late),
        "a late reading is dropped"
    );
    assert!(acquire(&mut desk, &mut lender));
    assert_eq!(
        desk.route(key(KeyCode::Right), WIDE),
        Route::Inspect,
        "proof: to verify"
    );
    assert!(acquire(&mut desk, &mut lender));
    assert_eq!((lender.fetched, lender.proved), (3, 1));
}

/// What the shell's worker does, inline: acquire what the face wants,
/// then hand it to the desk for the reading it was asked for.
fn acquire(desk: &mut Desk, lender: &mut Lender) -> bool {
    let Some((execution, wants, generation)) = desk.wanted() else {
        return false;
    };
    let got = acquire_all(lender, &execution, wants);
    desk.acquired(execution, generation, got)
}

/// A refused look of `path`, read with `witness`.
fn refused(path: &str, witness: &str) -> Inspected {
    Inspected::read(
        path,
        witness.to_owned(),
        "nika: x\nbogus: 1\n".to_owned(),
        Err(("NIKA-PARSE-005".to_owned(), "unknown key bogus".to_owned())),
    )
}
/// A run of thirty scheduled tasks, its last one picked, the object
/// holding the keys.
fn long_list() -> Desk {
    let exec = r#""execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"}"#;
    let frame = |n: u32, kind: &str, fields: &str| {
        let line = format!(
            r#"{{"correlation":null,{exec},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
        );
        Observed::Frame(RunFrame::decode(&line).expect("frame"))
    };
    let mut seen = vec![
        Observed::Asked {
            workflow: "long.nika".to_owned(),
            resume: false,
            typed: true,
            look: None,
            world: None,
        },
        frame(1, "workflow_started", ""),
    ];
    for n in 0..30 {
        let task = format!(r#"{{"key":"task","value":"step_{n:02}"}}"#);
        seen.push(frame(2 + n, "task_scheduled", &task));
    }
    let mut desk = demo();
    desk.observe(seen.into_iter());
    desk.focus.region = Region::Object;
    desk
}

/// The painted line of the picked task, and the object rows at `size`.
fn picked_line(desk: &mut Desk, size: (u16, u16)) -> (usize, usize) {
    desk.prepare(size, false, false);
    let Object::Workflow { body, .. } = desk.screen(false).object else {
        panic!("the run is in view");
    };
    let at = (body.iter().position(|l| l.to_string().starts_with("› "))).expect("a task is picked");
    let rows = usize::from(desk.extent(size).expect("the workspace").object_rows);
    (at, rows)
}

/// Only the height changes: the cached lines stay the same, the viewport
/// does not, and the picked task stays in view; a page key's scroll at
/// an unchanged size is never pulled back.
#[test]
fn a_height_only_resize_keeps_the_picked_task_in_view() {
    const TALL: (u16, u16) = (120, 40);
    const SHORT: (u16, u16) = (120, 24);
    let mut desk = long_list();
    for _ in 0..29 {
        assert_eq!(desk.route(key(KeyCode::Down), TALL), Route::Repaint);
        desk.prepare(TALL, false, false);
    }
    let (at, rows) = picked_line(&mut desk, TALL);
    let scroll = desk.focus.scroll;
    assert!(
        scroll <= at && at < scroll + rows,
        "{at} in {scroll}+{rows}"
    );
    let key_before = desk.drawn.as_ref().map(|d| d.key.clone());
    let (at, rows) = picked_line(&mut desk, SHORT);
    assert_eq!(
        desk.drawn.as_ref().map(|d| d.key.clone()),
        key_before,
        "the same rendering, kept"
    );
    let scroll = desk.focus.scroll;
    assert!(
        scroll <= at && at < scroll + rows,
        "{at} in {scroll}+{rows}"
    );
    assert_eq!(desk.route(key(KeyCode::Home), SHORT), Route::Repaint);
    let (_, _) = picked_line(&mut desk, SHORT);
    assert_eq!(desk.focus.scroll, 0, "the page keys are not pulled back");
    let (at, rows) = picked_line(&mut desk, TALL);
    let scroll = desk.focus.scroll;
    assert!(
        scroll <= at && at < scroll + rows,
        "back: {at} in {scroll}+{rows}"
    );
}

/// One run's observation, under its own execution: asked, started, one task done, settled.
fn observed_run(workflow: &str, uuid: &str, task: &str) -> Vec<Observed> {
    let exec = format!(r#""execution":{{"uuid":"{uuid}"}}"#);
    let frame = |n: u32, kind: &str, fields: &str| {
        let line = format!(
            r#"{{"correlation":null,{exec},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
        );
        Observed::Frame(RunFrame::decode(&line).expect("frame"))
    };
    let started = format!(
        r#"{{"key":"task","value":"{task}"}},{{"key":"note","value":"invoke · nika:write"}}"#
    );
    let ended = format!(
        r#"{{"key":"task","value":"{task}"}},{{"key":"output","value":"\"./out/{task}.md\""}}"#
    );
    let settled = format!(
        r#"{{"kind":"run_settled","status":"succeeded","cause":"normal",{exec},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"unsealed"}}"#
    );
    vec![
        Observed::Asked {
            workflow: workflow.to_owned(),
            resume: false,
            typed: true,
            look: None,
            world: None,
        },
        frame(1, "workflow_started", ""),
        frame(2, "task_started", &started),
        frame(3, "task_completed", &ended),
        Observed::Frame(RunFrame::decode(&settled).expect("settled")),
    ]
}

/// The second run used to replace the only inspectable result: the first leg was kept in
/// `past` but nothing could open it. It is now listed by its execution, opens as it was
/// observed (its own workflow and task), and the run in flight comes back unchanged.
#[test]
fn an_earlier_run_reopens_by_its_execution_and_the_current_one_returns() {
    let mut desk = demo();
    desk.observe(
        observed_run(
            "first.nika",
            "01a0ef11-0212-70de-a8b3-99de9427fcc1",
            "alpha",
        )
        .into_iter(),
    );
    let first = desk
        .live
        .as_ref()
        .and_then(LiveRun::execution)
        .expect("first bound");
    desk.observe(
        observed_run(
            "second.nika",
            "02b0ef11-0212-70de-a8b3-99de9427fcc2",
            "beta",
        )
        .into_iter(),
    );
    let second = desk
        .live
        .as_ref()
        .and_then(LiveRun::execution)
        .expect("second bound");
    assert_ne!(first, second);
    assert_eq!(desk.opened, Some(Target::Live));

    let screen = desk.screen(false);
    let index = (screen.aside.entries.iter())
        .position(|e| e.label.contains("first.nika") && e.label.contains("earlier"))
        .expect("the earlier run is listed");
    assert!(!screen.aside.entries[index].open, "listed, not in view yet");
    assert_eq!(desk.open(index), Route::Repaint);
    assert_eq!(desk.opened, Some(Target::Past(first)));
    desk.prepare(WIDE, false, false);
    let Object::Workflow { title, body } = desk.screen(false).object else {
        panic!("the earlier run is in view");
    };
    let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
    assert!(title.to_string().contains("01a0ef110212"), "{title}");
    assert!(rows.join(" ").contains("alpha"), "{rows:?}");
    assert!(!rows.join(" ").contains("beta"), "{rows:?}");
    assert_eq!(
        desk.live.as_ref().and_then(LiveRun::execution),
        Some(second),
        "opening an earlier run leaves the run in flight as it is"
    );

    let live = (desk.screen(false).aside.entries.iter())
        .position(|e| e.label.contains("second.nika") && !e.label.contains("earlier"))
        .expect("the current run is listed");
    assert_eq!(desk.open(live), Route::Repaint);
    assert_eq!(desk.opened, Some(Target::Live));
    desk.prepare(WIDE, false, false);
    let Object::Workflow { title, .. } = desk.screen(false).object else {
        panic!("the current run is in view");
    };
    assert!(title.to_string().contains("02b0ef110212"), "{title}");
}

/// The run object states where the asked bytes reach as the Session declared it, and a fresh run
/// of a workflow already run here names that earlier run: until a run can tell an effect already
/// done, it does every declared effect again (a warning when a service may be touched). A resumed
/// leg continues its own run and says nothing of the kind.
#[test]
fn a_repeated_run_names_the_earlier_one_beside_its_declared_reach() {
    let tasks = vec!["post".to_owned()];
    let world =
        nika_session::world::World::declared([("net.http", "127.0.0.1", tasks.as_slice())], []);
    let asked = |resume| Observed::Asked {
        workflow: "stock.nika".to_owned(),
        resume,
        typed: true,
        look: None,
        world: Some(Box::new(world.clone())),
    };
    let settled = |execution: &str| {
        let started = format!(
            r#"{{"correlation":null,"execution":{{"uuid":"{execution}"}},"fields":[{{"key":"workflow","value":"stock"}}],"id":{{"uuid":"01a0ef11-03a1-73d9-a2bc-2548bdab1943"}},"kind":"workflow_started","run":null,"timestamp":1}}"#
        );
        let done = format!(
            r#"{{"kind":"run_settled","status":"succeeded","cause":"normal","execution":{{"uuid":"{execution}"}},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"none"}}"#
        );
        [started, done].map(|line| Observed::Frame(RunFrame::decode(&line).expect("a frame")))
    };
    let rows = |desk: &mut Desk| {
        desk.prepare(WIDE, false, false);
        let Object::Workflow { body, .. } = desk.screen(false).object else {
            panic!("the run is in view");
        };
        body.iter().map(ToString::to_string).collect::<Vec<_>>()
    };
    let mut desk = demo();
    desk.observe(
        std::iter::once(asked(false)).chain(settled("01a0ef11-0212-70de-a8b3-99de9427fccc")),
    );
    let first = rows(&mut desk).join(" ");
    assert!(
        first.contains(
            "reaches, as declared · local services only: 127.0.0.1 · no connected service"
        ),
        "{first}"
    );
    assert!(
        !first.contains("again ·"),
        "a first run repeats nothing: {first}"
    );

    desk.observe(
        std::iter::once(asked(false)).chain(settled("01a0ef12-0212-70de-a8b3-99de9427fccc")),
    );
    let second = rows(&mut desk).join(" ");
    assert!(
        second.contains("again · after run 01a0ef110212 of this workflow (settled · succeeded)"),
        "{second}"
    );
    assert!(
        second.contains("a run does every effect it declares again"),
        "{second}"
    );

    desk.observe(std::iter::once(asked(true)));
    let resumed = rows(&mut desk).join(" ");
    assert!(
        !resumed.contains("again ·"),
        "a resumed leg continues its own run: {resumed}"
    );
}
