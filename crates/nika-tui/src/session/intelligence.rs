// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native conversation needs an intelligence. The Session still owns parsing,
//! selection and any interrupted request; this host cannot opt into its offline mode.
//!
//! The picker is a few lists this host browses locally ([`View`]): the home screen (the
//! recommendation, the current choice kept, every other choice, the author, decision and Run
//! roles, the default), every connection, one connection's models, the decision seat, and the
//! model for the next workflows' AI steps. Browsing applies nothing: only an option's own key
//! reaches the Session's choice, and each other change goes through one named Session door.

use super::{Live, SessionRuntime, TurnOutcome};
use crate::model::{Beat, Committed, Kind, Turn};
use nika_session::intelligence::{IntelligenceKind, UserIntelligencePreference};
use nika_session::outcome::{Refusal, RefusalClass};
use nika_session::runtime::DecisionChoice;
use nika_session::work;

mod describe;
mod groups;
mod home;
pub(super) mod keys;
use nika_tui_view::names;

/// The list the picker shows: local presentation, no selection authority.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum View {
    /// The recommendation, the current choice, every other choice, the roles, the default.
    #[default]
    Home,
    /// Every connection.
    Connections,
    /// One connection's models.
    Models(String),
    /// One connection's older models.
    Older(String),
    /// Who makes the typed decisions.
    Decision,
    /// The model for the AI steps of the next workflows.
    Run,
}

/// The lines the picker sends itself: each opens a list or reaches one Session door.
pub(super) mod line {
    /// The home screen (the Session reopens its choice).
    pub(in crate::session) const HOME: &str = "/intelligence";
    /// Every connection.
    pub(in crate::session) const CONNECTIONS: &str = "/intelligence connections";
    /// Every connection, for the author.
    pub(in crate::session) const AUTHOR: &str = "/intelligence author";
    /// One connection's models, its key after this.
    pub(in crate::session) const MODELS: &str = "/intelligence models ";
    /// Who makes the typed decisions.
    pub(in crate::session) const DECISION: &str = "/intelligence decision";
    /// Jev makes them.
    pub(in crate::session) const JEV: &str = "/intelligence decision jev";
    /// The author decides alone.
    pub(in crate::session) const ALONE: &str = "/intelligence decision author";
    /// The model for the next workflows' AI steps.
    pub(in crate::session) const RUN: &str = "/intelligence run";
    /// That model, its `provider/name` after this.
    pub(in crate::session) const RUN_MODEL: &str = "/intelligence run ";
    /// Keep this conversation's choice as the operator's default.
    pub(in crate::session) const DEFAULT: &str = "/intelligence default";
    /// Check a provider's API key once, its id after this.
    pub(in crate::session) const TEST: &str = "/intelligence test ";
    /// One connection's older models, its key after this.
    pub(in crate::session) const OLDER: &str = "/intelligence older ";
}

/// What the picker shows beside the Session's snapshot, as this host holds it.
pub(super) struct Shown<'a> {
    /// The list shown.
    pub(super) view: &'a View,
    /// The model the person asked for the next workflows' AI steps.
    pub(super) run: Option<&'a str>,
    /// The operator's saved default.
    pub(super) saved: Option<&'a UserIntelligencePreference>,
    /// The Jev seat the host named, and why it cannot be consulted when it cannot.
    pub(super) jev: Option<(&'a str, Option<&'a str>)>,
    /// This machine's probe rows: the names of the key variables each provider has set.
    pub(super) keys: &'a [keys::KeyNames],
    /// Whether this host can check an API key when the person asks.
    pub(super) testable: bool,
    /// The last key check the person asked for: its provider and verdict word.
    pub(super) checked: Option<(&'a str, &'a str)>,
    /// The person's language and the glyph column: the picker's own words are said in it; the
    /// Session's, the models' and the keys' names keep theirs.
    pub(super) voice: crate::lang::Voice,
}

/// Paint the list `shown` names over the settings choice `waiting` holds.
pub(super) fn project(waiting: &mut crate::model::Waiting, work: &work::Work, shown: &Shown<'_>) {
    let crate::model::Waiting::Selection { asked } = waiting else {
        return;
    };
    let Some(intelligence) = &work.intelligence else {
        return;
    };
    // Every list here offers a model that writes (an author, a Run step): never an embedding.
    let mut kept = intelligence.clone();
    kept.options
        .retain(|offer| describe::facts(offer).is_none_or(describe::writes));
    let intelligence = &kept;
    match shown.view {
        View::Home => home::home(asked, work, intelligence, shown),
        View::Connections => groups::connections(asked, intelligence, shown),
        View::Models(opened) => groups::models(asked, intelligence, opened, shown),
        View::Older(opened) => groups::older(asked, intelligence, opened, shown),
        View::Decision => home::decision(asked, intelligence, shown),
        View::Run => home::run(asked, intelligence, shown),
    }
    one_in_use(asked);
}

