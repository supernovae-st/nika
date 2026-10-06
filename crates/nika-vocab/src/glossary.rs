// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the `.nika` language calls the words other tools taught (node,
//! trigger, secret…) and its own envelope words, each with the form a
//! workflow writes. A teaching table over this crate's vocabulary: it reads
//! no file and grants nothing.

/// Each word with what the language calls it and the form a workflow writes.
pub const GLOSSARY: &[(&str, &str)] = &[
    (
        "node",
        "a `task` (a map entry under `tasks:` · one verb each) · the edges are `with:` bindings (data) and `after:` (order), never a canvas",
    ),
    (
        "step",
        "a `task` under `tasks:` (a map, never a list) · order is the DAG the bindings draw, not the file order",
    ),
    (
        "job",
        "a `task`, or a whole workflow file invoked as a child (`invoke: { workflow: ./child.nika }`)",
    ),
    (
        "trigger",
        "there is no trigger inside a workflow · a run is started by `nika run`, by `nika serve` (the resident firer) or by an armed cadence in the project file (`nika arm`)",
    ),
    (
        "cron",
        "an armed cadence in the project file (`nika arm` · `nika serve` fires it) · a workflow itself carries no schedule",
    ),
    (
        "schedule",
        "an armed cadence in the project file (`nika arm` · `nika serve` fires it) · a workflow itself carries no schedule",
    ),
    (
        "webhook",
        "`nika serve` is the resident door (authenticated loopback HTTP) · a workflow itself listens to nothing",
    ),
    (
        "secret",
        "`secrets:` at the envelope — store references only (`{ source: env, key: NAME }`), never a value · read as `${{ secrets.NAME }}` · reaches an effect only through an `egress:` door",
    ),
    (
        "credential",
        "`secrets:` at the envelope — store references only (`{ source: env, key: NAME }`), never a value · read as `${{ secrets.NAME }}`",
    ),
    (
        "action",
        "a builtin `nika:<name>` under `invoke:` (28 ship · `nika catalog --tools`) or an `mcp:<server>/<tool>` (`nika wire` adds a server)",
    ),
    (
        "plugin",
        "a builtin `nika:<name>` under `invoke:` or an MCP server (`mcp:<server>/<tool>` · `nika wire`)",
    ),
    (
        "integration",
        "an MCP server (`mcp:<server>/<tool>` under `invoke:` · `nika wire` adds one) or a builtin `nika:<name>`",
    ),
    (
        "connection",
        "an MCP server (`mcp:<server>/<tool>` under `invoke:` · `nika wire` adds one)",
    ),
    (
        "variable",
        "`inputs:` (caller-supplied · `--var k=v`) or `const:` (baked in the file) · read as `${{ inputs.x }}` / `${{ const.x }}` · `vars:` and `env:` are dead forms",
    ),
    (
        "environment variable",
        "a secret reference (`secrets: { NAME: { source: env, key: NAME } }`) · the environment is never read directly (`env:` is a dead form)",
    ),
    (
        "output",
        "`outputs:` at the envelope (`${{ tasks.x.output }}`) · a file lands through `nika:write` under a `permits.fs.write` grant",
    ),
    (
        "artifact",
        "a file landed by `nika:write` under a `permits.fs.write` grant · the run's evidence is the trace under `.nika/traces/`",
    ),
    (
        "loop",
        "`for_each: { items: … , max_parallel, fail_fast }` on a task · `${{ item }}` and `${{ index }}` inside",
    ),
    (
        "condition",
        "`when: \"${{ … }}\"` on a task (a CEL boolean) · `after: { x: failure }` routes on an outcome",
    ),
    (
        "retry",
        "`retry: { max_attempts, backoff_ms, backoff_strategy, jitter, on_codes }` on a task",
    ),
    (
        "timeout",
        "`timeout: \"30s\"` on a task (a duration string · max 24h)",
    ),
    (
        "approval",
        "a human gate · `invoke: { tool: \"nika:prompt\" }` pauses the run (exit 4) and `--resume <trace> --answer <task>=<value>` continues it",
    ),
    (
        "human",
        "a human gate · `invoke: { tool: \"nika:prompt\" }` pauses the run (exit 4) and `--resume` continues it",
    ),
    (
        "pipeline",
        "a workflow · one `.nika` file · nine envelope keys · `tasks:` a map · four verbs",
    ),
    (
        "function",
        "a `task` with one verb (`infer` · `exec` · `invoke` · `agent`) · a reusable one is a child workflow under `invoke: { workflow: … }`",
    ),
    (
        "permits",
        "the declared boundary: what the file may read (`fs.read`) · write (`fs.write`) · reach (`net.http`) · run (`exec`) · call (`tools`) · see (`env`) · absent = zero authority · a run refuses anything outside it · `nika check --infer-permits` writes the tightest block the body needs",
    ),
    (
        "inputs",
        "what the caller supplies at run time (`--var name=value`) · typed · a `default:` makes one a deployment knob · read as `${{ inputs.name }}`",
    ),
    (
        "const",
        "values baked in the file · read as `${{ const.name }}` · never a secret",
    ),
    (
        "secrets",
        "store references only (`{ source: env, key: NAME }`), never a value · read as `${{ secrets.NAME }}` · reaches an effect only through an `egress:` door",
    ),
    (
        "tasks",
        "the work: a map keyed by task id, one verb each (`infer` · `exec` · `invoke` · `agent`) · the order is the DAG the `with:` bindings and `after:` edges draw",
    ),
    (
        "outputs",
        "what the workflow returns (`name: ${{ tasks.x.output }}`) · the only place a task's output is read outside `with:`",
    ),
    (
        "model",
        "the default seat for every `infer` · `<provider>/<name>` · `mock/echo` rehearses offline · a task may name its own",
    ),
    (
        "with",
        "a task's bindings · `with: { name: \"${{ tasks.x.output }}\" }` IS the data edge · read inside the task as `${{ with.name }}`",
    ),
    (
        "after",
        "an order edge without data · `after: { x: success }` (or `failure` · `skipped` · `terminal` · `unwind`)",
    ),
    (
        "infer",
        "the verb for one model call · `prompt` (required) · `system` · `model` · `temperature` · `max_tokens` · `schema` for structured output",
    ),
    (
        "exec",
        "the verb for a process · `command: [\"prog\", \"arg\"]` (argv · no shell) or `shell: \"…\"` (the explicit door) · needs `permits.exec`",
    ),
    (
        "invoke",
        "the verb for a builtin (`nika:<name>`) · an MCP tool (`mcp:<server>/<tool>`) · or a child workflow (`workflow: ./x.nika`)",
    ),
    (
        "agent",
        "the verb for a governed multi-turn loop · `prompt` · `tools: [globs · default-deny]` · `max_turns` · `max_tokens_total`",
    ),
];

/// The entry for `word` (one word or two, any case), singular or plural:
/// « triggers » finds « trigger », « environment variables » the pair.
#[must_use]
pub fn entry(word: &str) -> Option<(&'static str, &'static str)> {
    let word = word.trim().to_lowercase();
    let singular = word.strip_suffix('s');
    GLOSSARY
        .iter()
        .find(|(known, _)| *known == word)
        .or_else(|| GLOSSARY.iter().find(|(known, _)| Some(*known) == singular))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::entry;

    #[test]
    fn an_entry_is_found_in_either_number_and_any_case() {
        assert_eq!(entry("Trigger").map(|(w, _)| w), Some("trigger"));
        assert_eq!(entry("triggers").map(|(w, _)| w), Some("trigger"));
        assert_eq!(entry("secrets").map(|(w, _)| w), Some("secrets"));
        assert_eq!(
            entry("environment variables").map(|(w, _)| w),
            Some("environment variable")
        );
        assert!(entry("comet").is_none());
    }
}
