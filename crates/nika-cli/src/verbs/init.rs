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
//!
//! Bare on a terminal, init then OFFERS the first workflow (the guided
//! three-question flow shared with bare `nika new` — the gh-repo-create
//! shape). `--yes`, any pipe, and CI keep the old behavior byte-for-byte.

use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::Path;

use crate::display::theme::Theme;
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
/// discipline. Concise on purpose (the engine teaches the rest via `check`)
/// — but COMPLETE over the live clap tree: a verb the binary ships and the
/// guide never names is a verb a wired agent will never reach (found stale
/// 2026-07-05 — the scaffold taught zero of the then-new train). The bin
/// test `the_scaffolded_agents_md_teaches_the_live_clap_tree` pins the
/// parity, derived from the tree itself.
const AGENTS_MD: &str = r"# AGENTS.md — Nika workflows in this repo

Nika is a sovereign AI workflow engine. Workflows are `*.nika.yaml` files,
**audited before they run**. (This guide is scaffolded by `nika init`.)

## The loop
- **Author** · `nika new --from <template> <file>.nika.yaml` (or write one —
  the envelope is `nika: v1` + `workflow: <kebab-id>` + `tasks:`).
- **Check** · `nika check <file>` — the static audit BEFORE any run (schema ·
  DAG · CEL · effects · permits · cost). Exit `0` clean · `2` findings.
  `--fix` applies the machine-applicable renames (typed did-you-mean only —
  fields · tools · args · deps · refs) and re-audits; ambiguity is skipped
  with a note, never guessed.
- **Run** · `nika run <file>` — execute · live render. Exit `0` ok · `1` failed.
  Inputs ride `--var key=value` (repeatable · unknown keys refused); a run
  paused on a `nika:prompt` resumes with `--resume <trace> --answer
  <task>=<value>` (confirm gates take booleans: `--answer approve=true`).
- **Pin** · `nika test <file> --update` writes `<file>.golden.json` from an
  offline mock run; `nika test <file>` replays and compares — deterministic,
  zero keys, the CI gate.
- **Diagnose** · `nika doctor` — the environment (providers · keys · config).
  `nika welcome` is the short mirror (machine · workspace · next commands).
- **Context** · `nika context --json` — the whole workspace truth in one
  call (every workflow audited · recent runs · costs · capped and says
  so). Read it before proposing edits.
- **Explain** · `nika explain NIKA-XXXX` teaches one error code ·
  `nika explain <file>` narrates a workflow (waves · cost · touches · how
  to run) — read it before handing a workflow to a human.
- **Wire** · `nika wire <cursor|vscode|windsurf|claude|codex|all>` — point an
  agent client's MCP config at the real oracle (idempotent · preserves other
  servers).

## The four verbs (exactly one per task)
`infer` (an LLM call) · `exec` (a shell command) · `invoke` (a `nika:` builtin
or MCP tool) · `agent` (a multi-turn ReAct loop).

## Hard rules (the validator enforces these — they catch ~90% of LLM errors)
- One verb per task · the verb IS the task key (never a `verb:` field).
- Any `${{ tasks.X }}` reference needs `depends_on: [X]` (arrays always).
- Quote any YAML scalar that STARTS with `${{` (an unquoted leading `${{`
  breaks the parse).
- `invoke` arguments live under `args:` (not `input:` / `params:`).
- `when:` is a `${{ }}` CEL boolean or the literal `true`/`false` — a bare
  string is rejected. `size()` is the only CEL function.
- `nika:write` needs `content:` · `nika:done` is valid only inside `agent.tools`.
- snake_case task ids · kebab-case `workflow:`.

## Don't invent structure — route to a skeleton
`nika new --from '?'` lists the embedded skeletons · `nika examples list` /
`show <slug>` reads a runnable example that exercises a construct ·
`nika schema` is the JSON Schema · `nika spec --canon` is the SSOT ·
`nika catalog` names the providers/models · `nika tools` names the `nika:`
builtins. Copy, fill, check.

