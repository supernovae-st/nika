// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! « est-ce que c'est actif ? », « ça tourne tout seul ? », « is it active? »: whether the saved
//! workflow runs on its own, answered by the Session from `nika.yaml` and this Mac's units, never
//! by a model (live gauntlet r08 and r02, 2026-10-17: right after « Installé et actif » the model
//! answered « Non, ce n'est pas actif », and right after a save « il n'est ni sauvegardé »).

use std::path::{Path, PathBuf};

use super::schedule::{cadence_words, declared_arm_entry};
use super::{Lang, SessionRuntime, TurnOutcome};

/// The questions, their filler aside (« bien », « déjà », « maintenant »).
const FORMS: &[&str] = &[
    "est-ce que c'est actif",
    "est-ce que c'est activé",
    "est-ce qu'il est actif",
    "est-ce qu'il est activé",
    "est-ce qu'elle est active",
    "est-ce qu'elle est activée",
    "est-ce actif",
    "est-ce activé",
    "c'est actif",
    "c'est activé",
    "il est actif",
    "il est activé",
    "est-il actif",
    "est-il activé",
    "est-ce que c'est installé",
    "est-ce qu'il est installé",
    "est-ce que le déclencheur est installé",
    "le déclencheur est installé",
    "c'est installé",
    "il est installé",
    "est-il installé",
    "est-ce que c'est programmé",
    "est-ce qu'il est programmé",
    "c'est programmé",
    "il est programmé",
    "est-ce que c'est planifié",
    "est-ce qu'il est planifié",
    "c'est planifié",
    "il est planifié",
    "est-ce que ça tourne tout seul",
    "ça tourne tout seul",
    "est-ce qu'il tourne tout seul",
    "il tourne tout seul",
    "est-ce qu'il va tourner tout seul",
    "il va tourner tout seul",
    "est-ce que ça va tourner tout seul",
    "ça va tourner tout seul",
    "est-ce qu'il se lance tout seul",
    "il se lance tout seul",
    "est-ce qu'il se lancera tout seul",
    "il se lancera tout seul",
    "is it active",
    "is it activated",
    "is it on",
    "is it installed",
    "is the trigger installed",
    "is it scheduled",
    "is it armed",
    "does it run on its own",
    "will it run on its own",
    "is it running on its own",
    "does it run by itself",
    "will it run by itself",
];

/// Words that ask nothing more, wherever they stand.
const FILLER: &[&str] = &[
    "bien",
    "déjà",
    "maintenant",
    "là",
    "actuellement",
    "vraiment",
    "alors",
    "donc",
    "et",
    "stp",
    "svp",
    "récap",
    "now",
    "yet",
    "already",
    "really",
    "please",
    "so",
];

/// What may follow the question: where (« sur ce Mac ») and when (« toutes les heures »).
const TAIL: &[&str] = &[
    "sur",
    "ce",
    "cet",
    "mac",
    "ordinateur",
    "toutes",
    "tous",
    "les",
    "heures",
    "jours",
    "chaque",
    "jour",
    "matin",
    "soir",
    "matins",
    "soirs",
    "semaine",
    "mois",
    "demain",
    "on",
    "this",
    "computer",
    "every",
    "day",
    "hour",
    "hours",
    "morning",
    "evening",
    "night",
    "week",
    "month",
    "tonight",
    "tomorrow",
];

/// Whether a line asks if the saved workflow runs on its own.
pub(super) fn is_active_question(line: &str) -> bool {
    let lowered = line.replace('\u{2019}', "'").to_lowercase();
    let words: Vec<&str> = (lowered.split(|c: char| c.is_whitespace() || "?!.,;:«»".contains(c)))
        .filter(|word| !word.is_empty() && !FILLER.contains(word))
        .collect();
    FORMS.iter().any(|form| {
        let form: Vec<&str> = form.split(' ').collect();
        words.len() >= form.len()
            && words[..form.len()] == form[..]
            && words[form.len()..].iter().all(|word| TAIL.contains(word))
    })
}

impl SessionRuntime {
    /// The answer to [`is_active_question`]: for the saved workflow, or each workflow `nika.yaml`
    /// declares when this conversation saved none, whether it runs on its own and what makes it.
    pub(super) fn active_turn(&self) -> TurnOutcome {
        let workflows: Vec<PathBuf> = match self.last_workflow.clone() {
            Some(workflow) => vec![workflow],
            None => nika_vocab::project::discover(&self.snapshot.root)
                .ok()
                .flatten()
                .map(|(_, project)| {
                    (project.arm().iter())
                        .map(|entry| PathBuf::from(&entry.workflow))
                        .collect()
                })
                .unwrap_or_default(),
        };
        if workflows.is_empty() {
            return TurnOutcome::Facts(
                self.language
                    .pick(
                        "nothing here runs on its own: no workflow is saved in this conversation and `nika.yaml` declares no schedule",
                        "rien ne tourne tout seul ici : aucun workflow n'est enregistré dans cette conversation et `nika.yaml` ne déclare aucune planification",
                    )
                    .to_owned(),
            );
        }
        let lines: Vec<String> = workflows.iter().map(|w| self.active_line(w)).collect();
        TurnOutcome::Facts(lines.join("\n"))
    }

