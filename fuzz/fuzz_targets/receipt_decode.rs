// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The 5th target (F-P1 · NEP-0012): the fortress decoder over
//! arbitrary bytes — the verifier's single untrusted-JSON entry point
//! must refuse or admit, never panic, hang or overflow. Seeds: the
//! golden + malicious corpus (`crates/nika-dap/tests/receipts/`).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(raw) = std::str::from_utf8(data) {
        // Admit or refuse TYPED — any panic is the bug class this
        // target exists to catch (CVE-2026-26209 · the decode surface).
        let _ = nika_dap::bounded::decode_untrusted_json(raw);
    }
});
