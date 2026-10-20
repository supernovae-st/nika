// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rows [`super::effect_rows`] writes, in French, for a person who writes French: the same
//! facts, each of the review's own phrases said in French, every path, host, tool, model, task
//! and amount as written (live gauntlet wave 6 r12 B1, 2026-10-18: « à l'exécution · model
//! output estimate · unbounded: `grade` declare(s) no `max_tokens`; Run admission is separate »
//! on a French card). A row this does not know stays as it is: never another fact.

/// Rows said whole.
const WHOLE: &[(&str, &str)] = &[
    ("removes files", "supprime des fichiers"),
    ("runs any program", "lance n'importe quel programme"),
    (
        "pauses for a human answer (`nika:prompt`)",
        "attend la réponse d'une personne (`nika:prompt`)",
    ),
    (
        "model output estimate · $0 · no direct model task in these checked bytes",
        "estimation de la sortie du modèle · $0 · aucune tâche de modèle dans ces octets vérifiés",
    ),
    (
        "model output estimate · $0 · mock model tasks",
        "estimation de la sortie du modèle · $0 · des tâches d'un modèle factice",
    ),
];

/// Rows that open with a phrase, then the values they name.
const OPENING: &[(&str, &str)] = &[
    ("removes files · within ", "supprime des fichiers · dans "),
    ("reads ", "lit "),
    ("writes ", "écrit "),
    ("network ", "réseau "),
    ("runs ", "lance "),
    ("tools ", "outils "),
    ("environment ", "environnement "),
];

/// Why an estimate is unbounded, each reason by its words around the tasks it names.
const UNBOUNDED: &[(&str, &str)] = &[
    (
        " declare(s) no `max_tokens`",
        " ne déclare(nt) aucun `max_tokens`",
    ),
    (
        " run(s) once per item of a list known only when the workflow runs",
        " tourne(nt) une fois par élément d'une liste connue seulement au lancement",
    ),
];

/// One row of [`super::effect_rows`] in French, its values as written; a row this does not know
/// is returned as it is.
#[must_use]
pub fn effect_row_fr(row: &str) -> String {
    if let Some((_, french)) = WHOLE.iter().find(|(english, _)| *english == row) {
        return (*french).to_owned();
    }
    let said = estimate(row)
        .or_else(|| beyond(row))
        .or_else(|| model(row))
        .or_else(|| secret(row));
    if let Some(said) = said {
        return said;
    }
    (OPENING.iter())
        .find_map(|(english, french)| {
            row.strip_prefix(english)
                .map(|rest| format!("{french}{rest}"))
        })
        .unwrap_or_else(|| row.to_owned())
}

/// `model output estimate · …`: unknown, unbounded and bounded.
fn estimate(row: &str) -> Option<String> {
    const LEAD: &str = "estimation de la sortie du modèle";
    if let Some(rest) = row.strip_prefix("model output estimate · unknown · ") {
        let count =
            rest.strip_suffix(" model task(s) with no catalog price — never counted as free")?;
        return Some(format!(
            "{LEAD} · inconnue · {count} tâche(s) de modèle sans prix au catalogue — jamais comptée(s) comme gratuite(s)"
        ));
    }
    if let Some(rest) = row.strip_prefix("model output estimate · unbounded: ") {
        let why = rest.strip_suffix("; Run admission is separate")?;
        let reasons: Option<Vec<String>> = why.split(" · ").map(reason).collect();
        return Some(format!(
            "{LEAD} · sans borne : {} ; l'admission du run se fait à part",
            reasons?.join(" · ")
        ));
    }
    let amount = row
        .strip_prefix("model output estimate ≤ ")?
        .strip_suffix(" at catalog prices · input tokens and other charges excluded")?;
    Some(format!(
        "{LEAD} ≤ {amount} aux prix du catalogue · jetons d'entrée et autres frais exclus"
    ))
}

/// One reason an estimate is unbounded.
fn reason(why: &str) -> Option<String> {
    match why {
        "a composed workflow has an unbounded step" => {
            Some("un workflow composé a une étape sans borne".to_owned())
        }
        "a token or iteration bound is missing" => {
            Some("il manque une borne de jetons ou d'itérations".to_owned())
        }
        _ => UNBOUNDED.iter().find_map(|(english, french)| {
            why.strip_suffix(english)
                .map(|tasks| format!("{tasks}{french}"))
        }),
    }
}

