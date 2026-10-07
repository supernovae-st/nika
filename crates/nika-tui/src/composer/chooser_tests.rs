// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The chooser's keyboard law, exact: what is selected, what the draft
//! holds, and that choosing never sends.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::chooser::{Chosen, Door, Entry};
use super::{Composer, ComposerAction};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn f6() -> KeyEvent {
    key(KeyCode::F(6))
}

/// The commands in the conversation's order, then one view key.
fn offered() -> Vec<Entry> {
    vec![
        Entry::command("/help", "Every command", "any time", "The card.", "help"),
        Entry::command(
            "/status",
            "Where you are",
            "any time",
            "Root, seat.",
            "model effort",
        ),
        Entry::command(
            "/show",
            "Exact bytes",
            "while a proposal waits",
            "Bytes.",
            "inspect",
        ),
        Entry::command(
            "/intelligence",
            "Choose the AI",
            "this session",
            "Choices.",
            "model",
        ),
        Entry::key(
            f6(),
            "F6",
            "Next panel",
            "view only",
            "Moves the keys.",
            "panel",
        ),
    ]
}

fn composer() -> Composer {
    let mut composer = Composer::new();
    composer.offer(offered());
    composer.set_focused(true);
    composer
}

fn type_text(composer: &mut Composer, text: &str) {
    for c in text.chars() {
        composer.handle(key(KeyCode::Char(c)));
    }
}

