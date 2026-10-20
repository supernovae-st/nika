---
name: "nika-authoring-api-sync-with-idempotency"
description: "Builds .nika workflows that sync structured records to an external HTTP API while guaranteeing idempotency via a persisted state file of processed IDs. Uses credential egress, nika:fetch with idempote"
nika_skill_role: "authoring"
nika_family: "family:api-sync-with-idempotency"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

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
2. Declare the credential under secrets with its source (env or file) and one egress rule for the sink that uses it, egress: [{ to: "nika:fetch" }]; the host is the literal of the fetch url, and the credential never reaches outputs.
3. Grant permits.tools nika:fetch, nika:read and nika:write, permits.net.http the exact API host, and permits.fs read and write on the state file path.
4. Task read_state: nika:read of the state file named in const, with on_error: { recover: "[]", on_codes: [NIKA-BUILTIN-READ-001] } so the first run, before any state exists, starts from an empty list; decode it with nika:jq fromjson.
5. Task compute_new: nika:jq over input: { records: "${{ with.records }}", seen: "${{ with.seen }}" }, both sides bound in with: and never spliced into the expression, keeping the records whose key is not among the seen ids.
6. Task push: nika:fetch POST once per new record (for_each over them, no on_error skip), with an idempotency-key header built from the record's stable key; add retry only with that header (NIKA-SEC-016 otherwise).
7. Task write_state: after: { push: success }, nika:write the seen ids plus the new records' ids computed before the push, never the push's responses: a response that reaches the write is the lethal trifecta (NIKA-SEC-009).
8. Wire with: edges from read_state → compute_diff → push → write_state; add after only for control if needed.
9. outputs: the new ids and the state written; a failed push fails the run before the state is written, so the next run sends the same records again under the same idempotency keys.

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
- the credential's egress names only nika:fetch
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
