// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A clause that asks Nika to save or run the workflow itself is no effect its tasks
//! perform. Live gauntlet, 2026-10-15: on the welcome demo, « Enregistre ce workflow et
//! lance-le », then Nika's own suggestion « sauvegarde-le sous le nom aggregate-by-key et
//! exécute-le », then the documented « run aggregate-by-key » were each read as a create the
//! workflow had to perform, so a workflow that only computes could never be saved from the
//! conversation. Only the workflow, a pronoun standing for it or a bare workflow name is read
//! here, never a destination: « enregistre-le dans out/resume.md » still writes a file.

/// The verbs that save or run the workflow itself.
const VERBS: &[&str] = &[
    "enregistre",
    "enregistrez",
    "enregistrer",
    "sauvegarde",
    "sauvegardez",
    "sauvegarder",
    "lance",
    "lancez",
    "lancer",
    "relance",
    "relancez",
    "relancer",
    "exécute",
    "exécutez",
    "exécuter",
    "execute",
    "run",
    "rerun",
    "save",
];

/// The verbs among [`VERBS`] that run, whose bare object may be a workflow's name.
const RUNS: &[&str] = &[
    "lance",
    "lancez",
    "lancer",
    "relance",
    "relancez",
    "relancer",
    "exécute",
    "exécutez",
    "exécuter",
    "execute",
    "run",
    "rerun",
];

/// The pronouns that stand for the workflow just shown (« lance-le », « save it »).
const PRONOUNS: &[&str] = &["le", "la", "it"];

/// The determiners before a noun naming the workflow.
const DETERMINERS: &[&str] = &[
    "ce", "cet", "cette", "le", "la", "mon", "ma", "this", "the", "my",
];

/// The nouns that name the workflow itself.
const NOUNS: &[&str] = &[
    "workflow",
    "automatisation",
    "automation",
    "démo",
    "demo",
    "programme",
];

/// The words a save or run of the workflow may carry beside its object: when, a polite word,
/// and the name it is saved under (« sous le nom aggregate-by-key »).
const TRAILING: &[&str] = &[
    "maintenant",
    "now",
    "once",
    "aussi",
    "too",
    "please",
    "stp",
    "svp",
    "alors",
    "directement",
    "tout",
    "de",
    "suite",
];

/// Whether `text` (a normalized clause, filler stripped) only asks to save or run the workflow
/// itself: its verb, then the workflow (a pronoun on the verb or after it, a determiner and a
/// workflow noun, or the bare name of a workflow to run), then nothing but a time, a polite
/// word or the name it is saved under. Any destination, path or other object keeps the
/// clause's ordinary reading.
pub(super) fn acts_on_the_workflow(text: &str) -> bool {
    // « enregistre ce workflow et lance-le », « enregistre et lance ce workflow »: every part is
    // such an act, or a bare verb whose workflow a later part names.
    let parts: Vec<&str> = CONNECTORS
        .iter()
        .fold(vec![text], |parts, connector| {
            parts
                .iter()
                .flat_map(|part| part.split(connector))
                .collect()
        })
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let bare = |part: &str| VERBS.contains(&part.trim_matches(|c: char| matches!(c, ',' | '.')));
    let acts = parts.iter().filter(|part| one_act(part)).count();
    acts > 0 && parts.iter().all(|part| one_act(part) || bare(part))
}

/// The connectors between two acts on the workflow.
const CONNECTORS: &[&str] = &[" et ", " and ", " puis ", " then ", ", "];

/// One save or run of the workflow itself, as [`acts_on_the_workflow`] reads it.
fn one_act(text: &str) -> bool {
    let words: Vec<&str> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| matches!(c, ',' | '.' | '!' | ';' | ':')))
        .filter(|w| !w.is_empty())
        .collect();
    let Some((first, rest)) = words.split_first() else {
        return false;
    };
    // « lance-le », « enregistre-la », « sauvegarde-le » carry their pronoun on the verb.
    let (verb, clitic) = match first.split_once('-') {
        Some((verb, pronoun)) if PRONOUNS.contains(&pronoun) => (verb, true),
        Some(_) => return false,
        None => (*first, false),
    };
    if !VERBS.contains(&verb) {
        return false;
    }
    let tail = if clitic {
        rest
    } else {
        match rest {
            ["it", tail @ ..] => tail,
            [determiner, noun, tail @ ..]
                if DETERMINERS.contains(determiner) && NOUNS.contains(noun) =>
            {
                tail
            }
            [elided, tail @ ..]
                if elided
                    .strip_prefix("l'")
                    .is_some_and(|n| NOUNS.contains(&n)) =>
            {
                tail
            }
            // « run aggregate-by-key », « lance veille.nika »: a workflow named alone.
            [name] if RUNS.contains(&verb) && workflow_name(name) => &[],
            _ => return false,
        }
    };
    plain_tail(tail)
}

