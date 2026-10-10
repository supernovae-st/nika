// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the chooser offers, in words: the slash commands the conversation
//! answers now ([`crate::model::Conversation::commands`], in its own order),
//! each with its effect, its scope and one sentence of help, then the view
//! keys of the presentation in use. Nothing else is listed: no command the
//! conversation does not name, no key the presentation does not route, and
//! no key that sends, answers, saves, runs or stops anything (`Enter`,
//! `Ctrl+C` keep their own place). A command these words do not know is
//! still listed, under its own name, with `/help` as its description.
//!
//! The words follow the Session's own help card: each command reads, asks
//! the first screen again, brings back what was kept, or closes the
//! session, and none of them answers what waits.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::composer::chooser::Entry;
use crate::model::{Kind, Presentation};
use crate::workspace::geometry::Layout;

/// A command's name, effect, scope, help and search words.
type Words = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

/// The Session's commands, in words.
const COMMANDS: &[Words] = &[
    (
        "/help",
        "What you can ask, and every command",
        "any time · reads only",
        "The Session's help card: work to describe, questions answered without AI, each command.",
        "help card commands keys questions",
    ),
    (
        "/status",
        "Where you are",
        "any time · reads only",
        "Project root, intelligence and where your context goes, authoring seat, any reasoning effort.",
        "status root project intelligence model seat context effort reasoning",
    ),
    (
        "/why",
        "What the waiting answer is for",
        "beside a question or a gate · reads only",
        "What the question or gate on screen decides and what it lets happen; it answers nothing.",
        "why explain question gate decision",
    ),
    (
        "/meaning",
        "What Nika kept of your request",
        "after a request · reads only",
        "Your request clause by clause, from the compiler's own ledger; a proposal keeps waiting.",
        "meaning understood request requirements clauses ledger",
    ),
    (
        "/proof",
        "What the last run's trace proves",
        "after a run · reads only",
        "Chain, seal, boundary and task hashes as nika trace verify judges them, and their limits.",
        "proof trace verify run evidence seal hashes",
    ),
    (
        "/details",
        "How the last workflow was built",
        "after a build · reads only",
        "Authoring model, calls, tokens and time, any explicit reasoning effort, decision seat, engine.",
        "details model calls tokens time effort reasoning strategy decision engine spec provenance",
    ),
    (
        "/show",
        "The proposal's exact bytes",
        "while a proposal waits · reads only",
        "The exact bytes a yes would save; the proposal keeps waiting.",
        "show inspect bytes source proposal review candidate",
    ),
    (
        "/intelligence",
        "Choose the AI this session reasons with",
        "this session · your next line chooses",
        "Shows the intelligence choices again; the next line you send is your choice.",
        "intelligence model ai provider access seat choose change switch",
    ),
    (
        "/restore",
        "Bring back what your last session kept",
        "kept from your last session",
        "The kept preparation or proposal, for a fresh review; no AI asked, nothing written until yes.",
        "restore kept draft previous last session round",
    ),
    (
        "/quit",
        "Close the session",
        "this session",
        "Leaves Nika; nothing waiting for your answer is applied.",
        "quit exit leave close",
    ),
];

/// What the presentation in use routes, for its view keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct View {
    /// The presentation in effect.
    pub(crate) presentation: Presentation,
    /// The workspace fits the terminal (below it the focus view stands in).
    pub(crate) fits: bool,
    /// The transcript is scrolled back from its latest row.
    pub(crate) scrolled: bool,
    /// Whose whole words `F2` reads now; `None` when no block's are short
    /// of whole on screen, and the key is not offered.
    pub(crate) read: Option<Read>,
    /// What `F4` shows now: the object expanded (`Workbench`) or restored
    /// (`Session`); `None` where it would change nothing, and is not offered.
    pub(crate) object: Option<Layout>,
}

/// Whose whole words the reader opens ([`super::diagnostic`]), read only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Read {
    /// A summarized refusal or a shortened banner.
    Diagnostic,
    /// A run's story, folded.
    Run,
    /// The current proposal the cards review.
    Proposal,
    /// The typed question waiting at the answer line.
    Question,
}

