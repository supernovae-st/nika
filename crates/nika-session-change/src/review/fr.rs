// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A proposal's words in French, for a person who writes French: the same facts, the Session's
//! own phrases said in French, every path, workflow byte, amount and identity as written (live
//! gauntlet wave 6 r01, r04, r10, r12, 2026-10-18: « money: $0.25 USD · session default · … »,
//! « when it runs: » and « apply this? (yes applies · … ) » on every French proposal, the money
//! lines read as a contradiction). The consent identity stays the one of the English words it
//! is computed from: only the words a person reads change. A line this does not know stays as
//! it is: never another fact. The session says a French person's proposals through it.

use nika_display::check_render::review::effect_row_fr;

/// Lines said whole.
const WHOLE: &[(&str, &str)] = &[
    ("  when it runs:", "  à l'exécution :"),
    (
        "  repaired before this preview (the fix ladder):",
        "  réparé avant cet aperçu (l'échelle de correction) :",
    ),
    (
        "apply this? (yes applies · no discards · questions keep it pending · nothing is written until you say yes)",
        "appliquer ? (« oui » applique · « non » écarte · une question la garde en attente · rien n'est écrit avant votre « oui »)",
    ),
];

/// The heading a compiled request's review opens its effects with: such a review keeps its
/// own words, and only its money lines are said in French (its other headings are not this
/// module's, and half a card in each language reads worse than either).
const REVIEW: &str = "\nCan touch\n";

/// Where a ceiling came from, as the money line says it.
const SOURCES: &[(&str, &str)] = &[
    (
        "the amount you gave, over the project default",
        "le montant que vous avez donné, au-dessus du défaut du projet",
    ),
    ("the amount you gave", "le montant que vous avez donné"),
    ("session default", "défaut de la session"),
    ("project default", "défaut du projet"),
    ("refused", "refusé"),
];

/// What the Session's own inference is held to, as its line says it.
const INFERENCE: &[(&str, &str)] = &[
    (
        "explicit unknown cost · finite one-time inference · no USD guarantee · separate Run review required",
        "coût inconnu déclaré · une inférence unique et finie · aucune garantie en USD · une revue du run à part est requise",
    ),
    (
        "catalog allowance tracked separately",
        "allocation du catalogue suivie à part",
    ),
    (
        "blocked by the Session monetary constraint",
        "bloquée par la contrainte d'argent de la session",
    ),
    (
        "no Session budget; no aggregate inference allowance; qualified priced calls observed at catalog estimates, never capped",
        "aucun budget de session ; aucune allocation globale d'inférence ; chaque appel tarifé qualifié est observé aux estimations du catalogue, sans plafond",
    ),
];

/// `text`, each line it knows said in French.
#[must_use]
pub fn lines(text: &str) -> String {
    let review = text.contains(REVIEW);
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut effects = false;
    for line in text.split_inclusive('\n') {
        let (body, end) = line
            .strip_suffix('\n')
            .map_or((line, ""), |body| (body, "\n"));
        // The rows under « when it runs: » are the effect rows of a check.
        let said = match body.strip_prefix("    · ") {
            Some(row) if effects && !review => format!("    · {}", effect_row_fr(row)),
            _ if review => money_fr(body).unwrap_or_else(|| body.to_owned()),
            _ => {
                effects = body == "  when it runs:";
                line_fr(body)
            }
        };
        out.push_str(&said);
        out.push_str(end);
    }
    out
}

/// One of the money lines in French, when `line` is one.
fn money_fr(line: &str) -> Option<String> {
    money(line)
        .or_else(|| inference(line))
        .or_else(|| quoted(line))
}

