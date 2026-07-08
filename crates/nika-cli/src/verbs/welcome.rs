// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika welcome` — the mirror moment (30-seconds surface · first contact).
//!
//! One command, one screen: what Nika IS (the tagline block), what THIS
//! machine already has (the shared `verbs::probe` engine — the same
//! detection `doctor` diagnoses with · one truth, two voices), what this
//! BINARY carries (counts DERIVED live from the embedded pack/catalog,
//! never hardcoded — the born-stale law), and the three commands to run
//! next (offline first · zero keys).
//!
//! Always offline (no `--ping` here — that stays doctor's opt-in), always
//! exit `0` (a greeting is never a failure — even a bare machine gets
//! routed, not scolded), and PRESENCE-only like everything probe-backed:
//! no secret value exists in this module by construction. Re-runnable
//! anytime: welcome is a living mirror, not a splash screen.

use std::fmt::Write as _;
use std::path::Path;

use crate::verbs::probe::{Probe, ProviderProbe};
use crate::verbs::{VerbOutput, probe};

/// The three next moves — the SAME golden path the bare-`nika` footer and
/// `init`'s hand-off teach (one story across every lost-user surface).
const START: [(&str, &str); 3] = [
    (
        "nika examples run 01-hello --model mock/echo",
        "offline proof · zero keys",
    ),
    ("nika new", "your first workflow — guided on a terminal"),
    ("nika init", "wire this repo (editor · agents)"),
];

/// What the current directory already holds — the workspace half of the
/// mirror (the machine half is the probe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Glance {
    /// Inside a git repository (any ancestor carries `.git`).
    git: bool,
    /// `*.nika.yaml` / `*.nika.yml` files under the directory (bounded walk).
    workflows: usize,
    /// An `AGENTS.md` sits at the root — the repo's agents are briefed.
    agents_md: bool,
}

/// Counts DERIVED from the embedded surfaces at call time — never typed by
/// hand, so they cannot drift from the binary that prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EngineCounts {
    builtins: usize,
    locals: usize,
    clouds: usize,
    examples: usize,
    templates: usize,
}

/// The `nika welcome` verb. `json` emits the versioned machine projection
/// (`welcome_version: 1` · additive-only, like every machine envelope).
#[must_use]
pub fn run(json: bool) -> VerbOutput {
    let probe = probe::collect(false);
    let glance = glance(Path::new("."));
    let counts = EngineCounts {
        builtins: nika_builtin::tool_defs().len(),
        locals: probe.providers.iter().filter(|p| !p.requires_key).count(),
        clouds: probe.providers.iter().filter(|p| p.requires_key).count(),
        examples: nika_pack::example_slugs().len(),
        templates: nika_pack::template_names().len(),
    };
    if json {
        return VerbOutput::ok(render_json(&probe, glance, counts));
    }
    VerbOutput::ok(render_human(&probe, glance, counts))
}

/// The workspace glance — a bounded, dot-dir-skipping walk (depth ≤ 4 ·
/// ≤ 4000 entries): a greeting must stay instant on a monorepo and must
/// never wander into `node_modules`/`target`.
fn glance(dir: &Path) -> Glance {
    let git = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .ancestors()
        .any(|a| a.join(".git").exists());
    let mut budget = 4000usize;
    let workflows = count_workflows(dir, 4, &mut budget);
    Glance {
        git,
        workflows,
        agents_md: dir.join("AGENTS.md").exists(),
    }
}

/// Directories a glance never enters — dependency/build trees dwarf the
/// workspace and a greeting has a latency budget, not a completeness one.
const SKIP_DIRS: [&str; 8] = [
    ".git",
    "node_modules",
    "target",
    ".venv",
    "venv",
    "dist",
    "build",
    "vendor",
];

fn count_workflows(dir: &Path, depth: u8, budget: &mut usize) -> usize {
    if depth == 0 || *budget == 0 {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for entry in entries.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            n += count_workflows(&path, depth - 1, budget);
        } else if name.ends_with(".nika.yaml") || name.ends_with(".nika.yml") {
            n += 1;
        }
    }
    n
}

/// One client's cell in the editors row (`cursor ✓` · `vscode ✗`).
fn client_cell(c: &crate::verbs::probe::ClientProbe) -> String {
    if c.current {
        format!("{} ✓", c.id)
    } else {
        format!("{} ✗", c.id)
    }
}

