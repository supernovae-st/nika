// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The scaffold briefs — every static body `nika init` can write, plus
//! the `targets()` table that names them (the ONE source `plan` and the
//! docs read). AGENTS.md is the contract; the per-client briefs (Cursor
//! rule · Copilot instructions · CLAUDE.md pointer) stay thin so they
//! cannot drift from it. Orchestration lives in the parent `init::`
//! module; this file is bytes.

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
  `--fix` applies the machine-applicable repairs (typed did-you-mean renames —
  fields · tools · args · edge targets · refs — plus the provable
  `depends_on` → `with:`/`after:` migration and `tasks.*` hoists) and
  re-audits; ambiguity is skipped with a note, never guessed.
- **Run** · `nika run <file>` — execute · live render. Exit `0` ok · `1` failed.
  Inputs ride `--var key=value` (repeatable · unknown keys refused); a run
  paused on a `nika:prompt` resumes with `--resume <trace> --answer
  <task>=<value>` (confirm gates take booleans: `--answer approve=true`).
- **Pin** · `nika test <file> --update` writes `<file>.golden.json` from an
  offline mock run; `nika test <file>` replays and compares — deterministic,
  zero keys, the CI gate.
- **Diagnose** · `nika doctor` — the environment (providers · keys · config).
  `nika welcome` is the short mirror (machine · workspace · next commands).
- **Context** · `nika welcome --deep --json` — the whole workspace truth in one
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
- `tasks.X` crosses a task boundary only through `with:` (the binding IS the
  data edge — the body reads `${{ with.<name> }}`, never `tasks.*` directly)
  or `after: {X: succeeded}` (control · predicates `succeeded` · `failed` ·
  `skipped` · `terminal`). `depends_on` is dead (`check --fix` migrates).
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
`nika spec --schema` is the JSON Schema · `nika spec --canon` is the SSOT ·
`nika catalog` names the providers/models · `nika catalog --tools` names the `nika:`
builtins. Copy, fill, check.

## Cost honesty (never hide unknown spend)
- `nika check` prints the ceiling BEFORE any token · `≥ $X FLOOR` means an
  unbounded task exists — name why, never round unknown to $0.
- A local model is unpriced compute, **never « free »**.
- `nika run <file> --max-cost-usd <n>` blocks BEFORE the call that would
  cross the cap.

## Understand · replay · prove
- `nika inspect <file>` — static anatomy: tasks · verbs · wave groups · cost.
- `nika inspect <file> --format mermaid|dot|json` — the ONE graph projector.
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
- Edges are declared, never restated: a `with:` binding reading `${{ tasks.id.output }}` IS the data edge · `after: { task_id: succeeded }` is the control edge (predicates: `succeeded` · `failed` · `skipped` · `terminal`). `depends_on` is dead — `nika check --fix` migrates it.
- Secrets are declared in a top-level `secrets:` block (e.g. `source: env`, `key: MY_KEY`) and referenced as `${{ secrets.name }}` — never inline literal keys; `${{ env.* }}` is for non-sensitive configuration.
- After every edit, run `nika check <file>` — `--fix` heals the mechanical
  renames, the diagnostics teach the rest.
- Unknown code? Run `nika explain NIKA-XXXX`.
"#;

/// `.cursor/mcp.json` — the project-scoped MCP wiring for Cursor: the
/// read-only oracle (9 tools) reaches the agent without any manual setup.
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
- `tasks.X` is read at the boundary only: `with: { alias: ${{ tasks.X.output }} }`
  is the data edge · `after: { X: succeeded }` orders without data · the
  body reads `${{ with.alias }}`.
- `invoke` arguments live under `args:` · secrets come from the
  environment (`${{ secrets.X }}`) — never inline.
- Never invent syntax: `nika spec --schema` is the JSON Schema · `nika catalog`
  / `nika catalog --tools` name the providers and builtins.
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

/// `.agents/skills/nika-authoring/SKILL.md` — the repo-level agent skill
/// (agentskills.io shape · discovered by Codex and every `.agents`-aware
/// client). Canonical copy: the engine's own Codex plugin at
/// `.agents/plugins/nika/skills/nika-authoring/SKILL.md` — a test enforces
/// byte parity so the two surfaces cannot drift.
const AGENT_SKILL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/skills/nika-authoring/SKILL.md"
));

