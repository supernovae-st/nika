---
name: "nika-authoring-event-driven-enrichment"
description: "Builds .nika workflows that receive an event as typed inputs, perform remote API lookups for enrichment, normalize the payload with jq or nika:jq, and POST the result to a destination endpoint under e"
nika_skill_role: "authoring"
nika_family: "family:event-driven-enrichment"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Enrich incoming event payload via remote lookup, normalize, and forward with HTTP mutation

## Scope
Builds .nika workflows that receive an event as typed inputs, perform remote API lookups for enrichment, normalize the payload with jq or nika:jq, and POST the result to a destination endpoint under event-driven temporality. Matches n8n-style app_event families such as arrivee-collaborateur and gestion-incidents that require http-mutation effects from inputs-payload + remote-api sources.

## When to use
- Event payload arrives as inputs (pattern:event-payload-inputs)
- Enrichment requires remote JSON fetch (pattern:fetch-remote-json) followed by normalization (pattern:normalize-schema)
- Result must be forwarded via POST (pattern:post-json) with secret egress (pattern:secret-egress) and declared permits (pattern:declared-boundary)
- Temporality is event-driven and effect is http-mutation

## When not to use
- No remote lookup or enrichment needed (use pure local transformation skill)
- Recurring or scheduled runs instead of event-driven (use lifecycle:scheduled family)
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
2. Declare typed inputs: for the incoming event payload (pattern:event-payload-inputs) using { object: { field: T } } form for objects
3. Declare const: for any fixed endpoint or key values
4. Declare secrets: with source:env|file and egress to the tool id for the fetch call and outputs (pattern:secret-egress, block:secret-egress-fetch)
5. Declare permits: with net.http exact hosts, no extra authority
6. Create fetch task using invoke.tool nika:fetch with url, headers containing secret, mode:jq (pattern:fetch-remote-json)
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
- secrets.* must carry egress to the tool id for the fetch call and outputs
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
- Compare outputs against expected normalized payload and confirm exactly one POST to declared host with no secret leakage in traces

## Positive example intent
Enrichir l'événement d'arrivée collaborateur avec les données RH, normaliser et poster vers le système de paie

## Counterexample intent
Summarize the event payload and store a report locally (different family: no http-mutation, uses local-file effect and nika:write instead of fetch)
