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
- **Author** · `nika new <template> <file>.nika.yaml` (or write one —
  the envelope is `nika: <id>` (kebab-case — the id lives ON the tag)
  + a `tasks:` MAP keyed by task id. A `tasks:` sequence refuses
  `NIKA-PARSE-022`).
- **Check** · `nika check <file>` — the static audit BEFORE any run (schema ·
  DAG · CEL · effects · permits · cost). Exit `0` clean · `2` findings.
  `--fix` applies the machine-applicable repairs (typed did-you-mean renames —
  fields · tools · args · edge targets · refs — plus the provable
  `depends_on` → `with:`/`after:` migration and `tasks.*` hoists) and
  re-audits; ambiguity is skipped with a note, never guessed.
- **Run** · `nika run <file>` — execute · live render. Exit `0` ok · `1` failed.
  Inputs ride `--var key=value` (repeatable · the flag names an `inputs:`
  declaration · unknown keys refused); a run
  paused on a `nika:prompt` resumes with `--resume <trace> --answer
  <task>=<value>` (confirm gates take booleans: `--answer approve=true`).
- **Pin** · `nika test <file> --update` writes `<file>.golden.json` from a
  simulated run — the model is `mock/echo` (offline · deterministic) and
  real effects are REFUSED, never performed; `nika test <file>` replays
  and compares — zero keys, the CI gate.
- **Arm** · `nika arm` — what this project's `nika.yaml` has ARMED (`arm:`)
  and when each beat next fires. READ-ONLY: it schedules nothing — the file
  proposes, the machine disposes.
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
`infer` (an LLM call) · `exec` (a subprocess — `command:` is argv, one token per
element, run via execve · no implicit shell: pipes, redirects and globs go in
`shell:` explicitly) · `invoke` (a `nika:` builtin or MCP tool) · `agent` (a
multi-turn ReAct loop).

## Hard rules (the validator enforces these — they catch ~90% of LLM errors)
- One verb per task · the verb IS the task key (never a `verb:` field).
- Values live in FOUR authorities, a closed family: `inputs:` (typed · caller-
  supplied) · `config:` (typed · deployment-supplied) · `const:` (fixed in the
  file) · `secrets:` (governed store references). `vars:` and `env:` are dead
  envelope fields (`NIKA-VALUES-001` · `NIKA-VALUES-002`) and any other
  namespace is `NIKA-VALUES-003` — classify each entry by the role it plays;
  `check --fix` migrates the `vars:` half, `env:` is a human classification.
- `tasks.X` crosses a task boundary only through `with:` (the binding IS the
  data edge — the body reads `${{ with.<name> }}`, never `tasks.*` directly)
  or `after: {X: success}` (control · predicates `success` · `failure` ·
  `skipped` · `terminal`). `depends_on` is dead (`check --fix` migrates).
- Quote any YAML scalar that STARTS with `${{` (an unquoted leading `${{`
  breaks the parse).
- `invoke` arguments live under `args:` (not `input:` / `params:`).
- `when:` is a `${{ }}` CEL boolean or the literal `true`/`false` — a bare
  string is rejected. `size()` is the only CEL function.
- `nika:write` needs `content:` · `nika:done` is valid only inside `agent.tools`.
- snake_case task ids · kebab-case workflow id (on `nika:`).

## Don't invent structure — route to a skeleton
`nika new '?'` lists the embedded skeletons · `nika try` /
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
- `nika key init|trust|rotate` — the run-signing key: it seals journals and
  signs workflows (print the fingerprint with `nika key trust` to enroll it).
- `nika sign <file>` — author-bind a workflow (detached `<file>.minisig`
  sidecar · the workflow itself never changes) · `nika sign --check <file>`
  verifies · `nika run --require-signature <file>` refuses an unsigned or
  invalidly-signed workflow BEFORE anything executes (exit 2).
- `nika trace evidence <run>` — export the evidence pack: journal + manifest
  (hash · boundary · trifecta · sandbox · seal grade) + receipt + VERIFY.md.
- `nika dap` — step a recorded run under a debugger UI, forward AND back.

## Servers (stdio · for editors and agent clients)
`nika lsp` (language server) · `nika mcp` (MCP: check/explain/schema/examples
as tools) · `nika completions <shell>` generates shell completions.
`nika model serve --model <path.gguf>` serves a local model on loopback
(OpenAI-compatible · needs a `local-infer` build — the default binary
prints the build recipe).

## Discipline
- `permits:` IS the boundary, and ABSENT MEANS ZERO AUTHORITY: an effect under
  no block refuses `NIKA-AUTH-006` at check, before a token is spent. A pure-
  compute body states the zero explicitly as `permits: {}`.
  `nika check --infer-permits` prints the tightest block; a bound is always a
  literal, never an interpolation (`NIKA-AUTH-007`).