/// The human mirror — sections: identity · this machine · this binary ·
/// start here. Pure over its inputs (tests pass synthetic probes).
fn render_human(probe: &Probe, glance: Glance, counts: EngineCounts) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "🦋 nika {} — Intent as Code. The workflow language for AI.",
        probe.version
    );
    let _ = writeln!(
        s,
        "   one file · 4 verbs · one binary · audited BEFORE it runs"
    );
    let _ = writeln!(
        s,
        "   every run records a tamper-evident, hash-chained trace"
    );
    let _ = writeln!(s);

    let _ = writeln!(s, "this machine");
    let editors: Vec<String> = probe.clients.iter().map(client_cell).collect();
    let unwired = probe.clients.iter().any(|c| !c.current);
    let _ = writeln!(
        s,
        "  editors    {}{}",
        editors.join(" · "),
        if unwired {
            "   → nika wire <client|all>"
        } else {
            ""
        }
    );
    let locals: Vec<&str> = probe
        .providers
        .iter()
        .filter(|p| !p.requires_key)
        .map(|p| p.id.as_str())
        .collect();
    if locals.is_empty() {
        let _ = writeln!(s, "  local      no local providers in this build");
    } else {
        let _ = writeln!(
            s,
            "  local      {} — no key needed · nika doctor --ping probes the ports",
            locals.join(" · ")
        );
    }
    let (present, total) = cloud_key_counts(&probe.providers);
    let _ = writeln!(
        s,
        "  keys       {present}/{total} cloud keys present · details + fixes → nika doctor"
    );
    let _ = writeln!(
        s,
        "  workspace  git {} · {} · agents {}",
        tick(glance.git),
        match glance.workflows {
            0 => "no workflows yet".to_owned(),
            1 => "1 workflow".to_owned(),
            n => format!("{n} workflows"),
        },
        if glance.agents_md {
            "briefed ✓ (AGENTS.md)".to_owned()
        } else {
            "not briefed → nika init".to_owned()
        }
    );
    let _ = writeln!(s);

    let _ = writeln!(s, "this binary");
    let _ = writeln!(
        s,
        "  4 verbs · {} builtins · {} local + {} cloud providers · {} runnable examples · {} templates",
        counts.builtins, counts.locals, counts.clouds, counts.examples, counts.templates
    );
    let _ = writeln!(s);

    let _ = writeln!(s, "start here (offline · zero keys)");
    let width = START
        .iter()
        .map(|(cmd, _)| cmd.chars().count())
        .max()
        .unwrap_or(0);
    for (cmd, why) in START {
        let _ = writeln!(s, "  {cmd:<width$}   # {why}");
    }
    s
}

fn tick(on: bool) -> &'static str {
    if on { "✓" } else { "—" }
}

/// Cloud keys: how many of the key-requiring providers have one PRESENT
/// (presence is a bool the probe observed — no value exists to leak).
fn cloud_key_counts(providers: &[ProviderProbe]) -> (usize, usize) {
    let clouds: Vec<&ProviderProbe> = providers.iter().filter(|p| p.requires_key).collect();
    let present = clouds.iter().filter(|p| p.key_present).count();
    (present, clouds.len())
}

