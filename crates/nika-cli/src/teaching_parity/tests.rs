// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Do the teaching surfaces and the clap tree say the same thing?
//!
//! Two directions, and both are needed. Forward: every real verb reaches
//! the reader. Reverse: every verb a surface names actually opens. The
//! forward test alone is blind to a taught door that no longer exists,
//! which is exactly how `examples` and `evidence` outlived the tree in
//! the map injected into every agent session.
//!
//! The clap tree is the authority — `get_subcommands()` returns hidden
//! subcommands too (`catalog`, `completions`, …), which are real doors,
//! so quiet is never mistaken for dead.

#![allow(clippy::expect_used)] // a test: an absent register IS the failure

use clap::{CommandFactory as _, Parser as _};

use crate::Cli;

/// Forward: the scaffolded `AGENTS.md` reaches every verb in the tree,
/// plus the flags an agent needs daily (inputs · resume · goldens).
#[test]
fn the_scaffolded_agents_md_teaches_the_live_clap_tree() {
    let agents = nika_cli::verbs::init::agents_md();
    for sub in Cli::command().get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
            continue; // clap's auto subcommand — not a teaching target
        }
        assert!(
            agents.contains(name),
            "the scaffolded AGENTS.md must teach `nika {name}`"
        );
    }
    for flag in ["--var", "--resume", "--answer", "--update"] {
        assert!(
            agents.contains(flag),
            "the scaffolded AGENTS.md must teach `{flag}`"
        );
    }
}

/// Reverse: no surface names a door the tree refuses. Judged over the
/// injected session map's `CLI:` register and the scaffolded `AGENTS.md`.
#[test]
fn no_teaching_surface_names_a_verb_the_tree_refuses() {
    let tree = Cli::command();
    let real: std::collections::BTreeSet<&str> = tree
        .get_subcommands()
        .map(clap::Command::get_name)
        .collect();

    let hook = include_str!("../../../../.agents/plugins/nika/scripts/session-context.sh");
    let taught = hook
        .split("CLI: nika ")
        .nth(1)
        .and_then(|rest| rest.split('.').next())
        .expect("the injected map carries a `CLI: nika …` register");
    // The register must not be empty, or every assertion below would
    // pass over nothing.
    let verbs: Vec<&str> = taught.split('|').collect();
    assert!(verbs.len() > 5, "the CLI register parsed to {verbs:?}");
    for v in verbs {
        assert!(
            real.contains(v),
            "the injected session map teaches `nika {v}`, which the tree refuses"
        );
    }

    let agents = nika_cli::verbs::init::agents_md();
    for dead in ["nika examples", "nika evidence"] {
        assert!(
            !agents.contains(dead),
            "the scaffolded AGENTS.md teaches `{dead}`, a door that does not open"
        );
    }
}

/// C01 · issue 1298 · `nika try` must name the same `--answer` / `--resume`
/// doors `nika run` already has. A gated showroom job pauses (exit 4)
/// without them; help that omits them is the measured 0.115.0 hole
/// (`5c5bd1ab5`: `nika try --help` had no `--answer`).
#[test]
fn try_help_names_answer_and_resume() {
    let mut cmd = Cli::command();
    let try_cmd = cmd.find_subcommand_mut("try").expect("try subcommand");
    let help = try_cmd.render_long_help().to_string();
    for flag in ["--answer", "--resume"] {
        assert!(
            help.contains(flag),
            "`nika try --help` must name `{flag}` (C01): {help}"
        );
    }
    // Same value-name spelling as `nika run` so a taught paste transfers.
    assert!(
        help.contains("TASK=VALUE"),
        "`nika try --answer` uses the run parser's TASK=VALUE: {help}"
    );
    assert!(
        help.contains("<TRACE>"),
        "`nika try --resume` uses the run parser's TRACE value: {help}"
    );
}

