// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A Run refused only because no ceiling can be proved asks the person for one instead of
//! refusing in the money law's words: the amounts an activation offers, one recommended from what
//! the workflow is known to cost, or a typed amount. The amount they state continues the Run
//! exactly as « run it max $X » would, through the explicit path of the money law; « yes » states
//! no amount, and none is ever applied for them. Every other refusal stands.

use std::path::Path;

use nika_onboard::compile::money;

use super::super::ceilings::{self, Evidence};
use super::{Ceiling, RunInputs, SessionRuntime, TurnOutcome, run_prefix};
use crate::authoring::is_cancel;
use crate::outcome::{Refusal, RefusalClass};
use crate::work::{Offer, Waiting};

/// The key a run's ceiling is asked under.
const RUN_CEILING: &str = "run.ceiling";

/// A run's ceiling as it is asked: what the recommendation reads, the language of the line that
/// asked the run, and why no ceiling could be proved, as the money law said it.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::runtime) struct Asked {
    evidence: Option<Evidence>,
    french: bool,
    why: String,
}

impl Asked {
    /// The ceiling `said` asks for `workflow` under `root`, refused as `refused` said.
    pub(super) fn new(root: &Path, workflow: &Path, said: &str, refused: &TurnOutcome) -> Self {
        let why = match refused {
            TurnOutcome::Refusal(refusal) => refusal.text.clone(),
            _ => String::new(),
        };
        Self {
            evidence: ceilings::evidence(root, workflow),
            french: french(said),
            why,
        }
    }

    fn offered(&self) -> Vec<(String, Option<String>)> {
        let lang = if self.french {
            super::super::Lang::Fr
        } else {
            super::super::Lang::En
        };
        ceilings::offered(self.evidence, lang)
    }

    /// The amount a reply could be: the one recommended.
    fn example(&self) -> String {
        let offered = self.offered();
        ceilings::recommended(&offered).unwrap_or("0.25").to_owned()
    }

    /// The question as a host that shows no options reads it: one short line, in the language
    /// of the line that asked the run when the Session knows it, English otherwise.
    fn question(&self) -> String {
        let example = self.example();
        if self.french {
            format!("Je n'ai pas lancé : quel plafond pour ce lancement ? (par ex. {example})")
        } else {
            format!("I did not start the run: what ceiling for this run? (e.g. {example})")
        }
    }

    /// The question's sentence and one short line, for a host that shows the options as rows.
    fn said(&self) -> String {
        if self.french {
            "Je n'ai pas lancé : quel plafond pour ce lancement ?\n  il ne dépensera pas plus · rien ne part avant ce choix".to_owned()
        } else {
            "I did not start the run: what ceiling for this run?\n  it may not spend more · nothing runs before you choose".to_owned()
        }
    }

    /// The amounts as options: each one's key is the amount typed, the one recommended says why.
    fn offers(&self) -> Vec<Offer> {
        (self.offered().into_iter())
            .map(|(amount, why)| match why {
                Some(why) => Offer::new(amount.clone(), format!("${amount} · {why}"), true),
                None => Offer::new(amount.clone(), format!("${amount}"), false),
            })
            .collect()
    }

    /// What still waits, and the way out.
    fn waits(&self) -> String {
        format!(
            "the run still waits for its ceiling · pick an amount, or type one, e.g. {} · `cancel` drops the run",
            self.example()
        )
    }
}

/// Whether the line that asked the run is French, as the run grammar reads its verb
/// (« lance-le », « relance »): the one language this Session knows of a conversation.
fn french(said: &str) -> bool {
    let lower = said.trim().to_lowercase();
    let first = (lower.split(|c: char| c.is_whitespace() || c == ',' || c == ':'))
        .next()
        .unwrap_or_default()
        .trim_end_matches(['.', '!']);
    let verb = first.rsplit_once('-').map_or(first, |(verb, _)| verb);
    matches!(verb, "lance" | "relance" | "exécute" | "teste")
}

/// The amount `line` states as a ceiling: a number alone (« 0.25 », « 0,25 », « $0.25 ») or one
/// the money grammar reads (« 0.25 USD », « budget 0.25 », « max $0.25 »), finite and not
/// negative. A line with no amount (« yes ») states none.
fn amount(line: &str) -> Option<f64> {
    let value = line.trim();
    let stated = money::parse(value).ok().and_then(|parsed| parsed.amount);
    let bare = || {
        let number = value.strip_prefix('$').unwrap_or(value).replace(',', ".");
        number.parse::<f64>().ok()
    };
    stated
        .or_else(bare)
        .filter(|amount| amount.is_finite() && *amount >= 0.0)
}

impl SessionRuntime {
    /// The workflow whose run waits for its ceiling, when one does.
    pub(in crate::runtime) fn pending_run_ceiling(&self) -> Option<&Path> {
        let inputs = self.run_inputs.as_ref()?;
        inputs.asked().map(|_| inputs.workflow())
    }

    /// Whether `line` answers the ceiling question waiting (any line but a run line, which is a
    /// request of its own): a closed Run's money, which amends no Session inference.
    pub(in crate::runtime) fn answers_run_ceiling(&self, line: &str) -> bool {
        self.pending_run_ceiling().is_some() && run_prefix(line).is_none()
    }

