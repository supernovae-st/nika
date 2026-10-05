// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Remove only explicit access options before interpreting filenames and inputs.
//! The canonical access resolver judges the value; no vocabulary is copied here.

/// Return the remaining Run words and the explicit pin; invalid or duplicate options refuse.
#[must_use]
pub fn parse(input: &str) -> Option<(String, Option<String>)> {
    let mut words = input.split_whitespace();
    let mut line = Vec::new();
    let mut access = None;
    let mut cost = false;
    while let Some(word) = words.next() {
        if !word.starts_with('-') {
            line.push(word);
            continue;
        }
        let (flag, inline) = word
            .split_once('=')
            .map_or((word, None), |(k, v)| (k, Some(v)));
        let used = match flag {
            "--access" => access.is_some(),
            "--max-cost-usd" => cost,
            _ => return None,
        };
        let value = inline.or_else(|| words.next())?;
        if used || value.is_empty() || value.starts_with('-') {
            return None;
        }
        if flag == "--access" {
            access = Some(value.to_owned());
        } else {
            cost = true;
            line.push(word);
            if inline.is_none() {
                line.push(value);
            }
        }
    }
    Some((line.join(" "), access))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn explicit_pin_is_removed_from_words_but_kept_verbatim() {
        for value in ["codex", "claude-code", "mock", "other.nika"] {
            for flag in [format!("--access {value}"), format!("--access={value}")] {
                assert_eq!(
                    parse(&format!("run one.nika {flag} name=Nika --max-cost-usd 0.1")),
                    Some((
                        "run one.nika name=Nika --max-cost-usd 0.1".into(),
                        Some(value.into())
                    ))
                );
            }
        }
        assert_eq!(parse("run one.nika"), Some(("run one.nika".into(), None)));
    }

    #[test]
    fn invalid_flags_cannot_be_ignored_or_turn_into_filenames() {
        for line in [
            "run it --access",
            "run it --access=",
            "run it --access --max-cost-usd 1",
            "run it --access=codex --access=codex",
            "run it --access codex --access=api",
            "run it --unknown=codex",
            "run it --access-extra=codex",
            "run it --max-cost-usd=1 --max-cost-usd 2",
            "run it --max-cost-usd",
            "run it --",
            "run it -x",
        ] {
            assert!(parse(line).is_none(), "{line}");
        }
    }
}
