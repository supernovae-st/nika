---
name: "nika-authoring-parallel-analysis-and-synthesis"
description: "Builds workflows that dispatch one input to several independent analysis tasks (fan-out via shared with: bindings) then fold their outputs into one synthesized answer (fan-in via group: + nika:jq). Co"
nika_skill_role: "authoring"
nika_family: "family:parallel-analysis-and-synthesis"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

# Fan-out parallel analyses then fan-in synthesis

## Scope
Builds workflows that dispatch one input to several independent analysis tasks (fan-out via shared with: bindings) then fold their outputs into one synthesized answer (fan-in via group: + nika:jq). Covers review and summarize goals on a single source.

## When to use
- request asks for multiple independent views, reviews or summaries of the same input
- those views must be combined into one final answer or report
- parallel execution is desired and synthesis is deterministic (jq) or generative

## When not to use
- single linear analysis (use draft-from-facts or linear-review)
- only fan-out with no synthesis step
- synthesis requires agentic loops or external tools beyond group + jq

## Facets
```json
{"goal": ["review", "summarize"], "source": ["inputs-payload", "local-file"], "transformation": ["aggregate", "structured-extraction"], "effect": ["none"], "authority": ["automatic"], "control_flow": ["fan-out", "fan-in"]}
```

## Required business information (human-owned)
- source file or input payload
- list of analysis prompts or task definitions
- synthesis rule or prompt
- output path or report format

## Machine-owned holes
- group name wiring
- with: bindings for fan-out
- ${{ group.<name> }} binding + jq expression for fan-in
- task topology and after: edges

## Required capabilities
- Summarizer
- Aggregator
- StructuredExtractor
- Ranker

## Procedure
1. declare one source task (invoke nika:read or inputs) that produces the shared input
2. create N analysis tasks, each with the same with: { source: "${{ tasks.source.output }}" } and its own infer prompt (fan-out by construction)
3. tag every analysis task with group: reviews
4. add a synthesis task that binds with: { views: "${{ group.reviews }}" } and folds the array of {id, status, output} records with nika:jq or an infer; it needs no after: (the group binding already waits for every member)
5. wire outputs: to expose the final synthesis; grant permits.fs.read and the effect tools the tasks use

## Compatible patterns
- fan-out-analyses
- fan-in-ledger
- draft-from-facts

## Compatible blocks
- block:group-fan-in-ledger
- block:csv-filter-total-report

## Authority and effects
- no permits required beyond read of source and tools used by analyses
- no secrets egress
- human gate only if nika:prompt is explicitly added before synthesis

## Common mistakes
- putting tasks.* inside when: or infer prompt instead of with: (NIKA-VAR-021)
- using for_each for analyses that differ by prompt: for_each repeats one task over items, distinct analyses are distinct tasks in one group
- forgetting to declare group: on every leg (NIKA-DAG-008)
- adding after: to the synthesis task: the group binding already waits for every member, and after: is a map, never a list

## Diagnostic remedies
- `NIKA-VAR-021` · hoist reference into with: binding
- `NIKA-DAG-008` · add group: <name> to each analysis task
- `NIKA-VAR-006` · replace for_each with group + shared with: source
- `NIKA-PARSE-019` · write after: as a map from a task to the outcome it waits for, never a list (the group consumer needs none)

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --var source=<text to review>
- compare outputs.synthesis against expected aggregate of the N analysis outputs

## Positive example intent
Read the PR diff once, run three independent code reviews in parallel, then synthesize a single summary of findings.

## Counterexample intent
Run one review then ask an agent to iterate on it (use agentic-research instead; this skill is for static fan-out/fan-in only)