/// The names listed, the selected one, and whether the draft is it.
fn listed(composer: &Composer) -> Option<(Door, Vec<String>, String, bool)> {
    let listing = composer.listing()?;
    let names = listing.entries.iter().map(|e| e.name.clone()).collect();
    let selected = listing
        .current()
        .map(|e| e.name.clone())
        .unwrap_or_default();
    Some((listing.door, names, selected, listing.whole))
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_slash_lists_the_commands_and_the_arrows_move_without_editing() {
    let mut c = composer();
    type_text(&mut c, "/");
    let commands = names(&["/help", "/status", "/show", "/intelligence"]);
    assert_eq!(
        listed(&c),
        Some((Door::Slash, commands.clone(), "/help".into(), false)),
        "view keys never join the slash list"
    );
    assert_eq!(c.choose(key(KeyCode::Down)), Chosen::Read);
    assert_eq!(c.choose(key(KeyCode::Down)), Chosen::Read);
    assert_eq!(listed(&c).map(|l| l.2), Some("/show".to_owned()));
    assert_eq!(c.choose(key(KeyCode::Up)), Chosen::Read);
    assert_eq!(listed(&c).map(|l| l.2), Some("/status".to_owned()));
    // Up from the first wraps to the last; BackTab goes up too.
    c.choose(key(KeyCode::Up));
    c.choose(key(KeyCode::Up));
    assert_eq!(listed(&c).map(|l| l.2), Some("/intelligence".to_owned()));
    c.choose(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(listed(&c).map(|l| l.2), Some("/show".to_owned()));
    assert_eq!(c.text(), "/", "moving the selection never edits the draft");
}

#[test]
fn tab_and_enter_insert_the_selection_and_only_a_whole_command_is_sent() {
    let mut c = composer();
    type_text(&mut c, "/s");
    assert_eq!(
        listed(&c),
        Some((
            Door::Slash,
            names(&["/status", "/show"]),
            "/status".into(),
            false
        ))
    );
    c.choose(key(KeyCode::Down));
    // Enter on a partial name inserts the selection: nothing is sent.
    assert_eq!(c.choose(key(KeyCode::Enter)), Chosen::Read);
    assert_eq!(c.text(), "/show");
    assert_eq!(
        listed(&c),
        Some((Door::Slash, names(&["/show"]), "/show".into(), true))
    );
    // Enter on the whole command is the composer's own: it sends.
    assert_eq!(c.choose(key(KeyCode::Enter)), Chosen::Pass);
    assert_eq!(
        c.handle(key(KeyCode::Enter)),
        ComposerAction::Submit("/show".to_owned())
    );
    let mut t = composer();
    type_text(&mut t, "/INT");
    assert_eq!(t.choose(key(KeyCode::Tab)), Chosen::Read);
    assert_eq!(t.text(), "/intelligence", "case aside, Tab completes");
}

#[test]
fn esc_closes_the_slash_list_for_this_draft_and_typing_opens_it_again() {
    let mut c = composer();
    type_text(&mut c, "/st");
    assert_eq!(c.choose(key(KeyCode::Esc)), Chosen::Read);
    assert!(c.listing().is_none());
    assert_eq!(c.text(), "/st", "Esc keeps the draft");
    // Closed, the list passes every key: a second Esc takes its usual way.
    assert_eq!(c.choose(key(KeyCode::Esc)), Chosen::Pass);
    assert_eq!(c.choose(key(KeyCode::Down)), Chosen::Pass);
    type_text(&mut c, "a");
    assert_eq!(listed(&c).map(|l| l.1), Some(names(&["/status"])));
    // Tab on a closed list opens it again instead of a one-line dump.
    c.choose(key(KeyCode::Esc));
    assert_eq!(c.choose(key(KeyCode::Tab)), Chosen::Read);
    assert_eq!(listed(&c).map(|l| l.1), Some(names(&["/status"])));
}

#[test]
fn paths_words_other_regions_and_recalled_lines_open_no_list() {
    for text in [
        "/tmp/a.csv",
        "/notes.md",
        "/ status",
        "read /status",
        "/st\nx",
    ] {
        let mut c = composer();
        c.paste(text);
        assert!(c.listing().is_none(), "{text:?}");
        assert_eq!(c.choose(key(KeyCode::Down)), Chosen::Pass, "{text:?}");
    }
    let mut c = composer();
    type_text(&mut c, "/zzz");
    assert!(c.listing().is_none(), "nothing begins with it");
    let mut away = composer();
    away.set_focused(false);
    type_text(&mut away, "/");
    assert!(away.listing().is_none(), "the keys are in another region");
    // History walks through a sent command: Up recalls, never selects.
    let mut h = composer();
    type_text(&mut h, "/status");
    h.handle(key(KeyCode::Enter));
    type_text(&mut h, "older then newer");
    h.handle(key(KeyCode::Enter));
    type_text(&mut h, "draft");
    h.handle(key(KeyCode::Up));
    h.handle(key(KeyCode::Up));
    assert_eq!(h.text(), "/status");
    assert!(h.listing().is_none(), "a recalled line is not being typed");
    assert_eq!(h.choose(key(KeyCode::Down)), Chosen::Pass);
    h.handle(key(KeyCode::Down));
    h.handle(key(KeyCode::Down));
    assert_eq!(h.text(), "draft", "history restores the unsent draft");
}

#[test]
fn the_palette_searches_apart_and_esc_returns_the_exact_draft() {
    let mut c = composer();
    c.paste("read ./notes\nand digest them");
    c.toggle_palette();
    assert_eq!(
        listed(&c).map(|l| (l.0, l.1)),
        Some((
            Door::Palette,
            names(&["/help", "/status", "/show", "/intelligence", "F6"])
        ))
    );
    for c_ in "model".chars() {
        assert_eq!(c.choose(key(KeyCode::Char(c_))), Chosen::Read);
    }
    assert_eq!(
        listed(&c).map(|l| l.1),
        Some(names(&["/status", "/intelligence"]))
    );
    assert!(c.paste_query("\nAI"));
    assert_eq!(listed(&c).map(|l| l.1), Some(names(&["/intelligence"])));
    assert_eq!(c.choose(key(KeyCode::Backspace)), Chosen::Read);
    assert_eq!(
        c.listing().and_then(|l| l.query.map(str::to_owned)),
        Some("model A".into())
    );
    assert_eq!(c.choose(key(KeyCode::Esc)), Chosen::Read);
    assert!(!c.palette_open());
    assert_eq!(c.text(), "read ./notes\nand digest them");
    assert!(!c.paste_query("x"), "a closed palette takes no paste");
}

#[test]
fn choosing_in_the_palette_inserts_a_command_or_hands_back_a_key() {
    let mut c = composer();
    c.toggle_palette();
    type_query(&mut c, "inspect");
    assert_eq!(c.choose(key(KeyCode::Enter)), Chosen::Inserted);
    assert!(!c.palette_open());
    assert_eq!(c.text(), "/show", "inserted, never sent");
    assert_eq!(c.aside(), None, "an empty box sets nothing aside");
    c.toggle_palette();
    type_query(&mut c, "panel");
    assert_eq!(c.choose(key(KeyCode::Tab)), Chosen::Press(f6()));
    assert!(!c.palette_open());
    assert_eq!(c.text(), "/show", "a key chosen leaves the draft as it was");
    c.toggle_palette();
    type_query(&mut c, "nothing like it");
    assert_eq!(c.choose(key(KeyCode::Enter)), Chosen::Read);
    assert!(c.palette_open(), "nothing listed: the palette stays open");
    // Any other key closes it and goes its ordinary way.
    assert_eq!(c.choose(f6()), Chosen::Pass);
    assert!(!c.palette_open());
    // In an empty box, Tab opens the palette; Ctrl+O's toggle closes it.
    let mut e = composer();
    assert_eq!(e.choose(key(KeyCode::Tab)), Chosen::Read);
    assert!(e.palette_open());
    e.toggle_palette();
    assert!(!e.palette_open());
}

fn type_query(composer: &mut Composer, text: &str) {
    for c in text.chars() {
        composer.choose(key(KeyCode::Char(c)));
    }
}

#[test]
fn a_draft_set_aside_by_the_palette_returns_unsent() {
    let mut c = composer();
    c.paste("use the CSV export\nnot the JSON one");
    c.toggle_palette();
    type_query(&mut c, "status");
    c.choose(key(KeyCode::Enter));
    assert_eq!(c.text(), "/status");
    assert_eq!(c.aside(), Some("use the CSV export\nnot the JSON one"));
    // Clearing the box (a fresh question's discard) keeps it aside.
    c.clear();
    assert_eq!(c.aside(), Some("use the CSV export\nnot the JSON one"));
    c.paste("/status");
    assert_eq!(
        c.handle(key(KeyCode::Enter)),
        ComposerAction::Submit("/status".to_owned())
    );
    assert_eq!(c.text(), "use the CSV export\nnot the JSON one");
    assert_eq!(c.aside(), None);
    // Esc brings it back at once, the command dropped, nothing sent.
    c.toggle_palette();
    type_query(&mut c, "help");
    c.choose(key(KeyCode::Enter));
    assert_eq!(c.text(), "/help");
    assert_eq!(
        c.choose(key(KeyCode::Esc)),
        Chosen::Read,
        "slash list first"
    );
    assert_eq!(c.choose(key(KeyCode::Esc)), Chosen::Read, "then the aside");
    assert_eq!(c.text(), "use the CSV export\nnot the JSON one");
    // Taken as a correction, the line leaves and the aside returns too.
    c.toggle_palette();
    type_query(&mut c, "status");
    c.choose(key(KeyCode::Enter));
    assert_eq!(c.take(), "/status");
    assert_eq!(c.text(), "use the CSV export\nnot the JSON one");
}

#[test]
fn the_unsent_draft_behind_a_recalled_line_is_what_is_set_aside() {
    let mut c = composer();
    type_text(&mut c, "sent earlier");
    c.handle(key(KeyCode::Enter));
    type_text(&mut c, "my unsent words");
    c.handle(key(KeyCode::Up));
    assert_eq!(c.text(), "sent earlier");
    c.toggle_palette();
    type_query(&mut c, "status");
    c.choose(key(KeyCode::Enter));
    assert_eq!(c.aside(), Some("my unsent words"));
    // A second command over typed words keeps both, earliest first.
    c.clear();
    c.paste("more words");
    c.toggle_palette();
    type_query(&mut c, "help");
    c.choose(key(KeyCode::Enter));
    assert_eq!(c.aside(), Some("my unsent words\nmore words"));
}

#[test]
fn an_edit_from_elsewhere_closes_the_palette_before_touching_the_draft() {
    let mut c = composer();
    c.paste("yes");
    c.toggle_palette();
    type_query(&mut c, "st");
    // Typeahead replayed into the draft: the palette closes, the draft shows.
    c.handle(key(KeyCode::Char('!')));
    assert!(!c.palette_open());
    assert_eq!(c.text(), "yes!");
    c.toggle_palette();
    c.paste(" data");
    assert!(!c.palette_open());
    assert_eq!(c.text(), "yes! data");
}
