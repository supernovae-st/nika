---
name: "nika-authoring-validate-and-quarantine"
description: "Builds workflows that validate each record with nika:validate (it returns {valid, errors} and never fails on invalid data), split the records by .valid, quarantine the invalid ones with their errors a"
nika_skill_role: "authoring"
nika_family: "family:validate-and-quarantine"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

# Validate records then skip malformed (quarantine + clean continuation)

## Scope
Builds workflows that validate each record with nika:validate (it returns {valid, errors} and never fails on invalid data), split the records by .valid, quarantine the invalid ones with their errors and feed only the valid subset downstream. Covers the validate-records pattern and block:validate-quarantine-total.

## When to use
- request names validate, schema check, quarantine malformed or skip bad records before totals or transforms
- input is structured records (csv or json) that must be filtered by compliance before any aggregate or write

## When not to use
- no validation step is mentioned (use pattern:parse-csv-records or pattern:aggregate-number directly)
- malformed input must stop the whole run (assert on the invalid count with nika:assert instead of quarantining)

## Facets
```json
{"goal": ["validate"], "source": ["structured-records"], "transformation": ["schema-validation"], "recovery": ["skip"], "control_flow": ["recovering"], "effect": ["none"], "authority": ["automatic"]}
```

## Required business information (human-owned)
- path or expression that yields the record array
- JSON Schema to validate against
- name of the clean-set variable passed to downstream tasks

## Machine-owned holes
- task topology that places validate before the consumer
- the split of the records by their report's .valid
- jq expression that extracts only the valid subset or the error list
- permits.tools entry for nika:validate

## Required capabilities
- Validator
- RecordTransformer

## Procedure
1. declare const for the records file and the schema (a schema file read with nika:read and decoded with fromjson, or a literal)
2. validate each record: nika:validate with data: "${{ item }}" and the schema, under for_each over the records; each iteration returns {valid, errors}
3. split: nika:jq over input: { reports: "${{ with.reports }}", records: "${{ with.records }}" } pairs each record with its report by index and keeps .valid == true apart from .valid == false; quarantine the invalid records with their errors (nika:write) and pass the valid ones on (block:validate-quarantine-total)

## Compatible patterns
- pattern:validate-records

## Compatible blocks
- block:validate-diff-convert
- block:csv-filter-total-report

## Authority and effects
- nika:validate is pure and needs no grant (listing it is allowed)
- no secrets egress
- no human gate required

## Common mistakes
- placing tasks.* references inside when: or prompt instead of with: (NIKA-VAR-021)
- wrapping nika:validate in on_error to catch invalid records: it never fails on invalid data, so nothing is caught; test .valid == false
- omitting the schema arg or passing a non-object schema (NIKA-BUILTIN-001)

## Diagnostic remedies
- `NIKA-VAR-021` · hoist the reference into a with: binding
- `NIKA-PARSE-005` · remove unknown keys; keep only the nine envelope keys
- `NIKA-BUILTIN-001` · supply the exact required args for nika:validate (data, schema)

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo with a records file holding one valid and one invalid record
- compare the valid set and the quarantined set with the schema's verdicts; no task is skipped or failed for an invalid record

## Positive example intent
Validate the orders records against the payment schema, skip any malformed rows, then compute totals only on the clean set.

## Counterexample intent
Validate the CSV and stop the run at the first invalid row (assert on the invalid count instead of quarantining)