/// The row that names the reasoning billed beyond `max_tokens`.
fn beyond(row: &str) -> Option<String> {
    let named = row
        .strip_prefix("reasoning beyond `max_tokens` · ")?
        .strip_suffix(
            " · billed as output, outside the estimate above; a Run's ceiling still bounds it",
        )?;
    Some(format!(
        "raisonnement au-delà de `max_tokens` · {named} · facturé comme sortie, hors de l'estimation ci-dessus ; le plafond d'un run le borne toujours"
    ))
}

/// `model <id> (tasks <ids>)`.
fn model(row: &str) -> Option<String> {
    let (model, tasks) = row.strip_prefix("model ")?.rsplit_once(" (tasks ")?;
    let tasks = tasks.strip_suffix(')')?;
    Some(format!("modèle {model} (tâches {tasks})"))
}

/// `secret <name> (key <key> · a reference, never a value)`.
fn secret(row: &str) -> Option<String> {
    let (name, key) = row.strip_prefix("secret ")?.rsplit_once(" (key ")?;
    let key = key.strip_suffix(" · a reference, never a value)")?;
    Some(format!(
        "secret {name} (clé {key} · une référence, jamais une valeur)"
    ))
}

#[cfg(test)]
mod tests {
    use super::effect_row_fr;

    /// Each row the review writes reads in French with its values as written; a row it does
    /// not know stays as it is.
    #[test]
    fn each_effect_row_reads_in_french_with_its_values() {
        let rows = [
            (
                "reads ./notes/*.md · ./etat.json",
                "lit ./notes/*.md · ./etat.json",
            ),
            ("writes ./out/**", "écrit ./out/**"),
            (
                "removes files · within ./inbox/**",
                "supprime des fichiers · dans ./inbox/**",
            ),
            ("removes files", "supprime des fichiers"),
            (
                "network feed.example · httpbin.org",
                "réseau feed.example · httpbin.org",
            ),
            ("runs any program", "lance n'importe quel programme"),
            ("runs git · jq", "lance git · jq"),
            (
                "tools nika:fetch · nika:write",
                "outils nika:fetch · nika:write",
            ),
            (
                "pauses for a human answer (`nika:prompt`)",
                "attend la réponse d'une personne (`nika:prompt`)",
            ),
            ("environment HOME", "environnement HOME"),
            (
                "model anthropic/claude-sonnet-5-5 (tasks grade · sum)",
                "modèle anthropic/claude-sonnet-5-5 (tâches grade · sum)",
            ),
            (
                "secret token (key GH_TOKEN · a reference, never a value)",
                "secret token (clé GH_TOKEN · une référence, jamais une valeur)",
            ),
            (
                "model output estimate · unbounded: `grade` declare(s) no `max_tokens` · `each` run(s) once per item of a list known only when the workflow runs; Run admission is separate",
                "estimation de la sortie du modèle · sans borne : `grade` ne déclare(nt) aucun `max_tokens` · `each` tourne(nt) une fois par élément d'une liste connue seulement au lancement ; l'admission du run se fait à part",
            ),
            (
                "model output estimate ≤ $0.0246 at catalog prices · input tokens and other charges excluded",
                "estimation de la sortie du modèle ≤ $0.0246 aux prix du catalogue · jetons d'entrée et autres frais exclus",
            ),
            (
                "model output estimate · unknown · 2 model task(s) with no catalog price — never counted as free",
                "estimation de la sortie du modèle · inconnue · 2 tâche(s) de modèle sans prix au catalogue — jamais comptée(s) comme gratuite(s)",
            ),
            (
                "reasoning beyond `max_tokens` · `summarize` (xai/grok-4.7) · billed as output, outside the estimate above; a Run's ceiling still bounds it",
                "raisonnement au-delà de `max_tokens` · `summarize` (xai/grok-4.7) · facturé comme sortie, hors de l'estimation ci-dessus ; le plafond d'un run le borne toujours",
            ),
            ("a row of another kind", "a row of another kind"),
        ];
        for (english, french) in rows {
            assert_eq!(effect_row_fr(english), french, "{english}");
        }
    }
}
