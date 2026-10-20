// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The status row in French, for a person who writes French: the same facts and the same scope
//! as the English words (`nika_display::front_door::status`), never another verdict. Live
//! gauntlet, 2026-10-16: French-speaking personas read « Ready for review · … » under every
//! French answer.

use std::path::Path;

/// A proposal still needs Save, then a separate Run.
pub(super) fn proposal<P: AsRef<Path>>(paths: impl Iterator<Item = P>) -> String {
    let files: Vec<_> = paths
        .map(|p| format!("`{}`", p.as_ref().display()))
        .collect();
    format!(
        "Prêt à relire · {} · rien n'est enregistré, rien n'a tourné",
        files.join(" · ")
    )
}

/// The last run's exit, scoped to that run so a later preparation inherits no verdict.
pub(super) fn run(exit: u8, workflow: Option<&Path>, refused: bool) -> String {
    let word = match exit {
        0 if refused => "Terminé · arrêté à votre « non »",
        0 => "Terminé · le run a réussi",
        1 => "Terminé · le run a échoué",
        2 => "Pas lancé · la vérification a refusé",
        3 => "Pas lancé · l'environnement a refusé",
        4 => "En pause · une approbation attend",
        130 => "Arrêté · le run a été interrompu",
        _ => "Terminé · un code inconnu",
    };
    workflow.map_or_else(
        || format!("Dernier run · {word}"),
        |w| format!("Dernier run · {word} · `{}`", w.display()),
    )
}

/// A live gate the Session selected.
pub(super) fn gate(workflow: &Path, task: &str) -> String {
    format!(
        "Attend votre réponse · `{}` en pause à `{task}`",
        workflow.display()
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Every exit the English row names has its French words, scoped to the same workflow.
    #[test]
    fn each_exit_reads_in_french_with_its_workflow() {
        for (exit, words) in [
            (0, "Terminé · le run a réussi"),
            (1, "Terminé · le run a échoué"),
            (2, "Pas lancé · la vérification a refusé"),
            (3, "Pas lancé · l'environnement a refusé"),
            (4, "En pause · une approbation attend"),
            (130, "Arrêté · le run a été interrompu"),
            (9, "Terminé · un code inconnu"),
        ] {
            assert_eq!(
                super::run(exit, Some(Path::new("a.nika")), false),
                format!("Dernier run · {words} · `a.nika`")
            );
        }
        assert_eq!(
            super::proposal([Path::new("a.nika")].into_iter()),
            "Prêt à relire · `a.nika` · rien n'est enregistré, rien n'a tourné"
        );
    }
}
