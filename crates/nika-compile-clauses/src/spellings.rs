// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The literals a clause states that the host observed spelled with other bytes (R4 A11), read
//! at exact token boundaries under Unicode canonical equivalence: pure over the clause, the
//! observed values and the column names. Ascended from `nika_compile::surface::observed` at the
//! 15k prod-LOC wall (ADR-145), unchanged; the bounded canonical-spelling law the core reads
//! (`equivalent_spellings`) stays there.

use unicode_normalization::UnicodeNormalization;

/// The literals `clause` states that the host observed spelled with other bytes, as
/// `(stated, observed)` pairs in the order observed (R4 A11). Each stated value retains
/// its actual bytes, including partly composed forms, and is canonically equivalent to
/// the observed value. Its exact token boundaries exclude letters, digits and combining
/// marks on either side. Column names and byte-identical values bind nothing.
#[must_use]
pub fn stated_spellings(
    clause: &str,
    observed: &[String],
    columns: &[String],
) -> Vec<(String, String)> {
    observed
        .iter()
        .filter_map(|value| {
            stated(clause, value, columns).map(|literal| (literal.to_owned(), value.clone()))
        })
        .collect()
}

/// Find the original span, not just its NFC and NFD renderings. Canonical decomposition
/// length bounds the scan: every source character contributes at least one code point,
/// and equivalent spans must have the same decomposed length, even when marks reorder.
fn stated<'a>(clause: &'a str, observed: &str, columns: &[String]) -> Option<&'a str> {
    let expected: String = observed.nfd().collect();
    let limit = expected.chars().count();
    if limit == 0 {
        return None;
    }
    for (start, _) in clause.char_indices() {
        if !boundary(clause[..start].chars().next_back()) {
            continue;
        }
        let mut decomposed = 0;
        for (offset, character) in clause[start..].char_indices() {
            let mut bytes = [0; 4];
            decomposed += character.encode_utf8(&mut bytes).nfd().count();
            if decomposed > limit {
                break;
            }
            let end = start + offset + character.len_utf8();
            let literal = &clause[start..end];
            if decomposed == limit {
                if literal != observed
                    && boundary(clause[end..].chars().next())
                    && !columns.iter().any(|column| column == literal)
                    && literal.nfd().eq(expected.chars())
                {
                    return Some(literal);
                }
                break;
            }
        }
    }
    None
}

fn boundary(character: Option<char>) -> bool {
    character
        .is_none_or(|c| !c.is_alphanumeric() && !unicode_normalization::char::is_combining_mark(c))
}
