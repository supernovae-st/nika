---
name: "nika-authoring-human-approved-external-effect"
description: "Requests that prepare an outbound effect (send-message or http-mutation), insert a nika:prompt gate, and execute the effect only on explicit human approval. Matches authority human-first + control_flo"
nika_skill_role: "authoring"
nika_family: "family:human-approved-external-effect"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

# Human-gated outbound effect

## Scope
Requests that prepare an outbound effect (send-message or http-mutation), insert a nika:prompt gate, and execute the effect only on explicit human approval. Matches authority human-first + control_flow gated + effect send-message/http-mutation.

## When to use
- the request names an outbound effect that must be confirmed before execution
- human approval is required before any send-message or http-mutation
- the effect must be prepared first, then gated, then conditionally executed

## When not to use
- no human gate is required (use pattern:draft-from-facts or pattern:post-json directly)
- the effect is a local write (use pattern:write-artifact)
- approval is not mentioned or the request is fully automatic

## Facets
```json
{"goal": ["notify", "publish", "sync"], "source": [], "transformation": [], "effect": ["send-message", "http-mutation"], "authority": ["human-first"], "control_flow": ["gated"]}
```

## Required business information (human-owned)
- endpoint or channel target
- message or payload content
- approval prompt text
- who answers the gate: an unattended run pauses until someone answers --answer <gate>=true or false

## Machine-owned holes
- gate task id and nika:prompt invocation
- with: approved binding and when: condition
- permits.net.http or permits.tools entries
- secrets.egress rules if any credential is used

## Required capabilities
- HumanApprovalGate
- Notifier
- ExternalMutator

## Procedure
1. declare const for the target endpoint or channel and any fixed payload parts
2. add permits.tools containing the effect tool (nika:fetch or nika:notify)
3. add permits.net.http with the exact host when the effect is nika:fetch
4. create the preparation task (infer, invoke or exec) that builds the payload
5. insert the gate task: nika:prompt in confirm mode with a message showing the payload (bound through with:) and no default, since a default answers by itself when nobody is there
6. create the effect task with with: { approved: "${{ tasks.review.output }}", payload: "${{ tasks.draft.output }}" } and when: "${{ with.approved == true }}"
7. bind the effect args from with: and const:; add secrets.egress if a secret reaches the sink
8. wire outputs to expose the gate decision and the effect result

## Compatible patterns
- pattern:human-approved-effect
- pattern:post-json
- pattern:notify-webhook
- pattern:secret-egress

## Compatible blocks
- block:prompt-gate-then-post

## Authority and effects
- permits must list the exact effect tool and net host
- human gate (nika:prompt) MUST sit immediately before the effect task
- no secret may reach the effect without an explicit egress sanction

## Common mistakes
- placing the effect before the gate, or giving the gate a default (it answers by itself and the person is never asked)
- omitting the when: condition on the effect task
- using a bare boolean input in when: (n56-when-bare-bool)
- forgetting permits.net.http for the target host

## Diagnostic remedies
- `NIKA-AUTH-006` · add the required permits block and list the exact tools/hosts
- `NIKA-SEC-006/007` · add secrets.egress for every secret that reaches the effect sink
- `n56-when-bare-bool` · write an explicit relation e.g. == true

## Validation recipe
- nika check <file.nika> --native-strict
- nika run <file.nika> --model mock/echo --output json pauses at the gate (exit 4); resume with --resume <trace> --answer review=false and check the effect was skipped, then with --answer review=true against a test endpoint
- verify the gate task settles before the effect and the effect is skipped when approved=false

## Positive example intent
Draft a recap then ask the human before posting it to the webhook

## Counterexample intent
Write a file to disk (no outbound effect and no human gate required)