## Cost honesty (never hide unknown spend)
- `nika check` prints the ceiling BEFORE any token · `≥ $X FLOOR` means an
  unbounded task exists — name why, never round unknown to $0.
- A local model is unpriced compute, **never « free »**.
- `nika run <file> --max-cost-usd <n>` blocks BEFORE the call that would
  cross the cap.

## Understand · replay · prove
- `nika inspect <file>` — static anatomy: tasks · verbs · wave groups · cost.
- `nika graph <file> --format mermaid|dot|json` — the ONE graph projector.
- `nika trace show|replay <run>` — the flight recorder (every run records).
- `nika trace verify <run>` — the journal is hash-chained: verify it after a
  run that matters, cite the trace instead of trusting a memory of the run.
- `nika dap` — step a recorded run under a debugger UI, forward AND back.

## Servers (stdio · for editors and agent clients)
`nika lsp` (language server) · `nika mcp` (MCP: check/explain/schema/examples
as tools) · `nika completions <shell>` generates shell completions.
`nika model serve --model <path.gguf>` serves a local model on loopback
(OpenAI-compatible · needs a `local-infer` build — the default binary
prints the build recipe).

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

Envelope: `nika: v1` (always · frozen forever). Extensions: `.nika.yaml` (canonical) and `.nika.yml`.

## 4 Verbs (locked forever)
- `infer:` LLM call (`prompt`, `system?`, `temperature?`, `schema?`, `max_tokens?`, `model?`)
- `exec:` subprocess (`command`, `cwd?`, `capture: text|structured`)
- `invoke:` builtin/MCP tool (`tool`, `args`) — HTTP fetch = `tool: nika:fetch` (a tool, not a verb)
- `agent:` multi-turn loop (`prompt`, `tools`, `max_turns`, `max_tokens_total`)

## Authoring Discipline
- Interpolation uses `${{ vars.x }}` · `${{ tasks.id.output }}` · `${{ env.KEY }}` · `${{ with.alias }}`.
- Bindings use `with: { alias: ${{ tasks.id.output }} }` then `${{ with.alias }}`.
- Models use the combined form `provider/name` (for example `mock/echo`, `ollama/qwen3.5:4b`, `mistral/mistral-small`).
- `depends_on` is always an array: `depends_on: [task_id]`.
- Secrets are declared in a top-level `secrets:` block (e.g. `source: env`, `key: MY_KEY`) and referenced as `${{ secrets.name }}` — never inline literal keys; `${{ env.* }}` is for non-sensitive configuration.
- After every edit, run `nika check <file>` — `--fix` heals the mechanical
  renames, the diagnostics teach the rest.
- Unknown code? Run `nika explain NIKA-XXXX`.
"#;

/// `.cursor/mcp.json` — the project-scoped MCP wiring for Cursor: the
/// read-only oracle (8 tools) reaches the agent without any manual setup.
/// Project-scoped (not global) so the config travels with the repo and
/// never touches the user's other projects.
const CURSOR_MCP: &str =
    "{ \"mcpServers\": { \"nika\": { \"command\": \"nika\", \"args\": [\"mcp\"] } } }\n";

/// `.github/copilot-instructions.md` — the GitHub Copilot repo brief.
/// Compact on purpose: the loop + the four hard rules that catch most
/// LLM authoring errors; AGENTS.md carries the full contract.
const COPILOT_INSTRUCTIONS: &str = r"# Nika workflows (`*.nika.yaml`) — Copilot brief

Nika workflows are audited BEFORE they run. The loop: author from a
skeleton (`nika new --from '?'` lists them) → `nika check <file>` after
EVERY edit → `nika check <file> --fix` heals the mechanical renames →
repair the rest from the diagnostics (`nika explain NIKA-XXXX`) →
only a clean file reaches a human.

Rules the validator enforces:
- Envelope `nika: v1` · one verb per task (`infer` · `exec` · `invoke` ·
  `agent`) · the verb IS the task key.
