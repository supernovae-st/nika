// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One intelligence option as a row a person reads: the model's display name,
//! then its marks and the facts the Session's inventory states of it (its
//! prices per million tokens, its context window, a key it still needs). Read
//! from the option's typed facts only; an option stating no model keeps the
//! Session's own words. The key sent is always the option's own.

use std::fmt::Write as _;

use nika_session::work::{self, ModelFacts};

use super::names;
use crate::lang::{Voice, fill};
use crate::model::Offer;

/// What the row of `offer` marks beside its facts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // four independent marks, each read on its own
pub(super) struct Marks {
    /// The model this conversation uses now.
    pub(super) current: bool,
    /// The model recommended for this conversation.
    pub(super) recommended: bool,
    /// The operator's saved default.
    pub(super) default: bool,
    /// The model asked for the next workflows' AI steps.
    pub(super) requested: bool,
}

impl Marks {
    /// The marks' words, in their order, in `voice`.
    fn words(self, voice: Voice) -> impl Iterator<Item = &'static str> {
        [
            (self.current, "current"),
            (self.recommended, "recommended"),
            (self.default, "default"),
            (self.requested, "requested"),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(move |(_, word)| voice.say(word))
    }
}

/// The facts an option states of its model, when it names one.
pub(super) fn facts(offer: &work::Offer) -> Option<&ModelFacts> {
    offer.values.iter().find_map(|value| value.choice.as_ref())
}

/// The row of `offer`: its model's display name and its facts, the picker's own words in
/// `voice`. Where the list does not say how it is reached (`route`: a list mixing connections,
/// a model found by its name), that access is part of its name, never only a quiet fact: its
/// provider's API, an agent account, or this machine.
pub(super) fn row(offer: &work::Offer, marks: Marks, route: bool, voice: Voice) -> Offer {
    let Some(facts) = facts(offer) else {
        // An option naming no model (an agent account's own) wears its marks all the same.
        let marked: Vec<&str> = marks.words(voice).collect();
        let row = Offer::new(&offer.key, &offer.label).noted(marked.join(" · "));
        return if marks.current { row.in_use() } else { row };
    };
    let name = names::display(&facts.model);
    let label = if route {
        format!("{name} · {}", access(facts, voice))
    } else {
        name
    };
    let current = marks.current;
    // A seat that writes no workflow says so on its model's row, as the Session's label does.
    let no_authoring = nika_session::authoring::NO_AUTHORING;
    let mut said = note(offer, facts, marks, false, voice);
    if offer.label.split(" · ").any(|part| part == no_authoring) {
        let words = voice.say(no_authoring);
        said = if said.is_empty() {
            words.to_owned()
        } else {
            format!("{words} · {said}")
        };
    }
    let row = Offer::new(&offer.key, label)
        .noted(said)
        .grouped(group(&facts.via, &facts.class));
    if current { row.in_use() } else { row }
}

/// The words of the provider `via` and its access `class` (`xai xAI API`): what a filter finds
/// a provider's rows by first, and what keeps them together.
pub(super) fn group(via: &str, class: &str) -> String {
    format!("{via} {}", access_of(via, class, Voice::default()))
}

/// Whether the model writes text, as an author writes a workflow and a Run step writes its
/// output: never one the catalogue prices for input alone (its output costs nothing: an
/// embedding), nor one whose name says it embeds or reranks.
pub(super) fn writes(facts: &ModelFacts) -> bool {
    let name = facts.model.rsplit('/').next().unwrap_or(&facts.model);
    let name = name.to_lowercase();
    let says = |word: &str| (name.split(['-', '_', '.', ' '])).any(|part| part.starts_with(word));
    let input_only = facts
        .output_usd_per_million
        .is_some_and(|price| price.0 <= 0.0)
        && facts
            .input_usd_per_million
            .is_some_and(|price| price.0 > 0.0);
    !(says("embed") || says("rerank") || input_only)
}

/// The marks, the route, the prices, the context window and what setup the
/// model still needs, in that order, in `voice`.
pub(super) fn note(
    offer: &work::Offer,
    facts: &ModelFacts,
    marks: Marks,
    route: bool,
    voice: Voice,
) -> String {
    let mut parts: Vec<String> = marks.words(voice).map(str::to_owned).collect();
    if facts.legacy {
        parts.push(voice.say("legacy").to_owned());
    }
    if route {
        parts.push(access(facts, voice));
    }
    parts.extend(price(facts, voice));
    if let Some(tokens) = facts.context_tokens {
        let tokens = tokens_word(tokens);
        parts.push(fill(
            voice.say("{tokens} context"),
            &[("{tokens}", &tokens)],
        ));
    }
    if !facts.configured {
        // The Session's own words for the setup (the variables it accepts).
        let setup = (offer.label.split_once("key required"))
            .map_or(String::new(), |(_, rest)| rest.to_owned());
        parts.push(format!("{}{setup}", voice.say("key required")));
    }
    parts.join(" · ")
}

