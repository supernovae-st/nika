// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rows a typed choice shows now. A settings list reads the own field as
//! its filter: every row whose names (its key, its label, its provider and
//! access words) hold a word starting with each typed term, case aside: first
//! the rows whose provider or access words hold them, then the others, each
//! provider's rows together where its first one stands, otherwise in the
//! offered order. Terms naming a row of the list itself by its provider or
//! access words (a connection) narrow it to such rows: the rows listed only
//! while a filter finds them (that connection's models) wait until it is
//! opened. A question, a decision or the knowledge choice never
//! filters: there the own field is the human's reply. A line starting with
//! `/` is a command sent as typed (`/intelligence 2 provider/model`, the
//! expert route), never a filter. Painting, the pointer and the keys read
//! the same rows; a selection keeps the offer's own index, so a row filtered
//! out is never the one `Enter` applies.

use super::{Answer, ScopeKind};
use crate::model::Offer;

impl Answer {
    /// The words a settings list filters by, lowercased; `None` where the
    /// own field is a reply, while it is empty, or for a command line.
    pub(super) fn terms(&self) -> Option<Vec<String>> {
        let scope = self.scope.as_ref()?;
        if scope.kind != ScopeKind::Settings {
            return None;
        }
        let typed = self.own.lines().join(" ");
        let typed = typed.trim();
        if typed.is_empty() || typed.starts_with('/') {
            return None;
        }
        Some(typed.split_whitespace().map(str::to_lowercase).collect())
    }

    /// The rows shown, by each offer's index in `offers`, in order, then a
    /// question's « Let Nika choose » row ([`delegation`]), then a multiple
    /// choice's Submit row (`offers.len()`).
    pub(super) fn rows(&self, offers: &[Offer], multiple: bool) -> Vec<usize> {
        let terms = self.terms();
        // The index built when the list opened, while it is this list's.
        let index = (self.scope.as_ref())
            .map(|scope| scope.haystacks.as_slice())
            .filter(|haystacks| haystacks.len() == offers.len());
        let mut rows: Vec<usize> = (offers.iter().enumerate())
            .filter(|(at, offer)| match (terms.as_deref(), index) {
                (Some(terms), Some(index)) => holds_all(&index[*at], terms),
                (Some(terms), None) => holds_all(&haystack(offer), terms),
                (None, _) => !offer.hidden(),
            })
            .map(|(at, _)| at)
            .collect();
        if let Some(terms) = terms.as_deref() {
            // Terms that name a connection narrow the list to it (live gauntlet 6, r11:
            // « openrouter » matched 376 of 376 rows; r02: « openai », 113 across providers).
            let names_a_row = |at: &usize| {
                (offers.get(*at)).is_some_and(|offer| !offer.hidden() && names(offer, terms))
            };
            if rows.iter().any(names_a_row) {
                rows.retain(|at| offers.get(*at).is_some_and(|offer| !offer.hidden()));
            }
            rows = providers_first(offers, rows, terms);
        }
        let asks = self.scope.as_ref().is_some_and(super::Scope::asks);
        rows.extend(delegation(asks, offers));
        if multiple {
            rows.push(offers.len());
        }
        rows
    }

    /// The filter changed: a settings list selects the row whose key is
    /// exactly what was typed, else its first match, else nothing. A command
    /// line selects nothing, and an emptied filter keeps the selection.
    pub(super) fn refilter(&mut self) {
        let Some(scope) = self.scope.as_ref() else {
            return;
        };
        if scope.kind != ScopeKind::Settings {
            return;
        }
        let typed = self.own.lines().join("\n");
        let typed = typed.trim();
        if typed.is_empty() {
            return;
        }
        let offers = &scope.offers;
        let rows = self.rows(offers, false);
        let exact = (offers.iter())
            .position(|offer| offer.key == typed)
            .filter(|index| rows.contains(index));
        let first = rows.first().copied().filter(|_| !typed.starts_with('/'));
        self.selected = exact.or(first);
    }
}

/// The one offer the author recommends, by index: `None` with none, or with
/// several (no single default stands for them).
pub(super) fn recommendation(offers: &[Offer]) -> Option<usize> {
    let mut recommended = (offers.iter().enumerate()).filter(|(_, offer)| offer.recommended);
    match (recommended.next(), recommended.next()) {
        (Some((at, _)), None) => Some(at),
        _ => None,
    }
}