/// Whether `name` reads as a workflow's name: a slug, or a `.nika` file named alone.
fn workflow_name(name: &str) -> bool {
    let path = std::path::Path::new(name);
    let file = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("nika"));
    let stem = if file {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
    } else {
        name
    };
    let slug = stem.len() >= 3
        && stem.contains(['-', '_'])
        && stem
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'));
    slug || (file && !stem.is_empty() && !name.contains('/'))
}

/// Whether what follows the workflow is only a time, a polite word or the name it is saved
/// under — never a destination or another object.
fn plain_tail(tail: &[&str]) -> bool {
    let mut words = tail.iter().peekable();
    while let Some(word) = words.next() {
        if TRAILING.contains(word) {
            continue;
        }
        // « sous le nom <name> », « under the name <name> », « as <name> »: a name, never a path.
        let named = match *word {
            "sous" | "under" => {
                words.next().is_some_and(|w| matches!(*w, "le" | "the"))
                    && words.next().is_some_and(|w| matches!(*w, "nom" | "name"))
            }
            "as" => true,
            _ => false,
        };
        let name = if named { words.next() } else { None };
        if name.is_none_or(|name| name.contains('/')) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::acts_on_the_workflow;

    /// The three lines of the welcome-demo journey read no effect the workflow must perform,
    /// and every clause is accounted for; a destination beside them still writes.
    #[test]
    fn the_demo_journey_lines_read_no_workflow_effect() {
        for line in [
            "Bon. Enregistre ce workflow et lance-le.",
            "sauvegarde-le sous le nom aggregate-by-key et exécute-le",
            "run aggregate-by-key",
        ] {
            let reading = super::super::read(line);
            assert!(
                reading.plan.effects.is_empty(),
                "{line}: {:?}",
                reading.plan.effects
            );
            assert!(!reading.session_commands.is_empty(), "{line}: {reading:?}");
            assert!(
                reading.hot_rejections().iter().all(|why| !why.contains("produced no element")
                    && !why.contains("unaccounted")),
                "{line}: {:?}",
                reading.hot_rejections()
            );
        }
        let reading = super::super::read("Résume notes.md et enregistre-le dans out/resume.md.");
        assert!(
            reading
                .plan
                .effects
                .iter()
                .any(|e| e.target.contains("out/resume.md")),
            "{:?}",
            reading.plan.effects
        );
        assert!(reading.session_commands.is_empty(), "{reading:?}");
    }

    #[test]
    fn a_save_or_run_of_the_workflow_itself_is_read_as_such() {
        for text in [
            "enregistre ce workflow",
            "lance-le",
            "sauvegarde-le sous le nom aggregate-by-key",
            "exécute-le",
            "run aggregate-by-key",
            "run it",
            "save it",
            "run it now",
            "enregistre la démo",
            "lance le workflow",
            "relance-le",
            "exécute-le maintenant",
            "lance veille-quotidienne.nika",
            "save this workflow as weekly-recap",
            "enregistre l'automatisation",
            "enregistre ce workflow et lance-le",
            "enregistre et lance ce workflow",
        ] {
            assert!(acts_on_the_workflow(text), "{text}");
        }
    }

    #[test]
    fn a_save_or_run_of_anything_else_keeps_its_reading() {
        for text in [
            "enregistre le résumé dans out/resume.md",
            "enregistre-le dans out/resume.md",
            "save it to report.md",
            "lance la sauvegarde des fichiers",
            "run the tests",
            "garde seulement les lignes payées",
            "exécute la commande ls",
            "lance une recherche sur hacker news",
            "save the results",
            "enregistre les factures",
            "run aggregate",
            "enregistre ce workflow dans ./out/x.md",
            "save it as ./out/x.md",
            "enregistre",
            "enregistre le résumé et lance-le",
            "enregistre et lance",
            "",
        ] {
            assert!(!acts_on_the_workflow(text), "{text}");
        }
    }
}
