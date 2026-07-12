// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Count-noun agreement — a thin delegate to the ONE vocabulary
//! (`display::vocab::count`), kept so the crate's 13+ call sites read
//! `crate::text::count` without a display path in every verb.

/// See [`crate::display::vocab::count`] — `1 task` · `3 tasks` ·
/// `2 retries` (consonant+y → ies) · compound nouns at the tail.
pub(crate) fn count(n: usize, noun: &str) -> String {
    crate::display::vocab::count(n, noun)
}
