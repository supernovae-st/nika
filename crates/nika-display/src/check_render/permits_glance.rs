// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The audited-line permits cell — the declared blast radius at a
//! glance (persona 4 · gauntlet g2). `--json` and `--infer-permits`
//! already named the grants; the default card said `declared`.

use nika_check::CheckReport;

/// Compact grant list for the audited card. Delegates to
/// [`nika_check::EffectivePermits::glance`] so MCP and the CLI card
/// cannot drift.
#[must_use]
pub(super) fn permits_glance(report: &CheckReport) -> String {
    report.permits.glance()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn glance_of(yaml: &str) -> String {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        permits_glance(&nika_check::check(&wf))
    }

    #[test]
    fn absent_is_none() {
        assert_eq!(
            glance_of("nika: w\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 1 }\n"),
            "none"
        );
    }

    #[test]
    fn empty_block_is_the_legal_zero() {
        assert_eq!(
            glance_of(
                "nika: w\npermits: {}\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 1 }\n"
            ),
            "{}"
        );
    }

    #[test]
    fn declared_grants_are_named() {
        let g = glance_of(
            "nika: w\npermits:\n  exec: [\"docker\"]\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"./docker-health.md\"]\ntasks:\n  t:\n    exec: { command: [\"docker\", \"ps\"] }\n",
        );
        assert!(g.contains("exec:docker"), "{g}");
        assert!(g.contains("tools:nika:write"), "{g}");
        assert!(g.contains("write:./docker-health.md"), "{g}");
        assert!(!g.contains("declared"), "{g}");
    }
}
