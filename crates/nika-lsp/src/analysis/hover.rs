// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hover documentation for the locked language vocabulary.
//!
//! The token under the cursor is matched against the [`vocab`]
//! tables — the 4 verbs, the top-level envelope keys, and the task-field
//! keys. A match yields a markdown [`Hover`] whose range is the token (so
//! the client highlights it), titled with the keyword and its category and
//! bodied with the one-line doc. A token that is not language vocabulary
//! (a task id, a string value, a provider) yields no hover — v0.1 documents
//! the LOCKED language surface only.
//!
//! Pure: `(text, offset) -> Option<Hover>`.

use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Range};

use super::position::LineIndex;
use super::vocab::{self, Entry};

/// Compute hover for the token under `offset`, if it names a verb or key.
#[must_use]
pub fn hover(text: &str, offset: usize) -> Option<Hover> {
    let (word, start, end) = word_at(text, offset)?;
    let (entry, category) = resolve(word)?;
    let index = LineIndex::new(text);
    let range = Range::new(index.position(start), index.position(end));
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: render(entry, category),
        }),
        range: Some(range),
    })
}

/// Resolve a word to its vocabulary entry + category label. Verbs win over
/// keys (a verb name is never also a top-level/task key).
fn resolve(word: &str) -> Option<(Entry, &'static str)> {
    if let Some(e) = vocab::lookup(vocab::VERBS, word) {
        return Some((e, "verb"));
    }
    if let Some(e) = vocab::lookup(vocab::TOP_LEVEL_KEYS, word) {
        return Some((e, "top-level key"));
    }
    if let Some(e) = vocab::lookup(vocab::TASK_FIELD_KEYS, word) {
        return Some((e, "task field"));
    }
    None
}

/// Render the markdown hover body.
fn render(entry: Entry, category: &str) -> String {
    format!("**`{}`** — _{}_\n\n{}", entry.name, category, entry.doc)
}

/// The identifier word at `offset` and its `[start, end)` byte range.
/// `None` when the cursor is not on an identifier char.
fn word_at(text: &str, offset: usize) -> Option<(&str, usize, usize)> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return None;
    }
    // Clamp into range; if the cursor sits just past an identifier (the
    // common « end of word » caret), step back one so the word is found.
    let probe = if offset >= len { len - 1 } else { offset };
    let probe = if !is_ident_byte(bytes.get(probe).copied())
        && probe > 0
        && is_ident_byte(bytes.get(probe - 1).copied())
    {
        probe - 1
    } else {
        probe
    };
    if !is_ident_byte(bytes.get(probe).copied()) {
        return None;
    }
    let mut start = probe;
    while start > 0 && is_ident_byte(bytes.get(start - 1).copied()) {
        start -= 1;
    }
    let mut end = probe + 1;
    while end < len && is_ident_byte(bytes.get(end).copied()) {
        end += 1;
    }
    text.get(start..end).map(|w| (w, start, end))
}

