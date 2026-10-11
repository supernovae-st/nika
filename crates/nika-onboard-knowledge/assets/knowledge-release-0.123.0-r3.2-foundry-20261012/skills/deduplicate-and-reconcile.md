---
name: "nika-authoring-deduplicate-and-reconcile"
description: "Identify duplicates by a key, refuse conflicts, reconcile with the current state using deterministic deduplicate/diff/lookup. Applies to sealed dedup/revision-check obligations such as factures-doublo"
nika_skill_role: "authoring"
nika_family: "family:deduplicate-and-reconcile"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Collapse duplicates, reconcile against reference

## Scope
Identify duplicates by a key, refuse conflicts, reconcile with the current state using deterministic deduplicate/diff/lookup. Applies to sealed dedup/revision-check obligations such as factures-doublons and reprise-approbation.

## When to use
- request names duplicate collapse by key with conflict refusal
- need to reconcile incoming records against a reference state deterministically
- quality_constraint requires deterministic and transformations include deduplicate/diff/lookup

## When not to use
- any generative drafting or summarization (use draft-report skill)
- CSV parsing without dedup (use parse-csv-records pattern)
- idempotent effect without prior state read (use dedup-idempotent-effect)

## Facets
```json
{"goal": ["validate", "transform"], "source": ["structured-records", "prior-run-state"], "transformation": ["deduplicate", "diff", "lookup"], "effect": ["none"], "authority": ["automatic"], "control_flow": ["linear"]}
```

## Required business information (human-owned)
- dedup key field name
- reference state file or path
- conflict refusal policy (error vs skip)
- input record source path or payload

## Machine-owned holes
- jq expression for unique_by(key) + conflict detection
- json_diff or json_merge_patch expressions
- lookup expression against reference
- glob patterns for fs permits

## Required capabilities
- RecordTransformer
- Comparator
- Validator
- DeterministicDecider

## Procedure
1. declare const for reference_path and dedup_key (human values)
2. read reference state with nika:read (pattern:deduplicate)
3. parse records with nika:convert or nika:jq
4. deduplicate with nika:jq using unique_by(.${{ const.dedup_key }}) and refuse conflicts
5. diff against reference with nika:json_diff (pattern:compare-snapshots)
6. lookup/reconcile with nika:jq or nika:validate
7. write reconciled state with nika:write if needed (pattern:write-artifact)
8. declare permits.fs.read/write and tools exactly for the builtins used

## Compatible patterns
- pattern:deduplicate
- pattern:compare-snapshots
- pattern:dedup-idempotent-effect
- pattern:declared-boundary

## Compatible blocks
- block:validate-diff-convert
- block:csv-filter-total-report

## Authority and effects
- permits.tools: nika:read nika:jq nika:json_diff nika:validate nika:write
- permits.fs.read/write for reference and output paths
- no secrets egress; human gate only if prompt required for conflict decision

## Common mistakes
- using model arithmetic instead of nika:jq for totals (NIKA-EXEC-001)
- missing conflict refusal in dedup expression (silent merge)
- reading tasks.* outside with: (NIKA-VAR-021)
- omitting permits for nika:json_diff

## Diagnostic remedies
- `NIKA-VAR-021` · hoist reference into with: binding
- `NIKA-SEC-004` · add exact tool to permits.tools
- `NIKA-PARSE-013` · rename extract binding away from reserved names

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --var KEY=... in isolated workspace
- compare outputs.diff and outputs.reconciled against expected json_diff and unique_by results

## Positive example intent
Identify duplicate invoices by numero, refuse any conflicting amounts, and reconcile the deduplicated list against the last approved state file.

## Counterexample intent
Summarize the reconciled invoices in natural language (use draft-report skill instead; this skill is deterministic only)
