// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The column names a request states beside its source ("columns
//! `order_id,customer,amount,status`", "colonnes …", "`(sku,nome,quantidade,minimo)`"): the
//! only place a plain word may be read as a column of the parsed records.

use super::rules::identifier_shaped;
use super::shape::fold;

/// Words that introduce a columns hint in the request.
const COLUMN_WORDS: &[&str] = &[
    "columns", "column", "colonnes", "colonne", "columnas", "columna", "colunas", "coluna",
    "spalten", "spalte", "headers", "header", "fields", "champs", "campos", "campi", "felder",
];

/// Connectors skipped between a columns word and its list.
const CONNECTORS: &[&str] = &[
    ":", "=", "-", "are", "sont", "son", "sono", "sind", "is", "est",
];
fn column_shaped(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_alphabetic)
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
}

/// The comma-separated list of column names starting at `from`, closed by a token
/// without a trailing comma or by a closing parenthesis.
fn list_after(words: &[&str], from: usize) -> Option<Vec<String>> {
    let mut at = from;
    while words
        .get(at)
        .is_some_and(|w| CONNECTORS.contains(&fold(w).as_str()))
    {
        at += 1;
    }
    let mut out = Vec::new();
    while let Some(raw) = words.get(at) {
        let closes = raw.contains(')');
        let core = raw.trim_matches(|c: char| {
            matches!(
                c,
                '(' | ')' | '.' | ';' | ':' | ',' | '"' | '\'' | '«' | '»'
            )
        });
        for part in core.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            if !column_shaped(part) {
                return None;
            }
            out.push(part.to_owned());
        }
        let continues = !closes && raw.trim_end_matches([')', '.', ';', ':']).ends_with(',');
        if !continues {
            break;
        }
        at += 1;
    }
    let single_identifier = matches!(out.as_slice(), [only] if identifier_shaped(only));
    (out.len() >= 2 || single_identifier).then_some(out)
}

/// A parenthesized comma list of column-shaped names, `(sku,nome,quantidade,minimo)`.
fn parenthesized(text: &str) -> Option<Vec<String>> {
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let inner_from = rest.get(open + 1..)?;
        let close = inner_from.find(')')?;
        let inner = inner_from.get(..close)?;
        if inner.contains(',') {
            let parts: Vec<String> = inner.split(',').map(str::trim).map(str::to_owned).collect();
            if parts.len() >= 2 && parts.iter().all(|p| column_shaped(p)) {
                return Some(parts);
            }
        }
        rest = inner_from.get(close + 1..)?;
    }
    None
}

/// The column names the request states ("columns `order_id,customer,amount,status`",
/// "colonnes …", "(sku,nome,quantidade,minimo)"), in their own spelling; empty when the
/// request states none.
pub(super) fn columns_hint(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for (at, word) in words.iter().enumerate() {
        let folded = fold(word.trim_matches(|c: char| !c.is_alphanumeric()));
        if COLUMN_WORDS.contains(&folded.as_str())
            && let Some(list) = list_after(&words, at + 1)
        {
            return list;
        }
    }
    parenthesized(text).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn the_columns_hint_is_the_list_the_request_states() {
        assert_eq!(
            columns_hint(
                "Read ./data/orders.csv (columns order_id,customer,amount,status), keep only the rows"
            ),
            cols(&["order_id", "customer", "amount", "status"])
        );
        assert_eq!(
            columns_hint(
                "Lis ./data/commandes.csv, colonnes : numero, client, montant, statut. Garde"
            ),
            cols(&["numero", "client", "montant", "statut"])
        );
        assert_eq!(
            columns_hint("Leia ./dados/estoque.csv (sku,nome,quantidade,minimo) e escreva"),
            cols(&["sku", "nome", "quantidade", "minimo"])
        );
        assert_eq!(
            columns_hint("the columns are Order ID, amount and status"),
            cols(&["Order"])
                .into_iter()
                .filter(|_| false)
                .collect::<Vec<_>>(),
            "a spaced name is not one column-shaped token"
        );
        assert_eq!(
            columns_hint("the column amount_eur holds euros"),
            cols(&["amount_eur"])
        );
        assert_eq!(columns_hint("the column is empty"), Vec::<String>::new());
        assert_eq!(
            columns_hint("Read ./data/orders.csv and keep the rows"),
            Vec::<String>::new()
        );
        assert_eq!(columns_hint("call f(x) then g(a, b)"), cols(&["a", "b"]));
    }
}
