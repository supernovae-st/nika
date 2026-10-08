// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, reason = "required fixture assertions")]

//! Repair guidance must describe the same exact-loopback exception as execution.

use nika_check::{CheckReport, check};
use nika_schema::{FileId, ParseMode, parse};

fn report(url: &str, grant: &str) -> CheckReport {
    let source = format!(
        "nika: probe\npermits:\n  tools: [nika:fetch]\n  net: {{http: [{grant}]}}\n\
         tasks:\n  fetch:\n    invoke: {{tool: 'nika:fetch', args: {{url: '{url}'}}}}\n"
    );
    check(&parse(&source, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
}

#[test]
fn loopback_guidance_names_the_exact_grant_and_that_grant_clears_the_finding() {
    for (url, host) in [
        ("http://127.0.0.1:65409/notifications/stock", "127.0.0.1"),
        ("http://localhost:3000/x", "localhost"),
        ("https://[::1]:8080/x", "::1"),
    ] {
        let denied = report(url, "");
        assert_eq!(denied.capability_escapes.len(), 1);
        let finding = &denied.capability_escapes[0];
        assert!(finding.floor);
        assert_eq!(
            finding.fix,
            Some(format!("add \"{host}\" to permits.net.http"))
        );
        assert_eq!(
            finding.detail,
            format!(
                "`nika:fetch` loopback host `{host}` is refused by the SSRF floor \
             (NIKA-SEC-005) until that exact host is declared in `permits.net.http`; \
             a wildcard or a different loopback host does not grant it"
            )
        );
        let unified = denied
            .findings
            .iter()
            .find(|finding| finding.code.as_deref() == Some("NIKA-SEC-005"))
            .expect("the repair reaches the shared diagnostic");
        assert_eq!(unified.task.as_deref(), Some("fetch"));
        assert_eq!(
            unified.message,
            format!(
                "{} (task `fetch`) — fix: add \"{host}\" to permits.net.http",
                finding.detail
            )
        );
        assert_eq!(
            report(url, &format!("'{host}'")).capability_escapes.len(),
            0
        );
    }
}

#[test]
fn non_loopback_floor_targets_never_receive_a_grant_repair() {
    for host in [
        "10.0.0.5",
        "169.254.169.254",
        "metadata.google.internal",
        "api.localhost",
    ] {
        let denied = report(&format!("http://{host}/x"), "");
        assert_eq!(denied.capability_escapes.len(), 1);
        let finding = &denied.capability_escapes[0];
        assert!(finding.floor);
        assert_eq!(finding.fix, None);
        assert_eq!(
            finding.detail,
            format!(
                "`nika:fetch` host `{host}` is refused by the always-on SSRF floor \
             (NIKA-SEC-005): this target cannot be admitted by `permits:` — \
             point the task at a public host"
            )
        );
    }
}
