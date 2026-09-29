// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Judge the independent spec's HTTP policy cases through the composed check.

mod common;

#[test]
fn http_response_policy_conformance() {
    if common::skip_in_mutants_sandbox() {
        return;
    }
    let root = common::spec_dir().join("conformance/tests/stdlib/builtins");
    let mut total = 0;
    let mut failures = Vec::new();
    for dir in common::fixture_dirs(&root) {
        let name = dir.to_string_lossy();
        if !name.contains("fetch-response-") && !name.contains("fetch-headers-traverse-") {
            continue;
        }
        total += 1;
        if let Some(failure) = common::fixture_verdict(&dir, true) {
            failures.push(format!("{name}: {failure}"));
        }
    }
    assert!(
        total >= 28,
        "HTTP policy corpus missing or truncated: {total}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