/// One filled radio per list (live gauntlet 5, r01 B11, r05 B11, r07 B10, r09 B14: the model
/// and its connection both « (●) … current »): the first row in use keeps it, the model where
/// models lead the list; a later row in use, the connection of that model or the same model
/// found again by a filter, keeps its « current » word without a second radio.
fn one_in_use(asked: &mut crate::model::Asked) {
    let crate::model::Shape::Choice(offers) = &mut asked.shape else {
        return;
    };
    let mut held = false;
    for offer in offers.iter_mut().filter(|offer| offer.current) {
        offer.current = !held;
        held = true;
    }
}

/// A decision seat's display name (`typesafe/jev-1.13.0` reads `Jev 1.13.0`).
pub(super) fn seat_name(model: &str) -> String {
    names::display(model)
}

pub(super) fn ready(runtime: &SessionRuntime) -> bool {
    runtime.intelligence_chosen()
        && runtime.intelligence.ready
        && !matches!(runtime.intelligence.kind, IntelligenceKind::None)
}

/// Whether the Session offers at least one intelligence to choose: the list can open.
pub(super) fn listed(runtime: &SessionRuntime) -> bool {
    runtime
        .work()
        .intelligence
        .is_some_and(|i| !i.options.is_empty())
}

/// What the opening says of the intelligence this conversation needs, in `lang`.
pub(super) fn guidance(runtime: &SessionRuntime, lang: nika_session::Lang) -> String {
    let available = listed(runtime);
    let voice = crate::lang::Voice { lang, ascii: false };
    // The typed route (`/intelligence 2 provider/model`) stays an expert fallback the list
    // never advertises: every model this machine offers is a row.
    let mut words = voice.say(if available && ready(runtime) {
        "Choose an intelligence below."
    } else if available {
        "Choose an intelligence below. A message typed here waits for your choice."
    } else {
        "This conversation needs an AI connection. Connect an account or configure a provider, then reopen Nika. /providers shows the current connections."
    }).to_owned();
    if let Some(why) = runtime
        .intelligence
        .why
        .as_deref()
        .filter(|_| !ready(runtime))
    {
        words.push('\n');
        words.push_str(why);
    }
    words
}

fn refused(why: &str) -> TurnOutcome {
    TurnOutcome::Refusal(Refusal::new(RefusalClass::IntelligenceRefused, why))
}

/// Said once when words typed over the open list became a message instead of a choice.
const UNCHANGED: &str = "intelligence unchanged";

/// Said when a choice would put in use the connection whose key its provider refused.
const NOT_APPLIED: &str = "key refused by {name} ({status}) · this choice is not applied · check the key or choose another connection";

/// The provider of the API connection this conversation uses now, when it uses one.
fn current_api(runtime: &SessionRuntime) -> Option<String> {
    let work = runtime.work();
    let selected = work.intelligence?.selected?;
    (selected.ready && selected.kind == "api")
        .then_some(selected.via)
        .flatten()
}

/// Said when those words met another decision waiting behind the list.
const UNCHANGED_HELD: &str =
    "intelligence unchanged · a decision is waiting: your words are back in the box, not sent";

impl Live {
    /// Whether `line`, sent while the list is open over a usable choice, is a message for the
    /// conversation rather than a choice: words that take no expert route
    /// ([`crate::composer::answer::expert_route`]) and cancel nothing. Without a usable choice
    /// the Session keeps such words until one is made.
    pub(super) fn converses(&self, line: &str) -> bool {
        let Some(runtime) = self.runtime.as_ref() else {
            return false;
        };
        let words = line.trim();
        runtime.pending_choice()
            && ready(runtime)
            && !words.is_empty()
            && !crate::composer::answer::expert_route(words)
            && !nika_session::authoring::is_cancel(words)
    }

    /// The provider whose key the person's last check found refused, and the status it said.
    fn refused_key(&self) -> Option<(&str, u16)> {
        match self.key_checked.as_ref()? {
            (provider, Ok(nika_session::KeyCheck::Refused { status, .. })) => {
                Some((provider.as_str(), *status))
            }
            _ => None,
        }
    }