/// The versioned machine mirror — additive-only (`welcome_version: 1`).
/// Names and booleans and counts, by construction: nothing in the probe
/// carries a value a secret could ride.
fn render_json(probe: &Probe, glance: Glance, counts: EngineCounts) -> String {
    let (present, total) = cloud_key_counts(&probe.providers);
    let clients: Vec<serde_json::Value> = probe
        .clients
        .iter()
        .map(|c| serde_json::json!({ "id": c.id, "wired": c.current }))
        .collect();
    let locals: Vec<&str> = probe
        .providers
        .iter()
        .filter(|p| !p.requires_key)
        .map(|p| p.id.as_str())
        .collect();
    let start: Vec<&str> = START.iter().map(|(cmd, _)| *cmd).collect();
    serde_json::json!({
        "welcome_version": 1,
        "version": probe.version,
        "machine": {
            "clients": clients,
            "local_providers": locals,
            "cloud_keys_present": present,
            "cloud_keys_total": total,
            "config": probe.config_path,
        },
        "workspace": {
            "git": glance.git,
            "workflows": glance.workflows,
            "agents_md": glance.agents_md,
        },
        "engine": {
            "verbs": 4,
            "builtins": counts.builtins,
            "local_providers": counts.locals,
            "cloud_providers": counts.clouds,
            "examples": counts.examples,
            "templates": counts.templates,
        },
        "start": start,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;
    use crate::verbs::probe::{ClientProbe, ImageProbe, PricingProbe, TtsProbe};

    fn synthetic_probe() -> Probe {
        Probe {
            version: "0.0.0-test".to_owned(),
            config_path: None,
            providers: vec![
                ProviderProbe {
                    id: "ollama".to_owned(),
                    requires_key: false,
                    key_present: false,
                    fix_var: "NIKA_OLLAMA_API_KEY".to_owned(),
                    structured_native: true,
                },
                ProviderProbe {
                    id: "mistral".to_owned(),
                    requires_key: true,
                    key_present: false,
                    fix_var: "MISTRAL_API_KEY".to_owned(),
                    structured_native: true,
                },
                ProviderProbe {
                    id: "anthropic".to_owned(),
                    requires_key: true,
                    key_present: true,
                    fix_var: "ANTHROPIC_API_KEY".to_owned(),
                    structured_native: true,
                },
            ],
            clients: vec![
                ClientProbe {
                    id: "cursor".to_owned(),
                    path: "~/.cursor/mcp.json".to_owned(),
                    present: true,
                    current: true,
                    stale: false,
                },
                ClientProbe {
                    id: "vscode".to_owned(),
                    path: "./.vscode/mcp.json".to_owned(),
                    present: false,
                    current: false,
                    stale: false,
                },
            ],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
        }
    }

    fn counts() -> EngineCounts {
        EngineCounts {
            builtins: 7,
            locals: 1,
            clouds: 2,
            examples: 3,
            templates: 2,
        }
    }

    #[test]
    fn human_mirror_carries_the_four_sections_and_no_key_names() {
        let text = render_human(
            &synthetic_probe(),
            Glance {
                git: true,
                workflows: 2,
                agents_md: false,
            },
            counts(),
        );
        for needle in [
            "Intent as Code",
            "this machine",
            "this binary",
            "start here",
            "hash-chained",
            "cursor ✓",
            "vscode ✗",
            "nika wire",
            "1/2 cloud keys present",
            "not briefed → nika init",
            "mock/echo",
        ] {
            assert!(text.contains(needle), "missing `{needle}`:\n{text}");
        }
        // PRESENT-NOT-PRINTED, one step further: welcome never even names
        // the env VARS — that is doctor's fix surface, not the mirror's.
        assert!(
            !text.contains("API_KEY"),
            "welcome must not name key variables:\n{text}"
        );
    }

    #[test]
    fn json_mirror_is_versioned_additive_and_value_free() {
        let raw = render_json(
            &synthetic_probe(),
            Glance {
                git: false,
                workflows: 0,
                agents_md: true,
            },
            counts(),
        );
        let v: serde_json::Value = serde_json::from_str(&raw).expect("welcome --json parses");
        assert_eq!(v["welcome_version"], 1);
        assert_eq!(v["machine"]["cloud_keys_present"], 1);
        assert_eq!(v["machine"]["cloud_keys_total"], 2);
        assert_eq!(v["machine"]["clients"][0]["wired"], true);
        assert_eq!(v["workspace"]["workflows"], 0);
        assert_eq!(v["engine"]["verbs"], 4);
        assert_eq!(v["start"].as_array().map(Vec::len), Some(3));
        assert!(
            !raw.contains("API_KEY") && !raw.contains("key_present"),
            "the JSON mirror carries counts, never per-key facts: {raw}"
        );
    }

    #[test]
    fn glance_counts_workflows_skips_heavy_dirs_and_sees_git() {
        let tmp = std::env::temp_dir().join(format!("nika-welcome-glance-{}", std::process::id()));
        let nested = tmp.join("flows");
        let heavy = tmp.join("node_modules");
        std::fs::create_dir_all(&nested).expect("mkdir");
        std::fs::create_dir_all(&heavy).expect("mkdir");
        std::fs::create_dir_all(tmp.join(".git")).expect("mkdir");
        std::fs::write(tmp.join("a.nika.yaml"), "x").expect("write");
        std::fs::write(nested.join("b.nika.yml"), "x").expect("write");
        std::fs::write(heavy.join("c.nika.yaml"), "x").expect("write");
        std::fs::write(tmp.join("AGENTS.md"), "x").expect("write");
        let g = glance(&tmp);
        std::fs::remove_dir_all(&tmp).ok();
        assert!(g.git, "sees the .git ancestor");
        assert_eq!(g.workflows, 2, "counts a.nika.yaml + flows/b.nika.yml only");
        assert!(g.agents_md);
    }

    #[test]
    fn welcome_is_always_a_success() {
        // A greeting is never a failure — even on a bare machine the verb
        // routes (doctor owns the gate semantics, welcome never gates).
        let out = run(false);
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("start here"), "{}", out.text);
        let json = run(true);
        assert_eq!(json.code, exit::OK);
        assert!(json.text.contains("\"welcome_version\":1"), "{}", json.text);
    }
}