- A spawned child inherits NOTHING from the engine: its environment is the
  runner floor ∪ the names in `permits: { env: [NAME] }` ∪ the task's own
  `env:` map. A variable the child needs must be named.
- Secrets come from the environment (`${{ secrets.X }}`) — never inline.
- `nika check` must be clean before `nika run` (audit-before-run is enforced).
- The wired shell hook's judge is `nika guard` (the execution seatbelt):
  before a `nika run` leaves the agent's shell it audits the exact file —
  a red file or a priced model without `--max-cost-usd` is denied with the
  findings; `guard_unavailable` means the judge could not see, never that
  the check passed.
";

/// `.cursor/rules/nika.mdc` — the agent-facing authoring floor for Cursor.
/// The kit's own language rule, `include_str!`d like its nine siblings —
/// one source, byte parity by construction. It was the LAST scaffolded
/// surface still duplicated as a literal, and it is the one that drifted:
/// the kit copy still taught `vars:`/`${{ env.X }}`/`succeeded` three
/// releases after the engine refused them (2026-07-28 audit). Kept compact
/// so it stays cheap enough to auto-load on every `.nika.yaml` edit.
const CURSOR_RULES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/rules/nika-workflow-language.mdc"
));

/// The standard `mcpServers` stanza — ONE body, three project files: the
/// read-only oracle (9 tools) reaches the agent without any manual setup,
/// project-scoped so the config travels with the repo and never touches
/// the user's other projects.
///   · `.cursor/mcp.json` — Cursor's project scope
///   · `.mcp.json` — the root file FOUR surfaces read natively: Claude
///     Code (project scope) · Grok Build (its Claude compat) · GitHub
///     Copilot CLI (cwd → repo root) · Warp (third-party interop)
///   · `.agents/mcp_config.json` — Antigravity CLI's workspace file
///     (`agy` · a stdio command entry is the standard shape; the
///     url→serverUrl rename touches remote servers only), living under
///     `.agents/` beside the skill — the cross-vendor convention Warp ·
///     Antigravity · Kimi · Amp share
const MCP_SERVERS: &str =
    "{ \"mcpServers\": { \"nika\": { \"command\": \"nika\", \"args\": [\"mcp\"] } } }\n";

/// `.github/copilot-instructions.md` — the GitHub Copilot repo brief.
/// Compact on purpose: the loop + the four hard rules that catch most
/// LLM authoring errors; AGENTS.md carries the full contract.
const COPILOT_INSTRUCTIONS: &str = r"# Nika workflows (`*.nika.yaml`) — Copilot brief

Nika workflows are audited BEFORE they run. The loop: author from a
skeleton (`nika new '?'` lists them) → `nika check <file>` after
EVERY edit → `nika check <file> --fix` heals the mechanical renames →
repair the rest from the diagnostics (`nika explain NIKA-XXXX`) →
only a clean file reaches a human.

Rules the validator enforces:
- Envelope `nika: <id>` · one verb per task (`infer` · `exec` · `invoke` ·
  `agent`) · the verb IS the task key.
- `tasks.X` is read at the boundary only: `with: { alias: ${{ tasks.X.output }} }`
  is the data edge · `after: { X: success }` orders without data · the
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