    /// A line over the open list that would put in use the connection whose key the person's
    /// last check found refused (live gauntlet 6, r02 B1: `OpenAI` answered 401 on every request
    /// after « Test the key » said so): one of its models or its own row, the current choice
    /// kept while it is that connection, or words that would be sent with it. Nothing reaches
    /// the Session: one plain line says the key was refused, the list stays open where it was
    /// for another choice, and typed words come back to the box. A later check that passes
    /// lifts it. `None` for any other line.
    pub(super) fn refused_connection(&mut self, line: &str) -> Option<Turn> {
        let runtime = self.runtime.as_ref()?;
        let (provider, status) = self.refused_key()?;
        if !runtime.pending_choice() {
            return None;
        }
        let words = line.trim();
        let answer = words.strip_prefix("/intelligence ").unwrap_or(words).trim();
        let own = format!("2 {provider}");
        let names_it = answer == own || answer.starts_with(&format!("{own}/"));
        let typed = self.converses(line);
        let keeps = answer == home::KEEP || typed;
        let puts_in_use = names_it || (keeps && current_api(runtime).as_deref() == Some(provider));
        if !puts_in_use {
            return None;
        }
        let voice = crate::lang::Voice {
            lang: self.lang(),
            ascii: self.ascii,
        };
        let (name, status) = (names::provider(provider), status.to_string());
        let said = crate::lang::fill(
            voice.say(NOT_APPLIED),
            &[("{name}", &name), ("{status}", &status)],
        );
        let mut beats = vec![Beat::Say(Committed::new(Kind::Refusal, said))];
        beats.extend(self.footer());
        beats.push(self.wait());
        if typed {
            beats.push(Beat::NotTaken(line.to_owned()));
        }
        Some(Turn {
            beats,
            handoff: None,
        })
    }

    /// Words typed over the open list that named no row ([`Self::converses`]): the list closes
    /// with the current choice kept, then the words reach the conversation as its next message,
    /// after one notice. Another decision waiting behind the list takes nothing from them: they
    /// come back to the box. `None` for any other line.
    pub(super) fn converse_over_list(&mut self, line: &str) -> Option<Turn> {
        if !self.converses(line) {
            return None;
        }
        let runtime = self.runtime.as_mut()?;
        let _ = runtime.submit(home::KEEP, &work::Waiting::IntelligenceChoice);
        self.intelligence_view = View::Home;
        let notice = |words: &str| Beat::Say(Committed::new(Kind::Notice, words));
        if runtime.waiting() != work::Waiting::Free {
            let mut beats = vec![notice(UNCHANGED_HELD)];
            beats.extend(self.footer());
            beats.push(self.wait());
            beats.push(Beat::NotTaken(line.to_owned()));
            return Some(Turn {
                beats,
                handoff: None,
            });
        }
        let outcome = runtime.submit(line, &work::Waiting::Free);
        let (mut beats, handoff) = self.map(outcome);
        beats.insert(0, notice(UNCHANGED));
        Some(Turn { beats, handoff })
    }
    /// Refuse only the offline selection and an initial cancellation. Neither can erase the
    /// Session's kept request; cancelling a change with a usable current route stays ordinary.
    /// While the picker is open, its own lines open its lists or reach their Session door.
    pub(super) fn require_intelligence(&mut self, line: &str) -> Option<TurnOutcome> {
        let pending = self.runtime.as_ref()?.pending_choice();
        let trimmed = line.trim();
        let direct = trimmed.strip_prefix("/intelligence ");
        if !pending && direct.is_none() {
            return None;
        }
        if pending {
            if trimmed == line::HOME {
                self.intelligence_view = View::Home;
            } else if let Some(outcome) = self.picker_line(trimmed) {
                return Some(outcome);
            }
        }
        let answer = direct.unwrap_or(trimmed).trim();
        let runtime = self.runtime.as_mut()?;
        let offline = self
            .census
            .choose(answer)
            .is_ok_and(|pref| matches!(pref.kind, IntelligenceKind::None));
        let cannot_cancel = !ready(runtime) && nika_session::authoring::is_cancel(answer);
        if !offline && !cannot_cancel {
            if runtime.pending_choice() && !ready(runtime) {
                return Some(runtime.request_before_intelligence(line));
            }
            return None;
        }
        if !runtime.pending_choice() {
            let _ = runtime.turn("/intelligence");
        }
        Some(TurnOutcome::Facts(if offline {
            "An AI connection is required for this conversation. The offline choice was not applied; your current selection and any waiting message are kept."
                .to_owned()
        } else {
            "Choose an AI connection to continue. Your waiting message is kept while this guide remains open."
                .to_owned()
        }))
    }