impl Read {
    /// What the reader opens for a block of `kind`: a proposal's preview, a
    /// question's words, a run's story, else a summarized diagnostic.
    pub(crate) fn of(kind: Kind) -> Self {
        match kind {
            Kind::Proposal => Self::Proposal,
            Kind::Question => Self::Question,
            Kind::Run => Self::Run,
            _ => Self::Diagnostic,
        }
    }
}

/// Everything to choose from: the conversation's `commands`, then the view
/// keys of `view`.
pub(crate) fn offered(commands: &[String], view: View) -> Vec<Entry> {
    let mut entries: Vec<Entry> = commands.iter().map(|name| command(name)).collect();
    entries.extend(keys(view));
    entries
}

/// One command the conversation answers, in words.
fn command(name: &str) -> Entry {
    match COMMANDS.iter().find(|words| words.0 == name) {
        Some(&(name, effect, scope, help, words)) => {
            Entry::command(name, effect, scope, help, words)
        }
        None => Entry::command(
            name,
            "A command this conversation answers",
            "/help describes it",
            "Send it alone on a line; the Session's help card says what it does.",
            "",
        ),
    }
}

/// The key that shows the full diagnostic ([`super::diagnostic`]).
pub(crate) const DIAGNOSTIC_KEY: KeyCode = KeyCode::F(2);

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

/// The `F2` entry while a block's whole words are read there, read only: the
/// question waiting at the answer line, the current proposal the cards
/// review, a summarized refusal or a folded run's story, at the reading
/// position.
fn details(view: View) -> Option<Entry> {
    let (effect, scope, help, words) = match view.read? {
        Read::Question => (
            "Full question",
            "current question · read only",
            "The question at your answer line, in the Session's own words; nothing is sent.",
            "details question full words why reply answer",
        ),
        Read::Proposal => (
            "Full proposal",
            "current proposal · read only",
            "The current proposal in the Session's own words; nothing is sent, saved or run.",
            "details proposal preview review full words policy cost",
        ),
        Read::Diagnostic => (
            "Full diagnostic",
            "summarized refusal · read only",
            "The Session's own words of the summarized refusal at your reading position; nothing is sent.",
            "details diagnostic error refusal raw evidence words",
        ),
        Read::Run => (
            "Full run story",
            "folded run · read only",
            "The whole story of the folded run at your reading position; nothing is sent or run.",
            "details run story steps folded full words",
        ),
    };
    let pressed = press(DIAGNOSTIC_KEY, KeyModifiers::NONE);
    Some(Entry::key(pressed, "F2", effect, scope, help, words))
}