/// The one offer `words` name, by index: its whole label, or the name its
/// row reads before « · », letter case and the spaces around aside. Words
/// naming no offer, or several, name none: they stay the person's words.
pub(crate) fn named<'a>(words: &str, offers: &'a [Offer]) -> Option<(usize, &'a Offer)> {
    let words = words.trim().to_lowercase();
    if words.is_empty() {
        return None;
    }
    let names = |offer: &Offer| {
        let label = offer.label.trim().to_lowercase();
        let name = (label.split_once(" · ")).map(|(name, _)| name.trim().to_owned());
        label == words || name.is_some_and(|name| name == words)
    };
    let mut found =
        (offers.iter().enumerate()).filter(|(_, offer)| !offer.hidden() && names(offer));
    match (found.next(), found.next()) {
        (Some(one), None) => Some(one),
        _ => None,
    }
}

/// The act of an approval gate a whole-line yes, no or why names: `oui`,
/// `yes`, `y`, `ok`, `valide`, `approuve` its `yes`; `non`, `no`, `n`,
/// `refuse` its `no`; `why` or `pourquoi` its `why?`. Letter case, a closing
/// mark and the spaces around aside; any other words name none.
pub(crate) fn yes_or_no<'a>(words: &str, offers: &'a [Offer]) -> Option<(usize, &'a Offer)> {
    let word = (words.trim())
        .trim_end_matches(['.', '!', '?', ' '])
        .to_lowercase();
    let key = match word.as_str() {
        "oui" | "yes" | "y" | "ok" | "valide" | "validé" | "approuve" | "approuvé" => "yes",
        "non" | "no" | "n" | "refuse" | "refusé" => "no",
        "why" | "pourquoi" => "why?",
        _ => return None,
    };
    (offers.iter().enumerate()).find(|(_, offer)| offer.key == key)
}

/// The « Let Nika choose » row of a question the author asked (`asks`) that
/// recommends an option, by the index past its offers and a multiple choice's
/// Submit row (`offers.len() + 1`).
pub(crate) fn delegation(asks: bool, offers: &[Offer]) -> Option<usize> {
    (asks && offers.iter().any(|offer| offer.recommended)).then_some(offers.len() + 1)
}

/// Whether `typed`, sent from a settings list, takes the expert route as
/// typed: a command line (`/intelligence 2 provider/model`), a pick alone or
/// with one name (`2`, `2 deepseek/deepseek-flash`, `4` alone), or a lone
/// `provider/model` name. Any other words matching no row are a message for
/// the conversation.
pub(crate) fn expert_route(typed: &str) -> bool {
    let mut words = typed.split_whitespace();
    match (words.next(), words.next(), words.next()) {
        (Some(first), ..) if crate::composer::chooser::command_word(first) => true,
        (Some("1" | "2" | "3"), _, None) | (Some("4"), None, None) => true,
        (Some(name), None, None) => name.contains('/'),
        _ => false,
    }
}

/// An offer's names (its key, its label, the provider and access words it is grouped by),
/// lowercased: what a typed filter searches, built once per list ([`super::Scope`]). Its note
/// (prices, marks, what it still needs) is never searched.
pub(super) fn haystack(offer: &Offer) -> String {
    let group = offer.group.as_deref().unwrap_or_default();
    format!("{} {} {group}", offer.key, offer.label).to_lowercase()
}

/// Whether every lowercased term starts a word of the lowercased `words` (live gauntlet 5, r04
/// B11: « xai » listed `MiniMax`, whose `minimaxai` holds it): `xai` finds `xai/grok-4.7` and
/// `xai-org/grok-2`, never `minimaxai/minimax-m1`. A word starts the text or follows a
/// character that is no letter or digit; a term that starts with none is found anywhere.
fn holds_all(words: &str, terms: &[String]) -> bool {
    terms.iter().all(|term| starts_a_word(words, term))
}