/// How the model is reached, in a word or two, its kind named: an API, an
/// agent account, or this machine.
fn access(facts: &ModelFacts, voice: Voice) -> String {
    access_of(&facts.via, &facts.class, voice)
}

/// How the provider `via` is reached through the access `class`, in a word or two, in `voice`.
fn access_of(via: &str, class: &str, voice: Voice) -> String {
    let via = names::provider(via);
    let phrase = match class {
        "api" => "{via} API",
        "harness" => "{via} agent account",
        "local" => "{via} on this machine",
        other => return format!("{via} {other}"),
    };
    fill(voice.say(phrase), &[("{via}", &via)])
}

/// The prices per million tokens the facts state, or how the route bills
/// when none is stated; an unknown price is said, never zero. Prices that
/// hold only for shorter prompts are the lowest: `from`, with how far they
/// hold (a longer prompt bills at a higher rate).
fn price(facts: &ModelFacts, voice: Voice) -> Option<String> {
    let usd =
        |price: Option<nika_session::work::UsdPerMillion>| price.map(|price| dollars(price.0));
    let stated = match (
        usd(facts.input_usd_per_million),
        usd(facts.output_usd_per_million),
    ) {
        (Some(input), Some(output)) => Some(fill(
            voice.say("{in} in / {out} out per MTok"),
            &[("{in}", &input), ("{out}", &output)],
        )),
        (None, Some(output)) => Some(fill(voice.say("{out} out per MTok"), &[("{out}", &output)])),
        (Some(input), None) => Some(fill(voice.say("{in} in per MTok"), &[("{in}", &input)])),
        (None, None) => None,
    };
    if let Some(prices) = stated {
        return Some(match facts.prices_up_to_tokens {
            Some(tokens) => {
                let tokens = tokens_word(tokens);
                let from = voice.say("from {prices} (≤{tokens} tokens)");
                fill(from, &[("{prices}", &prices), ("{tokens}", &tokens)])
            }
            None => prices,
        });
    }
    let words = match facts.billing.as_str() {
        "included_quota" => "in your plan",
        "local" => "no metered cost",
        "api_metered" => "price unknown",
        _ => return None,
    };
    Some(voice.say(words).to_owned())
}

/// A price in dollars, exact to the cent (to a tenth of a cent below one
/// dollar), whole when it is: `$3`, `$0.30`, `$1.25`, `$0.035`.
fn dollars(price: f64) -> String {
    let places = if price >= 1.0 { 2 } else { 3 };
    let words = format!("{price:.places$}");
    let Some((whole, fraction)) = words.split_once('.') else {
        return format!("${words}");
    };
    match fraction.trim_end_matches('0') {
        "" => format!("${whole}"),
        cents => format!("${whole}.{cents:0<2}"),
    }
}

