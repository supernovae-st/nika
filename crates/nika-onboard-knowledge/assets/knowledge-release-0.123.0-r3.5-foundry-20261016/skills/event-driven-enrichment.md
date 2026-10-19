---
name: "nika-authoring-event-driven-enrichment"
description: "Builds .nika workflows that receive an event as typed inputs, look up enrichment data in a remote JSON API, normalize the payload with nika:jq, and POST the result to a destination endpoint."
nika_skill_role: "authoring"
nika_family: "family:event-driven-enrichment"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

# Enrich incoming event payload via remote lookup, normalize, and forward with HTTP mutation

## Scope
Builds .nika workflows that receive an event as typed inputs, look up enrichment data in a remote JSON API, normalize the payload with nika:jq, and POST the result to a destination endpoint.

## When to use
- Event payload arrives as inputs (pattern:event-payload-inputs)
- Enrichment requires remote JSON fetch (pattern:fetch-remote-json) followed by normalization (pattern:normalize-schema)
- Result must be forwarded via POST (pattern:post-json) with secret egress (pattern:secret-egress) and declared permits (pattern:declared-boundary)
- Temporality is event-driven and effect is http-mutation

## When not to use
- No remote lookup or enrichment needed (use pure local transformation skill)
- a recurring run over a source rather than one event (the schedule is an arm entry of the project's nika.yaml; see family:scheduled-digest)
- Human gate or agentic research required before mutation (use prompt-gate-then-post block family)

## Facets
```json
{"goal": ["transform", "extract", "sync"], "source": ["inputs-payload", "remote-api"], "transformation": ["lookup", "convert", "structured-extraction"], "effect": ["http-mutation"], "authority": ["credential-required"], "control_flow": ["linear"]}
```

## Required business information (human-owned)
- destination endpoint URL or const name
- API host for permits.net.http
- secret name and source for authorization header
- normalization jq expression or nika:jq mapping rules
- input field names and types for the event payload

## Machine-owned holes
- jq expressions inside extract or nika:jq
- exact host globs in permits.net.http
- secret egress rules to the tool id for the fetch call and outputs
- task topology and after/when edges
- permits.net.http exact hosts

## Required capabilities
- RemoteResourceSource
- RecordTransformer
- ExternalMutator
- StructuredExtractor

## Procedure
1. Declare nika: kebab-id and model
2. Declare typed inputs for the event's fields (pattern:event-payload-inputs)
3. Declare const: for any fixed endpoint or key values
4. Declare the credential under secrets (source env or file) with an egress rule for nika:fetch, the only sink that uses it (pattern:secret-egress, block:secret-egress-fetch)
5. Declare permits: with net.http exact hosts, no extra authority
6. Create the lookup task: nika:fetch with the url, the credential in headers, mode: jq and jq: "." for the parsed JSON (pattern:fetch-remote-json)
7. Create normalize task using invoke.tool nika:jq or extract: with pure jq expression over previous output (no ${{ }} inside expression) (pattern:normalize-schema)
8. Create forward task using invoke.tool nika:fetch method:POST, content-type application/json, body from normalized output (pattern:post-json, block:idempotent-post-retry if retry needed)
9. Wire data edges with with: bindings and control edges with after: if needed; add when: only for conditional paths
10. Declare outputs: referencing the final task output
11. Run nika check --native-strict then validate with mock/echo run

## Compatible patterns
- event-payload-inputs
- fetch-remote-json
- normalize-schema
- post-json
- secret-egress
- declared-boundary

## Compatible blocks
- block:secret-egress-fetch
- block:idempotent-post-retry

## Authority and effects
- permits.net.http exact host required for fetch
- the credential's egress names nika:fetch; its taint never reaches outputs
- human gate required only if prompt block is added; otherwise automatic under declared permits

## Common mistakes
- Using bare inputs.flag in when: without == true (NIKA-VAR-005)
- Missing egress on secret used in fetch headers (NIKA-SEC-006)
- Loopback host in permits.net.http (NIKA-SEC-005)
- Placing tasks.* reference outside with: (NIKA-VAR-021)
- Retry on non-idempotent POST without idempotency-key (NIKA-SEC-016)
- Using type: object instead of { object: { field: T } } (NIKA-TYPE-001)
- Interpolating ${{ }} inside nika:jq expression strings

## Diagnostic remedies
- `NIKA-SEC-006` · Add egress rule on the secret for to: the tool id for the fetch call
- `NIKA-VAR-021` · Hoist tasks.* reference into with: binding
- `NIKA-SEC-005` · Use exact non-loopback host literal in permits.net.http
- `NIKA-SEC-016` · Add idempotency-key header or remove retry
- `NIKA-TYPE-001` · Write object types as { object: { field: T } }
- `NIKA-VAR-005` · Write explicit relation like == true for bare flags

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --var event=<json>
- Compare the outputs with the expected normalized payload against a test endpoint first: mock/echo replaces the model, never the HTTP calls

## Positive example intent
Enrichir l'événement d'arrivée collaborateur avec les données RH, normaliser et poster vers le système de paie

## Counterexample intent
Summarize the event payload and store a report locally (different family: no http-mutation, uses local-file effect and nika:write instead of fetch)
