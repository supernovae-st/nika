// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Masked lexical date-time formats, never values or evidence of a valid instant.

use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Counts only values already admitted by the observation's row/path/depth bounds.
#[derive(Default)]
pub(super) struct Temporal {
    sampled: u64,
    formats: BTreeMap<String, u64>,
}

impl Temporal {
    pub(super) fn observe(&mut self, text: Option<&str>) {
        self.sampled += 1;
        let Some(text) = text.filter(|s| s.len() <= 64 && s.is_ascii()) else {
            return;
        };
        let Some((form, offset)) = crate::fidelity::instant_shape(text) else {
            return;
        };
        let offset: String = offset
            .chars()
            .map(|c| if c.is_ascii_digit() { '9' } else { c })
            .collect();
        *self.formats.entry(format!("{form}{offset}")).or_default() += 1;
    }

    pub(super) fn finish(self) -> Value {
        if self.formats.is_empty() {
            return Value::Null;
        }
        let matched: u64 = self.formats.values().sum();
        json!({"sampled": self.sampled, "matched": matched, "formats": self.formats})
    }
}

/// Lexical date-time formats of the supplied sampled slots, every digit masked as `9`,
/// including offset digits. Missing/non-text slots count in `sampled`, not `matched`.
/// Only ASCII ISO-like date-times up to 64 bytes are classified; `Null` means none matched,
/// not that no time exists. No calendar, timezone, offset equality or instant is inferred.
/// The caller owns sampling bounds; this adds no reads and retains no original value.
#[must_use]
pub fn temporal_shapes<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Value {
    let mut counts = Temporal::default();
    for value in values {
        counts.observe(value);
    }
    counts.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_formats_mask_all_digits_and_count_only_recognized_shapes() {
        let values = [
            Some("2042-11-28T07:36"),
            Some("2043-10-27T08:35"),
            Some("2042-11-28T07:36:42Z"),
            Some("2042-11-28T07:36:42+04:00"),
            Some("2042-11-28T07:36:42-02:30"),
            Some("2042-11-28 07:36:42.123+0400"),
            Some("private-free-text"),
            Some("2042-11-28"),
            None,
        ];
        let out = temporal_shapes(values);
        assert_eq!(
            out,
            json!({"sampled": 9, "matched": 6, "formats": {
            "9999-99-99T99:99": 2, "9999-99-99T99:99:99Z": 1,
            "9999-99-99T99:99:99+99:99": 1, "9999-99-99T99:99:99-99:99": 1,
            "9999-99-99 99:99:99.999+9999": 1}})
        );
        for raw in values.into_iter().flatten() {
            assert!(!out.to_string().contains(raw));
        }
    }

    #[test]
    fn temporal_unmatched_and_oversized_values_add_no_metadata_or_authority() {
        let long = format!("2042-11-28T07:36:42.{}Z", "1".repeat(65));
        assert!(
            temporal_shapes([
                None,
                Some(""),
                Some("17"),
                Some("2042-11-28T07:36 secret"),
                Some("2042-11-28T07:36é"),
                Some(&long)
            ])
            .is_null()
        );
        // This is lexical context, deliberately not calendar validation or an instant law.
        assert_eq!(temporal_shapes([Some("2042-99-99T99:99")])["matched"], 1);
    }
}
