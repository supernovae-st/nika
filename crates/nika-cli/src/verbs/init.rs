// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika init [dir]` — scaffold a repo for Nika workflows (spec `nika-cli`
//! §2 v0.81 floor).
//!
//! Writes the editor schema wiring (`.vscode/settings.json` → `*.nika.yaml`
//! validates against the embedded JSON Schema), a Cursor rule, and an
//! `AGENTS.md` repo guide.
//! The human keeps the hand: an existing file is SKIPPED, never clobbered —
//! `--force` is the explicit override (same law as `nika new`). Diagnose-and-
//! report: every created/skipped path is named; a write failure is the one
//! environment error (`exit 3`).

use std::fmt::Write as _;
use std::path::Path;

use crate::verbs::VerbOutput;

/// `.vscode/settings.json` — wire `*.nika.yaml` to the canonical schema so any
/// editor (the YAML language server) validates workflows as you type.
const VSCODE_SETTINGS: &str = r#"{
  "yaml.schemas": {
    "https://nika.sh/spec/v1/workflow.schema.json": "*.nika.yaml"
  },
  "files.associations": {
    "*.nika.yaml": "yaml"
  }
}
"#;

/// `AGENTS.md` — the repo's agent guide: the loop, the four verbs, the
/// discipline. Concise on purpose (the engine teaches the rest via `check`).
const AGENTS_MD: &str = r"# AGENTS.md — Nika workflows in this repo

Nika is a sovereign AI workflow engine. Workflows are `*.nika.yaml` files,
**audited before they run**.

## The loop
- **Author** · `nika new --from <template> <file>.nika.yaml` (or write one —
  the envelope is `nika: v1` + `workflow: <kebab-id>` + `tasks:`).
- **Check** · `nika check <file>` — the static audit BEFORE any run (schema ·
  DAG · CEL · effects · permits · cost). Exit `0` clean · `2` findings.
- **Run** · `nika run <file>` — execute · live render. Exit `0` ok · `1` failed.
- **Diagnose** · `nika doctor` — the environment (providers · keys · config).
- **Explain** · `nika explain NIKA-XXXX` — teach one error code.

## The four verbs (exactly one per task)
`infer` (an LLM call) · `exec` (a shell command) · `invoke` (a `nika:` builtin
or MCP tool) · `agent` (a multi-turn ReAct loop).

## Discipline
- Every effect is gated by `permits:` (default-deny · `nika check --infer-permits`
  prints the tightest boundary).
- Secrets come from the environment (`${{ secrets.X }}`) — never inline.
- `nika check` must be clean before `nika run` (audit-before-run is enforced).
";

/// `.cursor/rules/nika.mdc` — the agent-facing authoring floor for Cursor.
/// Generated from the same 4-verb canon as AGENTS.md; kept compact so it is
/// cheap enough to auto-load on every `.nika.yaml` edit.
const CURSOR_RULES: &str = r#"---
description: Nika workflow language rules for AI assistance
globs: ["**/*.nika.yaml", "**/*.nika.yml"]
alwaysApply: false
---

# Nika Workflow Language

Envelope: `nika: v1` (always · frozen forever). Extension: `.nika.yaml`.

## 4 Verbs (locked forever)
- `infer:` LLM call (`prompt`, `system?`, `temperature?`, `schema?`)
- `exec:` subprocess (`command`, `cwd?`, `capture: text|structured`)
- `invoke:` builtin/MCP tool (`tool`, `args`) — HTTP fetch = `tool: nika:fetch` (a tool, not a verb)
- `agent:` multi-turn loop (`prompt`, `tools`, `max_turns`, `max_tokens_total`)

## Authoring Discipline
- Interpolation uses `${{ vars.x }}` · `${{ tasks.id.output }}` · `${{ env.KEY }}` · `${{ with.alias }}`.
- Bindings use `with: { alias: ${{ tasks.id.output }} }` then `${{ with.alias }}`.
- Models use the combined form `provider/name` (for example `mock/echo`, `openai/gpt-4o-mini`).
- `depends_on` is always an array: `depends_on: [task_id]`.
- Secrets come from the environment — never inline literal keys.
- After every edit, run `nika check <file>` and repair from diagnostics.
- Unknown code? Run `nika explain NIKA-XXXX`.
"#;

/// What `init` does (or declines to do) for one target file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    /// Write `body` to `path` (it is absent, or `--force`).
    Create { path: String, body: &'static str },
    /// Leave `path` untouched — it already exists (`--force` to overwrite).
    Skip { path: String },
}

/// The scaffold set · (relative path, body). The ONE source of what `init`
/// writes — `plan` and the docs both read it.
fn targets() -> [(&'static str, &'static str); 3] {
    [
        (".vscode/settings.json", VSCODE_SETTINGS),
        ("AGENTS.md", AGENTS_MD),
        (".cursor/rules/nika.mdc", CURSOR_RULES),
    ]
}

