---
name: "nika-authoring-api-sync-with-idempotency"
description: "Builds .nika workflows that sync structured records to an external HTTP API while guaranteeing idempotency via a persisted state file of processed IDs. Uses credential egress, nika:fetch with idempote"
nika_skill_role: "authoring"
nika_family: "family:api-sync-with-idempotency"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Push to API with idempotency and state file (no double-apply)

## Scope
Builds .nika workflows that sync structured records to an external HTTP API while guaranteeing idempotency via a persisted state file of processed IDs. Uses credential egress, nika:fetch with idempotency-key, nika:read/nika:write for the state file, and reconcile recovery on partial failure.

## When to use
- request names an external API endpoint + a state file path and wants to avoid duplicate mutations across runs
- effect is http-mutation combined with state-update and recovery is reconcile
- records come from structured-records or prior-run-state and must be filtered against the state file before POST/PUT

## When not to use
- pure in-memory dedup without a state file (use pattern:dedup-idempotent-effect only)
- GET-only or read-only remote calls (use pattern:read-known-file or block:secret-egress-fetch)
- human approval gate required before the mutation (use pattern:human-approved-effect)

## Facets
```json
{"goal": ["sync"], "source": ["structured-records", "remote-api", "prior-run-state"], "transformation": ["filter", "deduplicate", "join"], "effect": ["http-mutation", "state-update"], "authority": ["credential-required"], "control_flow": ["recovering", "linear"]}
```

## Required business information (human-owned)
- API base URL / endpoint (exact host for permits.net.http)
- state file path (for processed IDs)
- credential secret name (env or file) and its egress target
- records source (file path or input payload)
- idempotency key strategy or stable key expression

## Machine-owned holes
- jq expression that computes the set difference between incoming records and state file
- glob or exact path for state file in permits.fs
- exact host literal in permits.net.http
- idempotency-key header value construction
- permit list for nika:fetch + nika:read + nika:write

## Required capabilities
- ExternalMutator
- RecordTransformer
- ArtifactWriter
- StructuredRecordSource

## Procedure
1. Declare nika: name, model (mock/echo for rehearsal), inputs for records payload if any, const for stable key or endpoint pieces.
2. Declare secrets.<name> with source, key/path and egress: [{to: nika:fetch, host: <exact>}, {to: outputs}].
3. Write permits: {tools: ["nika:fetch","nika:read","nika:write"], net: {http: ["<host>"]}, fs: {read/write: ["<state-path>"]}}.
4. Task read_state: invoke nika:read on the state file path (from const or input).
5. Task compute_diff: invoke nika:jq with input bound to one side and the other embedded in the expression (or use extract:/json_merge_patch).
6. Task push: invoke nika:fetch POST/PUT with literal or with:-bound idempotency-key header; attach retry only if idempotency-key present.
7. Task write_state: invoke nika:write that merges new IDs into the state file (overwrite true).
8. Wire with: edges from read_state → compute_diff → push → write_state; add after only for control if needed.
9. outputs: the push result and final state summary; add on_error: {recover: ...} or reconcile pattern for partial failure.

## Compatible patterns
- pattern:dedup-idempotent-effect
- pattern:idempotent-write
- pattern:secret-egress
- pattern:declared-boundary
- pattern:post-json
- pattern:read-known-file
- pattern:write-artifact

## Compatible blocks
- block:idempotent-post-retry
- block:secret-egress-fetch

## Authority and effects
- permits.net.http exact host, permits.fs.read/write on state file, permits.tools nika:fetch+read+write
- secrets.<name> egress only to nika:fetch and outputs
- human gate NOT required unless the request explicitly asks for approval before mutation

## Common mistakes
- omitting idempotency-key on POST/PUT with retry > 1 → NIKA-SEC-016
- reading/writing state file without fs.read/fs.write permit → NIKA-AUTH-006
- secret used in fetch without matching egress rule → NIKA-SEC-006
- state file path not a literal in permits → NIKA-AUTH-007
- using shell under exec allowlist for curl instead of nika:fetch → n58-shell-under-allowlist

## Diagnostic remedies
- `NIKA-SEC-016` · add idempotency-key header or remove retry
- `NIKA-SEC-006` · add egress rule for the secret to nika:fetch
- `NIKA-AUTH-006` · add the required fs/net/tools entries to permits
- `NIKA-AUTH-007` · use literal bound for the path in permits

## Validation recipe
- nika check <file.nika> --native-strict
- nika run <file.nika> --model mock/echo
- compare: state file read before push, only unseen records sent, state file written after, no duplicate IDs across simulated runs

## Positive example intent
Sync the new orders to https://api.crm.example.com/v1/orders using ./state/processed.json to avoid sending the same order twice; use the API key from env CRM_TOKEN

## Counterexample intent
Fetch the current list of orders from the CRM API (no state file, no mutation, no idempotency needed)