/// The view keys `view` routes: none sends, answers or grants anything.
fn keys(view: View) -> Vec<Entry> {
    let full = view.presentation != Presentation::Inline;
    let workspace = view.presentation == Presentation::Workspace && view.fits;
    let none = KeyModifiers::NONE;
    let mut keys = Vec::new();
    if workspace {
        // The object expanded or restored, from every region (routed by the
        // desk), only where that changes it.
        let words = view.object.map(|next| {
            if next == Layout::Workbench {
                (
                    "Expand the object",
                    "Gives the object more room; the conversation keeps its composer and your draft; nothing is sent.",
                )
            } else {
                (
                    "Restore the object",
                    "Gives the conversation its room back; the object and your draft stay; nothing is sent.",
                )
            }
        });
        if let Some((effect, help)) = words {
            keys.push(Entry::key(
                press(KeyCode::F(4), none),
                "F4",
                effect,
                "workspace · view only",
                help,
                "expand restore object larger room layout arrange view",
            ));
        }
        keys.push(Entry::key(
            press(KeyCode::F(6), none),
            "F6",
            "Next panel",
            "workspace · view only",
            "Moves the keys to the project list, the conversation or the preview; Shift+F6 goes back.",
            "panel focus region project list files preview object aside inspect open workflow",
        ));
    }
    if full {
        keys.push(Entry::key(
            press(KeyCode::PageUp, none),
            "PgUp",
            "Earlier messages",
            "full screen · view only",
            "Moves the conversation a page back, PgDn a page forward; new messages keep your place.",
            "scroll page older earlier history transcript pgdn",
        ));
        if view.scrolled {
            keys.push(Entry::key(
                press(KeyCode::End, none),
                "End",
                "Back to the latest message",
                "when scrolled back · view only",
                "Returns the conversation to its newest row.",
                "latest live bottom newest end",
            ));
        }
    }
    keys.extend(details(view));
    let (effect, help) = if full {
        (
            "Back inline",
            "Returns to the inline view, whose finished lines stay in your scrollback; the draft stays.",
        )
    } else {
        (
            "Full screen",
            "Opens the workspace, or the focus view on a small terminal; the draft stays.",
        )
    };
    keys.push(Entry::key(
        press(KeyCode::Char('t'), KeyModifiers::CONTROL),
        "Ctrl+T",
        effect,
        "the draft stays · view only",
        help,
        "full screen inline workspace focus presentation switch",
    ));
    keys.push(Entry::key(
        press(KeyCode::Char('l'), KeyModifiers::CONTROL),
        "Ctrl+L",
        "Redraw the screen",
        "view only",
        "Paints every cell again, for a terminal that lost part of the screen.",
        "redraw repaint refresh",
    ));
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::chooser::Act;

    fn view(presentation: Presentation) -> View {
        View {
            presentation,
            fits: true,
            scrolled: false,
            read: None,
            object: Some(Layout::Workbench),
        }
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    /// Every command the Session answers has its words, and only those (plus
    /// the kept-session `/restore` it adds while something was kept): a new
    /// Session command fails here until it is described.
    #[test]
    fn the_words_cover_exactly_the_session_commands() {
        let mut session: Vec<&str> = nika_session::runtime::SLASH_COMMANDS.to_vec();
        session.push("/restore");
        let mut described: Vec<&str> = COMMANDS.iter().map(|words| words.0).collect();
        session.sort_unstable();
        described.sort_unstable();
        assert_eq!(described, session);
        // The renderer gives `·` its ASCII twin; every other glyph is ASCII.
        for words in COMMANDS {
            for text in [words.1, words.2, words.3] {
                assert!(text.chars().all(|c| c.is_ascii() || c == '·'), "{text}");
                assert!(text.chars().count() <= 100, "{text}");
            }
        }
    }

    /// The list is the conversation's, in its order; keys follow, and none
    /// of them is `Enter` or `Ctrl+C`.
    #[test]
    fn the_conversation_commands_lead_in_their_own_order() {
        let commands = ["/status", "/help", "/future"].map(str::to_owned);
        let entries = offered(&commands, view(Presentation::Workspace));
        assert_eq!(
            names(&entries),
            [
                "/status", "/help", "/future", "F4", "F6", "PgUp", "Ctrl+T", "Ctrl+L"
            ]
        );
        assert_eq!(entries[0].act, Act::Insert("/status".to_owned()));
        assert_eq!(entries[2].effect, "A command this conversation answers");
        for entry in &entries {
            if let Act::Press(key) = entry.act {
                assert_ne!(key.code, KeyCode::Enter, "{}", entry.name);
                let ctrl_c =
                    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
                assert!(!ctrl_c, "{}", entry.name);
            }
        }
        assert!(
            offered(&[], view(Presentation::Workspace))
                .iter()
                .all(|entry| matches!(entry.act, Act::Press(_)))
        );
    }

    /// Only keys the presentation routes: inline has no panel or page keys,
    /// the focus view (and a workspace too small to fit) no layout or panel
    /// key, `End` only when scrolled back, `F2` only while a summarized
    /// diagnostic exists.
    #[test]
    fn view_keys_follow_what_the_presentation_routes() {
        assert_eq!(
            names(&keys(view(Presentation::Inline))),
            ["Ctrl+T", "Ctrl+L"]
        );
        assert_eq!(
            names(&keys(view(Presentation::Focus))),
            ["PgUp", "Ctrl+T", "Ctrl+L"]
        );
        let small = View {
            fits: false,
            ..view(Presentation::Workspace)
        };
        assert_eq!(names(&keys(small)), ["PgUp", "Ctrl+T", "Ctrl+L"]);
        let busy_reading = View {
            scrolled: true,
            read: Some(Read::Diagnostic),
            ..view(Presentation::Workspace)
        };
        assert_eq!(
            names(&keys(busy_reading)),
            ["F4", "F6", "PgUp", "End", "F2", "Ctrl+T", "Ctrl+L"]
        );
        let inline = keys(view(Presentation::Inline));
        assert_eq!(inline[0].effect, "Full screen");
        assert_eq!(keys(view(Presentation::Focus))[1].effect, "Back inline");
    }

    /// `F2` names the words it opens, read only: the question waiting at the
    /// answer line, the current proposal the cards review, a summarized
    /// refusal or a folded run's story at the reading position; no entry while
    /// no block's words are read there.
    #[test]
    fn the_f2_entry_names_the_words_it_opens() {
        for (read, effect, scope) in [
            (
                Read::Question,
                "Full question",
                "current question · read only",
            ),
            (
                Read::Proposal,
                "Full proposal",
                "current proposal · read only",
            ),
            (
                Read::Diagnostic,
                "Full diagnostic",
                "summarized refusal · read only",
            ),
            (Read::Run, "Full run story", "folded run · read only"),
        ] {
            let entries = keys(View {
                read: Some(read),
                ..view(Presentation::Workspace)
            });
            let f2: Vec<&Entry> = entries.iter().filter(|entry| entry.name == "F2").collect();
            assert_eq!(f2.len(), 1, "{effect}");
            assert_eq!(f2[0].effect, effect);
            assert_eq!(f2[0].scope, scope);
            assert!(f2[0].help.contains("nothing is sent"), "{}", f2[0].help);
        }
        let silent = keys(view(Presentation::Workspace));
        assert!(silent.iter().all(|entry| entry.name != "F2"));
        assert_eq!(Read::of(Kind::Question), Read::Question);
        assert_eq!(Read::of(Kind::Proposal), Read::Proposal);
        for kind in [Kind::Refusal, Kind::Banner, Kind::Reply] {
            assert_eq!(Read::of(kind), Read::Diagnostic, "{kind:?}");
        }
    }

    /// `F4` names what it does now, view only: `Expand the object` where
    /// expanding enlarges it, `Restore the object` on an expansion, and no
    /// entry where it would change nothing (the object cannot grow there) or
    /// below the minimum; its words name the object and no layout mode, and
    /// choosing it presses `F4`. `F6` stays offered either way.
    #[test]
    fn the_f4_entry_names_what_it_does_now_and_only_where_it_acts() {
        for (object, effect) in [
            (Layout::Workbench, "Expand the object"),
            (Layout::Session, "Restore the object"),
        ] {
            let entries = keys(View {
                object: Some(object),
                ..view(Presentation::Workspace)
            });
            let f4: Vec<&Entry> = entries.iter().filter(|entry| entry.name == "F4").collect();
            assert_eq!(f4.len(), 1, "{effect}");
            let f4 = f4[0];
            let pressed = KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE);
            assert_eq!(f4.act, Act::Press(pressed));
            assert_eq!(f4.effect, effect);
            assert_eq!(f4.scope, "workspace · view only");
            for text in [&f4.effect, &f4.scope, &f4.help, &f4.words] {
                let lower = text.to_lowercase();
                assert!(
                    !lower.contains("session") && !lower.contains("workbench"),
                    "{text}"
                );
            }
            assert!(f4.help.contains("nothing is sent"), "{}", f4.help);
        }
        let unchanged = keys(View {
            object: None,
            ..view(Presentation::Workspace)
        });
        assert!(unchanged.iter().all(|entry| entry.name != "F4"));
        assert!(unchanged.iter().any(|entry| entry.name == "F6"));
        let small = View {
            fits: false,
            ..view(Presentation::Workspace)
        };
        assert!(keys(small).iter().all(|entry| entry.name != "F4"));
    }
}