/// `.cursor/agents/nika-*.md` — the three kit subagents, project-side.
/// Cursor's LOCAL plugin loader consumes MCP + skills ONLY (agents in a
/// local plugin manifest are ignored; the marketplace path processes the
/// full manifest but is submission-gated) — so the binary carries them
/// into the repo, where Cursor's project discovery (`.cursor/agents/`)
/// DOES read them. `include_str!` from the kit: one source, byte parity
/// by construction (the `AGENT_SKILL` precedent).
const CURSOR_AGENT_AUTHOR: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/agents/nika-author.md"
));
const CURSOR_AGENT_DEBUGGER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/agents/nika-debugger.md"
));
const CURSOR_AGENT_MIGRATOR: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/agents/nika-migrator.md"
));

/// `.cursor/rules/nika-delegation.mdc` — WHEN to hand a job to which
/// subagent (the kit's delegation rule, same one-source law).
const CURSOR_DELEGATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/rules/nika-delegation.mdc"
));

/// `.cursor/hooks-nika/*.sh` — the three seatbelts, verbatim from the
/// kit. The scripts sniff their dialect from stdin and self-silence
/// outside nika contexts, so carrying them project-side is safe by the
/// same proof that ships them in the plugin.
const HOOK_SESSION_CONTEXT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/scripts/session-context.sh"
));
const HOOK_CHECK_ON_EDIT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/scripts/check-on-edit.sh"
));
const HOOK_GUARD_RUN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/scripts/guard-run.sh"
));

/// `.cursor/hooks.json` — the project-level hook wiring. Commands point
/// at the scripts the SAME scaffold writes (project-relative), never at
/// a plugin root; a parity test walks each command back to `targets()`.
/// The kit's own hooks manifest — test-only anchor: the project manifest
/// below must mirror it structurally or the build fails.
#[cfg(test)]
const KIT_CURSOR_HOOKS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/hooks/cursor-hooks.json"
));

const CURSOR_HOOKS_JSON: &str = r#"{
  "hooks": {
    "sessionStart": [
      { "command": "./.cursor/hooks-nika/session-context.sh" }
    ],
    "afterFileEdit": [
      { "command": "./.cursor/hooks-nika/check-on-edit.sh" }
    ],
    "beforeShellExecution": [
      { "command": "./.cursor/hooks-nika/guard-run.sh" }
    ]
  }
}
"#;

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
/// writes — `plan` and the docs both read it.
pub(super) fn targets() -> [(&'static str, &'static str); 15] {
    [
        (".vscode/settings.json", VSCODE_SETTINGS),
        ("AGENTS.md", AGENTS_MD),
        (".cursor/rules/nika.mdc", CURSOR_RULES),
        (".cursor/rules/nika-delegation.mdc", CURSOR_DELEGATION),
        (".cursor/mcp.json", CURSOR_MCP),
        (".cursor/agents/nika-author.md", CURSOR_AGENT_AUTHOR),
        (".cursor/agents/nika-debugger.md", CURSOR_AGENT_DEBUGGER),
        (".cursor/agents/nika-migrator.md", CURSOR_AGENT_MIGRATOR),
        (".cursor/hooks.json", CURSOR_HOOKS_JSON),
        (
            ".cursor/hooks-nika/session-context.sh",
            HOOK_SESSION_CONTEXT,
        ),
        (".cursor/hooks-nika/check-on-edit.sh", HOOK_CHECK_ON_EDIT),
        (".cursor/hooks-nika/guard-run.sh", HOOK_GUARD_RUN),
        (".agents/skills/nika-authoring/SKILL.md", AGENT_SKILL),
        (".github/copilot-instructions.md", COPILOT_INSTRUCTIONS),
        ("CLAUDE.md", CLAUDE_MD),
    ]
}

