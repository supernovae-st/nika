// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The words a model wrote beside its calls, as the person reads them: never its reasoning. A
//! model may stream its reasoning as text with no reasoning field (live gauntlet wave 7 r11,
//! 2026-10-19: minimax-m3 through `OpenRouter` wrote its English working notes, « Actually
//! wait… », turn ids and finding names, once ending in `</mm:think>`, in the text before each
//! call, and the question card showed them to a French person).

use crate::Lang;

/// What the person reads of `said`, the text beside a call, in a conversation spoken in `lang`:
/// what follows the last closing reasoning tag, up to an opening one, and nothing when it is
/// written in another language than the person's: a model's notes to itself, never Nika's words
/// to them.
pub(super) fn beside_a_call(said: &str, lang: Lang) -> String {
    let shown = without_reasoning(said);
    let shown = shown.trim();
    match Lang::of(shown) {
        Some(other) if other != lang => String::new(),
        _ => shown.to_owned(),
    }
}

/// Whether a tag's name is a reasoning tag: `think`, `thinking` or `reasoning`, namespaced or
/// not (`mm:think`).
fn reasoning(name: &str) -> bool {
    let name = name.rsplit(':').next().unwrap_or(name);
    ["think", "thinking", "reasoning"]
        .iter()
        .any(|tag| name.eq_ignore_ascii_case(tag))
}

/// `text` without its reasoning: what follows the last closing reasoning tag, whether its
/// opening tag was streamed or not, up to an opening tag left unclosed.
fn without_reasoning(text: &str) -> &str {
    let (mut start, mut from) = (0, 0);
    while let Some(at) = text[from..].find("</") {
        let at = from + at;
        let Some(end) = text[at..].find('>') else {
            break;
        };
        let closed = at + end + 1;
        if reasoning(&text[at + 2..at + end]) {
            start = closed;
        }
        from = closed;
    }
    unclosed(&text[start..])
}

/// `text` up to an opening reasoning tag that no closing one follows.
fn unclosed(text: &str) -> &str {
    let mut from = 0;
    while let Some(at) = text[from..].find('<') {
        let at = from + at;
        let Some(end) = text[at..].find('>') else {
            break;
        };
        if reasoning(&text[at + 1..at + end]) {
            return &text[..at];
        }
        from = at + end + 1;
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A French conversation reads no English working notes beside a call; its own language
    /// stays, and an English conversation keeps English words.
    #[test]
    fn notes_in_another_language_are_never_said() {
        let notes = "Actually wait, let me think about the findings first. The user wants the \
                     sites checked every hour and I should ask for the time zone.";
        assert_eq!(beside_a_call(notes, Lang::Fr), "");
        assert_eq!(beside_a_call(notes, Lang::En), notes);
        let words = "Il me manque une seule information : le fuseau horaire de vos vérifications.";
        assert_eq!(beside_a_call(words, Lang::Fr), words);
        // Too short to say a language: kept.
        assert_eq!(beside_a_call("OK.", Lang::Fr), "OK.");
    }

    /// Reasoning tags hide what they hold: up to a closing one whose opening tag was never
    /// streamed, a whole span, and an opening one left unclosed.
    #[test]
    fn reasoning_tags_hide_the_reasoning() {
        let leaked =
            "Let me think. u1 says hourly, finding F3 stands </mm:think>Quel fuseau horaire ?";
        assert_eq!(beside_a_call(leaked, Lang::Fr), "Quel fuseau horaire ?");
        let span = "<think>the user wants a digest</think>Voici la proposition.";
        assert_eq!(beside_a_call(span, Lang::Fr), "Voici la proposition.");
        let open = "Voici la question.<THINKING>now I should";
        assert_eq!(beside_a_call(open, Lang::Fr), "Voici la question.");
        let other = "Le résultat est en <b>gras</b> dans le fichier de la semaine.";
        assert_eq!(beside_a_call(other, Lang::Fr), other);
    }
}
