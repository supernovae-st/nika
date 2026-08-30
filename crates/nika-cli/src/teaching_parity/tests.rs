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
