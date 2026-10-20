---
name: "nika-authoring-collection-filter-aggregate"
description: "Solves requests that start from structured records (CSV/JSON), keep only those admitted by a deterministic condition, then compute exact counts/sums/totals or grouped aggregates. Uses nika:convert + n"
nika_skill_role: "authoring"
nika_family: "family:collection-filter-aggregate"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

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
1. Declare const.source_path and const.output_path (the literal paths the request states)
2. tasks.read_source: invoke nika:read with the path from const
3. tasks.parse_source: invoke nika:convert (from: csv, to: json) over the text bound in with:
4. tasks.compute: invoke nika:jq with an expression that selects, then counts or sums with tonumber; the records arrive through with:
5. (optional) tasks.draft: an infer after compute that receives the computed facts through with: and is told to invent nothing
6. tasks.write_report: invoke nika:write with the path from const and the content from the previous output
7. outputs: expose the computed aggregate (typed when a caller depends on its shape)
8. permits: grant exactly the effect tools used (nika:read, nika:write) and the named paths; nika:convert and nika:jq are pure and need no grant

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
- asking an infer for counts, sums or totals: a model's arithmetic is not exact; compute them with nika:jq and give the model the computed facts
- using for_each over a scalar instead of an array (NIKA-VAR-006)
- summing CSV cells as text: nika:convert returns every cell as a string, so apply tonumber before add
- writing shell under permits.exec allowlist (NIKA-SEC-004)

## Diagnostic remedies
- `NIKA-SEC-006` · move the secret reference out of the prompt or add an egress sanction
- `NIKA-VAR-006` · change for_each.items to an array expression or remove for_each
- `NIKA-PARSE-019` · write after: as a map from a task to the outcome it waits for, never a list

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo in a copy of the project with a small orders.csv (source_path is a const, not an input)
- compare outputs.computed against expected exact totals; verify no model token usage on numeric tasks

## Positive example intent
From the paid payments in orders.csv keep only those with statut=='payé' and compute the exact count and total montant.

## Counterexample intent
Summarize the payments in a short French paragraph (use draft-from-facts instead because prose generation is required)
