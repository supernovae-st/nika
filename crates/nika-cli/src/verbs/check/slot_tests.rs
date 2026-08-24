// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The SLOT-scaffold refusal (#1066), in its own module.
//!
//! Split out of `tests.rs` because that file reached 1528 LOC — past the
//! 1500-per-file cap (ADR-023, hygiene vector 12) — the moment this test
//! landed on it. `mod repair_tests` beside `mod tests` is this file's own
//! precedent for a topic-named sibling.

use super::tests::checked_output;
use super::*;

/// #1066 · an unfilled SLOT scaffold is not a workflow. Comments die
/// with the YAML parse, so check must scan the source after it parses
/// and refuse (exit 2 · `clean: false`). Deleting the comments greens.
#[test]
fn unfilled_slot_comments_fail_check_and_clear_when_deleted() {
    const BODY: &str = "nika: slot-scaffold\nmodel: mock/echo\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10 }\n";
    const SLOTS: &str = "nika: slot-scaffold       # SLOT: kebab-case\nmodel: mock/echo\n# SLOT: the one model job\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10 }\n";
    let red = checked_output("slot-red.nika.yaml", SLOTS, false);
    assert_eq!(red.code, 2, "{}", red.text);
    assert!(
        red.text.contains("SLOT") && red.text.contains("scaffold"),
        "{}",
        red.text
    );
    let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
    let json = run(
        dir.join("slot-red.nika.yaml").to_str().expect("utf8"),
        true,
        false,
        None,
        Theme::new(false, true, false),
    );
    assert_eq!(json.code, 2, "{}", json.text);
    let payload: serde_json::Value = serde_json::from_str(&json.text).expect("json");
    assert_eq!(payload["clean"], false, "{payload:#}");
    assert!(payload.to_string().contains("SLOT"), "{payload:#}");
    let green = checked_output("slot-green.nika.yaml", BODY, false);
    assert_eq!(green.code, 0, "{}", green.text);
    assert!(!green.text.contains("SLOT"), "{}", green.text);
}
