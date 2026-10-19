---
name: "nika-authoring-batch-process-and-report"
description: "Solves requests that process every item of a collection one by one (typically with invoke or exec), skip the items that fail and report which succeeded and which failed. Uses for_each with on_error: {"
nika_skill_role: "authoring"
nika_family: "family:batch-process-and-report"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

# Per-item processing with ledger of successes and failures

## Scope
Solves requests that process every item of a collection one by one (typically with invoke or exec), skip the items that fail and report which succeeded and which failed. Uses for_each with on_error: { skip: true }, then pairs each input with its per-item output (null where the item failed) in one nika:jq ledger.

## When to use
- Request names many items or a folder of files to process individually
- Failures must be tolerated and reported rather than aborting the run
- A consolidated ledger or summary of per-item success/failure is required

## When not to use
- All items must succeed or workflow must fail fast (use linear or pattern:per-item-map without skip)
- No ledger/report is needed (use pattern:per-item-map alone)
- Processing is not item-based (use pattern:discover-files or single-task flows)

## Facets
```json
{"goal": ["transform", "aggregate"], "source": ["local-folder", "structured-records"], "transformation": ["filter", "structured-extraction"], "effect": ["none"], "authority": ["automatic"], "control_flow": ["per-item", "fan-in"], "recovery": ["skip"], "result": ["report"]}
```

## Required business information (human-owned)
- folder or collection source
- per-item processing logic or tool
- report aggregation rules or jq expression

## Machine-owned holes
- glob pattern for discovery
- for_each.max_parallel value
- the ledger's pairing of the inputs with the per-item outputs
- on_error.skip placement
- permits.fs/tools entries

## Required capabilities
- RecordTransformer
- Aggregator
- EnumerableSource

## Procedure
1. Declare the source folder as const (the folder the request names) and grant it as a glob in permits.fs.read (./reports/**)
2. Add permits for required tools (nika:glob, nika:read) and fs paths (permits.fs, permits.tools)
3. Create discover task with invoke.tool nika:glob (pattern:discover-files)
4. Create the per-item task: with: { paths: "${{ tasks.discover.output }}" }, for_each: { items: "${{ with.paths }}", max_parallel: 4, fail_fast: false } and on_error: { skip: true }, listing in on_codes the codes the processing may raise so an authority refusal still fails the run (pattern:per-item-map, pattern:skip-and-report)
5. Create the ledger task: bind the paths and the per-item outputs in with:, then nika:jq pairs them by index, [range(0; .paths | length) as $i | {path: .paths[$i], ok: (.outputs[$i] != null)}] (pattern:fan-in-ledger)
6. Wire outputs to the ledger task output (outputs.forms)

## Compatible patterns
- pattern:discover-files
- pattern:per-item-map
- pattern:skip-and-report
- pattern:fan-in-ledger
- pattern:bounded-parallel-map

## Compatible blocks
- block:glob-read-many
- block:for-each-extract-count
- block:on-error-skip-on-timeout

## Authority and effects
- permits.fs.read for source paths
- permits.tools for nika:glob/nika:read
- no secrets egress required
- no human gate required

## Common mistakes
- using bare for_each instead of for_each block (n20-for-each-bare)
- reading tasks.* inside when or body instead of with (n22-tasks-in-prompt)
- omitting on_error.skip when ledger of failures is needed
- expecting fail_fast: false alone to keep the run going: a failed item still fails the task; on_error: { skip: true } on the per-item task is what keeps it
- reading a failed item's output as data: it is null, so pair the outputs with their inputs by index

## Diagnostic remedies
- `NIKA-VAR-021` · hoist tasks.* reference into with: binding
- `NIKA-DAG-008` · add group: name to per-item task and bind group.<name> in ledger
- `n20-for-each-bare` · wrap for_each value in {items: ...} block

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo in a copy of the project whose folder holds a few files, one of them broken
- compare outputs.ledger length and per-item status array against expected successes/failures

## Positive example intent
Process every CSV in ./reports/, skip any that fail to parse, and produce a ledger of which succeeded and which failed with their errors.

## Counterexample intent
Run a single API call and email the result (use linear flow instead; no per-item or fan-in needed)
