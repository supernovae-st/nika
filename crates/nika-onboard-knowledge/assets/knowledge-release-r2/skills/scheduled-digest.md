---
name: "nika-authoring-scheduled-digest"
description: "Builds a linear .nika workflow that gathers from a source (fetch/read), summarizes (infer/jq), and delivers a notification (nika:notify) for recurring digest requests. Cadence lives outside the file."
nika_skill_role: "authoring"
nika_family: "family:scheduled-digest"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Recurring digest: gather-summarize-notify

## Scope
Builds a linear .nika workflow that gathers from a source (fetch/read), summarizes (infer/jq), and delivers a notification (nika:notify) for recurring digest requests. Cadence lives outside the file.

## When to use
- request specifies every <cadence> gather source summarize deliver to channel
- the request reads like a periodic report or recap sent to people (for example a weekly summary posted to a team channel)
- facets contain temporality:recurring + goal:summarize+notify + effect:send-message + control_flow:linear

## When not to use
- one-off run (use once-only skill)
- event-driven trigger (use event-driven skill)
- needs human gate before send (use human-approved-effect pattern)
- requires fan-out or per-item processing
- writes local artifact instead of notify (use write-artifact pattern)

## Facets
```json
{"goal": ["summarize", "notify"], "source": ["remote-api", "feed", "structured-records"], "transformation": ["aggregate", "text-extraction"], "effect": ["send-message"], "authority": ["automatic"], "control_flow": ["linear"], "temporality": ["recurring"]}
```

## Required business information (human-owned)
- source endpoint or path
- channel/target for delivery
- cadence (external)
- summary prompt or jq expression

## Machine-owned holes
- jq expressions for extraction/aggregate
- exact host in permits.net.http
- glob patterns for fs if local source
- trigger record in nika.yaml arm:

## Required capabilities
- Summarizer
- Notifier
- RemoteResourceSource
- StructuredExtractor

## Procedure
1. declare nika: kebab-id and model
2. declare inputs for any runtime knobs (required:false + default) or const for fixed source/channel
3. add secrets only if credential needed, with egress to the exact tool id used (e.g. to: nika:fetch); if the credential is sent to the notify endpoint, verify nika:notify is an invoke sink whose egress to: is the tool id; otherwise route the credential only through the sanctioned sink
4. declare permits.tools for nika:fetch (which declares net), permits.net.http exact host, permits.fs if local read
5. create linear tasks: gather (invoke nika:fetch or nika:read; for local multi-file use nika:glob then for_each read/convert), summarize (infer with prompt or invoke nika:jq), deliver (invoke nika:notify)
6. wire data edges with with: bindings; no after: needed for success path
7. set run: {entropy:none, clock:virtual} if deterministic
8. add outputs: referencing final task.output
9. ensure no schedule/cron key (NIKA-PARSE-005)

## Compatible patterns
- pattern:scheduled-run
- pattern:fetch-remote-json
- pattern:summarize-text
- pattern:notify-webhook
- pattern:declared-boundary
- pattern:secret-egress

## Compatible blocks
- block:secret-egress-fetch
- block:permits-full-boundary

## Authority and effects
- permits.tools: nika:fetch; permits.net.http exact host; secrets.egress to fetch/notify/outputs if credential used; no human gate required

## Common mistakes
- putting cadence inside .nika (NIKA-PARSE-005)
- using tasks.* outside with: (NIKA-VAR-021)
- missing exact host in permits.net.http
- secret reaching infer without egress sanction (NIKA-SEC-006)
- applying to local write intent (counterexample_intent)

## Diagnostic remedies
- `NIKA-PARSE-005` · remove schedule key; cadence belongs in nika.yaml arm:
- `NIKA-VAR-021` · hoist reference into with: binding
- `NIKA-SEC-006` · add secrets.<name>.egress to the sink

## Validation recipe
- nika check <file.nika> --native-strict
- nika run <file.nika> --model <model> (or --output json)
- compare outputs against expected notification payload

## Positive example intent
Every week fetch the team's activity feed, summarize it and deliver the recap to the #reports Slack channel

## Counterexample intent
Every day fetch the feed and write a local markdown file (use write-artifact pattern instead of notify)
