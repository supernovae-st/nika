// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A new conversation in the same project (NIK-197). The Session keeps one durable conversation
//! per project; this door seals it under a dated name, never deleting it, so its receipts and any
//! unknown API charge stay on record, and begins the next one in a fresh journal under the same
//! lease. The conversation's own state (its turns, intent, rounds, draft and agent tree) is
//! cleared; the project's facts stay: its saved programs and kept run, the intelligence and
//! knowledge choices, a paused run's gate, and an unresolved charge, said plainly. Nothing is
//! begun while another decision waits for the person.

use super::history::HistoryMode;
use super::{Lang, SessionRuntime, TurnOutcome, agent};
use crate::outcome::{Refusal, RefusalClass};
use crate::work::Waiting;

impl SessionRuntime {
    /// Begin a new conversation in this project: the current one is sealed and kept, the next
    /// starts empty beside the project's facts. Returns what the person reads: that it began,
    /// where the previous one is kept, and what carries over.
    ///
    /// # Errors
    /// A decision waits for the person (a proposal, a question, a value, a choice, a cost
    /// review), a run is under way, or the conversation's history cannot be sealed or begun
    /// again: nothing was changed.
    pub fn begin_new_conversation(&mut self) -> Result<String, Refusal> {
        let lang = self.language;
        match self.waiting() {
            Waiting::Free | Waiting::Gate { .. } => {}
            _ => {
                return Err(Refusal::new(
                    RefusalClass::WrongState,
                    lang.pick(
                        "a decision still waits for you · answer it or set it aside (`no`, `cancel`), then begin a new conversation",
                        "une décision vous attend encore · répondez-y ou écartez-la (« non », « annuler »), puis commencez une nouvelle conversation",
                    ),
                ));
            }
        }
        if let HistoryMode::Active(history) = &self.history
            && history.run == super::history::RunState::AwaitingObservation
        {
            return Err(Refusal::new(
                RefusalClass::WrongState,
                lang.pick(
                    "a run is under way · a new conversation begins once it is observed",
                    "un run est en cours · une nouvelle conversation commence une fois qu'il est observé",
                ),
            ));
        }
        if let HistoryMode::Blocked(why) = &self.history {
            return Err(Refusal::new(RefusalClass::Io, why.clone()));
        }
        let uncertain = matches!(&self.history, HistoryMode::Active(h) if h.uncertain);
        self.clear_conversation();
        let carried = self.saved_conversation();
        let stamp = jiff::Timestamp::now()
            .strftime("%Y%m%dT%H%M%SZ")
            .to_string();
        let sealed = match &mut self.history {
            HistoryMode::Active(history) => match history.begin_anew(carried, &stamp) {
                Ok(sealed) => Some(sealed),
                Err(error) => {
                    let why = format!("the conversation could not begin again: {error}");
                    self.history = HistoryMode::Blocked(why.clone());
                    return Err(Refusal::new(RefusalClass::Io, why));
                }
            },
            _ => None,
        };
        if self.agent.is_some() {
            let dir = match &self.history {
                HistoryMode::Active(history) => Some(history.store_dir()),
                _ => None,
            };
            self.agent = Some(match dir {
                Some(dir) => agent::Driver::resumed(dir, None),
                None => agent::Driver::fresh(),
            });
        }
        Ok(self.new_conversation_words(sealed.as_deref(), uncertain, lang))
    }

    /// `/new`: the door above, as a turn.
    pub(super) fn new_conversation_turn(&mut self) -> TurnOutcome {
        match self.begin_new_conversation() {
            Ok(said) => TurnOutcome::Facts(said),
            Err(refused) => TurnOutcome::Refusal(refused),
        }
    }

    /// Forget what this conversation said and worked on; keep the project's facts.
    fn clear_conversation(&mut self) {
        self.intent = super::IntentDraft::default();
        self.recent.clear();
        self.preparation_before = super::durable::PreparationBefore::default();
        self.basis = None;
        self.restored_draft = None;
        self.restored_round = None;
        self.decided = None;
        self.answered = None;
        self.run_told.clear();
        self.last_answer = None;
        self.proposed_revision = None;
        self.revising = None;
        self.questions = super::question::Identities::default();
        self.last_recovery = None;
        self.last_outcome = None;
        self.declined = super::declined::Declined::default();
        self.pending_trigger = None;
        self.activation_proposal = None;
        self.declared = None;
        self.interrupted = None;
        self.routes.clear();
    }

    /// What the person reads once the new conversation began.
    fn new_conversation_words(&self, sealed: Option<&str>, uncertain: bool, lang: Lang) -> String {
        let mut said = match (sealed, lang) {
            (Some(sealed), Lang::Fr) => format!(
                "Nouvelle conversation · la précédente est gardée telle quelle (`{sealed}`, à côté de son journal)"
            ),
            (Some(sealed), _) => format!(
                "New conversation · the previous one is kept as it was (`{sealed}`, beside its journal)"
            ),
            (None, Lang::Fr) => "Nouvelle conversation".to_owned(),
            (None, _) => "New conversation".to_owned(),
        };
        said.push_str(lang.pick(
            "\n  this project's workflows, schedules and runs stay as they are · say what you want to build or run",
            "\n  les workflows, planifications et runs de ce projet restent tels quels · dites ce que vous voulez construire ou lancer",
        ));
        if let Some(gate) = &self.pending_gate {
            let _ = match lang {
                Lang::Fr => std::fmt::Write::write_fmt(
                    &mut said,
                    format_args!(
                        "\n  un run en pause attend toujours votre réponse à `{}` · elle reste à donner",
                        gate.task
                    ),
                ),
                _ => std::fmt::Write::write_fmt(
                    &mut said,
                    format_args!(
                        "\n  a paused run still waits for your answer at `{}` · it is still yours to give",
                        gate.task
                    ),
                ),
            };
        }
        if uncertain {
            said.push_str(lang.pick(
                "\n  a charge or a result of the previous conversation is still unknown · it stays on record, and a later success does not settle it",
                "\n  une dépense ou un résultat de la conversation précédente reste inconnu · il reste au dossier, et un succès plus tard ne le règle pas",
            ));
        }
        said
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use crate::outcome::RefusalClass;
    use crate::runtime::TurnOutcome;
    use crate::runtime::tests::{saved_daily_copy, tree};

    /// A new conversation with nothing waiting forgets the turns and keeps the saved workflow;
    /// one while a decision waits changes nothing.
    #[test]
    fn a_new_conversation_forgets_the_turns_and_keeps_the_project() {
        let dir = tree();
        let mut s = saved_daily_copy(dir.path());
        let TurnOutcome::Question { .. } = s.turn("active-le") else {
            panic!("the activation asks its first value");
        };
        let refused = s.begin_new_conversation().expect_err("a value waits");
        assert_eq!(refused.class, RefusalClass::WrongState);
        assert!(
            refused.text.contains("une décision vous attend"),
            "{}",
            refused.text
        );
        let _ = s.turn("annuler");
        let said = s.begin_new_conversation().expect("nothing waits");
        assert!(
            said.starts_with("Nouvelle conversation") && said.contains("restent tels quels"),
            "{said}"
        );
        assert!(
            s.recent.is_empty() && s.intent.goal.is_none(),
            "the turns are forgotten"
        );
        assert!(
            s.last_workflow.is_some(),
            "the saved workflow stays the project's"
        );
    }
}
