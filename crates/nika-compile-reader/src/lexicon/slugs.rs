// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Constant names derived from verbatim phrases: ASCII, articles dropped, at most three
//! tokens, cut at the first connector (`le client dans le CRM` → `client`).

use super::cues::ARTICLES;
use super::normalize;

/// Slug of a verbatim phrase for a constant name: ASCII letters, articles dropped, at most three tokens.
#[must_use]
pub fn slug(phrase: &str) -> String {
    let lower = normalize(phrase);
    let cut = [
        " dans ",
        " in ",
        " from ",
        " depuis ",
        " sur ",
        " on ",
        " to ",
        " vers ",
        " pour ",
        " for ",
        " avec ",
        " with ",
        " correspondant",
        " correspondante",
        " di ",
        " del ",
        " della ",
        " en ",
        " sobre ",
        " con ",
        " per ",
        " para ",
    ]
    .iter()
    .filter_map(|m| lower.find(m))
    .min()
    .unwrap_or(lower.len());
    let head = lower.get(..cut).unwrap_or(&lower);
    let tokens: Vec<String> = head
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .flat_map(|t| t.split('\''))
        .map(|t| {
            t.chars()
                .map(fold_ascii)
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
        })
        .filter(|t| !t.is_empty() && !ARTICLES.contains(&t.as_str()))
        .take(3)
        .collect();
    if tokens.is_empty() {
        "record".to_owned()
    } else {
        tokens.join("_")
    }
}

fn fold_ascii(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' | 'á' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' | 'í' => 'i',
        'ô' | 'ö' | 'ó' => 'o',
        'û' | 'ù' | 'ü' | 'ú' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        c if c.is_ascii_alphanumeric() => c,
        _ => '_',
    }
}
