// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's SOURCE identity seam — the hash the journal's
//! `workflow_sha256` / `def_hash` fields speak.
//!
//! A journal names the definition it recorded: `workflow_sha256` hashes
//! the exact bytes the composer read, and CRLF/BOM sources additionally
//! record `workflow_sha256_lf` (the LF normal form) so drift checks can
//! tell an editor re-encode from a content edit. Hash + normal form
//! live together here; the stamping happens in the composer and the
//! comparison in dap replay. Descended from `nika-dap::source_id`
//! 2026-07-22 (the run-composer descent needed the primitive at L≤3 —
//! the taxonomy owner keeps the one home: sha2 is pure, zero I/O).

/// The run's source identity: sha256 hex over the exact bytes read.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// LF normal form of a workflow source: UTF-8 BOM stripped, CRLF → LF.
/// The SAME content re-encoded by an editor hashes identically here —
/// a re-encode is not an edit (the 0.96.0 dap review proved the raw
/// byte-compare cried drift on CRLF-only churn).
#[must_use]
pub fn lf_normal_form(source: &str) -> std::borrow::Cow<'_, str> {
    let stripped = source.strip_prefix('\u{feff}').unwrap_or(source);
    if stripped.contains("\r\n") {
        std::borrow::Cow::Owned(stripped.replace("\r\n", "\n"))
    } else {
        std::borrow::Cow::Borrowed(stripped)
    }
}

#[cfg(test)]
mod lf_normal_form_tests {
    use super::lf_normal_form;

    #[test]
    fn strips_bom_and_crlf_only() {
        assert_eq!(lf_normal_form("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(lf_normal_form("\u{feff}a\nb"), "a\nb");
        // Untouched LF content BORROWS (no allocation on the common path).
        assert!(matches!(
            lf_normal_form("a\nb"),
            std::borrow::Cow::Borrowed("a\nb")
        ));
        // A lone \r is CONTENT (old-Mac endings are extinct; do not
        // normalize what an editor would not produce today).
        assert_eq!(lf_normal_form("a\rb"), "a\rb");
    }
}
