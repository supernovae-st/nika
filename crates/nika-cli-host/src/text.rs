// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Count-noun agreement — a thin delegate to the ONE vocabulary
//! (`display::vocab::count`), kept so this crate's 3 call sites read
//! `crate::text::count` without a display path in every module (the cli
//! member's own 13+ ride its re-export of the same seam).

/// See [`crate::display::vocab::count`] — `1 task` · `3 tasks` ·
/// `2 retries` (consonant+y → ies) · compound nouns at the tail.
#[must_use]
pub fn count(n: usize, noun: &str) -> String {
    crate::display::vocab::count(n, noun)
}

/// See [`crate::display::vocab::usd`] — the 4-decimal USD grain,
/// ceiling-honest at the bottom (`0.0001`, never a fabricated
/// `0.0000`; an exact zero stays `0.0000`: that zero is true).
#[must_use]
pub fn usd(amount: f64) -> String {
    crate::display::vocab::usd(amount)
}