/// Whether `term` starts a word of `words` ([`holds_all`]).
fn starts_a_word(words: &str, term: &str) -> bool {
    if !term.chars().next().is_some_and(char::is_alphanumeric) {
        return words.contains(term);
    }
    let mut previous: Option<char> = None;
    for (at, c) in words.char_indices() {
        let starts = previous.is_none_or(|p| !p.is_alphanumeric());
        if starts && words.get(at..).is_some_and(|rest| rest.starts_with(term)) {
            return true;
        }
        previous = Some(c);
    }
    false
}

/// Whether `offer`'s provider and access words hold every term: the terms name its connection.
fn names(offer: &Offer, terms: &[String]) -> bool {
    (offer.group.as_deref()).is_some_and(|words| holds_all(&words.to_lowercase(), terms))
}

/// The rows `found` by `terms`, by index: first those whose provider or access words hold
/// every term (`xai` finds the xAI connection and its models before a Hugging Face model whose
/// name holds it), then the others, each provider's rows together where its first one stands.
/// A row naming no provider stays where it is among them.
fn providers_first(offers: &[Offer], found: Vec<usize>, terms: &[String]) -> Vec<usize> {
    let group = |at: usize| offers.get(at).and_then(|offer| offer.group.as_deref());
    let named = |at: &usize| offers.get(*at).is_some_and(|offer| names(offer, terms));
    let (first, rest): (Vec<usize>, Vec<usize>) = found.into_iter().partition(named);
    let mut rows = together(first, group);
    rows.extend(together(rest, group));
    rows
}