- Any `${{ tasks.X }}` reference needs `depends_on: [X]`.
- `invoke` arguments live under `args:` · secrets come from the
  environment (`${{ secrets.X }}`) — never inline.
- Never invent syntax: `nika schema` is the JSON Schema · `nika catalog`
  / `nika tools` name the providers and builtins.
- Cost honesty: unknown spend is declared, never rounded to $0 · a local
  model is unpriced, never « free ».

See AGENTS.md (scaffolded by `nika init`) for the full contract.
";

/// `CLAUDE.md` — a thin pointer, zero-drift by construction: Claude Code
/// auto-loads it, and the ONE contract it needs lives in AGENTS.md (the
/// scaffold parity-tested against the live clap tree).
const CLAUDE_MD: &str = r"# Nika workflows in this repo

Read `AGENTS.md` — it is the Nika contract for every agent (the loop ·
the four verbs · the hard rules · cost honesty · trace proof). It is
scaffolded by `nika init` and stays parity-tested against the binary.

Quick oracle: `nika check <file>` after every edit (`--fix` heals the
mechanical renames) · `nika explain <code|file>` teaches · `nika welcome`
mirrors this machine.
";

/// What `init` does (or declines to do) for one target file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    /// Write `body` to `path` (it is absent, or `--force`).
    Create { path: String, body: &'static str },
    /// Leave `path` untouched — it already exists (`--force` to overwrite).
    Skip { path: String },
}

/// `.agents/skills/nika-authoring/SKILL.md` — the repo-level agent skill
/// (agentskills.io shape · discovered by Codex and every `.agents`-aware
/// client). Canonical copy: the engine's own Codex plugin at
/// `.agents/plugins/nika/skills/nika-authoring/SKILL.md` — a test enforces
/// byte parity so the two surfaces cannot drift.
const AGENT_SKILL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/skills/nika-authoring/SKILL.md"
));

/// The scaffolded agent guide, exposed for the bin-side parity test —
/// a verb the binary ships and the guide never names is a verb a wired
/// agent will never reach (found stale 2026-07-05: the scaffold taught
/// zero of the then-new train). The test derives the expectation from
/// the live clap tree itself, so the guide can never silently lag.
#[must_use]
pub fn agents_md() -> &'static str {
    AGENTS_MD
}