    /// One of the picker's own lines: the list it opens, its guide in the list's head and
    /// nothing said in the conversation ([`Self::guided`]), or the Session door it reaches and
    /// what that door said. `None` for any other line.
    fn picker_line(&mut self, said: &str) -> Option<TurnOutcome> {
        let voice = crate::lang::Voice {
            lang: self.lang(),
            ascii: self.ascii,
        };
        let runtime = self.runtime.as_mut()?;
        let (view, outcome) = match said {
            line::CONNECTIONS | line::AUTHOR => (
                View::Connections,
                self.guided(voice.say("Choose a connection below. No selection has changed.")),
            ),
            line::DECISION => (
                View::Decision,
                self.guided(voice.say(
                    "Choose who makes this conversation's typed decisions. Nothing changes until you choose.",
                )),
            ),
            line::JEV | line::ALONE => {
                let choice = if said == line::JEV {
                    DecisionChoice::Jev
                } else {
                    DecisionChoice::AuthorAlone
                };
                let outcome = runtime.choose_decision(choice);
                let view = if matches!(outcome, TurnOutcome::Refusal(_)) {
                    View::Decision
                } else {
                    View::Home
                };
                (view, outcome)
            }
            line::RUN => (
                View::Run,
                self.guided(voice.say(
                    "Choose the model the author uses for the AI steps of the workflows written from now on. Workflows already written do not change.",
                )),
            ),
            line::DEFAULT => (View::Home, runtime.save_operator_default()),
            _ => {
                if let Some(model) = said.strip_prefix(line::RUN_MODEL) {
                    return Some(self.run_model(model.trim()));
                }
                if let Some(provider) = said.strip_prefix(line::TEST) {
                    return Some(self.test_key(provider.trim()));
                }
                let snapshot = runtime.work();
                if let Some(group) = groups::requested_after(&snapshot, said, line::OLDER) {
                    let words = voice.say("Older models of this connection: earlier generations, dated snapshots and legacy models. No selection has changed.");
                    self.intelligence_view = View::Older(group);
                    return Some(self.guided(words));
                }
                let group = groups::requested(&snapshot, said)?;
                let guidance = voice.say(groups::guidance(&snapshot, &group));
                (View::Models(group), self.guided(guidance))
            }
        };
        self.intelligence_view = view;
        Some(outcome)
    }

    /// The list `words` guide, opened: they stand in the list's head while it is open, and the
    /// turn says nothing in the conversation (live gauntlet, r02 to r09: « Choose a connection
    /// below. No selection has changed. » stayed in the history before « Intelligence: … »).
    fn guided(&mut self, words: &str) -> TurnOutcome {
        self.picker_guide = Some(words.to_owned());
        TurnOutcome::Facts(String::new())
    }

    /// One free check of `provider`'s API key, only because the person picked it: the host's
    /// checker sends one request that runs no model, and its verdict is said and kept beside
    /// the row; the key is never shown. Nothing is sent without a checker.
    fn test_key(&mut self, provider: &str) -> TurnOutcome {
        let Some(check) = self.check_key.as_ref() else {
            return refused("This host has no key check: nothing was sent.");
        };
        let checked = check(provider);
        let voice = crate::lang::Voice {
            lang: self.lang(),
            ascii: self.ascii,
        };
        let (_, sentence) = keys::verdict(provider, &checked, voice);
        self.key_checked = Some((provider.to_owned(), checked));
        TurnOutcome::Facts(sentence)
    }

    /// The model for the next workflows' AI steps. No Session field holds such a preference,
    /// so it is said to the author as the person's own line, once the picker has closed with
    /// the author kept; workflows already written do not change. Nothing is sent while no
    /// author is chosen or while another decision waits.
    fn run_model(&mut self, model: &str) -> TurnOutcome {
        let Some(runtime) = self.runtime.as_mut() else {
            return refused("No conversation is open: nothing was sent.");
        };
        if !ready(runtime) {
            return refused(
                "Choose an author first: the model for the next workflows is told to it. Nothing was sent.",
            );
        }
        let _ = runtime.submit("cancel", &work::Waiting::IntelligenceChoice);
        let waiting = runtime.waiting();
        self.intelligence_view = View::Home;
        if waiting != work::Waiting::Free {
            return TurnOutcome::Facts(
                "Not sent: a decision is waiting. Settle it, then choose Run again.".to_owned(),
            );
        }
        self.run_requested = Some(model.to_owned());
        let said = format!("Use {model} for the AI steps of the next workflows.");
        runtime.submit(&said, &waiting)
    }
}

/// Hermetic host-mechanics fixtures select a configured route whose injected reasoner never
/// uses a provider. This exercises the same native opening contract as a real selection.
#[cfg(test)]
pub(crate) fn fixture() -> (super::IntelligenceCensus, super::UserIntelligencePreference) {
    let mut census = super::IntelligenceCensus::empty();
    census.locals.push("ollama".into());
    let preference = super::UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".into(),
        },
        None,
    );
    (census, preference)
}
