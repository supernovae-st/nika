---
name: "nika-authoring-classify-and-route"
description: "Builds workflows that first assign one label from a closed set (an infer with returns, or nika:decide) then run the branch of that label. Each branch task tests the label in its when:; a fallback can "
nika_skill_role: "authoring"
nika_family: "family:classify-and-route"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

# Classify then route

## Scope
Builds workflows that first assign one label from a closed set (an infer with returns, or nika:decide) then run the branch of that label. Each branch task tests the label in its when:; a fallback can wait for a branch's skip through an after: map naming that branch with skipped.

## When to use
- request names a closed label set and per-label actions
- classification must precede routing decisions
- support-triage or qualification-prospects style flows

## When not to use
- pure classification without subsequent routing (use pattern:classify-with-enum alone)
- routing or branching without a prior classification step (use pattern:branch-and-merge)
- open-set or free-text classification
- splitting a collection into multiple label-specific outputs (single classification produces one label so only one branch executes)

## Facets
```json
{"goal": ["classify", "route"], "source": [], "transformation": [], "effect": [], "authority": [], "control_flow": ["branch"]}
```

## Required business information (human-owned)
- closed set of labels
- classification prompt or decision rules
- per-label action definitions

## Machine-owned holes
- when: predicates
- after: topology
- with: binding shape for skipped branches (machine-owned) while label value is human-owned (the closed label set)
- permits.tools for any invoked actions

## Required capabilities
- Classifier
- DeterministicDecider

## Procedure
1. declare inputs for the payload and const for the label set when it is fixed; one payload gives one label, so exactly one branch runs
2. create the classify task: an infer with returns: { object: { label: { enum: [bug, question] } } }, the closed set the request names, or nika:decide when the rules are explicit (pattern:classify-with-enum, block:infer-returns-enum)
3. for each label create a branch task with with: { label: "${{ tasks.classify.output.label }}" } and when: "${{ with.label == 'bug' }}" (pattern:branch-and-merge, block:when-skipped-fallback); a branch that acts on the world after a person's approval puts a nika:prompt gate with no default before its effect

## Compatible patterns
- pattern:classify-with-enum
- pattern:branch-and-merge

## Compatible blocks
- block:infer-returns-enum
- block:when-skipped-fallback

## Authority and effects
- permits.tools for any nika:log/nika:notify/etc used in branches
- human gate via nika:prompt before side-effect tasks when authority=human-first

## Common mistakes
- bare boolean in when: (NIKA-VAR-005)
- tasks.* reference outside with: (NIKA-VAR-021)
- missing permits for branch actions

## Diagnostic remedies
- `NIKA-VAR-005` · write explicit relation e.g. == true
- `NIKA-VAR-021` · hoist reference into with: binding

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --var payload=...; compare outputs.label and which branch executed

## Positive example intent
Classify the ticket as bug or question then route to the corresponding handler

## Counterexample intent
Classify the ticket into any label without routing (no closed set or per-label actions)
