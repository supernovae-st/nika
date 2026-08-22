// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The audited-line permits cell — the declared blast radius at a
//! glance (persona 4 · gauntlet g2). `--json` and `--infer-permits`
//! already named the grants; the default card said `declared`.

use nika_check::CheckReport;
use nika_schema::types::{ExecPermit, Permits};

/// Compact grant list for the audited card. Absent is `none`; an
/// explicit empty block is `{}` (the legal zero, not the undeclared
/// one). Cells join on spaces so the outer ` · ` separators stay one
/// field.
#[must_use]
pub(super) fn permits_glance(report: &CheckReport) -> String {
    match report.permits.declared.as_ref() {
        None => "none".to_owned(),
        Some(p) => format_grants(p),
    }
}

fn format_grants(p: &Permits) -> String {
    let mut cells = Vec::new();
    match &p.exec {
        Some(ExecPermit::Any) => cells.push("exec:any".to_owned()),
        Some(ExecPermit::Programs(ps)) if !ps.is_empty() => {
            cells.push(format!("exec:{}", ps.join(",")));
        }
        _ => {}
    }
    if let Some(tools) = &p.tools
        && !tools.is_empty()
    {
        cells.push(format!("tools:{}", tools.join(",")));
    }
    if let Some(fs) = &p.fs {
        if !fs.read.is_empty() {
            cells.push(format!("read:{}", fs.read.join(",")));
        }
        if !fs.write.is_empty() {
            cells.push(format!("write:{}", fs.write.join(",")));
        }
    }
    if let Some(net) = &p.net
        && !net.http.is_empty()
    {
        cells.push(format!("http:{}", net.http.join(",")));
    }
    if let Some(env) = &p.env
        && !env.is_empty()
    {
        cells.push(format!("env:{}", env.join(",")));
    }
    if cells.is_empty() {
        "{}".to_owned()
    } else {
        cells.join(" ")
    }
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
