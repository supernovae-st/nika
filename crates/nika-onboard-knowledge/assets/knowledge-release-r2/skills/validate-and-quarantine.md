---
name: "nika-authoring-validate-and-quarantine"
description: "Builds workflows that run nika:validate on a record set, apply on_error: skip to quarantine failures, and feed only the clean subset downstream. Covers the validate-records pattern and etl-state skele"
nika_skill_role: "authoring"
nika_family: "family:validate-and-quarantine"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Validate records then skip malformed (quarantine + clean continuation)

## Scope
Builds workflows that run nika:validate on a record set, apply on_error: skip to quarantine failures, and feed only the clean subset downstream. Covers the validate-records pattern and etl-state skeletons that separate schema-validation from later aggregation or transform steps.

## When to use
- - request names validate / schema check / quarantine malformed / skip bad records before totals or transforms
- - input is structured-records or csv/json that must be filtered by compliance before any aggregate or write

## When not to use
- - no validation step is mentioned (use pattern:parse-csv-records or pattern:aggregate-number directly)
- - malformed input must abort the whole workflow (use default on_error fail instead of skip)

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
- on_error: skip wiring
- jq expression that extracts only the valid subset or the error list
- permits.tools entry for nika:validate

## Required capabilities
- Validator
- RecordTransformer

## Procedure
1. 1. declare inputs or const for the record source and the schema literal
2. 2. add a task that invokes nika:validate with data and schema args under for_each (pattern:validate-records), attach on_error: { skip: true }
3. 3. create a downstream task that binds the validate output (array of iteration records) and uses nika:jq inspecting .status == "skipped" to separate valids/invalids, then wire value edges via with: and add permits.tools + fs.read for the source; expose the clean set in outputs

## Compatible patterns
- pattern:validate-records

## Compatible blocks
- block:validate-diff-convert
- block:csv-filter-total-report

## Authority and effects
- permits.tools must list nika:validate (pure builtin, no extra grant)
- no secrets egress
- no human gate required

## Common mistakes
- placing tasks.* references inside when: or prompt instead of with: (NIKA-VAR-021)
- using on_error: recover instead of skip for quarantine (wrong recovery semantics)
- omitting the schema arg or passing a non-object schema (NIKA-BUILTIN-001)

## Diagnostic remedies
- `NIKA-VAR-021` · hoist the reference into a with: binding
- `NIKA-PARSE-005` · remove unknown keys; keep only the nine envelope keys
- `NIKA-BUILTIN-001` · supply the exact required args for nika:validate (data, schema)

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --var source=./data/sample.json
- compare outputs.clean against the subset that passed the schema; verify skipped tasks appear with .status == skipped

## Positive example intent
Validate the orders records against the payment schema, skip any malformed rows, then compute totals only on the clean set.

## Counterexample intent
Validate the CSV then abort the whole run on the first bad row (use default failure instead of on_error skip)
