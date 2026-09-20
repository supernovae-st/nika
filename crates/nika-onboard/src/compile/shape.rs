// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The structural shape a plan and its request carry: which instruction is an operation
//! (a numeric rule is code, never prompt guidance), which work is distributive (one draft
//! per item), which lookup selects one record by an identifier. Everything here is
//! deterministic text evidence over the plan's own elements and the request's verbatim
//! words; nothing invents an element, and every promoted element keeps an exact excerpt.

use super::plan::{Op, Plan, Step};

/// Comparison cues (EN · FR · ES · IT · PT · DE, diacritics folded) that make the number
/// beside them a rule for code. Longest phrases first so a window matches whole.
const COMPARISON_CUES: &[&str] = &[
    "strictly greater than",
    "strictly less than",
    "strictly greater",
    "strictly less",
    "greater than",
    "more than",
    "less than",
    "fewer than",
    "at least",
    "at most",
    "above",
    "below",
    "over",
    "under",
    "exceeds",
    "exceeding",
    "plus grand que",
    "plus grande que",
    "plus grands que",
    "plus grandes que",
    "plus petit que",
    "plus petite que",
    "superieur a",
    "superieure a",
    "superieurs a",
    "superieures a",
    "inferieur a",
    "inferieure a",
    "inferieurs a",
    "inferieures a",
    "au moins",
    "au plus",
    "plus de",
    "moins de",
    "mayor que",
    "mayores que",
    "mayor a",
    "menor que",
    "menores que",
    "menor a",
    "mas de",
    "menos de",
    "maggiore di",
    "maggiori di",
    "minore di",
    "minori di",
    "piu di",
    "meno di",
    "superiore a",
    "inferiore a",
    "maior que",
    "menor que",
    "mais de",
    "menos de",
    "grosser als",
    "kleiner als",
    "mehr als",
    "weniger als",
    "mindestens",
    "hochstens",
];

/// Symbols that compare the number beside them.
const COMPARISON_SYMBOLS: &[&str] = &[">=", "<=", "≥", "≤", ">", "<"];

/// A number followed by a size unit bounds prose, not data.
const SIZE_UNITS: &[&str] = &[
    "word",
    "words",
    "mot",
    "mots",
    "line",
    "lines",
    "ligne",
    "lignes",
    "bullet",
    "bullets",
    "puce",
    "puces",
    "sentence",
    "sentences",
    "phrase",
    "phrases",
    "character",
    "characters",
    "chars",
    "caractere",
    "caracteres",
    "paragraph",
    "paragraphs",
    "paragraphe",
    "paragraphes",
    "palabra",
    "palabras",
    "linea",
    "lineas",
    "oracion",
    "oraciones",
    "frase",
    "frasi",
    "parola",
    "parole",
    "riga",
    "righe",
    "caratteri",
    "wort",
    "worter",
    "zeile",
    "zeilen",
    "satz",
    "satze",
    "zeichen",
    "token",
    "tokens",
    "page",
    "pages",
];

/// A number followed by an attempt or turn unit bounds a loop, not data.
const ATTEMPT_UNITS: &[&str] = &[
    "attempt",
    "attempts",
    "try",
    "tries",
    "retry",
    "retries",
    "turn",
    "turns",
    "time",
    "times",
    "fois",
    "essai",
    "essais",
    "tentative",
    "tentatives",
    "tour",
    "tours",
    "iteration",
    "iterations",
    "round",
    "rounds",
    "cycle",
    "cycles",
    "intento",
    "intentos",
    "vuelta",
    "vueltas",
    "tentativo",
    "tentativi",
    "versuch",
    "versuche",
    "runde",
    "runden",
];

/// Lowercase with Latin diacritics folded to ASCII, so every table matches one spelling.
fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => out.push('a'),
            'ç' => out.push('c'),
            'è' | 'é' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' | 'í' => out.push('i'),
            'ô' | 'ö' | 'ó' | 'õ' => out.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => out.push('u'),
            'ñ' => out.push('n'),
            'ß' => out.push_str("ss"),
            other => out.push(other),
        }
    }
    out
}

