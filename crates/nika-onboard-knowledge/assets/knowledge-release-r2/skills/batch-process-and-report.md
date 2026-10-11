---
name: "nika-authoring-batch-process-and-report"
description: "Solves requests that require iterating over a collection of items, applying per-item processing (typically via invoke or exec), skipping individual failures, and folding outcomes into a single report/"
nika_skill_role: "authoring"
nika_family: "family:batch-process-and-report"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Per-item processing with ledger of successes and failures

## Scope
Solves requests that require iterating over a collection of items, applying per-item processing (typically via invoke or exec), skipping individual failures, and folding outcomes into a single report/ledger via group fan-in. Uses for_each + on_error.skip + group to produce an array of {id,status,output,error} records.

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
- group name and binding
- on_error.skip placement
- permits.fs/tools entries

## Required capabilities
- RecordTransformer
- Aggregator
- EnumerableSource

## Procedure
1. Declare inputs for the collection source and any per-item parameters (inputs.typed)
2. Add permits for required tools (nika:glob, nika:read) and fs paths (permits.fs, permits.tools)
3. Create discover task with invoke.tool nika:glob (pattern:discover-files)
4. Create per-item task with for_each: {items: ${{with.paths}}, max_parallel, fail_fast:false} + on_error: {skip:true} + group: <name> (pattern:per-item-map, pattern:skip-and-report)
5. Create ledger task that binds exactly one ${{ group.<name> }} in with: (the only legal door; no after: needed) and uses nika:jq to produce the report (pattern:fan-in-ledger)
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
- forgetting group binding in with (NIKA-DAG-008)
- adding after: to ledger when group binding already creates the fan-in edge

## Diagnostic remedies
- `NIKA-VAR-021` · hoist tasks.* reference into with: binding
- `NIKA-DAG-008` · add group: name to per-item task and bind group.<name> in ledger
- `n20-for-each-bare` · wrap for_each value in {items: ...} block

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --input source=./testdata --model mock/echo
- compare outputs.ledger length and per-item status array against expected successes/failures

## Positive example intent
Process every CSV in ./reports/, skip any that fail to parse, and produce a ledger of which succeeded and which failed with their errors.

## Counterexample intent
Run a single API call and email the result (use linear flow instead; no per-item or fan-in needed)
