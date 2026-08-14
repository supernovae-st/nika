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

use clap::CommandFactory as _;

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
