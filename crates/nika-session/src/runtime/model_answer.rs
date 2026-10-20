// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `model` question in the person's words. The compiler asks which model runs a
//! workflow's language steps as `<provider>/<model>`; a person on a subscription seat (codex,
//! claude-code) answered « choisis toi-même » or « prends Claude Haiku » and was told « Name the
//! runtime model as `<provider>/<model>` » (live gauntlet, 2026-10-15). The question now offers a
//! model this machine can run (the chosen seat's own, else the recommended configured one): an
//! empty line takes it, a reply that leaves the choice to Nika takes it, and a reply naming or
//! describing a configured model is read against the models offered, never a free invention.

use std::fmt::Write as _;

use nika_onboard::compile::CompileQuestion;
use nika_session_change::reply::delegates;

use super::SessionRuntime;
use super::authoring::{seat_offer, unpriced_model_text};
use crate::authoring::AuthoringSeat;
use crate::work::{ModelFacts, Offer};

/// The most models one reading of a reply is offered.
const MAX_CHOICES: usize = 16;

/// What a reply at the `model` question was read as.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ModelReading {
    /// Not read: the reply binds as typed (no reader, nothing offered, or a model id typed).
    AsTyped,
    /// The person left the choice to Nika: the offered model.
    Delegated(String),
    /// A configured model the reply names or describes.
    Named(String),
    /// Nothing bound, and why.
    Waits(String),
}

/// The facts an option states of its model.
fn facts(offer: &Offer) -> Option<&ModelFacts> {
    offer.values.iter().find_map(|value| value.choice.as_ref())
}

/// The configured API and local models this machine can run a workflow's language steps on.
fn runnable(options: &[Offer]) -> impl Iterator<Item = &str> {
    (options.iter().filter_map(facts))
        .filter(|f| f.configured && matches!(f.class.as_str(), "api" | "local"))
        .map(|f| f.model.as_str())
}

/// The words of a reply long enough to name a model family (`haiku`, `deepseek`, `gpt`).
fn words(line: &str) -> Vec<String> {
    (line.split(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect()
}

/// The one bounded reading of a reply at the `model` question: the models offered, the reply
/// verbatim, and one instruction — the model it designates, DELEGATED, or NONE.
pub(super) fn model_prompt(label: &str, line: &str, choices: &[String]) -> String {
    let offered = (choices.iter())
        .map(|model| format!("«{model}»"))
        .collect::<Vec<_>>()
        .join(" · ");
    format!(
        "Nika, an automation tool, asked a human which model should run the language steps of their workflow: «{label}».\nThe models available here are exactly: {offered}.\nThe human replied: «{reply}».\nIf the reply leaves the choice to Nika instead of naming a model (as in « choisis toi-même », « carte blanche », « prends le meilleur », « peu importe », « you decide »), answer DELEGATED. If the reply designates one of the available models, by its name, its family or a description of it, copy that model exactly as listed. Otherwise, or if it could be several, answer NONE.\nValue:",
        reply = line.trim(),
    )
}

/// The model a reading names, when it is one of `choices`, its quotes and `Value:` cue aside.
fn chosen<'a>(reply: &str, choices: &'a [String]) -> Option<&'a String> {
    let mut lines = reply.lines().map(str::trim).filter(|l| !l.is_empty());
    let (Some(first), None) = (lines.next(), lines.next()) else {
        return None;
    };
    let said = first
        .strip_prefix("Value:")
        .unwrap_or(first)
        .trim()
        .trim_matches(['«', '»', '"', '\'', '`', ' ']);
    choices.iter().find(|model| model.as_str() == said)
}

impl SessionRuntime {
    /// The model the `model` question offers: the chosen seat's own model, else the configured
    /// model this conversation recommends (the operator's saved default, else the newest of the
    /// configured providers) when it can run a workflow's steps; `None` when there is none.
    pub(super) fn offered_run_model(&self) -> Option<String> {
        if let AuthoringSeat::Provider { model } = &self.seat {
            return Some(model.clone());
        }
        let options = self.intelligence_options();
        let key = self.recommended(&options)?;
        let (class, model) = key.split_once(' ')?;
        matches!(class, "2" | "3").then(|| model.to_owned())
    }

