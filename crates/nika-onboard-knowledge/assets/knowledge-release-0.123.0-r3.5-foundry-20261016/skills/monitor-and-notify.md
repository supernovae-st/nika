---
name: "nika-authoring-monitor-and-notify"
description: "Builds workflows that compare a source with the snapshot the previous scheduled run saved, notify only when it changed and save the new snapshot. A local source (an export another tool drops) runs una"
nika_skill_role: "authoring"
nika_family: "family:monitor-and-notify"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

# Detect change via poll-diff-notify

## Scope
Builds workflows that compare a source with the snapshot the previous scheduled run saved, notify only when it changed and save the new snapshot. A local source (an export another tool drops) runs unattended (block:watch-export-notify); a page or an API fetched beside the snapshot file is the lethal trifecta (NIKA-SEC-009), so a person's nika:prompt gate holds the notification and the save. The schedule is an arm entry of the project's nika.yaml with its IANA zone, never a key of the .nika file.

## When to use
- request asks to poll a URL or API and alert on any change
- change detection must be deterministic (json_diff or equivalent)
- notification is the only side-effect and must be gated on actual difference

## When not to use
- one-time read or fetch without comparison (use read-known-file or fetch-page-content)
- agentic research or multi-turn summarization required
- alerting on one fetched value against a threshold (a price above a limit): compare it with a const, keep no snapshot file, and no person's gate is needed

## Facets
```json
{"goal": ["monitor", "notify"], "source": ["remote-api", "web-page"], "transformation": ["diff"], "effect": ["send-message"], "authority": ["automatic"], "control_flow": ["linear", "recovering"]}
```

## Required business information (human-owned)
- URL or API endpoint to poll
- notification channel and target
- the file that keeps the previous snapshot between runs
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
2. declare const for the source (the export file, or the URL), the webhook target and the snapshot file (a path or URL taken from inputs into permits is a taint, NIKA-AUTH-008)
3. grant permits.fs.read on the source and the snapshot, permits.fs.write on the snapshot, permits.net.http the webhook host, and permits.tools nika:read, nika:jq, nika:json_diff, nika:notify and nika:write (block:watch-export-notify)
4. read the current value: nika:read of the export, decoded with fromjson; a fetched source (nika:fetch with mode: jq and jq: ".") makes the workflow a lethal trifecta, so a nika:prompt gate with no default comes before the notify and the save
5. read the previous snapshot with nika:read and on_error: { recover: "null", on_codes: [NIKA-BUILTIN-READ-001] } for the first run, decode it with fromjson, then nika:json_diff with before the previous value and after the fetched one, both bound in with:
6. changed: nika:jq over input: { previous: "${{ with.previous }}", patch: "${{ with.patch }}" } with expression: ".previous != null and (.patch | length > 0)" (json_diff returns an RFC 6902 array, empty when nothing changed); notify: with: { changed: "${{ tasks.changed.output }}" }, when: "${{ with.changed == true }}", nika:notify with channel: webhook, the target and the message
7. save: after: { changed: success, notify: terminal }, with the notify's status bound in with: and when: "${{ with.notified != 'failure' }}", so a failed notification keeps the old snapshot and a failed comparison never overwrites it
8. wire outputs to expose diff result and notification status

## Compatible patterns
- pattern:state-file
- pattern:compare-snapshots
- pattern:notify-webhook
- pattern:guard-on-value
- pattern:retryable-read
- pattern:fallback-value

## Compatible blocks
- block:watch-export-notify
- block:validate-diff-convert

## Authority and effects
- permits.net.http required for fetch; permits.tools for nika:notify; secrets target needs egress to nika:notify
- if a gate is added, place a nika:prompt with no default before the notify effect, bind approved: "${{ tasks.review.output }}" in the notify's with: and guard it with when: "${{ with.approved == true }}"
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
- run twice against a test webhook: the first run saves the snapshot and notifies nothing, the second, after the source changed, notifies once

## Positive example intent
Poll https://example.com/prices every hour and notify the webhook when any price changes

## Counterexample intent
Summarize the changed page with an agent (requires agentic skill, not pure poll-diff-notify)