/// The kit's Codex manifest — test-only anchor: the plugin page's first
/// contact (description · defaultPrompt) is pinned against the three-door
/// CTA contract so a copy edit cannot silently re-expose engine
/// capabilities as the entry points.
#[cfg(test)]
const KIT_CODEX_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.agents/plugins/nika/.codex-plugin/plugin.json"
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
pub(super) fn targets() -> [(&'static str, &'static str); 17] {
    [
        (".vscode/settings.json", VSCODE_SETTINGS),
        ("AGENTS.md", AGENTS_MD),
        (".cursor/rules/nika.mdc", CURSOR_RULES),
        (".cursor/rules/nika-delegation.mdc", CURSOR_DELEGATION),
        (".cursor/mcp.json", MCP_SERVERS),
        (".mcp.json", MCP_SERVERS),
        (".agents/mcp_config.json", MCP_SERVERS),
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

    /// Every teaching surface in the kit, as (kit-relative path, body).
    /// Append-only history is never rewritten (cross-source §2.7), so the
    /// CHANGELOG stays out of the sweep.
    fn kit_teaching_surfaces() -> Vec<(String, String)> {
        const HISTORY: &[&str] = &["CHANGELOG.md"];
        let kit =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.agents/plugins/nika");
        let mut stack = vec![kit.clone()];
        let mut out = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("the kit directory is readable") {
                let path = entry.expect("a readable dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !matches!(ext, "md" | "mdc" | "sh") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&kit)
                    .expect("under the kit root")
                    .to_string_lossy()
                    .replace('\\', "/");
                if HISTORY.contains(&rel.as_str()) {
                    continue;
                }
                out.push((
                    rel,
                    std::fs::read_to_string(&path).expect("a utf-8 surface"),
                ));
            }
        }
        out
    }

    // Banned in every teaching surface — no exemption, ever.
    const DEAD_USAGE: &[(&str, &str)] = &[
        (
            "${{ vars.",
            "NIKA-VALUES-001 · dead namespace — read `inputs:` or `const:`",
        ),
        (
            "${{ env.",
            "NIKA-VALUES-002 · dead namespace — read `config:` or `secrets:`",
        ),
        (
            "capture: text",
            "capture is stdout · stderr · combined · structured",
        ),
        (
            "workflow: <",
            "the envelope `workflow:` key died 2026-08-12 — the id lives ON `nika:` (NIKA-PARSE-020/021 RETIRED, never reused)",
        ),
        // The SAME dead fact, stated in prose. The literal above
        // matched a YAML placeholder and sailed straight past
        // « a `workflow:` OBJECT carrying `id:` » — which is how the
        // brief kept teaching the dead envelope for two days after
        // the nuke. A denylist keyed on ONE spelling only ever
        // catches that spelling.
        (
            "`workflow:` OBJECT",
            "the envelope `workflow:` key died 2026-08-12 — say `nika: <id>`",
        ),
    ];
    // NAMING a retired form is not TEACHING it: a port table whose
    // left column is « dead form » has to spell the dead form, and a
    // language surface earns its keep by inoculating a model that
    // still carries 0.105 priors. So these are banned everywhere the
    // subject is HOW TO WRITE ONE WORKFLOW (the commands, the
    // task-shaped skills) and allowed on the surfaces whose subject
    // is the LANGUAGE ITSELF or the port.
    const DEAD_NAMING: &[(&str, &str)] = &[
        (
            "`vars:`",
            "NIKA-VALUES-001 · classify into `inputs:`/`const:`",
        ),
        ("`depends_on`", "dead edge form — `with:` / `after:`"),
        (": succeeded", "NIKA-DAG-005 · the predicate is `success`"),
        (": failed", "NIKA-DAG-005 · the predicate is `failure`"),
    ];
    const MAY_NAME_THE_RETIRED: &[&str] = &[
        "skills/nika-migration/SKILL.md",
        "agents/nika-migrator.md",
        "agents/nika-author.md",
        "skills/nika-authoring/SKILL.md",
        "rules/nika-workflow-language.mdc",
    ];

    /// The kit may never TEACH a form the live engine refuses.
    ///
    /// Found empirically 2026-07-28: three releases after the engine
    /// began refusing them, the kit still taught `vars:`, `${{ env.X }}`,
    /// `after: { t: succeeded }`, a scalar `workflow:` and
    /// `capture: text` — so a file written from the plugin's own
    /// instructions died at PARSE. The pattern behind it is the lesson:
    /// every scaffolded surface that had a parity test (AGENTS.md vs the
    /// live clap tree) stayed current, and every surface without one
    /// rotted. This is that test for the rest of the kit — a denylist of
    /// forms the engine refuses, plus a REAL `nika check` over every
    /// complete workflow the kit prints.
    ///
    /// A migration surface may NAME a dead form (porting is its job); it
    /// may never USE one. That asymmetry is the whole exemption.
    #[test]
    fn the_kit_never_teaches_a_form_the_engine_refuses() {
        let surfaces = kit_teaching_surfaces();
        let mut sins: Vec<String> = Vec::new();

        for (rel, body) in &surfaces {
            for (needle, why) in DEAD_USAGE {
                if body.contains(needle) {
                    sins.push(format!("{rel} USES the dead form `{needle}` — {why}"));
                }
            }
            if !MAY_NAME_THE_RETIRED.contains(&rel.as_str()) {
                for (needle, why) in DEAD_NAMING {
                    if body.contains(needle) {
                        sins.push(format!("{rel} teaches `{needle}` — {why}"));
                    }
                }
            }

            // Every COMPLETE workflow the kit prints audits for real.
            //
            // ⚠️ The selector used to be `starts_with("nika: v1")`, which
            // named the envelope the 2026-08-12 nuke retired. It was not
            // leaking — the kit prints no complete workflow today, so the
            // loop had no subject either way — but it was ARMED: the next
            // workflow the kit gained would have started `nika: <id>`,
            // been skipped in silence, and this test would have kept
            // printing a green for an audit it never ran. A selector
            // keyed on a dead spelling is a gate waiting to stop looking.
            for block in body.split("```").skip(1).step_by(2) {
                let yaml = block.split_once('\n').map_or("", |(_, rest)| rest);
                let head = yaml.trim_start();
                if !(head.starts_with("nika:") && yaml.contains("tasks:")) {
                    continue;
                }
                match nika_schema::parse(
                    yaml,
                    nika_schema::FileId::new(0),
                    nika_schema::ParseMode::Strict,
                ) {
                    Err(e) => {
                        sins.push(format!("{rel}: a taught workflow does not PARSE: {e:?}"));
                    }
                    Ok(wf) => {
                        if !nika_check::check(&wf).is_clean() {
                            sins.push(format!(
                                "{rel}: a taught workflow does not audit CLEAN — the kit must \
                                 never print YAML a user cannot run"
                            ));
                        }
                    }
                }
            }
        }

        assert!(sins.is_empty(), "the kit drifted:\n  {}", sins.join("\n  "));
        assert!(
            surfaces.len() >= 14,
            "expected the whole kit scanned (skills · agents · commands · rules · scripts), got {}",
            surfaces.len()
        );
    }

    /// The flag-day vocabulary must be PRESENT, not merely un-rotten: a
    /// silent deletion would pass the denylist above while leaving an
    /// agent unable to write a legal envelope.
    #[test]
    fn the_kit_teaches_the_three_value_authorities_and_the_boundary() {
        for (name, body) in [
            ("the language rule", CURSOR_RULES),
            ("the authoring skill", AGENT_SKILL),
        ] {
            for needle in ["inputs:", "const:", "secrets:"] {
                assert!(
                    body.contains(needle),
                    "{name} must name the `{needle}` authority"
                );
            }
            // …and NEVER the fourth. A kit that still offers `config:`
            // teaches an envelope the parser refuses (`NIKA-PARSE`), and
            // a denylist that only checks for ABSENCE of rot would pass
            // it. The authorities are exactly three.
            assert!(
                !body.contains("${{ config."),
                "{name} must not teach the dead `config` namespace"
            );
            assert!(
                body.contains("NIKA-AUTH-006"),
                "{name} must teach that an absent `permits:` block is zero authority"
            );
            assert!(
                body.contains("permits: {}"),
                "{name} must teach the legal zero for a pure-compute body"
            );
        }
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
        for needle in ["nika new", "nika try", "nika spec --schema"] {
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
        assert_eq!(t.len(), 17);
        let paths: Vec<&str> = t.iter().map(|(p, _)| *p).collect();
        for expected in [
            ".vscode/settings.json",
            "AGENTS.md",
            ".cursor/rules/nika.mdc",
            ".cursor/rules/nika-delegation.mdc",
            ".cursor/mcp.json",
            ".mcp.json",
            ".agents/mcp_config.json",
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

    /// The Codex plugin page opens with the three DOORS — one per visitor
    /// state (create · discover · continue), never three engine
    /// capabilities of equal weight (UX audit 2026-07-30 · three-door CTA
    /// spec). Create is the primary door and the first CTA in the chat —
    /// the platform's Try now is a system CTA and is never duplicated —
    /// and « Nothing runs automatically. » is persistent copy, not a
    /// tooltip. Validation and trace diagnosis stay available BEHIND the
    /// Continue door; they no longer compete with first value.
    #[test]
    fn the_codex_manifest_opens_with_the_three_doors() {
        let create = "Help me turn one task I repeat into a Nika workflow.";
        let discover = "Teach me what Nika is through one small, concrete and safe example.";
        let cont = "Inspect the current project's Nika state in read-only mode.";
        for door in [create, discover, cont] {
            assert!(
                KIT_CODEX_MANIFEST.contains(door),
                "missing door prompt: {door}"
            );
        }
        let pos = |needle: &str| KIT_CODEX_MANIFEST.find(needle).expect("door present");
        assert!(
            pos(create) < pos(discover) && pos(discover) < pos(cont),
            "create is the primary door — first in chat order"
        );
        assert!(
            KIT_CODEX_MANIFEST.contains("Nothing runs automatically."),
            "the safety line is persistent copy, not a tooltip"
        );
        // The retired capability trio (audit 2026-07-30): three internal
        // capabilities presented as equals. They moved behind Continue.
        for retired in [
            "Turn this repeatable task into a checked Nika workflow.",
            "Validate this .nika.yaml file and repair every finding.",
            "Diagnose this failed Nika run from its trace.",
        ] {
            assert!(
                !KIT_CODEX_MANIFEST.contains(retired),
                "retired capability prompt still on the page: {retired}"
            );
        }
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
