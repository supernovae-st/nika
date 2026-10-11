---
name: "nika-authoring-collection-filter-aggregate"
description: "Solves requests that start from structured records (CSV/JSON), keep only those admitted by a deterministic condition, then compute exact counts/sums/totals or grouped aggregates. Uses nika:convert + n"
nika_skill_role: "authoring"
nika_family: "family:collection-filter-aggregate"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Filter and aggregate a collection with exact numeric totals

## Scope
Solves requests that start from structured records (CSV/JSON), keep only those admitted by a deterministic condition, then compute exact counts/sums/totals or grouped aggregates. Uses nika:convert + nika:jq exclusively for the numeric work; never hands numbers to an infer/agent.

## When to use
- request names a collection of records and asks to keep a subset then produce exact numeric results
- quality_constraint includes exact-totals and transformation includes filter/aggregate/group
- source is structured-records or a known file that converts to records

## When not to use
- request asks for prose, summary or draft from the numbers (use draft-from-facts)
- request needs to discover files or read an unknown folder (use read-known-file or glob)
- aggregation must be approximate or model-assisted

## Facets
```json
{"goal": ["aggregate"], "source": ["structured-records", "local-file"], "transformation": ["filter", "aggregate", "group"], "effect": ["none"], "authority": ["automatic"], "control_flow": ["linear"]}
```

## Required business information (human-owned)
- source file path or folder
- output file path (if writing a report)
- filter condition (field + value)
- aggregation spec (count, sum field, group key)

## Machine-owned holes
- jq expression that performs select + length/add/tonumber/group_by
- const values for paths when not supplied by human
- permits.fs.read/write globs derived from the paths

## Required capabilities
- RecordTransformer
- Aggregator
- FormatConverter

## Procedure
1. 1. Declare const.source_path and const.output_path (human supplies literal paths)
2. 2. tasks.read_source: invoke nika:read with path from const
3. 3. tasks.parse_source: invoke nika:convert (from:csv to:json) via with binding
4. 4. tasks.compute: invoke nika:jq with expression containing select(.field==value) then length/add/tonumber; bind records via with
5. 5. (optional) tasks.draft: infer only after compute, passing ${{ with.computed }} and instructing 'inventing nothing'
6. 6. tasks.write_report: invoke nika:write with path and content from previous output
7. 7. outputs: expose the computed aggregate object (typed when possible)
8. 8. permits: list exactly the tools used and fs globs for the named paths

## Compatible patterns
- parse-csv-records
- filter-subset
- group-by
- aggregate-number
- workflow-outputs
- read-known-file
- validate-records
- typed-output

## Compatible blocks
- block:csv-filter-total-report
- block:typed-inputs-outputs

## Authority and effects
- permits.tools: nika:read nika:convert nika:jq nika:write
- permits.fs.read/write: exact paths supplied by human
- no secrets; no human gate required

## Common mistakes
- placing numeric computation inside infer prompt (triggers NIKA-SEC-006 or quality_constraint violation)
- using for_each over a scalar instead of an array (NIKA-VAR-006)
- omitting has_header on nika:convert when CSV has header
- writing shell under permits.exec allowlist (NIKA-SEC-004)

## Diagnostic remedies
- `NIKA-SEC-006` · move the secret reference out of the prompt or add an egress sanction
- `NIKA-VAR-006` · change for_each.items to an array expression or remove for_each
- `NIKA-PARSE-019` · use nika: prefix exactly once for builtins

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --var source_path=./data/orders.csv
- compare outputs.computed against expected exact totals; verify no model token usage on numeric tasks

## Positive example intent
From the paid payments in orders.csv keep only those with statut=='payé' and compute the exact count and total montant.

## Counterexample intent
Summarize the payments in a short French paragraph (use draft-from-facts instead because prose generation is required)
