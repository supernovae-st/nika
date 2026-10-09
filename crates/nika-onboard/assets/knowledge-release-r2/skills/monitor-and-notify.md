---
name: "nika-authoring-monitor-and-notify"
description: "Builds workflows that poll a remote page or API, compare the current response against a prior snapshot using deterministic diff, and emit a notification only on detected change. Covers gallery price-w"
nika_skill_role: "authoring"
nika_family: "family:monitor-and-notify"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Detect change via poll-diff-notify

## Scope
Builds workflows that poll a remote page or API, compare the current response against a prior snapshot using deterministic diff, and emit a notification only on detected change. Covers gallery price-watch, config-drift and release-radar patterns with recovery via on_error.recover and transformation via diff.

## When to use
- request asks to poll a URL or API and alert on any change
- change detection must be deterministic (json_diff or equivalent)
- notification is the only side-effect and must be gated on actual difference

## When not to use
- one-time read or fetch without comparison (use read-known-file or fetch-page-content)
- agentic research or multi-turn summarization required
- local file monitoring or scheduled cron (no schedule key exists)

## Facets
```json
{"goal": ["monitor", "notify"], "source": ["remote-api", "web-page"], "transformation": ["diff"], "effect": ["send-message"], "authority": ["automatic"], "control_flow": ["linear", "recovering"]}
```

## Required business information (human-owned)
- URL or API endpoint to poll
- notification channel and target
- snapshot storage path or prior-run reference
- optional severity or message template

## Machine-owned holes
- permits.net.http exact hosts and permits.tools list
- when: CEL predicate over with binding
- topology of fetch → diff → notify edges

## Required capabilities
- RemoteResourceSource
- Comparator
- Notifier

## Procedure
1. declare nika: kebab-id and model
2. declare inputs for URL, target and any human knobs; const for fixed snapshot key
3. add permits.net.http with exact host and permits.tools containing nika:fetch, nika:notify
4. create fetch task: invoke nika:fetch with url from inputs, mode text or json
5. create diff task: with: { before: prior snapshot, after: fetch.output }; invoke nika:json_diff
6. create notify task: with: { changed: ${{ tasks.diff.output.changed }} }; after: { diff: success }; when: "${{ with.changed == true }}"; invoke nika:notify with target and message
7. recover the notify task with a literal value or point recover at a non-dependent task (NIKA-DAG-004); place an `on_error.recover` value on the diff task; add timeout on fetch
8. wire outputs to expose diff result and notification status

## Compatible patterns
- pattern:fetch-page-content
- pattern:compare-snapshots
- pattern:notify-webhook
- pattern:guard-on-value
- pattern:retryable-read
- pattern:recover-value

## Compatible blocks
- block:fetch-modes-source
- block:validate-diff-convert

## Authority and effects
- permits.net.http required for fetch; permits.tools for nika:notify; secrets target needs egress to nika:notify
- if a gate is added, place nika:prompt as a task before the notify effect, bind approved: ${{ tasks.<gate>.output }} in notify's with:, and gate with when: "${{ with.approved == true }}"
- effect send-message only on actual diff change

## Common mistakes
- using bare boolean in when: (NIKA-VAR-005)
- fetching loopback host without exact literal (NIKA-SEC-005)
- omitting permits.net or permits.tools (NIKA-AUTH-006)
- retry on non-idempotent fetch without idempotency-key (NIKA-SEC-016)

## Diagnostic remedies
- `NIKA-VAR-005` · write explicit relation e.g. == true
- `NIKA-SEC-005` · declare exact loopback literal or change host
- `NIKA-AUTH-006` · add permits block with net.http and tools
- `NIKA-SEC-016` · add idempotency-key header or drop retry

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model <model>
- compare outputs.diff and outputs.notify against expected change/no-change cases

## Positive example intent
Poll https://example.com/prices every hour and notify the webhook when any price changes

## Counterexample intent
Summarize the changed page with an agent (requires agentic skill, not pure poll-diff-notify)
