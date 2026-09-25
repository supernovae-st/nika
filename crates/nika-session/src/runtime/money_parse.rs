// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One lexical money reader for Prepare and Run. Quotes and path tokens are
//! data. Only a currency or monetary anchor gives a number monetary meaning.

pub(super) const INVALID: &str = "the monetary ceiling must be a finite, nonnegative amount in USD — no work was prepared or run";
const CONFLICTING: &str =
    "the request names different monetary ceilings — name one finite, nonnegative amount in USD";

#[derive(Default)]
pub(super) struct ParsedMoney {
    pub amount: Option<f64>,
    pub literal: Option<String>,
    pub replaced_default: Option<f64>,
    pub unbound_number: bool,
    pub money_only: bool,
}

fn amount(raw: &str) -> Result<f64, &'static str> {
    let raw = raw.strip_prefix('$').unwrap_or(raw);
    let normalized = raw.replace(',', ".");
    let value = normalized.parse::<f64>().map_err(|_| INVALID)?;
    if value.is_finite() && value >= 0.0 {
        Ok(value.abs())
    } else {
        Err(INVALID)
    }
}

fn currency(word: &str) -> bool {
    matches!(word, "$" | "usd" | "dollar" | "dollars")
}

fn anchor(word: &str) -> bool {
    matches!(
        word,
        "budget" | "ceiling" | "plafond" | "cap" | "cost" | "--max-cost-usd"
    )
}

fn filler(word: &str) -> bool {
    matches!(
        word,
        "of" | "to" | "at" | "under" | "de" | "à" | "is" | ":" | "="
    )
}

fn data(word: &str) -> bool {
    word.contains('/')
        || word.contains('\\')
        || word.contains('=')
        || (word.contains('.') && !word.starts_with('$') && word.parse::<f64>().is_err())
}

/// Blank quoted data. An apostrophe inside
/// a word (l'abonnement) is not the opening of a string.
fn outside_quotes(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut closing = None;
    let mut escaped = false;
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if let Some(end) = closing {
                if !escaped && c == end {
                    closing = None;
                }
                escaped = !escaped && c == '\\';
                ' '
            } else {
                closing = match c {
                    '"' | '`' => Some(c),
                    '\'' if i == 0 || !chars[i - 1].is_alphanumeric() => Some(c),
                    '«' => Some('»'),
                    '“' => Some('”'),
                    _ => None,
                };
                if closing.is_some() { ' ' } else { c }
            }
        })
        .collect()
}

fn token(raw: &str) -> &str {
    let raw = raw.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | ':' | '?' | '!'));
    raw.strip_suffix('.').unwrap_or(raw)
}

fn without_currency(raw: &str) -> &str {
    for suffix in ["dollars", "dollar", "usd"] {
        if raw.len() > suffix.len()
            && raw
                .get(raw.len() - suffix.len()..)
                .is_some_and(|end| end.eq_ignore_ascii_case(suffix))
        {
            return &raw[..raw.len() - suffix.len()];
        }
    }
    raw
}

/// Compact monetary syntax has to be recognized before the generic data
/// exclusion for assignments. File names and paths remain data; a decimal
/// monetary literal (including an overflowing exponent) is not a file name.
fn compact_literal(raw: &str) -> Option<&str> {
    if anchor(&raw.to_lowercase()) || raw.contains('/') || raw.contains('\\') {
        return None;
    }
    let value = if let Some((name, value)) = raw.split_once(['=', ':']) {
        if !anchor(&name.to_lowercase()) {
            return None;
        }
        value
    } else {
        raw
    };
    let literal = without_currency(value);
    // Recognized attached currency commits this token to amount validation:
    // malformed money cannot fall back to the dotted-filename exception.
    if literal != value {
        return Some(literal);
    }
    if value == raw {
        return None;
    }
    if literal.contains('.')
        && literal.replace(',', ".").parse::<f64>().is_err()
        && !literal
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | 'e' | 'E' | '+' | '-'))
        && !matches!(
            literal.to_lowercase().as_str(),
            ".nan" | ".inf" | "-.inf" | "+.inf"
        )
    {
        return None;
    }
    Some(literal)
}

pub(super) fn parse(input: &str) -> Result<ParsedMoney, &'static str> {
    let visible = outside_quotes(input);
    let tokens: Vec<&str> = visible.split_whitespace().map(token).collect();
    let lower: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    let mut used = vec![false; tokens.len()];
    let mut parsed = ParsedMoney::default();
    let replacement = lower
        .iter()
        .position(|t| matches!(t.as_str(), "replace" | "replaces" | "override" | "remplace"));
    for (i, word) in lower.iter().enumerate() {
        if used[i] {
            continue;
        }
        let compact = compact_literal(tokens[i]);
        let index = if compact.is_some()
            || word.starts_with("--max-cost-usd=")
            || (word.starts_with('$') && word != "$" && !data(word))
        {
            Some(i)
        } else if word.starts_with("--max-cost-usd") && word != "--max-cost-usd" {
            return Err(INVALID);
        } else if anchor(word) || currency(word) {
            if currency(word) && i > 0 && used[i - 1] {
                continue;
            }
            if currency(word)
                && i > 0
                && !data(&lower[i - 1])
                && (lower[i - 1].parse::<f64>().is_ok()
                    || lower[i - 1]
                        .starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '$')))
            {
                Some(i - 1)
            } else {
                let mut end = i + 1;
                while lower.get(end).is_some_and(|w| filler(w) || currency(w)) {
                    end += 1;
                }
                Some(end)
            }
        } else {
            None
        };
        let Some(index) = index else { continue };
        let raw = compact.unwrap_or(without_currency(tokens.get(index).ok_or(INVALID)?));
        let raw = raw.strip_prefix("--max-cost-usd=").unwrap_or(raw);
        let value = amount(raw)?;
        let default_reference = replacement.is_some_and(|r| r < index)
            && lower[..index]
                .iter()
                .rposition(|w| matches!(w.as_str(), "default" | "défaut"))
                .is_some_and(|d| lower[d + 1..index].iter().all(|w| filler(w)));
        if default_reference {
            if parsed
                .replaced_default
                .is_some_and(|p| p.to_bits() != value.to_bits())
            {
                return Err(CONFLICTING);
            }
            parsed.replaced_default = Some(value);
        } else {
            if parsed
                .amount
                .is_some_and(|p| p.to_bits() != value.to_bits())
            {
                return Err(CONFLICTING);
            }
            parsed.amount = Some(value);
            parsed.literal.get_or_insert_with(|| raw.to_owned());
        }
        used[i.min(index)..=i.max(index)].fill(true);
    }
    parsed.unbound_number = lower.iter().enumerate().any(|(i, w)| {
        !used[i]
            && !data(w)
            && (w.parse::<f64>().is_ok() || w.starts_with(|c: char| c.is_ascii_digit()))
    });
    parsed.money_only = parsed.amount.is_some()
        && lower
            .iter()
            .enumerate()
            .all(|(i, w)| used[i] || currency(w) || filler(w) || w.is_empty());
    Ok(parsed)
}