/// A token count as read: `1M`, `1.5M`, `200k`, `131k`, `800`.
fn tokens_word(tokens: u64) -> String {
    let mut words = String::new();
    if tokens >= 1_000_000 {
        let tenths = (tokens + 50_000) / 100_000;
        let _ = write!(words, "{}", tenths / 10);
        if !tenths.is_multiple_of(10) {
            let _ = write!(words, ".{}", tenths % 10);
        }
        words.push('M');
    } else if tokens >= 1_000 {
        let _ = write!(words, "{}k", (tokens + 500) / 1_000);
    } else {
        let _ = write!(words, "{tokens}");
    }
    words
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use nika_session::work::{OfferValue, ValueRole};

    fn option(key: &str, label: &str, facts: ModelFacts) -> work::Offer {
        let value = OfferValue::new(ValueRole::Value, facts.model.clone(), None);
        work::Offer::new(key, label, false).with_values(vec![value.with_choice(Some(facts))])
    }

    fn model_facts(
        model: &str,
        (via, class, billing): (&str, &str, &str),
        configured: bool,
    ) -> ModelFacts {
        let route = (via.to_owned(), class.to_owned(), billing.to_owned());
        ModelFacts::new("author", model, route, configured, None)
    }

    #[test]
    fn a_model_row_reads_its_name_marks_prices_and_context() {
        let api = ("anthropic", "api", "api_metered");
        let sonnet = model_facts("anthropic/claude-sonnet-5-5", api, true).with_catalogue(
            Some(2.0),
            Some(1_000_000),
            false,
        );
        let mut priced = sonnet.clone();
        priced.output_usd_per_million = Some(nika_session::work::UsdPerMillion(10.0));
        let offer = option("2 anthropic/claude-sonnet-5-5", "raw label", priced);
        let marks = Marks {
            current: true,
            recommended: true,
            ..Marks::default()
        };
        let row = row(&offer, marks, true, Voice::default());
        assert_eq!(row.key, "2 anthropic/claude-sonnet-5-5", "the exact key");
        assert_eq!(row.label, "Claude Sonnet 5.5 · Anthropic API");
        assert_eq!(
            row.note,
            "current · recommended · $2 in / $10 out per MTok · 1M context"
        );
        let quiet = super::row(&offer, Marks::default(), false, Voice::default());
        assert_eq!(quiet.label, "Claude Sonnet 5.5");
        assert_eq!(quiet.note, "$2 in / $10 out per MTok · 1M context");
        assert_eq!(
            note(
                &offer,
                facts(&offer).expect("facts"),
                Marks::default(),
                true,
                Voice::default()
            ),
            "Anthropic API · $2 in / $10 out per MTok · 1M context",
            "a note that routes keeps the access first"
        );
    }

    #[test]
    fn unknown_prices_are_said_and_a_missing_key_keeps_the_sessions_setup_words() {
        let deepseek = ("deepseek", "api", "api_metered");
        let legacy = model_facts("deepseek/deepseek-chat", deepseek, false).with_catalogue(
            None,
            Some(131_072),
            true,
        );
        let label = "deepseek/deepseek-chat · deepseek · api_metered · key required · configure NIKA_DEEPSEEK_API_KEY or DEEPSEEK_API_KEY";
        let row = super::row(
            &option("2 deepseek/deepseek-chat", label, legacy),
            Marks::default(),
            false,
            Voice::default(),
        );
        assert_eq!(row.label, "DeepSeek Chat");
        assert_eq!(
            row.note,
            "legacy · price unknown · 131k context · key required · configure NIKA_DEEPSEEK_API_KEY or DEEPSEEK_API_KEY"
        );
        let seat = model_facts(
            "anthropic/claude-opus-5-5",
            ("claude-code", "harness", "included_quota"),
            true,
        );
        let row = super::row(
            &option("1 claude-code/claude-opus-5-5", "x", seat),
            Marks::default(),
            true,
            Voice::default(),
        );
        assert_eq!(row.label, "Claude Opus 5.5 · Claude Code agent account");
        assert_eq!(row.note, "in your plan");
        let plain = work::Offer::new(
            "2 openrouter",
            "openrouter API · configured default model",
            false,
        );
        let row = super::row(&plain, Marks::default(), true, Voice::default());
        assert_eq!(
            (row.label.as_str(), row.note.as_str()),
            (plain.label.as_str(), "")
        );
    }

    /// Prices a tariff states only for shorter prompts read as the lowest, with how far they
    /// hold; the context stays the model's own window.
    #[test]
    fn prices_that_hold_for_shorter_prompts_read_from_with_their_reach() {
        let api = ("anthropic", "api", "api_metered");
        let mut haiku = model_facts("anthropic/claude-haiku-5-5", api, true)
            .with_catalogue(Some(0.1), Some(1_000_000), false)
            .with_prices_up_to(Some(100_000));
        haiku.output_usd_per_million = Some(nika_session::work::UsdPerMillion(0.5));
        let offer = option("2 anthropic/claude-haiku-5-5", "raw label", haiku);
        let row = row(&offer, Marks::default(), true, Voice::default());
        assert_eq!(row.label, "Claude Haiku 5.5 · Anthropic API");
        assert_eq!(
            row.note,
            "from $0.10 in / $0.50 out per MTok (≤100k tokens) · 1M context"
        );
    }

    #[test]
    fn prices_and_token_counts_read_short_and_exact() {
        for (price, words) in [
            (3.0, "$3"),
            (15.0, "$15"),
            (0.27, "$0.27"),
            (1.25, "$1.25"),
            (0.035, "$0.035"),
            (0.3, "$0.30"),
            (1.1, "$1.10"),
        ] {
            assert_eq!(dollars(price), words, "{price}");
        }
        for (tokens, words) in [
            (1_000_000, "1M"),
            (1_048_576, "1M"),
            (1_500_000, "1.5M"),
            (200_000, "200k"),
            (131_072, "131k"),
            (800, "800"),
        ] {
            assert_eq!(tokens_word(tokens), words, "{tokens}");
        }
    }
}