/// C01 · the clap tree accepts the run-shaped paste, including a
/// repeatable `--answer` and a `--resume` path on the same invocation.
#[test]
fn try_parses_answer_and_resume_the_same_as_run() {
    let cli = Cli::try_parse_from([
        "nika",
        "try",
        "ceo-monday-brief",
        "--answer",
        "approve=true",
        "--max-cost-usd",
        "0.01",
    ])
    .expect("`nika try <slug> --answer approve=true` must parse (C01)");
    assert!(
        matches!(
            &cli.command,
            Some(crate::Command::Try(args))
                if args.slug.as_deref() == Some("ceo-monday-brief")
                    && args.answer == ["approve=true"]
                    && args.resume.is_none()
        ),
        "try --answer must land on Try with the pre-seeded gate"
    );

    let cli = Cli::try_parse_from([
        "nika",
        "try",
        "pr-review-fanout",
        "--answer",
        "approve=true",
        "--answer",
        "reviewer=ok",
        "--resume",
        "trace.ndjson",
    ])
    .expect("`nika try` must accept repeatable --answer plus --resume (C01)");
    assert!(
        matches!(
            &cli.command,
            Some(crate::Command::Try(args))
                if args.answer == ["approve=true", "reviewer=ok"]
                    && args.resume.as_deref() == Some(std::path::Path::new("trace.ndjson"))
        ),
        "try must accept repeatable --answer plus --resume"
    );

    // An unknown flag still refuses — the new doors are named, not a
    // clap remainder dump.
    assert!(
        Cli::try_parse_from(["nika", "try", "01-hello", "--not-a-gate-flag"]).is_err(),
        "an unknown try flag must still refuse"
    );
}

/// The nine tools `nika mcp` actually serves (validate ×3 + learn ×6).
const MCP_ORACLE_TOOLS: [&str; 9] = [
    "nika_check",
    "nika_inspect",
    "nika_explain",
    "nika_schema",
    "nika_examples",
    "nika_template",
    "nika_canon",
    "nika_catalog",
    "nika_tools",
];

/// C02 · issue 1303 · `nika mcp --help` must name the read-only oracle,
/// at least three of the nine served tools, and `nika run` as the
/// execute door. Transport flags without the catalog is the measured
/// 0.115.0 hole (`5c5bd1ab5`: help listed `--transport` / `--port` /
/// `--bind`, not the tools, and never said the server cannot run).
#[test]
fn mcp_help_names_the_read_only_oracle_and_the_run_door() {
    let mut cmd = Cli::command();
    let mcp = cmd.find_subcommand_mut("mcp").expect("mcp subcommand");
    let help = mcp.render_long_help().to_string();
    assert!(
        help.contains("read-only"),
        "`nika mcp --help` must say the oracle is read-only (C02): {help}"
    );
    let named = MCP_ORACLE_TOOLS
        .iter()
        .filter(|name| help.contains(*name))
        .count();
    assert!(
        named >= 3,
        "`nika mcp --help` must name at least 3 of the 9 tools, named {named} (C02): {help}"
    );
    assert!(
        help.contains("nika run"),
        "`nika mcp --help` must name `nika run` as the execute door (C02): {help}"
    );
    assert!(
        help.contains(crate::verbs::mcp_pins::OPERATOR_HELP),
        "`nika mcp --help` must carry the mcp verb's honesty card verbatim: {help}"
    );
}

/// C11 · issues 1249/1317 · the postcard names both first-run doors.
#[test]
fn default_help_names_try_and_new() {
    let help = crate::help_card::human_help();
    assert!(help.contains("try"), "C11 try: {help}");
    assert!(help.contains("new"), "C11 new: {help}");
}

/// UX-2 · issue 1317 · `permits` is glossed as the file's blast radius.
#[test]
fn default_help_glosses_permits() {
    let help = crate::help_card::human_help();
    assert!(
        help.contains("what this file is allowed to touch"),
        "UX-2: {help}"
    );
}

/// B08 · issue 1315 · isolation is documented on the postcard.
#[test]
fn default_help_documents_isolation() {
    let help = crate::help_card::human_help();
    assert!(help.contains("env -i"), "B08 env -i: {help}");
    assert!(help.contains("HOME=$scratch"), "B08 HOME scratch: {help}");
}

/// B07+I02 · `--help --all --plain` must not drop `--all` to clap.
#[test]
fn help_all_plain_is_the_full_surface() {
    use crate::help_card::{HelpKind, classify_help};
    assert_eq!(
        classify_help(&["--help", "--all", "--plain"]),
        Some(HelpKind::All)
    );
}