/// One token of a folded constraint: a word, a number or a comparison symbol, with the
/// surrounding punctuation dropped and a symbol glued to a number split off.
fn tokens(folded: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in folded.split_whitespace() {
        let word = raw.trim_matches(|c: char| {
            matches!(
                c,
                '.' | ',' | ';' | ':' | '(' | ')' | '"' | '\'' | '«' | '»' | '!' | '?'
            )
        });
        if word.is_empty() {
            continue;
        }
        if COMPARISON_SYMBOLS.contains(&word) {
            out.push(word.to_owned());
            continue;
        }
        let symbol = COMPARISON_SYMBOLS
            .iter()
            .find(|s| word.starts_with(*s) && word.len() > s.len());
        if let Some(symbol) = symbol {
            out.push((*symbol).to_owned());
            out.push(word[symbol.len()..].to_owned());
        } else {
            out.push(word.to_owned());
        }
    }
    out
}

fn is_number(token: &str) -> bool {
    let digits = token.trim_start_matches(['-', '+', '€', '$']);
    let digits = digits.trim_end_matches(['%', '€', '$']);
    !digits.is_empty()
        && digits.starts_with(|c: char| c.is_ascii_digit())
        && digits
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ','))
}

/// A digit beside a comparison cue, not bounded by a size, attempt or turn unit, and
/// not the concurrency bound the assembler already consumes: a rule that must run as code.
pub(super) fn numeric_rule(text: &str) -> bool {
    if super::bindings::parallel_bound(text).is_some() {
        return false;
    }
    let folded = fold(text);
    let words = tokens(&folded);
    for (index, token) in words.iter().enumerate() {
        if !is_number(token) {
            continue;
        }
        let after = words.get(index + 1).map(String::as_str).unwrap_or_default();
        let after2 = words.get(index + 2).map(String::as_str).unwrap_or_default();
        let unit = |w: &str| SIZE_UNITS.contains(&w) || ATTEMPT_UNITS.contains(&w);
        if unit(after) || (matches!(after, "a" | "de" | "di" | "of") && unit(after2)) {
            continue;
        }
        let before = words.get(index.wrapping_sub(1)).map(String::as_str);
        let symbol_beside = before.is_some_and(|w| COMPARISON_SYMBOLS.contains(&w))
            || COMPARISON_SYMBOLS.contains(&after);
        let phrase_before = (1..=3).any(|width| {
            index >= width && {
                let phrase = words[index - width..index].join(" ");
                COMPARISON_CUES.contains(&phrase.as_str())
            }
        });
        if symbol_beside || phrase_before {
            return true;
        }
    }
    false
}

