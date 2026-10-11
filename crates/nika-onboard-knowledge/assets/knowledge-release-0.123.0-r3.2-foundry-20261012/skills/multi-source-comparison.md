---
name: "nika-authoring-multi-source-comparison"
description: "Handles requests that read two or more sources, compare them on explicit criteria, produce a ranking or selection, and explain the outcome. Uses join/sort via nika:jq, optional infer for prose explana"
nika_skill_role: "authoring"
nika_family: "family:multi-source-comparison"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Compare sources and choose/rank with criteria

## Scope
Handles requests that read two or more sources, compare them on explicit criteria, produce a ranking or selection, and explain the outcome. Uses join/sort via nika:jq, optional infer for prose explanation, and nika:decide when a deterministic choice is required.

## When to use
- request names two or more sources and comparison criteria
- output is a ranked list, selection, or decision with explanation
- sources are files, records, or API results that can be joined

## When not to use
- single-source summarization or extraction (use draft-from-facts or structured-extraction)
- pure classification without ranking (use infer-returns-enum)
- multi-source merge without selection criteria (use join-sources only)

## Facets
```json
{"goal": ["compare", "rank"], "source": ["local-file", "local-folder", "structured-records", "remote-api"], "transformation": ["join", "sort", "aggregate"], "effect": ["none", "write-artifact"], "authority": ["automatic", "human-first"], "control_flow": ["linear", "fan-in"]}
```

## Required business information (human-owned)
- paths or URLs of the two or more sources
- comparison criteria or ranking key(s)
- desired output format (report, decision, ranked list)
- optional output file path

## Machine-owned holes
- jq expressions for join, sort_by, and selection
- globs or permits.fs.read entries for source paths
- topology of read/parse/join/rank tasks
- when predicates for optional human gate

## Required capabilities
- ReadableSource
- StructuredRecordSource
- Comparator
- Ranker
- Aggregator
- DeterministicDecider

## Procedure
1. declare each source path or endpoint as const (human supplies literal values)
2. add permits.fs.read or net.http for every source; require fs.read/fs.write path globs for nika:read/nika:write if used
3. create one read/parse task per source using invoke nika:read + nika:convert or nika:fetch
4. add a join task that binds all parsed outputs via with: and runs nika:jq with join expression
5. add a rank/select task using nika:jq sort_by + limit or nika:decide with bundle+evidence
6. optionally add an infer task that receives the ranked result via with: and produces explanation (draft-from-facts pattern)
7. if human approval required, insert nika:prompt gate before any write or external effect
8. wire outputs: to the final ranked/decision value or report artifact

## Compatible patterns
- join-sources
- rank-top-k
- draft-from-facts
- typed-output
- aggregate-number

## Compatible blocks
- block:csv-filter-total-report
- block:prompt-gate-then-post
- block:infer-returns-enum

## Authority and effects
- permits.tools lists only effect-carrying tools (mcp: targets); nika:convert/nika:jq/nika:decide are pure-compute and need no grant.
- fs.read globs for every source file; net.http exact hosts for remote sources; fs.write path globs when nika:write produces an artifact
- human gate (nika:prompt) required before any write-artifact or http-mutation effect when authority is human-first

## Common mistakes
- using model arithmetic for totals or ranking instead of nika:jq (NIKA-EXEC-001 or quality_constraint violation)
- reading tasks.* directly inside infer prompt instead of with: binding (NIKA-VAR-021)

## Diagnostic remedies
- `NIKA-VAR-021` · hoist tasks.* references into with: bindings before the infer or jq task
- `NIKA-SEC-004` · add explicit egress sanction or move secret out of prompt
- `NIKA-AUTH-006` · add permits block with required tools and fs/net entries

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --output json
- compare jq output arrays for correct join order and sort keys; verify ranked result matches criteria

## Positive example intent
Read the three supplier quotes in ./quotes/*.csv, rank them by total cost then delivery time, and produce a short decision report.

## Counterexample intent
Summarize a single PDF file (use draft-from-facts instead; no multi-source join or ranking required)
