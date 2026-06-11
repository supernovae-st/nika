// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Halt conditions — the EOS token **set** + user stop sequences.
//!
//! The #1 correctness bug in local-generation loops (well-documented, e.g.
//! llguidance #305): a family can have **more than one** end token. Qwen3 emits
//! both `<|im_end|>` (151645, turn end) AND `<|endoftext|>` (151643); a loop
//! that breaks on only one risks an **infinite repetition loop** when the model
//! produces the other. So EOS is a SET, never a single id.
//!
//! Stop sequences are matched at the **string** level (on the decoded text),
//! not by token id — safer across tokenizations where a stop string may not be
//! a single token. candle's examples ship no stop-sequence support; this is
//! ours. candle-free + fully unit-tested in the default build.

/// Decides when generation must halt.
#[derive(Debug, Clone, Default)]
pub struct StopController {
    eos_ids: Vec<u32>,
    stop_strings: Vec<String>,
}

impl StopController {
    /// Build from the family EOS id set and the request's stop strings.
    /// Empty stop strings are dropped (an empty `ends_with` matches everything).
    #[must_use]
    pub fn new(eos_ids: impl IntoIterator<Item = u32>, stop_strings: &[String]) -> Self {
        Self {
            eos_ids: eos_ids.into_iter().collect(),
            stop_strings: stop_strings
                .iter()
                .filter(|s| !s.is_empty())
                .cloned()
                .collect(),
        }
    }

    /// True if `token` is any of the family's end tokens (the SET check).
    #[must_use]
    pub fn is_eos(&self, token: u32) -> bool {
        self.eos_ids.contains(&token)
    }

    /// Whether any stop strings are configured — lets a generation loop skip
    /// the per-step decode entirely when there is nothing to match.
    #[must_use]
    pub fn has_stop_strings(&self) -> bool {
        !self.stop_strings.is_empty()
    }

    /// The longest configured stop string in bytes (0 when none) — the
    /// window a generation loop must inspect at the text's tail. Decoding
    /// only a token suffix covering this length (instead of the whole
    /// buffer every step) turns the per-step stop check from O(n²) over a
    /// long generation into O(window).
    #[must_use]
    pub fn max_stop_len(&self) -> usize {
        self.stop_strings.iter().map(String::len).max().unwrap_or(0)
    }

    /// True if the accumulated decoded text ends with any stop sequence.
    #[must_use]
    pub fn hit_stop_string(&self, text: &str) -> bool {
        self.stop_strings.iter().any(|s| text.ends_with(s.as_str()))
    }

    /// When a stop string matched, trim it (and anything after) off the output
    /// so the caller never sees the stop marker itself — the `OpenAI` contract.
    ///
    /// Uses `rfind` (LAST occurrence) to match the `ends_with` direction of
    /// [`Self::hit_stop_string`] — detection fires on the TRAILING marker, so
    /// the trim must cut there too, not at an earlier one in the body (cutting
    /// the first `STOP` in `"say STOP then STOP"` would discard far more than
    /// intended).
    #[must_use]
    pub fn trim_stop<'t>(&self, text: &'t str) -> &'t str {
        for s in &self.stop_strings {
            if let Some(idx) = text.rfind(s.as_str()) {
                return &text[..idx];
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eos_is_a_set_not_a_single_token() {
        // Qwen3's two end tokens — both must halt (the bug-#1 guard).
        let s = StopController::new([151_645, 151_643], &[]);
        assert!(s.is_eos(151_645), "<|im_end|> must halt");
        assert!(s.is_eos(151_643), "<|endoftext|> must halt too");
        assert!(!s.is_eos(42), "an ordinary token must not halt");
    }

    #[test]
    fn max_stop_len_is_the_longest_in_bytes() {
        // The tail-window contract: 0 when none; the LONGEST (in bytes —
        // multi-byte stops like "…" count their UTF-8 length) otherwise.
        assert_eq!(StopController::new([], &[]).max_stop_len(), 0);
        let s = StopController::new([], &["ab".to_owned(), "wxyz".to_owned()]);
        assert_eq!(s.max_stop_len(), 4);
        let multi = StopController::new([], &["…".to_owned()]); // 3 UTF-8 bytes
        assert_eq!(multi.max_stop_len(), 3);
    }

    #[test]
    fn has_stop_strings_reflects_configuration() {
        // Direct pin (mutant killer: →true / →false / delete-!): the loop
        // uses this to skip per-step decoding, so both polarities matter.
        assert!(!StopController::new([1], &[]).has_stop_strings());
        assert!(StopController::new([], &["X".to_owned()]).has_stop_strings());
        // All-empty strings are dropped at construction → still false.
        assert!(!StopController::new([], &[String::new()]).has_stop_strings());
    }

    #[test]
    fn empty_stop_strings_are_dropped() {
        // A bare "" would make ends_with("") match everything → instant halt.
        let s = StopController::new([], &[String::new(), "STOP".to_owned()]);
        assert!(
            !s.hit_stop_string("anything"),
            "empty stop must not match all"
        );
        assert!(s.hit_stop_string("...STOP"));
    }

    #[test]
    fn stop_string_matches_on_suffix() {
        let s = StopController::new([], &["\n\n".to_owned()]);
        assert!(s.hit_stop_string("answer\n\n"));
        assert!(!s.hit_stop_string("answer\n"));
    }

    #[test]
    fn trim_stop_removes_marker_and_tail() {
        let s = StopController::new([], &["END".to_owned()]);
        assert_eq!(s.trim_stop("hello END garbage"), "hello ");
        assert_eq!(s.trim_stop("no marker here"), "no marker here");
    }

    #[test]
    fn trim_stop_cuts_the_trailing_marker_not_an_earlier_one() {
        // Regression (review S3): trim must match the ends_with direction of
        // detection — the LAST occurrence, not the first. Cutting the first
        // would silently discard the middle of legitimate output.
        let s = StopController::new([], &["STOP".to_owned()]);
        assert_eq!(s.trim_stop("say STOP then STOP"), "say STOP then ");
    }
}