/// The schema-wiring body, exposed so the wizard's canvas step can stamp
/// the chosen `nika.dag.theme` into the SAME JSON it would have written.
pub(super) const fn vscode_settings() -> &'static str {
    VSCODE_SETTINGS
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "args:",                // invoke args under args:, not input:/params:
            "quote",                // quote any scalar that starts with ${{
            "size()",               // the only CEL function in the v0.1 subset
            "content:",             // nika:write needs content:
            "`with:`",              // tasks.* crosses the boundary through with:
            "`after:",              // …and after: is the control door (W2 · the flow)
            "`depends_on` is dead", // the dead form is NAMED, with its codemod
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
        for needle in ["nika new --from", "nika examples", "nika spec --schema"] {
            assert!(
                AGENTS_MD.contains(needle),
                "the guide names the discovery command `{needle}`"
            );
        }
    }

    /// The scaffold table stays complete — every family present (schema
    /// wiring · contract · per-client briefs · skill · Cursor project
    /// equipment: subagents + delegation + the three seatbelts).
    #[test]
    fn targets_names_every_brief_family() {
        let t = targets();
        assert_eq!(t.len(), 15);
        let paths: Vec<&str> = t.iter().map(|(p, _)| *p).collect();
        for expected in [
            ".vscode/settings.json",
            "AGENTS.md",
            ".cursor/rules/nika.mdc",
            ".cursor/rules/nika-delegation.mdc",
            ".cursor/mcp.json",
            ".cursor/agents/nika-author.md",
            ".cursor/agents/nika-debugger.md",
            ".cursor/agents/nika-migrator.md",
            ".cursor/hooks.json",
            ".cursor/hooks-nika/session-context.sh",
            ".cursor/hooks-nika/check-on-edit.sh",
            ".cursor/hooks-nika/guard-run.sh",
            ".agents/skills/nika-authoring/SKILL.md",
            ".github/copilot-instructions.md",
            "CLAUDE.md",
        ] {
            assert!(paths.contains(&expected), "{expected} missing");
        }
    }

    /// Every command in the project hooks wiring resolves to a script the
    /// SAME scaffold writes — a hooks.json naming a path init does not
    /// create would be a dead seatbelt on every fresh repo.
    #[test]
    fn project_hooks_point_at_scaffolded_scripts() {
        let paths: Vec<&str> = targets().iter().map(|(p, _)| *p).collect();
        let wiring: serde_json::Value =
            serde_json::from_str(CURSOR_HOOKS_JSON).expect("hooks.json parses");
        let hooks = wiring["hooks"].as_object().expect("hooks object");
        assert_eq!(hooks.len(), 3, "three seatbelts, no more, no fewer");
        for (event, entries) in hooks {
            for entry in entries.as_array().expect("entry array") {
                let cmd = entry["command"].as_str().expect("command string");
                let rel = cmd.strip_prefix("./").unwrap_or(cmd);
                assert!(
                    paths.contains(&rel),
                    "{event} points at {rel}, which the scaffold never writes"
                );
            }
        }
    }

    /// The project manifest mirrors the KIT's — same events, same script
    /// basenames, only the path prefix differs (workspace vs plugin
    /// scope). The sibling test above checks internal consistency; this
    /// one checks the SOURCE: a seatbelt added to the kit that the
    /// project manifest misses fails here, not silently in the field.
    #[test]
    fn project_hooks_manifest_mirrors_the_kit() {
        let ours: serde_json::Value =
            serde_json::from_str(CURSOR_HOOKS_JSON).expect("project manifest parses");
        let kit: serde_json::Value =
            serde_json::from_str(KIT_CURSOR_HOOKS).expect("kit manifest parses");
        let shape = |v: &serde_json::Value| -> Vec<(String, String)> {
            let mut out: Vec<(String, String)> = v["hooks"]
                .as_object()
                .expect("hooks object")
                .iter()
                .flat_map(|(event, entries)| {
                    entries
                        .as_array()
                        .expect("entry array")
                        .iter()
                        .map(|e| {
                            let cmd = e["command"].as_str().expect("command string");
                            let base = cmd.rsplit('/').next().expect("basename");
                            (event.clone(), base.to_owned())
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            out.sort();
            out
        };
        assert_eq!(
            shape(&ours),
            shape(&kit),
            "project hooks.json drifted from the kit manifest"
        );
    }

    /// The subagents keep their kit identity — Cursor matches them by
    /// frontmatter `name:`, and a renamed kit agent must fail HERE, not
    /// silently in every scaffolded repo.
    #[test]
    fn cursor_subagents_carry_their_kit_names() {
        for (body, name) in [
            (CURSOR_AGENT_AUTHOR, "nika-author"),
            (CURSOR_AGENT_DEBUGGER, "nika-debugger"),
            (CURSOR_AGENT_MIGRATOR, "nika-migrator"),
        ] {
            assert!(
                body.contains(&format!("name: {name}")),
                "{name} frontmatter drifted from the kit"
            );
        }
        assert!(
            CURSOR_DELEGATION.contains("nika-author"),
            "the delegation rule must route to the subagents it ships with"
        );
    }
}
