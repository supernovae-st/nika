// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The help card in French, for a person who writes French: the same commands and the same
//! laws as the English card (`nika_display::front_door::SESSION_HELP`), with the gestures said
//! the way that person says them (« lance … », « active-le »). Live gauntlet, 2026-10-16: every
//! French-speaking persona met the English card between French answers.

/// The help card.
pub(super) const SESSION_HELP: &str = "texte                décrivez le travail à construire (« lis ./notes, rédige un résumé, écris ./out/resume.md ») · Nika le compile,
                     demande ce qu'il ne peut pas inventer, montre le workflow et ne l'écrit que quand vous dites oui · consentir n'est jamais lancer
lance …              lance le workflow accepté, ou celui que vous nommez (« lance brief.nika avec un plafond de 0,05 $ ») · un run en pause vous demande
active-le            déclare dans nika.yaml la planification que votre demande prévoit (Nika demande le fuseau, ce qu'il faut faire d'un run manqué, le plafond), puis, sur un Mac, propose d'installer son déclencheur sur votre oui · « désactive-le » le retire
texte                demandez, avec vos mots · le moteur répond sans IA : vos workflows · le verdict d'un fichier (« est-ce que X est valide »)
                     · les outils intégrés · les fournisseurs · un exemple ou un modèle pour une tâche · un code (« explique NIKA-… »)
                     · ce que Nika appelle un nœud, une étape, un déclencheur, un secret, une action · le reste va à l'intelligence choisie, avec vos mots
/intelligence        l'IA avec laquelle cette session raisonne · rouvre le choix de l'intelligence
/status              où vous en êtes : la racine du projet, l'intelligence et où va votre contexte, le siège qui écrit le workflow
/why                 à côté d'une question ou d'une approbation : à quoi sert la réponse, ce qu'elle permet · rien n'est consommé
/meaning             ce que Nika a retenu de votre demande : les clauses du compilateur, ou vos mots et les valeurs du workflow · une proposition attend toujours
/proof               après un run : ce que sa trace enregistre (chaîne · sceau · frontière · empreintes des tâches) et ce qu'elle ne prouve pas · jugé par `nika trace verify`, jamais par un second lecteur
/details             comment le dernier workflow a été construit : le moteur d'écriture et son modèle, les appels, les jetons et le temps, la stratégie, le siège de décision, l'identité du moteur et de la spec · avancé, à la demande
/show                quand une proposition attend : affiche ses octets exacts (la revue montre la frontière)
/new                 commence une nouvelle conversation dans ce projet · l'actuelle est gardée · ses workflows, planifications et runs restent
/help                cette carte
/quit                ferme la session
Nommez un fichier de workflow dans votre question pour que la session le lise (seuls les fichiers sous la racine du projet sont lus).";

/// The help line of `/restore` for a kept draft.
pub(super) const RESTORE_HELP: &str = "/restore            revoir la proposition gardée de votre dernière session (sa demande, un aperçu neuf) · sans IA · rien n'est écrit avant votre oui";

/// The help line of `/restore` for a kept round.
pub(super) const ROUND_HELP: &str = "/restore            reprendre la préparation gardée sans demander à l'IA · sa question revient, ou « continue » relance son juge · l'enregistrement reste à part";

/// The help line of `/knowledge`.
pub(super) const KNOWLEDGE_HELP: &str = "/knowledge embedded  utiliser la connaissance intégrée à Nika pour cette conversation (le réglage qui en nommait une autre a été refusé) · un message retenu reprend une fois · « /knowledge » montre celle en usage";

#[cfg(test)]
mod tests {
    use super::SESSION_HELP;

    /// The commands a card lists, in order.
    fn commands(card: &str) -> Vec<&str> {
        (card.lines())
            .filter_map(|line| line.split_whitespace().next())
            .filter(|word| word.starts_with('/'))
            .collect()
    }

    /// The French card lists every command of the English card, in the same order: the
    /// language changes the words, never what a person can do.
    #[test]
    fn the_french_card_lists_every_command_of_the_english_card() {
        let english = commands(super::super::HELP);
        assert!(english.len() > 8, "{english:?}");
        assert_eq!(commands(SESSION_HELP), english);
        assert!(
            SESSION_HELP.contains("\n/help "),
            "the conditional lines' anchor"
        );
    }
}
