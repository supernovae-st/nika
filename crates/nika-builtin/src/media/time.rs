// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Manifest timestamps — one clock-to-RFC3339 seam for every media
//! family (was duplicated verbatim in `image/` and `tts/` · review P3.5).

use nika_kernel::io::clock::ClockDyn;

/// The manifest `created_at` — RFC 3339 UTC from the injected clock
/// (total: a pre-epoch or unrepresentable clock falls back loudly to
/// the epoch string rather than panicking).
pub(crate) fn rfc3339_now<C: ClockDyn>(clock: &C) -> String {
    jiff::Timestamp::try_from(clock.system_now())
        .map_or_else(|_| "1970-01-01T00:00:00Z".to_owned(), |ts| ts.to_string())
}

#[cfg(test)]
mod tests {
    use nika_kernel_mock::MockClock;

    use super::*;

    #[test]
    fn rfc3339_shape_holds() {
        let stamp = rfc3339_now(&MockClock::default());
        assert!(stamp.ends_with('Z') && stamp.contains('T'), "{stamp}");
    }
}