/// `rows` with each group's rows together, the groups in the order of their first row.
fn together<'a>(rows: Vec<usize>, group: impl Fn(usize) -> Option<&'a str>) -> Vec<usize> {
    let mut groups: Vec<(Option<&str>, Vec<usize>)> = Vec::new();
    for at in rows {
        let name = group(at);
        let same = (groups.iter_mut()).find(|(seen, _)| seen.is_some() && *seen == name);
        match same {
            Some((_, members)) => members.push(at),
            None => groups.push((name, vec![at])),
        }
    }
    groups
        .into_iter()
        .flat_map(|(_, members)| members)
        .collect()
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use crate::composer::Composer;
    use crate::composer::answer::Offered;
    use crate::model::{Asked, Offer, Shape, Waiting};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// A filter finds the rows whose provider or access words hold its terms first, then the
    /// others; each provider's rows stand together where its first one stands, and a row
    /// naming no provider keeps its place.
    #[test]
    fn a_filter_finds_the_provider_first_and_keeps_each_provider_together() {
        let offers = vec![
            Offer::new("/intelligence connections", "Other choice…").noted("every connection"),
            Offer::new("2 huggingface/xai-org/grok-2", "Grok 2 · Hugging Face API")
                .grouped("huggingface Hugging Face API"),
            Offer::new("2 xai/grok-4.7", "Grok 4.7 · xAI API").grouped("xai xAI API"),
            Offer::new("2 huggingface/xai-org/grok-1", "Grok 1 · Hugging Face API")
                .grouped("huggingface Hugging Face API"),
            Offer::new("/intelligence models 2 xai", "xai · API · 1 model").grouped("xai xAI API"),
        ];
        let listed = offers.clone();
        let waiting = Waiting::selection(Asked::new(
            "Choose",
            "",
            false,
            Shape::Choice(offers),
            "w",
            1,
        ));
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.follow(&waiting);
        composer.paste_own("xai");
        let found: Vec<&str> = (composer.offer_rows(&waiting).into_iter())
            .filter_map(|at| listed.get(at))
            .map(|offer| offer.key.as_str())
            .collect();
        assert_eq!(
            found,
            [
                "2 xai/grok-4.7",
                "/intelligence models 2 xai",
                "2 huggingface/xai-org/grok-2",
                "2 huggingface/xai-org/grok-1",
            ]
        );
    }

    /// Terms naming a connection narrow a connections list to it (live gauntlet 6, r11:
    /// « openrouter » matched 376 of 376 rows; r02: « openai », 113 across providers): the
    /// models listed only while a filter finds them wait until it is opened, other providers'
    /// models naming the term included; terms naming no connection still find the models they
    /// name, and the first row found is the one `Enter` applies.
    #[test]
    fn terms_naming_a_connection_narrow_the_list_to_it() {
        let connection = |name: &str, group: &str| {
            Offer::new(
                format!("/intelligence models 2 {name}"),
                format!("{name} · API · 40 models"),
            )
            .grouped(group)
        };
        let model = |key: &str, label: &str, group: &str| {
            Offer::new(key, label).grouped(group).found_only()
        };
        let (openai, openrouter) = ("openai OpenAI API", "openrouter OpenRouter API");
        let offers = vec![
            connection("openai", openai),
            connection("openrouter", openrouter),
            model("2 openai/gpt-6-astra", "GPT-6 Astra · OpenAI API", openai),
            model(
                "2 openrouter/openai/gpt-6-luna",
                "GPT-6 Luna · OpenRouter API",
                openrouter,
            ),
            model(
                "2 openrouter/anthropic/claude-sonnet-5-5",
                "Claude Sonnet 5.5 · OpenRouter API",
                openrouter,
            ),
        ];
        let listed = offers.clone();
        let waiting = Waiting::selection(Asked::new(
            "Choose an AI connection",
            "",
            false,
            Shape::Choice(offers),
            "8:intelligence",
            8,
        ));
        let found = |words: &str| {
            let mut composer = composer(&waiting);
            typed(&mut composer, words);
            let keys: Vec<String> = (composer.offer_rows(&waiting).into_iter())
                .filter_map(|at| listed.get(at))
                .map(|offer| offer.key.clone())
                .collect();
            (keys, composer.offer_selected(&waiting))
        };
        assert_eq!(
            found("openrouter"),
            (
                vec!["/intelligence models 2 openrouter".to_owned()],
                Some(1)
            )
        );
        assert_eq!(
            found("openai"),
            (vec!["/intelligence models 2 openai".to_owned()], Some(0))
        );
        assert_eq!(
            found("sonnet"),
            (
                vec!["2 openrouter/anthropic/claude-sonnet-5-5".to_owned()],
                Some(4)
            )
        );
        assert_eq!(
            found("luna"),
            (vec!["2 openrouter/openai/gpt-6-luna".to_owned()], Some(3))
        );
    }

    /// A filter finds what starts a word of a row's names, never letters inside a word (live
    /// gauntlet 5, r04 B11: « xai » listed `MiniMax`, whose Hugging Face organisation is
    /// `MiniMaxAI`), nor its note: `xai` finds the xAI rows and a model of the `xai-org`
    /// organisation, and « 5.5 » a model whose name holds that version.
    #[test]
    fn a_filter_finds_word_starts_in_names_never_letters_inside_a_word() {
        let offers = vec![
            Offer::new("2 xai/grok-4.7", "Grok 4.7 · xAI API").grouped("xai xAI API"),
            Offer::new(
                "2 huggingface/minimaxai/minimax-m1",
                "MiniMax M1 · Hugging Face API",
            )
            .grouped("huggingface Hugging Face API"),
            Offer::new("2 huggingface/xai-org/grok-2", "Grok 2 · Hugging Face API")
                .grouped("huggingface Hugging Face API"),
            Offer::new(
                "2 anthropic/claude-sonnet-5-5",
                "Claude Sonnet 5.5 · Anthropic API",
            )
            .noted("current · xai-compatible")
            .grouped("anthropic Anthropic API"),
        ];
        let listed = offers.clone();
        let waiting = Waiting::selection(Asked::new(
            "Choose",
            "",
            false,
            Shape::Choice(offers),
            "w",
            1,
        ));
        let found = |words: &str| {
            let mut composer = Composer::new();
            composer.set_focused(true);
            composer.follow(&waiting);
            composer.paste_own(words);
            (composer.offer_rows(&waiting).into_iter())
                .filter_map(|at| listed.get(at))
                .map(|offer| offer.key.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            found("xai"),
            ["2 xai/grok-4.7", "2 huggingface/xai-org/grok-2"],
            "no MiniMax, and no note"
        );
        assert_eq!(found("mini"), ["2 huggingface/minimaxai/minimax-m1"]);
        assert_eq!(found("5.5"), ["2 anthropic/claude-sonnet-5-5"]);
        assert_eq!(found("claude-son"), ["2 anthropic/claude-sonnet-5-5"]);
    }

    /// A catalogue page: several providers' models, one of them listed twice
    /// by name across two connections, and the row back.
    fn models() -> Waiting {
        let offers = [
            (
                "2 anthropic/claude-sonnet-5-5",
                "Claude Sonnet 5.5 · anthropic",
            ),
            ("2 deepseek/deepseek-flash", "DeepSeek Flash · deepseek"),
            ("2 openai/gpt-6.1-sol", "GPT-6.1 Sol · openai"),
            ("2 deepseek/deepseek-pro", "DeepSeek Pro · deepseek"),
            (
                "1 claude-code/claude-sonnet-5-5",
                "Claude Sonnet 5.5 · seat",
            ),
            ("/intelligence", "All connections"),
        ];
        let offers = offers.iter().map(|(k, l)| Offer::new(*k, *l)).collect();
        Waiting::selection(Asked::new(
            "Choose a model",
            "",
            false,
            Shape::Choice(offers),
            "5:intelligence",
            5,
        ))
    }

    fn composer(waiting: &Waiting) -> Composer {
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.follow(waiting);
        composer
    }

    fn typed(composer: &mut Composer, words: &str) {
        for c in words.chars() {
            assert!(composer.own_key(key(KeyCode::Char(c))), "{c}");
        }
    }

    /// Typed words narrow a settings list to the rows holding every term,
    /// case aside, in the offered order; the first match is selected and
    /// `Enter` applies it by its exact key.
    #[test]
    fn typed_words_filter_a_settings_list_and_enter_applies_the_first_match() {
        let waiting = models();
        let mut composer = composer(&waiting);
        assert_eq!(composer.offer_rows(&waiting), [0, 1, 2, 3, 4, 5]);
        typed(&mut composer, "SONNET");
        assert_eq!(composer.offer_rows(&waiting), [0, 4]);
        assert_eq!(composer.offer_filter(&waiting).as_deref(), Some("SONNET"));
        assert_eq!(composer.offer_selected(&waiting), Some(0), "first match");
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Down)),
            Offered::Moved
        );
        assert_eq!(composer.offer_selected(&waiting), Some(4), "the next match");
        for code in [KeyCode::Down, KeyCode::PageDown, KeyCode::End] {
            composer.offer_key(&waiting, key(code));
            assert_eq!(composer.offer_selected(&waiting), Some(4), "{code:?}");
        }
        assert!(!composer.select_offer(&waiting, 1), "a row filtered out");
        typed(&mut composer, " seat");
        assert_eq!(composer.offer_rows(&waiting), [4]);
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Enter)),
            Offered::Answer("1 claude-code/claude-sonnet-5-5".into())
        );
        assert!(!composer.offers_open(&waiting), "sent once");
    }

    /// The exact key typed is the row selected, ahead of an earlier match;
    /// words that match no row (an unlisted model) are sent as typed, and so
    /// is a command line, which never filters.
    #[test]
    fn an_exact_key_wins_and_unmatched_words_or_a_command_are_sent_as_typed() {
        let longer_first = Waiting::selection(Asked::new(
            "Choose a model",
            "",
            false,
            Shape::Choice(vec![
                Offer::new("2 p/model-a-long", "Model A long"),
                Offer::new("2 p/model-a", "Model A"),
            ]),
            "6:intelligence",
            6,
        ));
        let mut composer = composer(&longer_first);
        composer.paste_own("2 p/model-a");
        assert_eq!(composer.offer_rows(&longer_first), [0, 1], "both match");
        assert_eq!(
            composer.offer_selected(&longer_first),
            Some(1),
            "the exact key"
        );
        let waiting = models();
        let mut composer = self::composer(&waiting);
        composer.paste_own("2 deepseek/deepseek-v9");
        assert!(composer.offer_rows(&waiting).is_empty(), "no row matches");
        assert_eq!(composer.offer_selected(&waiting), None);
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Enter)),
            Offered::Answer("2 deepseek/deepseek-v9".into()),
            "the expert route: the exact words"
        );
        let mut composer = self::composer(&waiting);
        composer.offer_key(&waiting, key(KeyCode::Down));
        composer.paste_own("/intelligence 2 deepseek/deepseek-flash");
        assert_eq!(
            composer.offer_rows(&waiting).len(),
            6,
            "a command never filters"
        );
        assert_eq!(composer.offer_filter(&waiting), None);
        assert_eq!(composer.offer_selected(&waiting), None, "nor selects");
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Enter)),
            Offered::Answer("/intelligence 2 deepseek/deepseek-flash".into())
        );
    }

    /// `Esc` empties the filter first (every row back, the selection kept),
    /// then closes the list; a filter emptied by `Backspace` keeps it too.
    #[test]
    fn esc_or_backspace_empties_the_filter_and_keeps_the_selection() {
        let waiting = models();
        let mut composer = composer(&waiting);
        typed(&mut composer, "pro");
        assert_eq!(composer.offer_selected(&waiting), Some(3));
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Esc)),
            Offered::Moved
        );
        assert_eq!(composer.own_reply(), "");
        assert_eq!(composer.offer_rows(&waiting).len(), 6);
        assert_eq!(composer.offer_selected(&waiting), Some(3), "kept");
        typed(&mut composer, "gpt");
        assert_eq!(composer.offer_selected(&waiting), Some(2));
        for _ in 0..3 {
            assert!(composer.own_key(key(KeyCode::Backspace)));
        }
        assert_eq!(composer.offer_rows(&waiting).len(), 6);
        assert_eq!(composer.offer_selected(&waiting), Some(2), "kept");
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Esc)),
            Offered::Answer("cancel".into()),
            "nothing typed: one press closes, the current choice kept"
        );
    }

    /// An offer listed only while a filter finds it stays out of the list,
    /// the keys and a press until typed words find it; then it is a row like
    /// any other and `Enter` applies its own key.
    #[test]
    fn a_found_only_offer_appears_when_typed_words_find_it() {
        let offers = vec![
            Offer::new(
                "/intelligence models 2 anthropic",
                "anthropic · API · 6 models",
            ),
            Offer::new(
                "/intelligence models 2 deepseek",
                "deepseek · API · 3 models",
            ),
            Offer::new("2 anthropic/claude-sonnet-5-5", "Claude Sonnet 5.5").found_only(),
        ];
        let waiting = Waiting::selection(Asked::new(
            "Choose an AI connection",
            "",
            false,
            Shape::Choice(offers),
            "8:intelligence",
            8,
        ));
        let mut composer = composer(&waiting);
        assert_eq!(composer.offer_rows(&waiting), [0, 1], "out of the list");
        assert!(!composer.select_offer(&waiting, 2), "and out of reach");
        for _ in 0..3 {
            composer.offer_key(&waiting, key(KeyCode::Down));
        }
        assert_eq!(
            composer.offer_selected(&waiting),
            Some(1),
            "the keys stop before it"
        );
        typed(&mut composer, "son");
        assert_eq!(composer.offer_rows(&waiting), [2], "found by its name");
        assert_eq!(composer.offer_selected(&waiting), Some(2));
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Enter)),
            Offered::Answer("2 anthropic/claude-sonnet-5-5".into())
        );
    }

    /// The expert route is a command, a pick alone or with one name, or a
    /// lone `provider/model`; a sentence is never one, whatever its words.
    #[test]
    fn the_expert_route_is_a_pick_a_command_or_a_model_name_never_a_sentence() {
        for expert in [
            "2",
            " 2 deepseek/deepseek-flash ",
            "1 claude-code/claude-sonnet-5-5",
            "3 ollama",
            "4",
            "/intelligence 2 deepseek/deepseek-flash",
            "/help",
            "deepseek/deepseek-v9",
        ] {
            assert!(super::expert_route(expert), "{expert:?}");
        }
        for sentence in [
            "",
            "summarise my inbox every morning",
            "2 cats and a dog",
            "4 reports",
            "hello",
            "/notes.md please",
            "use deepseek/deepseek-flash please",
        ] {
            assert!(!super::expert_route(sentence), "{sentence:?}");
        }
    }

    /// Words naming exactly one offer of a decision or a question (its whole
    /// label or the name before « · », case and spaces around aside) pick it
    /// on `Enter`, as selecting it would; words naming none, part of one or
    /// several stay the person's words.
    #[test]
    fn an_offers_name_typed_beside_a_choice_picks_it() {
        let decision = Waiting::action(
            "save",
            Asked::new(
                "What should happen to this reviewed proposal?",
                "",
                true,
                Shape::Choice(vec![
                    Offer::new("yes", "Save workflow"),
                    Offer::new("no", "Discard this proposal"),
                    Offer::new("/show", "Inspect the exact changes"),
                ]),
                "action:1",
                1,
            ),
        );
        for (words, sent, picked) in [
            ("Inspect the exact changes", "/show", Some(2)),
            ("  inspect THE exact changes ", "/show", Some(2)),
            ("Inspect", "Inspect", None),
            ("save workflow please", "save workflow please", None),
        ] {
            let mut composer = composer(&decision);
            typed(&mut composer, words);
            assert_eq!(
                composer.offer_named(&decision),
                picked,
                "{words:?} as painted"
            );
            assert_eq!(
                composer.offer_key(&decision, key(KeyCode::Enter)),
                Offered::Answer(sent.into()),
                "{words:?}"
            );
            assert_eq!(composer.own_reply(), "", "{words:?} left the field");
            if picked.is_some() {
                assert_eq!(composer.offer_selected(&decision), picked);
            }
        }
        let sources = Waiting::asked(
            "source",
            Asked::new(
                "Which source?",
                "",
                true,
                Shape::Choice(vec![
                    Offer::new("hn", "Hacker News · news.ycombinator.com"),
                    Offer::new("tc", "TechCrunch · techcrunch.com/feed"),
                    Offer::new("tc2", "TechCrunch · techcrunch.com/startups"),
                ]),
                "q:1",
                1,
            ),
        );
        let mut composer = composer(&sources);
        typed(&mut composer, "hacker news");
        assert_eq!(
            composer.offer_key(&sources, key(KeyCode::Enter)),
            Offered::Answer("hn".into())
        );
        let mut composer = self::composer(&sources);
        typed(&mut composer, "TechCrunch");
        assert_eq!(composer.offer_named(&sources), None, "two offers");
        assert_eq!(
            composer.offer_key(&sources, key(KeyCode::Enter)),
            Offered::Answer("TechCrunch".into())
        );
    }

    /// At an approval gate a typed whole-line yes, no or why picks its act on
    /// `Enter`, its radio painted while it is typed; other words send nothing
    /// and say so (nothing answers the gate for the person), a command is sent
    /// as typed, and `Esc` empties the words, never answering no. A decision
    /// that takes no reply and is no approval gate takes no words.
    #[test]
    fn an_approval_gate_takes_a_typed_yes_or_no() {
        let gate = |key: &str| {
            let mut asked = Asked::new(
                "Answer this workflow confirmation",
                "",
                true,
                Shape::Choice(vec![
                    Offer::new("yes", "Approve this step"),
                    Offer::new("no", "Refuse this step"),
                    Offer::new("why?", "Explain this step"),
                ]),
                "action:gate",
                1,
            );
            asked.free_text = false;
            Waiting::action(key, asked)
        };
        let approval = gate("gate");
        for (words, sent, picked) in [
            ("oui", "yes", 0),
            ("Valide", "yes", 0),
            ("ok", "yes", 0),
            ("y", "yes", 0),
            ("approuve", "yes", 0),
            ("NON", "no", 1),
            ("refuse", "no", 1),
            ("n", "no", 1),
            ("pourquoi ?", "why?", 2),
        ] {
            let mut composer = composer(&approval);
            typed(&mut composer, words);
            assert_eq!(composer.offer_named(&approval), Some(picked), "{words:?}");
            assert_eq!(
                composer.offer_key(&approval, key(KeyCode::Enter)),
                Offered::Answer(sent.into()),
                "{words:?}"
            );
            assert_eq!(composer.own_reply(), "", "{words:?}");
        }
        let mut composer = composer(&approval);
        typed(&mut composer, "peut-être");
        assert_eq!(composer.offer_named(&approval), None);
        assert_eq!(
            composer.offer_key(&approval, key(KeyCode::Enter)),
            Offered::Notice(crate::composer::answer::YES_OR_NO)
        );
        assert_eq!(composer.own_reply(), "peut-être", "the words stay");
        assert!(composer.offers_armed(&approval), "nothing sent");
        assert_eq!(
            composer.offer_key(&approval, key(KeyCode::Esc)),
            Offered::Moved
        );
        assert_eq!(composer.own_reply(), "", "Esc empties the words");
        let esc = composer.offer_key(&approval, key(KeyCode::Esc));
        assert!(!matches!(esc, Offered::Answer(_)), "{esc:?}");
        let mut composer = self::composer(&approval);
        typed(&mut composer, "/why");
        assert_eq!(
            composer.offer_key(&approval, key(KeyCode::Enter)),
            Offered::Answer("/why".into())
        );
        let cost = gate("run_cost");
        let mut composer = self::composer(&cost);
        assert!(!composer.own_key(key(KeyCode::Char('o'))), "no words there");
    }

    /// Discard asks once before it is done: on the save decision the first
    /// `Enter` on « Discard this proposal » asks and sends nothing, the next
    /// `Enter` discards. Any other key drops the question, and `Esc` keeps the
    /// proposal, its row still selected. The act's typed name asks the same;
    /// Save is done at once.
    #[test]
    fn discard_asks_once_before_it_is_done() {
        let save = Waiting::action(
            "save",
            Asked::new(
                "What should happen to this reviewed proposal?",
                "",
                true,
                Shape::Choice(vec![
                    Offer::new("yes", "Save workflow"),
                    Offer::new("save & run", "Save and run once"),
                    Offer::new("no", "Discard this proposal"),
                    Offer::new("/show", "Inspect the exact changes"),
                ]),
                "action:1",
                1,
            ),
        );
        let question = "Discard this proposal? Enter again · Esc keeps it";
        let mut composer = composer(&save);
        for _ in 0..3 {
            composer.offer_key(&save, key(KeyCode::Down));
        }
        assert_eq!(composer.offer_selected(&save), Some(2));
        let enter = |composer: &mut Composer| composer.offer_key(&save, key(KeyCode::Enter));
        assert_eq!(enter(&mut composer), Offered::Notice(question));
        assert!(composer.offers_armed(&save), "nothing sent");
        assert_eq!(
            composer.offer_key(&save, key(KeyCode::Esc)),
            Offered::Notice("kept · nothing was discarded")
        );
        assert_eq!(composer.offer_selected(&save), Some(2), "its row kept");
        assert_eq!(
            enter(&mut composer),
            Offered::Notice(question),
            "asked again"
        );
        composer.offer_key(&save, key(KeyCode::Up));
        composer.offer_key(&save, key(KeyCode::Down));
        assert_eq!(
            enter(&mut composer),
            Offered::Notice(question),
            "a move drops it"
        );
        assert_eq!(enter(&mut composer), Offered::Answer("no".into()));
        let mut composer = self::composer(&save);
        typed(&mut composer, "Discard this proposal");
        assert_eq!(enter(&mut composer), Offered::Notice(question));
        assert_eq!(composer.offer_selected(&save), Some(2));
        assert_eq!(enter(&mut composer), Offered::Answer("no".into()));
        let mut composer = self::composer(&save);
        composer.offer_key(&save, key(KeyCode::Down));
        assert_eq!(
            enter(&mut composer),
            Offered::Answer("yes".into()),
            "Save at once"
        );
    }

    /// Only a settings list filters: under a question the own field is the
    /// human's reply, every offer stays listed and `Enter` sends the words.
    #[test]
    fn a_question_never_filters_its_offers() {
        let question = Waiting::asked(
            "const.currency",
            Asked::new(
                "Which currency?",
                "",
                true,
                Shape::Choice(vec![Offer::new("eur", "Euro"), Offer::new("usd", "Dollar")]),
                "w1",
                1,
            ),
        );
        let mut composer = composer(&question);
        // Words naming no offer (a name alone picks it: see above).
        typed(&mut composer, "in euros");
        assert_eq!(composer.offer_rows(&question), [0, 1]);
        assert_eq!(composer.offer_filter(&question), None);
        assert_eq!(
            composer.offer_selected(&question),
            Some(0),
            "the row it opened on: typing never chooses for you"
        );
        assert_eq!(
            composer.offer_key(&question, key(KeyCode::Enter)),
            Offered::Answer("in euros".into())
        );
    }
}