/// One line in French, when it is one of the Session's own.
fn line_fr(line: &str) -> String {
    // A workflow's own bytes are never said otherwise.
    if line.starts_with("  │ ") && !line.starts_with("  │ tasks: ") {
        return line.to_owned();
    }
    if let Some((_, french)) = WHOLE.iter().find(|(english, _)| *english == line) {
        return (*french).to_owned();
    }
    let said = money(line)
        .or_else(|| inference(line))
        .or_else(|| changed(line))
        .or_else(|| check(line))
        .or_else(|| quoted(line));
    said.unwrap_or_else(|| line.to_owned())
}

/// `money: $… USD · <source> · proposal/Run ceiling · <project> · …`.
fn money(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("money: ")?
        .strip_suffix(" · policy cap unknown · machine cap unknown · execution requires a separate Run and downstream admission")?;
    let (amount, rest) = rest.split_once(" · ")?;
    let (source, project) = rest.split_once(" · proposal/Run ceiling · ")?;
    let source = SOURCES
        .iter()
        .find(|(english, _)| *english == source)
        .map(|(_, french)| *french)?;
    let project = match project {
        "no project default" => "aucun défaut de projet".to_owned(),
        other => format!(
            "défaut du projet {}",
            other.strip_prefix("project default ")?
        ),
    };
    Some(format!(
        "argent : {amount} · {source} · plafond de la proposition et du run · {project} · plafond de la politique inconnu · plafond de la machine inconnu · l'exécution demande un run à part et son admission"
    ))
}

/// `Session inference: <how>; separate from the proposal/Run ceiling; …`.
fn inference(line: &str) -> Option<String> {
    let how = line
        .strip_prefix("Session inference: ")?
        .strip_suffix("; separate from the proposal/Run ceiling; catalog estimates are not invoices or a hard billing cap; billed cost unknown")?;
    let how = match INFERENCE.iter().find(|(english, _)| *english == how) {
        Some((_, french)) => (*french).to_owned(),
        None => format!(
            "allocation du catalogue {}",
            how.strip_prefix("catalog allowance ")?
        ),
    };
    Some(format!(
        "inférence de la session (la préparation par Nika) : {how} ; à part du plafond de la proposition et du run ; les estimations du catalogue ne sont ni des factures ni un plafond de facturation ; coût facturé inconnu"
    ))
}

/// The lines of a change: what it creates or replaces, and the tasks it holds.
fn changed(line: &str) -> Option<String> {
    if let Some(goal) = line.strip_prefix("proposed change · ") {
        return Some(format!("changement proposé · {goal}"));
    }
    if let Some(rest) = line.strip_prefix("  creates `") {
        let (path, count) = rest.rsplit_once("` (")?;
        let count = count.strip_suffix(" lines)")?;
        return Some(format!("  crée `{path}` ({count} lignes)"));
    }
    if let Some(rest) = line.strip_prefix("  replaces `") {
        let (path, rest) = rest.split_once("` whole (")?;
        let (count, witness) = rest.split_once(" lines · the file as it is now is witnessed ")?;
        let witness = witness.strip_suffix(')')?;
        return Some(format!(
            "  remplace `{path}` en entier ({count} lignes · le fichier tel qu'il est maintenant est attesté {witness})"
        ));
    }
    if let Some(count) =
        (line.strip_prefix("  │ tasks: ")).and_then(|r| r.strip_suffix(" (in run order above)"))
    {
        return Some(format!(
            "  │ tasks: {count} (dans l'ordre du run, ci-dessus)"
        ));
    }
    let count = line
        .strip_prefix("  └─ `/show` prints the exact ")?
        .strip_suffix(" lines")?;
    Some(format!("  └─ `/show` affiche les {count} lignes exactes"))
}

/// The check line, `  check of these bytes · <path> · clean ✔`, or its findings.
fn check(line: &str) -> Option<String> {
    let rest = line.strip_prefix("  check of these bytes · ")?;
    let (path, verdict) = rest.rsplit_once(" · ")?;
    let verdict = match verdict {
        "clean ✔" => "propre ✔",
        "findings ✖" => "constats ✖",
        _ => return None,
    };
    Some(format!("  vérification de ces octets · {path} · {verdict}"))
}

