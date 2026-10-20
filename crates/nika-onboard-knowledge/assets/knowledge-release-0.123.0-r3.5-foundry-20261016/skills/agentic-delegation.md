---
name: "nika-authoring-agentic-delegation"
description: "Solves requests where steps cannot be enumerated in advance by delegating to a single bounded agent task using the agent verb with explicit tools and skills. Handles ambiguous-outcome uncertainty via "
nika_skill_role: "authoring"
nika_family: "family:agentic-delegation"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

# Delegate open-ended job to governed agent loop

## Scope
Solves requests where steps cannot be enumerated in advance by delegating to a single bounded agent task using the agent verb with explicit tools and skills. Handles ambiguous-outcome uncertainty via max_turns and max_tokens_total under permits.tools and permits.fs.read constraints.

## When to use
- steps cannot be enumerated ahead of time
- need bounded agent with explicit tools/skills
- control_flow is agentic and uncertainty is ambiguous-outcome

## When not to use
- steps are enumerable (use linear or per-item patterns instead)
- pure computation or fixed pipeline (use infer/exec/invoke)
- requires fan-in or recovery (use group or on_error patterns)

## Facets
```json
{"goal": ["research", "review", "extract"], "source": ["local-file", "local-folder", "inputs-payload"], "transformation": ["none"], "effect": ["none"], "authority": ["automatic"], "control_flow": ["agentic"]}
```

## Required business information (human-owned)
- agent prompt and system text
- explicit list of tool ids and SKILL.md paths
- max_turns and max_tokens_total bounds
- schema for final output when structured

## Machine-owned holes
- tools globs subset of permits.tools
- skills paths validated inside permits.fs.read
- permit checks for every tool used

## Required capabilities
- AgenticResearch
- SelfChecker
- HumanApprovalGate

## Procedure
1. declare nika: kebab-id and model at envelope level
2. declare permits.tools with exact tool ids and permits.fs.read for skills and data paths
3. create single task using agent verb with prompt, system, tools (literal list), skills (literal SKILL.md paths), max_turns, max_tokens_total, temperature, schema
4. declare outputs referencing tasks.<id>.output
5. run nika check --native-strict then isolated mock/echo run

## Compatible patterns
- pattern:agent-bounded-research

## Compatible blocks
- (none)

## Authority and effects
- permits.tools lists every tool id used by agent
- permits.fs.read lists every SKILL.md and data path
- no secrets egress unless explicitly sanctioned to agent
- an agent whose tools fetch, beside a file read (its skills or data), is the lethal trifecta (NIKA-SEC-009): every effect its output reaches waits for a nika:prompt gate with no default, bound in with: and tested in when:

## Common mistakes
- agent.tools outside permits.tools (NIKA-SEC-004)
- skills using glob instead of static path (NIKA-AGENT-003)
- reaching max_turns fails the task (NIKA-AGENT-001); the loop ends on a final answer or on nika:done, which needs no permits grant
- tasks.* reference inside agent prompt (NIKA-VAR-021)

## Diagnostic remedies
- `NIKA-AGENT-001` · raise max_turns or tighten the prompt
- `NIKA-AGENT-003` · replace glob with explicit static SKILL.md path inside permits.fs.read
- `NIKA-VAR-021` · hoist reference into with: binding

## Validation recipe
- nika check <file.nika> --native-strict
- nika run <file.nika> --model mock/echo
- compare final output shape against declared schema and verify no tool used outside permits.tools

## Positive example intent
Review the code in ./src and report the top three maintainability issues using only the allowed read and done tools.

## Counterexample intent
Run a fixed three-step pipeline to convert CSV to report (use infer + invoke instead of agent)
