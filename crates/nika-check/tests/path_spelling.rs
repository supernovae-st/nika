// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    reason = "required fixture and finding assertions"
)]

//! The pure checker names a lexical mismatch without claiming filesystem identity.
//! No path in this fixture is opened, created, canonicalized or executed.

use nika_check::{CheckReport, check};
use nika_schema::{FileId, ParseMode, parse};

#[derive(Clone, Copy, Debug)]
enum Surface {
    Read,
    Write,
    Exec,
}

fn report(surface: Surface, grant: &str, requested: &str) -> CheckReport {
    let (category, grant_kind, action) = match surface {
        Surface::Read => (
            "read",
            "tools: [nika:read]",
            format!("invoke: {{ tool: nika:read, args: {{ path: '{requested}' }} }}"),
        ),
        Surface::Write => (
            "write",
            "tools: [nika:write]",
            format!("invoke: {{ tool: nika:write, args: {{ path: '{requested}', content: hi }} }}"),
        ),
        Surface::Exec => (
            "read",
            "exec: [cat]",
            format!("exec: {{ command: [cat, '{requested}'] }}"),
        ),
    };
    let source = format!(
        "nika: path-spelling\npermits:\n  {grant_kind}\n  fs: {{ {category}: ['{grant}'] }}\ntasks:\n  probe:\n    {action}\n"
    );
    check(&parse(&source, FileId::new(0), ParseMode::Strict).expect("valid fixture"))
}

#[test]
fn mismatched_spellings_name_both_sides_and_teach_the_static_limit() {
    for surface in [Surface::Read, Surface::Write, Surface::Exec] {
        for (grant, requested) in [
            ("/private/tmp/nika/allowed.txt", "/tmp/nika/allowed.txt"),
            ("/tmp/nika/allowed.txt", "/private/tmp/nika/allowed.txt"),
        ] {
            let report = report(surface, grant, requested);
            assert!(!report.is_clean(), "{surface:?}: mismatch stays refused");
            assert!(
                report
                    .failure_plan
                    .iter()
                    .any(|e| e.task == "probe" && e.code == "NIKA-SEC-004"),
                "{surface:?}: the public failure plan retains the refusal"
            );
            let escape = report
                .capability_escapes
                .iter()
                .find(|e| e.category == "fs")
                .expect("filesystem mismatch");
            assert!(escape.fix.is_none(), "never prescribe a wider grant");
            let finding = report
                .findings
                .iter()
                .find(|f| f.code.as_deref() == Some("NIKA-SEC-004"))
                .expect("public refusal");
            let message = &finding.message;
            for phrase in [grant, requested, "lexically", "does not resolve symlinks"] {
                assert!(message.contains(phrase), "{surface:?}: {message}");
            }
            assert!(message.contains("If both paths refer"), "{message}");
            assert!(message.contains("consistent spelling"), "{message}");
        }
    }
}

#[test]
fn a_consistent_spelling_passes_without_asserting_an_alias_exists() {
    for surface in [Surface::Read, Surface::Write, Surface::Exec] {
        for path in ["/tmp/nika/allowed.txt", "/private/tmp/nika/allowed.txt"] {
            let report = report(surface, path, path);
            assert!(report.is_clean(), "{surface:?}: {:?}", report.findings);
            assert!(report.capability_escapes.is_empty());
        }
    }
}

#[test]
fn unrelated_host_paths_and_parent_traversal_stay_refused_without_a_grant_fix() {
    for requested in ["/etc/passwd", "../../private.txt"] {
        let report = report(Surface::Read, "./out/**", requested);
        assert!(!report.is_clean());
        let escape = report
            .capability_escapes
            .iter()
            .find(|e| e.category == "fs")
            .expect("escape remains refused");
        assert!(escape.fix.is_none());
        assert!(escape.detail.contains("./out/**"));
        assert!(escape.detail.contains("deliberate operator choice"));
        assert!(escape.detail.contains("never the default repair"));
    }
}