/// PURE plan over an injected existence oracle — `Create` an absent file (or
/// any when `force`), `Skip` an existing one. Testable without the filesystem.
pub(crate) fn plan(dir: &str, exists: &dyn Fn(&str) -> bool, force: bool) -> Vec<Action> {
    targets()
        .into_iter()
        .map(|(rel, body)| {
            let path = join(dir, rel);
            if !force && exists(&path) {
                Action::Skip { path }
            } else {
                Action::Create { path, body }
            }
        })
        .collect()
}

/// Join a base dir and a relative path the same way the apply step will.
fn join(dir: &str, rel: &str) -> String {
    Path::new(dir).join(rel).to_string_lossy().into_owned()
}

/// Render the report (✔ created · · skipped · ✖ write error).
pub(crate) fn render(lines: &[(char, String)]) -> String {
    let mut s = String::new();
    for (glyph, msg) in lines {
        let _ = writeln!(s, "{glyph} {msg}");
    }
    s
}

/// Scaffold `dir` (default `.`). Creates parent dirs as needed. A write
/// failure is the one environment error (`exit 3`); everything else is `0`.
#[must_use]
pub fn run(dir: &str, force: bool) -> VerbOutput {
    let plan = plan(dir, &|p| Path::new(p).exists(), force);
    let mut lines: Vec<(char, String)> = Vec::new();
    let mut failed = false;

    for action in plan {
        match action {
            Action::Skip { path } => {
                lines.push((
                    '·',
                    format!("skipped {path} (exists · --force to overwrite)"),
                ));
            }
            Action::Create { path, body } => match write_file(&path, body) {
                Ok(()) => lines.push(('✔', format!("created {path}"))),
                Err(e) => {
                    failed = true;
                    lines.push(('✖', format!("{path}: {e}")));
                }
            },
        }
    }

    let text = render(&lines);
    if failed {
        VerbOutput::env(text)
    } else {
        VerbOutput::ok(text)
    }
}

/// Create any missing parent dirs, then write the file.
fn write_file(path: &str, body: &str) -> std::io::Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_creates_both_when_nothing_exists() {
        let p = plan(".", &|_| false, false);
        assert_eq!(p.len(), 3);
        assert!(p.iter().all(|a| matches!(a, Action::Create { .. })));
        // The schema wiring + agent guide + Cursor rule are the targets.
        let paths: Vec<&str> = p
            .iter()
            .map(|a| match a {
                Action::Create { path, .. } | Action::Skip { path } => path.as_str(),
            })
            .collect();
        assert!(paths.iter().any(|p| p.ends_with("settings.json")));
        assert!(paths.iter().any(|p| p.ends_with("AGENTS.md")));
        assert!(paths.iter().any(|p| p.ends_with("nika.mdc")));
    }

    #[test]
    fn plan_skips_an_existing_file_without_force() {
        // AGENTS.md already there · settings.json/rules absent → one Skip, two Create.
        let p = plan(".", &|path| path.ends_with("AGENTS.md"), false);
        assert!(
            p.iter()
                .any(|a| matches!(a, Action::Skip { path } if path.ends_with("AGENTS.md")))
        );
        assert!(
            p.iter().any(
                |a| matches!(a, Action::Create { path, .. } if path.ends_with("settings.json"))
            )
        );
        assert!(
            p.iter()
                .any(|a| matches!(a, Action::Create { path, .. } if path.ends_with("nika.mdc")))
        );
    }

    #[test]
    fn force_overwrites_everything() {
        let p = plan(".", &|_| true, true);
        assert!(p.iter().all(|a| matches!(a, Action::Create { .. })));
    }

    #[test]
    fn join_respects_the_target_dir() {
        let p = plan("/tmp/proj", &|_| false, false);
        assert!(p.iter().any(|a| matches!(a, Action::Create { path, .. }
                if path == "/tmp/proj/.vscode/settings.json")));
        assert!(p.iter().any(|a| matches!(a, Action::Create { path, .. }
                if path == "/tmp/proj/.cursor/rules/nika.mdc")));
    }

    #[test]
    fn the_embedded_bodies_are_valid() {
        // The schema wiring is parseable JSON · AGENTS.md names the loop.
        let v: serde_json::Value =
            serde_json::from_str(VSCODE_SETTINGS).expect("settings.json is valid JSON");
        assert!(
            v.get("yaml.schemas")
                .and_then(|s| s.get("https://nika.sh/spec/v1/workflow.schema.json"))
                .is_some(),
            "wires *.nika.yaml to the canonical schema"
        );
        assert!(
            AGENTS_MD.contains("nika check"),
            "the guide teaches the loop"
        );
        assert!(
            CURSOR_RULES.contains("4 Verbs") && CURSOR_RULES.contains("nika:fetch"),
            "the Cursor rule teaches the locked language shape"
        );
    }

    #[test]
    fn render_marks_created_and_skipped() {
        let out = render(&[
            ('✔', "created .vscode/settings.json".to_owned()),
            (
                '·',
                "skipped AGENTS.md (exists · --force to overwrite)".to_owned(),
            ),
        ]);
        assert!(out.contains("✔ created"));
        assert!(out.contains("· skipped"));
    }
}
