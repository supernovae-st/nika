// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A rehearsal's lines in French, for a person who writes French: the same facts, each of the
//! engine's own phrases said in French, the names, paths, excerpts and reasons as written (live
//! gauntlet r04 B3, 2026-10-17: « répétition · Rehearsed on a copy of your files … » and « not run:
//! … » under French answers). A line this map does not know stays as it is: never another fact.

/// Whole lines, word for word.
const WHOLE: &[(&str, &str)] = &[
    (
        "Rehearsed on a copy of your files · nothing ran on the originals · observed result of this candidate · `yes` saves these exact bytes; running is a separate line (« run it »)",
        "Répété sur une copie de vos fichiers · rien n'a tourné sur les originaux · résultat observé de ce candidat · « oui » enregistre exactement ces octets ; lancer est une ligne à part (« lance-le »)",
    ),
    ("  no text output observed", "  aucun texte produit observé"),
    (
        "tried on no page observed from the network",
        "essayé sans aucune page observée sur le réseau",
    ),
];

/// What a trial does not run, by what it needs.
const NEEDS: &[(&str, &str)] = &[
    ("the model step", "l'étape du modèle"),
    ("a process", "un processus"),
    ("a request other than GET", "une requête autre que GET"),
    ("the network", "le réseau"),
    ("a crawl", "un parcours de site"),
    ("a jq extraction", "une extraction jq"),
    ("a notification", "une notification"),
    ("a person", "une personne"),
    ("an mcp server", "un serveur MCP"),
    ("a provider", "un fournisseur"),
    ("any effect outside the room", "tout effet hors de la copie"),
];

/// What a read-back line says of a file.
const READ_BACK: &[(&str, &str)] = &[
    ("not written by the run", "pas écrit par le run"),
    ("written by the run", "écrit par le run"),
    ("whole text observed", "texte entier observé"),
    ("prefix only", "début seulement"),
];

/// `english`, a rehearsal's lines, in French line by line; its final line break kept.
pub(super) fn lines(english: &str) -> String {
    let mut said: Vec<String> = english.lines().map(line).collect();
    if english.ends_with('\n') {
        said.push(String::new());
    }
    said.join("\n")
}

/// One line in French, or as written when it is no phrase of the engine's.
fn line(text: &str) -> String {
    if let Some((_, fr)) = WHOLE.iter().find(|(en, _)| *en == text) {
        return (*fr).to_owned();
    }
    if let Some(rest) = text.strip_prefix("tried on the pages observed at ")
        && let Some((at, pages)) = rest.split_once(": ")
    {
        let pages = pages.replace(" bytes)", " octets)");
        return format!("essayé sur les pages observées à {at} : {pages}");
    }
    if let Some(rest) = text.strip_prefix("not run: ") {
        let needs: Vec<String> = rest.split(" · ").map(need).collect();
        return format!("pas lancé : {}", needs.join(" · "));
    }
    if let Some(rest) = text.strip_prefix("nor what reads them: ") {
        return format!("ni ce qui les lit : {rest}");
    }
    if let Some(rest) = text.strip_prefix("not observed: ") {
        return format!("pas observé : {rest}");
    }
    let unrun = " comes from steps it did not run: never what a run writes";
    if let Some(paths) =
        (text.strip_prefix("what it read back of ")).and_then(|rest| rest.strip_suffix(unrun))
    {
        return format!(
            "ce qu'il a relu de {paths} vient d'étapes non lancées : jamais ce qu'un run écrit"
        );
    }
    if let Some(rest) = text.strip_prefix("Rehearsal not run · ")
        && let Some(reason) = rest.strip_suffix(
            " · source-only preview · no output observed · `yes` saves only; running is separate",
        )
    {
        return format!(
            "Répétition non lancée · {reason} · aperçu de la source seulement · aucun résultat observé · « oui » enregistre seulement ; lancer est à part"
        );
    }
    if let Some(rest) = text.strip_prefix("  read back · ") {
        let (facts, excerpt) = rest.split_once(" · excerpt ").unwrap_or((rest, ""));
        let mut facts = facts.to_owned();
        for (en, fr) in READ_BACK {
            facts = facts.replace(en, fr);
        }
        return if excerpt.is_empty() {
            format!("  relu · {facts}")
        } else {
            format!("  relu · {facts} · extrait {excerpt}")
        };
    }
    if let Some(rest) = text.strip_prefix("  rehearsed for the proposal ")
        && let Some(proposal) = rest.strip_suffix(" you saw, carried with these same bytes")
    {
        return format!(
            "  répété pour la proposition {proposal} que vous avez vue, reprise avec ces mêmes octets"
        );
    }
    text.to_owned()
}

/// One thing a trial does not run, `need (task)`, in French.
fn need(item: &str) -> String {
    (NEEDS.iter())
        .find_map(|(en, fr)| item.strip_prefix(en).map(|task| format!("{fr}{task}")))
        .unwrap_or_else(|| item.to_owned())
}

#[cfg(test)]
mod tests {
    use super::lines;

    /// Every line a trial and a rehearsal say reads in French, the names and paths kept.
    #[test]
    fn a_rehearsals_lines_read_in_french() {
        let english = "Rehearsed on a copy of your files · nothing ran on the originals · observed result of this candidate · `yes` saves these exact bytes; running is a separate line (« run it »)\n  read back · `out/digest.md` · written by the run · whole text observed · excerpt \"# Digest\"\ntried on the pages observed at 10:42 UTC: https://feed.example/items (5 bytes)\nnot run: the model step (summarize) · a request other than GET (post) · any effect outside the room\nnor what reads them: shape, write_digest\n";
        let french = lines(english);
        assert_eq!(
            french,
            "Répété sur une copie de vos fichiers · rien n'a tourné sur les originaux · résultat observé de ce candidat · « oui » enregistre exactement ces octets ; lancer est une ligne à part (« lance-le »)\n  relu · `out/digest.md` · écrit par le run · texte entier observé · extrait \"# Digest\"\nessayé sur les pages observées à 10:42 UTC : https://feed.example/items (5 octets)\npas lancé : l'étape du modèle (summarize) · une requête autre que GET (post) · tout effet hors de la copie\nni ce qui les lit : shape, write_digest\n"
        );
    }

    /// A rehearsal not run keeps its reason as written; a line the map does not know is unchanged.
    #[test]
    fn a_reason_and_an_unknown_line_stay_as_written() {
        let english = "Rehearsal not run · task list_inbox runs nika:glob, a tool unknown to the surface a rehearsal runs · source-only preview · no output observed · `yes` saves only; running is separate\nsomething else entirely";
        assert_eq!(
            lines(english),
            "Répétition non lancée · task list_inbox runs nika:glob, a tool unknown to the surface a rehearsal runs · aperçu de la source seulement · aucun résultat observé · « oui » enregistre seulement ; lancer est à part\nsomething else entirely"
        );
    }
}
