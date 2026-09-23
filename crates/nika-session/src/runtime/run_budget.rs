// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Explicit monetary intent at the run door. A time or a count is not a
//! budget; malformed money never falls back to the project's default.

const INVALID: &str = "the run ceiling must be a finite, nonnegative amount in USD — for example « run it with a ceiling of $0.25 »; the run was not started";
const AMBIGUOUS: &str = "that number has no explicit monetary meaning — name a ceiling in USD, or describe the timing/change first; the run was not started";
const CONFLICTING: &str =
    "the run names different ceilings — name one amount in USD; the run was not started";

fn token(raw: &str) -> &str {
    let raw = raw.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | '"' | '\'' | '`'));
    raw.strip_suffix('.').unwrap_or(raw)
}

fn amount(raw: &str) -> Result<f64, &'static str> {
    let value = raw
        .strip_prefix('$')
        .unwrap_or(raw)
        .parse::<f64>()
        .map_err(|_| INVALID)?;
    if value.is_finite() && value >= 0.0 {
        // Canonicalize negative zero so equivalent zero ceilings agree.
        Ok(value.abs())
    } else {
        Err(INVALID)
    }
}

fn record(ceiling: &mut Option<f64>, value: f64) -> Result<(), &'static str> {
    if ceiling.is_some_and(|previous| previous.to_bits() != value.to_bits()) {
        return Err(CONFLICTING);
    }
    *ceiling = Some(value);
    Ok(())
}

/// Parse a run's explicit ceiling. Only currency or a monetary anchor
/// grants a number that meaning. Unbound numeric conditions must be
/// clarified before any run, regardless of the conversational label.
pub(super) fn ceiling_in(input: &str) -> Result<Option<f64>, &'static str> {
    let lower = input.to_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().map(token).collect();
    let mut consumed = vec![false; tokens.len()];
    let mut ceiling = None;
    for (index, word) in tokens.iter().copied().enumerate() {
        if consumed[index] {
            continue;
        }
        if let Some(value) = word.strip_prefix("--max-cost-usd=") {
            record(&mut ceiling, amount(value)?)?;
            consumed[index] = true;
            continue;
        }
        if word.starts_with("--max-cost-usd") && word != "--max-cost-usd" {
            return Err(INVALID);
        }
        if word.starts_with('$') && word != "$" {
            record(&mut ceiling, amount(word)?)?;
            consumed[index] = true;
            continue;
        }
        // Currency can also follow an already parsed monetary amount.
        if word == "usd" && index > 0 && consumed[index - 1] {
            continue;
        }
        if !matches!(
            word,
            "ceiling" | "cap" | "cost" | "usd" | "budget" | "plafond" | "$" | "--max-cost-usd"
        ) {
            continue;
        }
        let mut end = index + 1;
        while tokens.get(end).is_some_and(|word| {
            matches!(
                *word,
                "of" | "to" | "at" | "under" | "de" | "à" | "usd" | "$"
            )
        }) {
            end += 1;
        }
        let value = tokens.get(end).ok_or(INVALID)?;
        record(&mut ceiling, amount(value)?)?;
        consumed[index..=end].fill(true);
    }
    for (index, word) in tokens.iter().copied().enumerate() {
        if consumed[index]
            || word.contains('=')
            || std::path::Path::new(word)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("nika"))
        {
            continue;
        }
        // Include times such as 9:00 and 9am, which are not f64 values.
        if word.parse::<f64>().is_ok() || word.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(AMBIGUOUS);
        }
    }
    Ok(ceiling)
}