/// The scaffold set · (relative path, body). The ONE source of what `init`
/// writes — `plan` and the docs both read it. AGENTS.md is the contract;
/// the per-client briefs (Cursor rule · Copilot instructions · CLAUDE.md
/// pointer) stay thin so they cannot drift from it.
fn targets() -> [(&'static str, &'static str); 7] {
    [
        (".vscode/settings.json", VSCODE_SETTINGS),
        ("AGENTS.md", AGENTS_MD),
        (".cursor/rules/nika.mdc", CURSOR_RULES),
        (".cursor/mcp.json", CURSOR_MCP),
        (".agents/skills/nika-authoring/SKILL.md", AGENT_SKILL),
        (".github/copilot-instructions.md", COPILOT_INSTRUCTIONS),
        ("CLAUDE.md", CLAUDE_MD),
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

/// The beginner's next move · init used to end SILENTLY (the 2026-07-05
/// beginner walk: « you init and… sit there ») — an onboarding surface
/// must hand over to the next command. Golden path: offline proof in
/// 10s → scaffold → audit-before-tokens. Byte-stable additively: the
/// #158 lines never change (scripts grep them); the community footer
/// joined 2026-07-12 — init is a once-per-repo ceremony, the honest
/// place for the one ask (working commands stay marketing-free).
const NEXT_BLOCK: &str = "next ·\n  nika examples run 01-hello --model mock/echo   # offline proof · zero keys\n  nika new                                       # your first workflow — guided on a terminal\n  nika new --from chain my-first.nika.yaml       # the same, scriptable\n  nika check my-first.nika.yaml                  # audit before a single token\n\nopen source · a star on github.com/supernovae-st/nika helps others find nika";

/// Scaffold `dir` (default `.`). Creates parent dirs as needed. A write
/// failure is the one environment error (`exit 3`); everything else is `0`.
///
/// Bare on a terminal (and not `--yes`) the scaffold report prints
/// immediately and init hands over to the guided first-workflow flow —
/// the gh-repo-create shape (bare on a TTY is guided · flags and pipes
/// keep the exact old output).
#[must_use]
pub fn run(dir: &str, force: bool, yes: bool, theme: Theme) -> VerbOutput {
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
        return VerbOutput::env(text);
    }
    if !yes && std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        // Interactive: the wizard prints the report itself (the lib bans
        // println! — output flows through its writer), then offers the
        // first workflow. Declined → the classic hand-off, so nobody
        // exits without a next command.
        return match crate::verbs::new::offer_first_workflow(dir, &text, theme) {
            Some(v) => v,
            None => VerbOutput::ok(NEXT_BLOCK.to_owned()),
        };
    }
    VerbOutput::ok(format!("{text}\n\n{NEXT_BLOCK}"))
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
    use crate::verbs::exit;

    /// The cursor rule teaches the REAL binary surface — external review
    /// (awesome-cursorrules PR 332 · issue #390) caught the generator
    /// omitting `max_tokens`/`model` from the infer signature, teaching
    /// env-sourced secrets instead of the declared `secrets:` block, and
    /// naming one extension while the globs match two. These pins keep
    /// the teaching surface honest against the schema.
    #[test]
    fn cursor_rule_teaches_the_shipped_surface() {
        assert!(
            CURSOR_RULES.contains("`max_tokens?`, `model?`"),
            "infer signature must carry max_tokens/model (issue #390)"
        );
        assert!(
            CURSOR_RULES.contains("${{ secrets.name }}"),
            "secrets guidance must teach the secrets namespace, not bare env"
        );
        assert!(
            CURSOR_RULES.contains("declared in a top-level `secrets:` block"),
            "secrets guidance must teach the declared block"
        );
        assert!(
            CURSOR_RULES.contains("`.nika.yaml` (canonical) and `.nika.yml`"),
            "prose must name both extensions the globs match"
        );
        assert!(
            CURSOR_RULES.contains("**/*.nika.yml"),
            "the yml glob stays — the prose now matches it"
        );
    }

    #[test]
    fn successful_init_hands_over_to_the_next_command() {
        // The 2026-07-05 beginner walk: init ended SILENTLY (4 files ·
        // no workflow · no next step). An onboarding surface must hand
        // over — the ok-path text carries the golden path.
        let tmp = std::env::temp_dir().join(format!("nika-init-handover-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            Theme::new(false, false, false),
        );
        std::fs::remove_dir_all(&tmp).ok();
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("next ·"), "{}", out.text);
        assert!(out.text.contains("nika examples run 01-hello"));
        assert!(out.text.contains("nika check"));
    }

    /// `--yes` (and any non-terminal) is the byte-stable script shape —
    /// the report and the classic hand-off, zero prompts.
    #[test]
    fn yes_keeps_the_non_interactive_shape() {
        let tmp = std::env::temp_dir().join(format!("nika-init-yes-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            Theme::new(false, false, false),
        );
        std::fs::remove_dir_all(&tmp).ok();
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("✔ created"), "{}", out.text);
        assert!(
            out.text.contains(NEXT_BLOCK),
            "the classic block survives verbatim: {}",
            out.text
        );
    }

    #[test]
    fn plan_creates_both_when_nothing_exists() {
        let p = plan(".", &|_| false, false);
        assert_eq!(p.len(), 7);
        assert!(p.iter().all(|a| matches!(a, Action::Create { .. })));
        // Schema wiring + agent guide + per-client briefs are the targets.
        let paths: Vec<&str> = p
            .iter()
            .map(|a| match a {
                Action::Create { path, .. } | Action::Skip { path } => path.as_str(),
            })
            .collect();
        assert!(paths.iter().any(|p| p.ends_with("settings.json")));
        assert!(paths.iter().any(|p| p.ends_with("AGENTS.md")));
        assert!(paths.iter().any(|p| p.ends_with("nika.mdc")));
        assert!(paths.iter().any(|p| p.ends_with(".cursor/mcp.json")));
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with(".agents/skills/nika-authoring/SKILL.md"))
        );
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with(".github/copilot-instructions.md"))
        );
        assert!(paths.iter().any(|p| p.ends_with("CLAUDE.md")));
    }

    #[test]
    fn the_client_briefs_stay_thin_pointers_to_the_contract() {
        // The per-client briefs must ROUTE to AGENTS.md (the parity-tested
        // contract), teach the check loop, and carry the cost-honesty law —
        // thin by design so they cannot drift into a second truth.
        for (name, body) in [("copilot", COPILOT_INSTRUCTIONS), ("claude", CLAUDE_MD)] {
            assert!(body.contains("AGENTS.md"), "{name} routes to the contract");
            assert!(body.contains("nika check"), "{name} teaches the loop");
        }
        assert!(
            COPILOT_INSTRUCTIONS.contains("never « free »"),
            "cost honesty reaches the Copilot brief"
        );
        assert!(
            CLAUDE_MD.contains("nika welcome"),
            "the Claude pointer names the mirror"
        );
    }

    #[test]
    fn the_agent_skill_has_the_agentskills_frontmatter() {
        // The skill is embedded from the repo's own Codex plugin (SSOT) —
        // assert the contract survives edits over there: frontmatter with
        // name + trigger-loaded description, and the loop's oracle commands.
        assert!(AGENT_SKILL.starts_with("---\n"), "yaml frontmatter opens");
        assert!(AGENT_SKILL.contains("name: nika-authoring"));
        assert!(
            AGENT_SKILL.contains("description: Author, check and repair"),
            "description front-loads the trigger"
        );
        assert!(AGENT_SKILL.contains("nika check"));
        assert!(AGENT_SKILL.contains("nika explain NIKA-XXXX"));
        assert!(AGENT_SKILL.contains("--infer-permits"));
        assert!(
            AGENT_SKILL.contains("--fix"),
            "the skill teaches the in-binary repair loop"
        );
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
        // Teaching-parity (arrival gauntlet 2026-07-11): the binary ships
        // an in-binary repair loop — every scaffolded teaching surface
        // must name it, or agents hand-repair what one flag heals.
        for (name, body) in [
            ("AGENTS.md", AGENTS_MD),
            ("copilot-instructions.md", COPILOT_INSTRUCTIONS),
            ("CLAUDE.md", CLAUDE_MD),
            ("nika.mdc", CURSOR_RULES),
        ] {
            assert!(body.contains("--fix"), "{name} teaches check --fix");
        }
        assert!(
            CURSOR_RULES.contains("4 Verbs") && CURSOR_RULES.contains("nika:fetch"),
            "the Cursor rule teaches the locked language shape"
        );
    }

    #[test]
    fn agents_md_carries_the_hard_rules_that_catch_llm_errors() {
        // The 6 syntax rules the validator enforces (spec AGENTS.md
        // §Writing-a-workflow) that catch ~90% of LLM authoring errors —
        // beyond the permits/secrets discipline already present.
        for needle in [
            "args:",      // invoke args under args:, not input:/params:
            "quote",      // quote any scalar that starts with ${{
            "size()",     // the only CEL function in the v0.1 subset
            "content:",   // nika:write needs content:
            "depends_on", // required for any ${{ tasks.X }} reference
        ] {
            assert!(
                AGENTS_MD.contains(needle),
                "the guide teaches the hard rule `{needle}`"
            );
        }
    }

    #[test]
    fn agents_md_points_at_the_learning_surface() {
        // A wired agent must know the embedded surfaces exist, or it
        // improvises structure instead of routing to a template.
        for needle in ["nika new --from", "nika examples", "nika schema"] {
            assert!(
                AGENTS_MD.contains(needle),
                "the guide names the discovery command `{needle}`"
            );
        }
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