    /// The ceiling a waiting run asks, for a host that shows its options natively
    /// ([`Waiting::RunCeiling`]): its key, the question as its sentence and one short line (the
    /// options are the host's rows, never repeated as text), the amounts (one recommended from
    /// the workflow's last run or its static cost floor, with why in its label), and that another
    /// typed amount is admitted. Picking an option answers as typing its key does, through the
    /// same door and the same money law; nothing is applied without that answer. Read-only: it
    /// answers, applies and authorizes nothing.
    #[must_use]
    pub fn waiting_run_ceiling_choices(&self) -> Option<(&'static str, String, Vec<Offer>, bool)> {
        let inputs = self.run_inputs.as_ref()?;
        let asked = inputs.asked()?;
        let waits = Waiting::RunCeiling {
            workflow: inputs.workflow().to_path_buf(),
        };
        (self.waiting() == waits).then(|| (RUN_CEILING, asked.said(), asked.offers(), true))
    }

    /// Ask the person for the ceiling of the run `inputs` holds; its inputs wait behind it.
    pub(super) fn ask_ceiling(&mut self, inputs: RunInputs) -> TurnOutcome {
        let question = inputs.asked().map(Asked::question).unwrap_or_default();
        self.intent.unresolved = vec![format!(
            "the ceiling of this run of `{}`",
            inputs.workflow().display()
        )];
        self.run_inputs = Some(inputs);
        TurnOutcome::Question {
            key: RUN_CEILING.to_owned(),
            question,
        }
    }

    /// « why? » beside the ceiling asked: why none could be proved, and what the answer does.
    pub(in crate::runtime) fn explain_ceiling(&self) -> Option<String> {
        let inputs = self.run_inputs.as_ref()?;
        let asked = inputs.asked()?;
        let head = asked.why.lines().next().unwrap_or_default();
        let head = head.split(" — ").next().unwrap_or_default();
        let head = head.split(" · ").next().unwrap_or_default();
        Some(format!(
            "`{}` was not started: {head}\n  the amount you choose is the most this run may spend, never a default, and nothing runs before it\n  {}",
            inputs.workflow().display(),
            asked.waits()
        ))
    }

    /// The person's line as the ceiling of the waiting run: an amount continues the run through
    /// the explicit path of the money law, as « run it max $X » does; `cancel` or `no` drops it;
    /// a run line leaves the question for a request of its own; any other line states no amount
    /// and the run keeps waiting.
    pub(super) fn answer_ceiling_unrecorded(
        &mut self,
        mut inputs: RunInputs,
        line: &str,
    ) -> TurnOutcome {
        if is_cancel(line) || super::super::is_no(line) {
            self.intent.unresolved.clear();
            self.remember(line, "(run request discarded)");
            return TurnOutcome::Facts(
                "run discarded · nothing ran · no ceiling was set · say « run it » with one when you know it, e.g. « run it max $0.25 »"
                    .to_owned(),
            );
        }
        // A run line is a request of its own: the question it leaves set nothing.
        if run_prefix(line).is_some() {
            self.intent.unresolved.clear();
            return self.turn_unrecorded(line);
        }
        let Some(asked) = inputs.asked().cloned() else {
            return self.request_or_ask(inputs);
        };
        // A command-shaped line is never an amount.
        if let Some(text) = super::super::protocol::unserved_command(line) {
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!("{text}\n  {}", asked.waits()),
            ));
        }
        let Some(amount) = amount(line) else {
            let said = line.trim();
            let why = if said.is_empty() {
                format!("an amount is needed: {}", asked.waits())
            } else {
                format!("« {said} » is not an amount: {}", asked.waits())
            };
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(RefusalClass::EmptyAnswer, why));
        };
        let said = inputs.said.clone();
        match self.run_money(&said, &inputs.workflow, Some(amount)) {
            Ok(admitted) => {
                inputs.ceiling = Ceiling::Admitted(admitted, "stated for this run");
                self.last_workflow = Some(inputs.workflow.clone());
                self.intent.unresolved.clear();
                self.remember(line, "(run ceiling stated · run requested)");
                self.request_or_ask(inputs)
            }
            Err(refused) => {
                self.intent.unresolved.clear();
                refused.outcome()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{amount, french};

    #[test]
    fn an_amount_is_a_number_or_money_and_never_a_yes() {
        for (line, read) in [
            ("0.25", Some(0.25)),
            (" 0,25 ", Some(0.25)),
            ("$0.50", Some(0.5)),
            ("1", Some(1.0)),
            ("0.20 USD", Some(0.2)),
            ("budget 0.10", Some(0.1)),
            ("max $1", Some(1.0)),
            ("0", Some(0.0)),
            ("yes", None),
            ("oui", None),
            ("", None),
            ("-1", None),
            ("$-1", None),
            ("NaN", None),
            ("inf", None),
            ("0.25 or 0.50", None),
        ] {
            assert_eq!(amount(line), read, "{line:?}");
        }
    }

    #[test]
    fn the_run_verb_says_whether_the_line_asked_in_french() {
        for line in [
            "lance-le",
            "Relance-le stp",
            "exécute-la",
            "teste-le",
            "relance le workflow",
        ] {
            assert!(french(line), "{line}");
        }
        for line in [
            "run it",
            "save & run",
            "execute it",
            "test it",
            "Rerun it please",
            "",
        ] {
            assert!(!french(line), "{line}");
        }
    }
}