    /// Whether `workflow` runs on its own, in one line of the person's language.
    fn active_line(&self, workflow: &Path) -> String {
        let lang = self.language;
        let root = &self.snapshot.root;
        let shown = workflow.display();
        let Some(entry) = declared_arm_entry(root, workflow) else {
            let asked = (self.last_workflow.as_deref() == Some(workflow))
                .then(|| self.last_trigger.as_ref()?.source_hint.clone())
                .flatten();
            return match (lang, asked) {
                (Lang::Fr, Some(when)) => format!(
                    "Pas encore : `{shown}` est enregistré, mais la planification demandée (« {when} ») n'est pas déclarée, donc rien ne le lance tout seul · « active-le » la déclare"
                ),
                (Lang::Fr, None) => format!(
                    "Non : `{shown}` n'a pas de planification, il tourne quand vous le demandez (« lance-le ») · pour qu'il tourne tout seul, dites quand (« chaque matin à 8h »)"
                ),
                (_, Some(when)) => format!(
                    "Not yet: `{shown}` is saved, but the schedule asked for (« {when} ») is not declared, so nothing runs it on its own · « activate » declares it"
                ),
                (_, None) => format!(
                    "No: `{shown}` has no schedule, it runs when you ask (« run it ») · to run it on its own, say when (« every morning at 8 »)"
                ),
            };
        };
        let words = cadence_words(&entry.cadence, lang);
        if entry.actif == Some(false) {
            return match lang {
                Lang::Fr => format!(
                    "Non : la planification de `{shown}` ({words}) est suspendue (`actif: false` dans `nika.yaml`)"
                ),
                _ => format!(
                    "No: the schedule of `{shown}` ({words}) is suspended (`actif: false` in `nika.yaml`)"
                ),
            };
        }
        let label = super::schedule_ledger::label_of(root, workflow);
        if label
            .as_deref()
            .is_some_and(|label| self.trigger_host.installed(root, label))
        {
            let next = (super::trigger_install::next_fire(&entry.cadence)).map_or_else(
                String::new,
                |next| match lang {
                    Lang::Fr => format!(" · prochain run : {next}"),
                    _ => format!(" · next run: {next}"),
                },
            );
            return match lang {
                Lang::Fr => format!(
                    "Oui : `{shown}` est installé sur ce Mac et tourne tout seul, {words}{next} · « désactive-le » le retire"
                ),
                _ => format!(
                    "Yes: `{shown}` is installed on this Mac and runs on its own, {words}{next} · « deactivate it » removes it"
                ),
            };
        }
        let nika = super::schedule::program();
        // The install decision already waiting is the way, said as its answers.
        if self.install_offer.is_some() {
            return match lang {
                Lang::Fr => format!(
                    "Pas encore : `{shown}` est déclaré dans `nika.yaml` ({words}) mais aucun déclencheur n'est installé sur ce Mac · la question d'installation attend votre réponse : 1 pour installer et activer, 2 pour garder la planification déclarée seulement"
                ),
                _ => format!(
                    "Not yet: `{shown}` is declared in `nika.yaml` ({words}) but no trigger is installed on this Mac · the install question waits for your answer: 1 to install and activate, 2 to keep the schedule declared only"
                ),
            };
        }
        let how = match (lang, self.trigger_host.installs()) {
            (Lang::Fr, true) => " · « active-le » propose de l'installer".to_owned(),
            (Lang::Fr, false) => format!(
                " · un déclencheur doit tourner sur cette machine (`{nika} arm --emit launchd --write` installe l'unité système)"
            ),
            (_, true) => " · « activate it » offers to install it".to_owned(),
            (_, false) => format!(
                " · a firer must run on this machine (`{nika} arm --emit launchd --write` installs the OS unit)"
            ),
        };
        match lang {
            Lang::Fr => format!(
                "Pas encore : `{shown}` est déclaré dans `nika.yaml` ({words}) mais aucun déclencheur n'est installé sur ce Mac, donc rien ne le lance tout seul{how}"
            ),
            _ => format!(
                "Not yet: `{shown}` is declared in `nika.yaml` ({words}) but no trigger is installed on this Mac, so nothing runs it on its own{how}"
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::is_active_question;
    use crate::intelligence::{DataLocus, IntelligenceKind};
    use crate::reasoner::NoReasoner;
    use crate::runtime::tests::{ready, tree};
    use crate::runtime::{SessionRuntime, TurnOutcome};

    /// Asked beside a waiting proposal, the question leaves it waiting: the next yes saves it.
    #[test]
    fn the_active_question_leaves_a_waiting_proposal() {
        let dir = tree();
        std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
        std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
        let mut s = SessionRuntime::open(
            dir.path(),
            ready(IntelligenceKind::None, DataLocus::None),
            Box::new(NoReasoner),
        );
        let TurnOutcome::Proposal { .. } =
            s.turn("Chaque matin à 8h, lis ./notes/brief.md et écris-le dans ./out/copie.md")
        else {
            panic!("proposed");
        };
        let TurnOutcome::Facts(said) = s.turn("est-ce que c'est actif ?") else {
            panic!("the Session answers");
        };
        assert!(said.starts_with("rien ne tourne tout seul ici"), "{said}");
        let TurnOutcome::Facts(saved) = s.consent("oui") else {
            panic!("the proposal still waited for its yes");
        };
        assert!(saved.contains("Enregistré"), "{saved}");
    }

    /// The ways people ask it, and lines that only mention activity.
    #[test]
    fn the_active_question_reads_the_ways_people_ask_it() {
        for line in [
            "est-ce que c'est actif ?",
            "Est-ce qu'il est activé ?",
            "c'est bien actif ?",
            "Récap stp : est-ce qu'il va tourner tout seul toutes les heures maintenant ?",
            "il est installé sur ce Mac ?",
            "is it active?",
            "Is it running on its own now?",
        ] {
            assert!(is_active_question(line), "{line}");
        }
        for line in [
            "active-le",
            "rends-le actif chaque matin",
            "est-ce que c'est actif dans le rapport ?",
            "le fichier est actif",
            "is it active in the report?",
        ] {
            assert!(!is_active_question(line), "{line}");
        }
    }
}