/// Every numeric-rule constraint becomes a compute step anchored in the request, inserted
/// right after the last source step so every later step sees the computed result, and
/// leaves the prompt guidance. A constraint that is not a verbatim excerpt of the request
/// stays a constraint (nothing is invented); a compute step that already carries the rule
/// is not duplicated. Applying this twice changes nothing.
pub(super) fn promote_numeric_rules(plan: &mut Plan, intent: &str) {
    let rules: Vec<String> = plan
        .constraints
        .iter()
        .filter(|c| numeric_rule(c))
        .cloned()
        .collect();
    for constraint in rules {
        let detail = constraint.trim().to_owned();
        let carried = plan
            .steps
            .iter()
            .any(|s| s.op == Op::Compute && s.detail.contains(detail.as_str()));
        if !carried {
            let Some(evidence) = super::cognition::exact_excerpt(intent, &constraint) else {
                continue;
            };
            if let Some(existing) = plan.steps.iter_mut().find(|s| s.op == Op::Compute) {
                if !existing.detail.is_empty() {
                    existing.detail.push_str(" ; ");
                }
                existing.detail.push_str(&detail);
            } else {
                let at = plan
                    .steps
                    .iter()
                    .rposition(|s| matches!(s.op, Op::Read | Op::Fetch | Op::Lookup | Op::Search))
                    .map_or(0, |i| i + 1);
                plan.steps.insert(
                    at,
                    Step {
                        op: Op::Compute,
                        evidence,
                        detail,
                        categories: Vec::new(),
                    },
                );
            }
        }
        plan.constraints.retain(|c| c != &constraint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(op: Op, detail: &str, evidence: &str) -> Step {
        Step {
            op,
            evidence: evidence.to_owned(),
            detail: detail.to_owned(),
            categories: Vec::new(),
        }
    }

    #[test]
    fn a_numeric_rule_is_a_digit_beside_a_comparison_cue() {
        for rule in [
            "keep only the rows whose amount is strictly greater than 100",
            "whose total_eur is above 120",
            "products with fewer than 10 units",
            "orders whose total is at least 50 EUR",
            "amount > 100",
            "amount >= 100",
            "montant ≥ 100",
            "quantité inférieure à 10",
            "les commandes dont le montant est plus grand que 200",
            "au moins 3 articles en stock",
            "productos con menos de 10 unidades",
            "cantidad mayor que 5",
            "articoli con quantità minore di 5",
            "Bestellungen mit Betrag größer als 100",
            "mindestens 3 Stück",
        ] {
            assert!(numeric_rule(rule), "{rule}");
        }
        for guidance in [
            "a brief of under 150 words",
            "un résumé de chaque en 3 lignes max",
            "at most 3 attempts",
            "no more than 5 turns",
            "Process at most 2 products at a time",
            "traite 3 fichiers à la fois",
            "the top 3 countries",
            "as 5 bullets",
            "in a warm tone",
            "at most 2 pages",
            "au plus 4 phrases",
            "never retry more than 3 times",
            "at most two of them",
        ] {
            assert!(!numeric_rule(guidance), "{guidance}");
        }
    }

    #[test]
    fn a_numeric_rule_constraint_becomes_a_compute_step_after_the_sources() {
        let intent = "Read ./data/orders.csv, keep only the rows whose amount is strictly greater than 100, and write ./out/summary.md with one line stating the total.";
        let rule = "keep only the rows whose amount is strictly greater than 100";
        let mut plan = Plan {
            steps: vec![
                step(Op::Read, "./data/orders.csv", "Read ./data/orders.csv"),
                step(
                    Op::Draft,
                    "one line stating the total",
                    "one line stating the total",
                ),
            ],
            constraints: vec![rule.to_owned(), "in a warm tone".to_owned()],
            ..Plan::default()
        };
        promote_numeric_rules(&mut plan, intent);
        let ops: Vec<Op> = plan.steps.iter().map(|s| s.op).collect();
        assert_eq!(ops, [Op::Read, Op::Compute, Op::Draft]);
        assert_eq!(plan.steps[1].detail, rule);
        assert_eq!(plan.steps[1].evidence, rule);
        assert_eq!(plan.constraints, ["in a warm tone"]);
        // Idempotent.
        let once = plan.clone();
        promote_numeric_rules(&mut plan, intent);
        assert_eq!(plan, once);
        // A rule wrapped by the model is anchored through its folded excerpt.
        let mut wrapped = Plan {
            steps: vec![step(
                Op::Read,
                "./data/orders.csv",
                "Read ./data/orders.csv",
            )],
            constraints: vec![
                "keep only the rows whose  amount is strictly\ngreater than 100".to_owned(),
            ],
            ..Plan::default()
        };
        promote_numeric_rules(&mut wrapped, intent);
        assert_eq!(wrapped.steps[1].evidence, rule);
        assert!(wrapped.constraints.is_empty());
        // A rule the request never spelled stays guidance: nothing is invented.
        let mut foreign = Plan {
            steps: vec![step(
                Op::Read,
                "./data/orders.csv",
                "Read ./data/orders.csv",
            )],
            constraints: vec!["amount above 500".to_owned()],
            ..Plan::default()
        };
        promote_numeric_rules(&mut foreign, intent);
        assert_eq!(foreign.steps.len(), 1);
        assert_eq!(foreign.constraints, ["amount above 500"]);
        // No source step: the rule leads the plan.
        let mut sourceless = Plan {
            steps: vec![step(Op::Draft, "the total", "the total")],
            constraints: vec![rule.to_owned()],
            ..Plan::default()
        };
        promote_numeric_rules(&mut sourceless, intent);
        assert_eq!(sourceless.steps[0].op, Op::Compute);
        // An existing compute step absorbs a second rule instead of a second step.
        let mut two = Plan {
            steps: vec![
                step(Op::Read, "./data/orders.csv", "Read ./data/orders.csv"),
                step(Op::Compute, "the total", "the total"),
            ],
            constraints: vec![rule.to_owned()],
            ..Plan::default()
        };
        promote_numeric_rules(&mut two, intent);
        assert_eq!(two.steps.len(), 2);
        assert_eq!(two.steps[1].detail, format!("the total ; {rule}"));
    }
}