/// Whether a byte is part of an identifier (`[A-Za-z0-9_]`).
fn is_ident_byte(b: Option<u8>) -> bool {
    matches!(b, Some(c) if c == b'_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(h: &Hover) -> &str {
        match &h.contents {
            HoverContents::Markup(m) => &m.value,
            _ => "",
        }
    }

    #[test]
    fn hover_on_verb_returns_its_doc() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    infer: { prompt: \"hi\" }\n";
        let at = yaml.find("infer").expect("verb") + 2;
        let h = hover(yaml, at).expect("hover present");
        assert!(body(&h).contains("**`infer`**"), "{}", body(&h));
        assert!(body(&h).contains("single LLM call"));
        assert!(body(&h).contains("verb"));
    }

    #[test]
    fn hover_on_top_level_key() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: \"x\" }\n";
        let at = yaml.find("tasks").expect("key") + 1;
        let h = hover(yaml, at).expect("hover present");
        assert!(body(&h).contains("**`tasks`**"));
        assert!(body(&h).contains("top-level key"));
    }

    #[test]
    fn hover_on_task_field() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    depends_on: []\n    exec: { command: \"x\" }\n";
        let at = yaml.find("depends_on").expect("field") + 3;
        let h = hover(yaml, at).expect("hover present");
        assert!(body(&h).contains("**`depends_on`**"));
        assert!(body(&h).contains("task field"));
    }

    #[test]
    fn hover_carries_a_range_over_the_token() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    agent: { prompt: \"x\", tools: [\"nika:read\"] }\n";
        let agent_byte = yaml.find("agent").expect("verb");
        let h = hover(yaml, agent_byte).expect("hover");
        let index = LineIndex::new(yaml);
        let range = h.range.expect("range present");
        assert_eq!(range.start, index.position(agent_byte));
        assert_eq!(range.end, index.position(agent_byte + "agent".len()));
    }

    #[test]
    fn hover_on_non_vocabulary_returns_none() {
        // a task id is not language vocabulary → no hover
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: my_task\n    exec: { command: \"x\" }\n";
        let at = yaml.find("my_task").expect("id") + 2;
        assert!(hover(yaml, at).is_none());
    }

    #[test]
    fn hover_on_whitespace_returns_none() {
        let yaml = "nika: v1\n";
        // a space/newline position is not an identifier
        let at = yaml.find('\n').expect("nl");
        assert!(hover(yaml, at).is_none());
    }

    #[test]
    fn word_at_returns_exact_word_and_span_across_two_words() {
        // "ab cd": the space at byte 2 sits just PAST `ab`, so word_at steps
        // back ONE byte and returns `ab`, not `cd`. Every offset inside or
        // bounding a word maps to that exact word + its [start, end) span.
        let text = "ab cd";
        assert_eq!(word_at(text, 0), Some(("ab", 0, 2)), "start of ab");
        assert_eq!(word_at(text, 1), Some(("ab", 0, 2)), "inside ab");
        assert_eq!(
            word_at(text, 2),
            Some(("ab", 0, 2)),
            "the space just past `ab` steps back to ab, NOT forward to cd"
        );
        assert_eq!(word_at(text, 3), Some(("cd", 3, 5)), "start of cd");
        assert_eq!(word_at(text, 4), Some(("cd", 3, 5)), "inside cd");
        assert_eq!(word_at(text, 5), Some(("cd", 3, 5)), "end-of-text past cd");
    }

    #[test]
    fn word_at_handles_single_char_word_and_leading_space() {
        // A one-char word resolves at both its char and one-past (end init
        // `probe + 1` must not collapse the [0,1) span).
        assert_eq!(word_at("x", 0), Some(("x", 0, 1)));
        assert_eq!(word_at("x", 1), Some(("x", 0, 1)), "one past a 1-char word");
        // A leading space at byte 0 has no prior ident to step back to → None
        // (the `probe > 0` guard must hold; `>=` would underflow-probe).
        assert_eq!(word_at(" y", 0), None, "leading space resolves to no word");
        assert_eq!(word_at(" y", 1), Some(("y", 1, 2)), "the y after the space");
    }

    #[test]
    fn word_at_past_end_clamps_to_len_minus_one_then_steps_back() {
        // "ab " ends in a SPACE. An offset AT or PAST the end (>= len) clamps
        // to `len - 1` (the trailing space), then the step-back finds `ab`.
        // If the clamp uses `len` (e.g. `len / 1`) instead of `len - 1`, the
        // probe lands out of range, the step-back sees the space as the prior
        // byte, and the word is lost (None).
        let text = "ab ";
        assert_eq!(text.len(), 3, "two letters + a trailing space");
        assert_eq!(
            word_at(text, 3),
            Some(("ab", 0, 2)),
            "offset == len clamps to len-1 (the space) then steps back to ab"
        );
        assert_eq!(
            word_at(text, 99),
            Some(("ab", 0, 2)),
            "an offset far past the end behaves the same"
        );
    }

    #[test]
    fn word_at_does_not_overrun_the_word_on_the_right() {
        // The right-expansion `end < len` must stop exactly at the word end.
        // "exec," → the word is `exec` (0..4), the comma is excluded.
        assert_eq!(word_at("exec,", 0), Some(("exec", 0, 4)), "comma excluded");
        assert_eq!(
            word_at("exec,", 4),
            Some(("exec", 0, 4)),
            "caret on the comma"
        );
        // and the whole-string word reaches the true end (end==len boundary).
        assert_eq!(word_at("infer", 5), Some(("infer", 0, 5)), "end == len");
    }

    #[test]
    fn hover_just_past_word_still_resolves() {
        // caret at the byte right after `exec` — common end-of-word position
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: \"x\" }\n";
        let after = yaml.find("exec").expect("verb") + "exec".len();
        let h = hover(yaml, after).expect("hover");
        assert!(body(&h).contains("**`exec`**"));
    }
}