    /// The offer under the `model` question: the seat's own (`seat_offer`), else the
    /// recommended configured model, which an empty line takes.
    pub(super) fn model_offer(&self) -> Option<String> {
        if let Some(offer) = seat_offer(&self.seat) {
            return Some(offer);
        }
        let model = self.offered_run_model()?;
        let mut offer = format!(
            "Enter takes `{model}` (recommended here) · or name another model · « choisis toi-même » leaves it to Nika"
        );
        if let Some(warning) = unpriced_model_text(&model) {
            let _ = write!(offer, "\n  {warning}");
        }
        Some(offer)
    }

    /// The models a reply may designate, best first: the offered one, then each configured
    /// model carrying a word of the reply, then each configured provider's default model.
    fn run_model_choices(&self, line: &str) -> Vec<String> {
        let options = self.intelligence_options();
        let said = words(line);
        let mut choices: Vec<String> = self.offered_run_model().into_iter().collect();
        let mut push = |model: &str| {
            if choices.len() < MAX_CHOICES && !choices.iter().any(|m| m == model) {
                choices.push(model.to_owned());
            }
        };
        for model in runnable(&options) {
            let id = model.to_lowercase();
            if said.iter().any(|word| id.contains(word.as_str())) {
                push(model);
            }
        }
        for provider in nika_catalog::all_providers() {
            let default = format!("{}/{}", provider.id, provider.default_model);
            if runnable(&options).any(|model| model == default) {
                push(&default);
            }
        }
        choices
    }

    /// The reading of a reply at the `model` question that names no `<provider>/<model>`: one
    /// metered call under a chosen intelligence, against the models offered here.
    pub(super) fn read_model_answer(
        &mut self,
        question: &CompileQuestion,
        line: &str,
    ) -> ModelReading {
        if !self.reads_answers() {
            return ModelReading::AsTyped;
        }
        let choices = self.run_model_choices(line);
        let Some(offered) = choices.first().cloned() else {
            return ModelReading::AsTyped;
        };
        self.activity(&crate::activity::Activity::now(
            crate::activity::Phase::Understanding,
            "reading your answer",
        ));
        match self.reason_with_money(&model_prompt(&question.label, line, &choices), true) {
            Ok(reply) if delegates(&reply.text) => ModelReading::Delegated(offered),
            Ok(reply) => chosen(&reply.text, &choices).map_or_else(
                || {
                    ModelReading::Waits(format!(
                        "I could not tell which model « {} » names — nothing was bound · `{offered}` is the one offered here",
                        line.trim()
                    ))
                },
                |model| ModelReading::Named(model.clone()),
            ),
            Err(error) => ModelReading::Waits(format!(
                "I could not read your answer ({error}) — nothing was bound"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_names_an_offered_model_or_nothing() {
        let choices = vec![
            "anthropic/claude-sonnet-5-5".to_owned(),
            "anthropic/claude-haiku-5-5".to_owned(),
        ];
        for reply in [
            "anthropic/claude-haiku-5-5",
            "Value: anthropic/claude-haiku-5-5",
            "«anthropic/claude-haiku-5-5»",
        ] {
            assert_eq!(
                chosen(reply, &choices).map(String::as_str),
                Some("anthropic/claude-haiku-5-5"),
                "{reply}"
            );
        }
        for reply in [
            "NONE",
            "anthropic/claude-opus-5-5",
            "claude-haiku-5-5",
            "anthropic/claude-haiku-5-5\nanthropic/claude-sonnet-5-5",
            "",
        ] {
            assert_eq!(chosen(reply, &choices), None, "{reply}");
        }
    }

    #[test]
    fn the_prompt_offers_exactly_the_choices_and_delegation() {
        let prompt = model_prompt(
            "Which explicit runtime provider/model should classify and draft?",
            "  choisis toi-même ",
            &["deepseek/deepseek-flash".to_owned()],
        );
        assert!(
            prompt.contains("exactly: «deepseek/deepseek-flash»."),
            "{prompt}"
        );
        assert!(
            prompt.contains("The human replied: «choisis toi-même»."),
            "{prompt}"
        );
        assert!(prompt.contains("answer DELEGATED"), "{prompt}");
        assert!(prompt.ends_with("Value:"), "{prompt}");
    }

    #[test]
    fn family_words_are_the_longer_ones_lowercased() {
        assert_eq!(
            words("Prends Claude Haiku, ça ira !"),
            ["prends", "claude", "haiku", "ira"]
        );
    }
}
