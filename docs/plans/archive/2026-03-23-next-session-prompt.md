# Next Session Prompt — Copy-paste this to start

```
Continue the Nika DX v0.42 roadmap. Previous session (2026-03-23) did a mega audit: 73 commits, 60+ issues fixed, 9.8/10 user score.

Read the plan at docs/plans/2026-03-23-dx-v042-roadmap.md and the memory files for context.

## What was done (v0.41.x)
- VS Code extension: fixed schema, snippets, menus, binary check, LSP default
- All AI editor rules moved to user scope (~/) — machine.rs handles 6 editors
- init_ai.rs stripped -82% (1499→272 lines), AGENTS.md lightweight
- Auto-setup on first command (maybe_run_auto_setup in main.rs)
- is_ci() skips setup in CI (12 env vars + headless)
- Content hash protection for user-customized rules (xxh3-64)
- Course system: TDD fixes for check_no_todos, per-exercise checks
- Showcase: placeholder substitution on extract
- 31 map-style task blocks → list-style in missions.rs
- xAI added to TUI provider modal

## What to do now (v0.42)

### Priority 1: CUT NOISE
1. Kill `nika setup` command entirely (absorb into init + doctor --fix)
2. Slim `.nika/` init (remove 6 empty/unused files: memory.yaml, policies.yaml, user.yaml, proposed/, cache/, memory/)
3. Hide power-user commands from --help (schema, features, workflow, lsp)
4. Simplify `nika new` to `nika new <name> [--verb exec]`

### Priority 2: WOW MOMENTS
5. `nika` bare = live 3-second demo (built-in DAG, no API key)
6. Inline DAG summary before `nika run`
7. Editor reveal: show each editor name, not just count
8. Provider list as guide: `✗ openai → nika keys set openai`
9. Memorable end-of-run: `3 tasks | 2 parallel | 1.2k tokens | $0.003 | 847ms`

### Priority 3: ARCHITECTURE
10. Extract setup_shared.rs (machine.rs + setup.rs duplicate logic)
11. Split machine.rs → machine_status.rs + machine_install.rs
12. Move rule constants to rules/ dir with include_str!()
13. Add 24h cooldown to quick_editor_scan

Use subagent-driven development. TDD for Rust changes. 1 commit per logical fix.

Vision: "Write a YAML file. Nika runs it as DAG, calls any AI, tracks every cent, configures every tool. One command."
```