/// The words a decision binds: `monetary input: «…»`, `request: «…»`.
fn quoted(line: &str) -> Option<String> {
    if let Some(words) = line.strip_prefix("monetary input: «") {
        return Some(format!("montant indiqué : «{words}"));
    }
    line.strip_prefix("request: «")
        .map(|words| format!("demande : «{words}"))
}

#[cfg(test)]
mod tests {
    use super::lines;

    /// The Session's own lines of a proposal read in French, its values and the workflow's
    /// bytes as written; a line it does not know stays as it is.
    #[test]
    fn a_proposal_reads_in_french_with_its_values_and_bytes_as_written() {
        let english = "proposed change · le digest du jour\n  creates `digest.nika` (12 lines)\n  ┌─ `digest.nika`\n  │ nika: digest\n  │ tasks: 3 (in run order above)\n  └─ `/show` prints the exact 12 lines\n  check of these bytes · `digest.nika` · clean ✔\n  when it runs:\n    · reads ./notes/*.md\n    · model output estimate ≤ $0.0246 at catalog prices · input tokens and other charges excluded\napply this? (yes applies · no discards · questions keep it pending · nothing is written until you say yes)\nmoney: $0.25 USD · session default · proposal/Run ceiling · no project default · policy cap unknown · machine cap unknown · execution requires a separate Run and downstream admission\nSession inference: no Session budget; no aggregate inference allowance; qualified priced calls observed at catalog estimates, never capped; separate from the proposal/Run ceiling; catalog estimates are not invoices or a hard billing cap; billed cost unknown\nrequest: «résume les notes»\nan unknown line\n";
        let french = "changement proposé · le digest du jour\n  crée `digest.nika` (12 lignes)\n  ┌─ `digest.nika`\n  │ nika: digest\n  │ tasks: 3 (dans l'ordre du run, ci-dessus)\n  └─ `/show` affiche les 12 lignes exactes\n  vérification de ces octets · `digest.nika` · propre ✔\n  à l'exécution :\n    · lit ./notes/*.md\n    · estimation de la sortie du modèle ≤ $0.0246 aux prix du catalogue · jetons d'entrée et autres frais exclus\nappliquer ? (« oui » applique · « non » écarte · une question la garde en attente · rien n'est écrit avant votre « oui »)\nargent : $0.25 USD · défaut de la session · plafond de la proposition et du run · aucun défaut de projet · plafond de la politique inconnu · plafond de la machine inconnu · l'exécution demande un run à part et son admission\ninférence de la session (la préparation par Nika) : aucun budget de session ; aucune allocation globale d'inférence ; chaque appel tarifé qualifié est observé aux estimations du catalogue, sans plafond ; à part du plafond de la proposition et du run ; les estimations du catalogue ne sont ni des factures ni un plafond de facturation ; coût facturé inconnu\ndemande : «résume les notes»\nan unknown line\n";
        assert_eq!(lines(english), french);
    }

    /// A compiled request's review keeps its own words: only its money lines are said in French.
    #[test]
    fn a_review_says_only_its_money_in_french() {
        let review = "Nika proposes `x.nika`:\nCan touch\n  external effects · none\n  creates `x.nika` (3 lines)\nmoney: $0.25 USD · session default · proposal/Run ceiling · no project default · policy cap unknown · machine cap unknown · execution requires a separate Run and downstream admission\n";
        let said = lines(review);
        assert!(said.contains("  creates `x.nika` (3 lines)"), "{said}");
        assert!(
            said.contains("argent : $0.25 USD · défaut de la session"),
            "{said}"
        );
    }

    /// A workflow's bytes are never translated, even when a line reads as one of the Session's.
    #[test]
    fn a_workflow_line_is_never_said_otherwise() {
        let text = "  │ when it runs:\n  │ request: «x»\n";
        assert_eq!(lines(text), text);
    }
}
