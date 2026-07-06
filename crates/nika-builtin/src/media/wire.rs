// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared HTTP-wire scaffold for the media provider adapters — the parts
//! that were copied verbatim across image/{openai,gemini,local,xai} and
//! tts/{openai,local,elevenlabs} (architect review P1.1 · the 7× set).
//!
//! Re-homing only: the exact behavior every adapter had, named ONCE.
//! The transient matrix is a SPEC-NORMATIVE
//! rule (« Transient per the spec status table · 5xx/408/429 ») — inlined
//! seven times is seven places to miss when the spec's table changes.

/// Is this HTTP status transient (worth a retry)? The spec's status table:
/// 5xx server errors + 408 Request Timeout + 429 Too Many Requests. THE
/// single authority — `retry.on_codes` correctness rides on it matching
/// the spec across every media provider.
pub(crate) fn transient_status(status: u16) -> bool {
    matches!(status, 500..=599 | 408 | 429)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_matches_the_spec_status_table() {
        for s in [500, 502, 503, 599, 408, 429] {
            assert!(transient_status(s), "{s} is transient");
        }
        for s in [200, 400, 401, 403, 404, 422] {
            assert!(!transient_status(s), "{s} is terminal");
        }
    }
}
