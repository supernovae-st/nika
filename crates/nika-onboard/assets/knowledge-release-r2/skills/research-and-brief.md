---
name: "nika-authoring-research-and-brief"
description: "Solves requests that gather information via tools inside explicit budgets (max_turns, max_tokens_total) then produce a sourced brief or draft. Uses agent: for the research phase and infer: or draft-fr"
nika_skill_role: "authoring"
nika_family: "family:research-and-brief"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Bounded agentic research then sourced brief

## Scope
Solves requests that gather information via tools inside explicit budgets (max_turns, max_tokens_total) then produce a sourced brief or draft. Uses agent: for the research phase and infer: or draft-from-facts for the brief, enforcing cite-sources via schema or prompt discipline.

## When to use
- request names research or competitor-radar under a token/turn budget
- output must be a sourced brief or report with citations
- tools are whitelisted and bounded by agent contract

## When not to use
- pure linear CSV/structured processing (use csv-filter-total-report)
- unbounded open research without max_turns or max_tokens_total
- simple fetch-then-write without agent loop

## Facets
```json
{"goal": ["research", "draft"], "source": ["remote-api", "web-page", "structured-records"], "transformation": ["structured-extraction", "text-extraction"], "effect": ["write-artifact"], "authority": ["automatic"], "control_flow": ["agentic"]}
```

## Required business information (human-owned)
- research topic or query
- max_turns or max_tokens_total budget
- output brief path
- tool whitelist or MCP servers
- citation style or schema for brief

## Machine-owned holes
- agent.tools globs inside permits.tools
- agent.skills static paths
- jq expressions for post-processing facts
- permits.fs.read/write globs
- topology of after/with edges

## Required capabilities
- AgenticResearch
- Drafter
- Summarizer
- StructuredExtractor
- ArtifactWriter

## Procedure
1. declare nika: kebab-id and model
2. declare const for topic, budget numbers, output_path
3. declare permits.fs.read for nika:read, permits.net.http for nika:fetch, permits.tools for any MCP servers, permits.fs.read for skills/data
4. create research task using agent: with prompt, system, tools (whitelist), skills, max_turns, max_tokens_total, schema requiring citations (or returns:)
5. bind research output via with: into draft task using infer: or pattern:draft-from-facts with prompt enforcing cite-sources; draft task declares its own returns:/schema:
6. add write_report task using invoke: { tool: nika:write, args: { path: ${{ const.output_path }}, content: ${{ with.draft }} } } and grant permits.fs.write for that path
7. set outputs to the brief or computed facts

## Compatible patterns
- pattern:agent-bounded-research
- pattern:draft-from-facts
- pattern:write-artifact
- pattern:declared-boundary

## Compatible blocks
- block:agent-tools-skills

## Authority and effects
- permits.tools governs effect-carrying tools (mcp: targets, fs/net/exec builtins); nika:done is loop-only and needs no grant
- permits.fs.write for output brief path
- no secrets egress unless explicitly sanctioned to agent/infer
- human gate only if nika:prompt used before external mutation

## Common mistakes
- omitting max_turns or max_tokens_total (agent runs unbounded)
- agent.tools outside permits.tools (NIKA-AUTH-006)
- infer prompt allowing invention instead of cite-sources
- writing shell: under exec allowlist (NIKA-SEC-004)

## Diagnostic remedies
- `NIKA-AGENT-001` · raise max_turns or tighten the prompt
- `NIKA-AUTH-006` · add missing tool to permits.tools and agent.tools
- `NIKA-SEC-006` · add egress sanction for any secret used in agent prompt

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --max-cost-usd 0.10

## Positive example intent
Research the top 3 competitors of AcmeCorp using web search tools under 8 turns and 6000 tokens, then write a sourced brief in ./out/brief.md citing all sources.

## Counterexample intent
Process orders.csv to compute paid totals and write a report (use csv-filter-total-report instead; no agent loop needed)
