---
name: "nika-authoring-parallel-analysis-and-synthesis"
description: "Builds workflows that dispatch one input to several independent analysis tasks (fan-out via shared with: bindings) then fold their outputs into one synthesized answer (fan-in via group: + nika:jq). Co"
nika_skill_role: "authoring"
nika_family: "family:parallel-analysis-and-synthesis"
nika_skill_status: "candidate_revise"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Fan-out parallel analyses then fan-in synthesis

## Scope
Builds workflows that dispatch one input to several independent analysis tasks (fan-out via shared with: bindings) then fold their outputs into one synthesized answer (fan-in via group: + nika:jq). Covers review and summarize goals on a single source.

## When to use
- - request asks for multiple independent views/reviews/summaries of the same input
- - those views must be combined into one final answer or report
- - parallel execution is desired and synthesis is deterministic (jq) or generative

## When not to use
- - single linear analysis (use draft-from-facts or linear-review)
- - only fan-out with no synthesis step
- - synthesis requires agentic loops or external tools beyond group + jq

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
1. 1. declare one source task (invoke nika:read or inputs) that produces the shared input
2. 2. create N analysis tasks, each with identical with: { source: ${{ tasks.source.output }} } and distinct infer prompts (fan-out by construction)
3. 3. tag every analysis task with group: <name>
4. 4. add a synthesis task that binds with: { views: ${{ group.<name> }} } and runs nika:jq or infer to fold the array of {id,status,output} records; the synthesis task must omit after: entirely (group binding creates the fan-in edges automatically)
5. 5. wire outputs: to expose the final synthesis; add permits.fs.read / tools as needed

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
- using for_each instead of group for independent analyses (NIKA-VAR-006)
- forgetting to declare group: on every leg (NIKA-DAG-008)
- writing after: list on synthesis task instead of omitting it

## Diagnostic remedies
- `NIKA-VAR-021` · hoist reference into with: binding
- `NIKA-DAG-008` · add group: <name> to each analysis task
- `NIKA-VAR-006` · replace for_each with group + shared with: source
- `NIKA-PARSE-019` · omit after: on the group consumer task

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --inputs-json '{"source":"..."}'
- compare outputs.synthesis against expected aggregate of the N analysis outputs

## Positive example intent
Read the PR diff once, run three independent code reviews in parallel, then synthesize a single summary of findings.

## Counterexample intent
Run one review then ask an agent to iterate on it (use agentic-research instead; this skill is for static fan-out/fan-in only)
