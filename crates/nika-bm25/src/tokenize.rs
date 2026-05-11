// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Maison tokenizer · `std::char` only · zero dep.
//!
//! Discipline · lowercase ASCII split on non-alphanumeric · keeps the
//! algorithm fully deterministic without `regex` / `unicode-segmentation`
//! deps (per ADR-038 zero-ML-dep posture + Gallant `ripgrep` 0-alloc
//! ratchet BLUEPRINT v1.5).
//!
//! For richer Unicode segmentation (NFKD · CJK · etc.) external crates can
//! pre-tokenize and feed the iterator API directly · see `BmIndex::with_iter`.

/// Tokenize a string slice into lowercase alphanumeric word tokens.
///
/// Splits on any character where [`char::is_alphanumeric`] is `false`.
/// Empty tokens are skipped. ASCII case-folding via [`char::to_ascii_lowercase`].
#[must_use]
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            buf.push(ch.to_ascii_lowercase());
        } else if !buf.is_empty() {
            out.push(std::mem::take(&mut buf));
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_basic() {
        assert_eq!(
            tokenize("auto car insurance"),
            vec!["auto", "car", "insurance"]
        );
    }

    #[test]
    fn lowercase_fold() {
        assert_eq!(
            tokenize("AUTO Car INSURANCE"),
            vec!["auto", "car", "insurance"]
        );
    }

    #[test]
    fn split_on_punct() {
        assert_eq!(
            tokenize("auto, car; insurance!"),
            vec!["auto", "car", "insurance"]
        );
    }

    #[test]
    fn empty_input() {
        assert!(tokenize("").is_empty());
    }

    #[test]
    fn whitespace_only() {
        assert!(tokenize("   \t\n  ").is_empty());
    }

    #[test]
    fn numeric_kept() {
        assert_eq!(tokenize("doc42 v3.0"), vec!["doc42", "v3", "0"]);
    }
}
