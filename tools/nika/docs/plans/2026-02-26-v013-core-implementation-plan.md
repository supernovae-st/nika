# Nika v0.13 Core Implementation Plan

**Date:** 2026-02-26
**Author:** Claude + Thibaut
**Status:** Draft
**Target:** v0.13.0 (Plan B) + v0.14.0 (Plan C)

---

## Decisions Summary

Toutes les décisions issues du brainstorming :

| Question | Choix | Description |
|----------|-------|-------------|
| Q1: Scope CORE | **C - Full v0.9 Vision** | Memory + Agents + Skills + Heartbeat |
| Q2: Structure .nika/ | **A - Complète** | Tous les fichiers et dossiers |
| Q3: nika init | **C - Interactive** | Prompts avec valeurs par défaut |
| Q4: Agent Format | **A - Full SOUL** | soul, rules, workflow, handoffs |
| Q5: Skill Format | **C - Hybride** | .skill.yaml OU directory/ |
| Q6: Boot Sequence | **A - Complete** | 6 phases avec toutes les validations |

---

## Current State Analysis

### Already Implemented (v0.12.x)

| Feature | Status | Location |
|---------|--------|----------|
| 5 Verbs | Done | src/ast/action.rs |
| 6 Builtins | Done | nika:run, sleep, log, emit, assert, prompt |
| ChatWorkflow | Done | src/runtime/chat_workflow.rs (1014 lines, 65 tests) |
| DAG Execution | Done | src/runtime/executor.rs, src/dag/ |
| MCP Client | Done | src/mcp/ (rmcp v0.16) |
| RigAgentLoop | Done | src/runtime/rig_agent_loop.rs |
| Session Persistence | Done | .nika/sessions/ |
| Config System | Done | .nika/config.toml |

### Missing for Plan B (Schema @0.6)

| Feature | Priority | Files to Create |
|---------|----------|-----------------|
| Memory AST | P0 | src/ast/memory.rs |
| Agent Definition AST | P0 | src/ast/agent_def.rs |
| Skill Definition AST | P0 | src/ast/skill_def.rs |
| User Profile | P1 | src/ast/user.rs |
| Policies | P1 | src/ast/policies.rs |
| Memory Loader | P0 | src/runtime/memory_loader.rs |
| Agent Resolver | P0 | src/runtime/agent_resolver.rs |
| Skill Loader | P0 | src/runtime/skill_loader.rs |
| Boot Sequence | P1 | src/runtime/boot.rs |
| Policy Enforcer | P1 | src/runtime/policy_enforcer.rs |
| nika init Interactive | P1 | src/commands/init.rs (update) |

### Missing for Plan C (Heartbeat)

| Feature | Priority | Files to Create |
|---------|----------|-----------------|
| Heartbeat AST | P0 | src/ast/heartbeat.rs |
| Cron Scheduler | P0 | src/heartbeat/cron.rs |
| Heartbeat Daemon | P0 | src/heartbeat/daemon.rs |
| Trigger System | P1 | src/heartbeat/triggers.rs |
| Heartbeat CLI | P1 | src/commands/heartbeat.rs |

---

## Plan B: Schema @0.6 + Boot + Policies

**Estimated Time:** ~24h
**New Tests:** 132

### Phase B1: AST Types (~4h)

#### B1.1: Memory AST (src/ast/memory.rs)

- MemoryConfig struct (enabled, backend, ttl_secs, max_entries, scopes)
- MemoryBackend enum (File, Sqlite, Redis)
- MemoryScope struct (name, persist, ttl_secs)
- Tests: 12 unit tests

#### B1.2: Agent Definition AST (src/ast/agent_def.rs)

- AgentDef struct (id, schema, soul, rules, workflow, handoffs)
- AgentSoul struct (role, mission, personality, values)
- AgentRules struct (must, never)
- AgentWorkflow struct (mcp, max_turns, depth_limit, stop_conditions)
- AgentHandoff struct (to, when, context)
- Tests: 18 unit tests

#### B1.3: Skill Definition AST (src/ast/skill_def.rs)

- SkillDef struct (id, schema, name, description, params, workflow, output)
- SkillParam struct (name, description, required, default)
- SkillWorkflow enum (Inline, Path)
- SkillOutput struct (format, template)
- OutputFormat enum (Json, Yaml, Markdown, Text)
- Tests: 15 unit tests

#### B1.4: User Profile AST (src/ast/user.rs)

- UserProfile struct (name, email, timezone, language, context)
- Default values: timezone="UTC", language="en-US"
- Tests: 8 unit tests

#### B1.5: Policies AST (src/ast/policies.rs)

- Policies struct (execution, budget, network)
- ExecutionPolicies struct (allow_commands, block_commands, confirm_destructive, max_execution_secs)
- BudgetPolicies struct (daily_token_limit, monthly_cost_limit_cents, warn_at_percent)
- NetworkPolicies struct (allow_domains, block_domains, allow_localhost)
- Tests: 14 unit tests

### Phase B2: Runtime Loaders (~5h)

#### B2.1: Memory Loader (src/runtime/memory_loader.rs)

- MemoryManager struct (config, stores)
- MemoryStore struct (scope, entries)
- MemoryEntry struct (value, created_at, ttl_secs)
- Methods: load(), get(), set(), persist()
- Tests: 16 unit tests

#### B2.2: Agent Resolver (src/runtime/agent_resolver.rs)

- AgentResolver struct (agents HashMap)
- Load from .nika/agents/*.agent.yaml
- Methods: load(), get(), list()
- Tests: 12 unit tests

#### B2.3: Skill Loader (src/runtime/skill_loader.rs)

- SkillLoader struct (skills HashMap)
- LoadedSkill struct (def, source)
- SkillSource enum (File, Directory)
- Discover from:
  - .nika/skills/*.skill.yaml (file format)
  - .nika/skills/<name>/SKILL.yaml (directory format)
- Tests: 14 unit tests

### Phase B3: Boot Sequence (~4h)

#### B3.1: Boot Module (src/runtime/boot.rs)

6-Phase Boot Sequence:
1. Identity - project name, schema version from config.toml
2. User - load user.yaml profile
3. Memory - initialize MemoryManager
4. Policies - load policies.yaml
5. Context - initialize MCP servers (async)
6. Skills - discover agents and skills

- BootContext struct (project_name, schema_version, user, memory, policies, agents, skills)
- BootSequence struct with phase methods
- Tests: 18 unit tests

### Phase B4: Policy Enforcement (~3h)

#### B4.1: Policy Enforcer (src/runtime/policy_enforcer.rs)

- PolicyEnforcer struct (policies)
- Methods: check(), check_shell(), check_fetch(), check_budget()
- Glob pattern matching for command allow/block lists
- URL domain validation for network policies
- Tests: 15 unit tests

### Phase B5: Interactive nika init (~3h)

#### B5.1: Update Init Command (src/commands/init.rs)

Interactive prompts using dialoguer:
1. Project name (default: directory name)
2. Provider selection (claude, openai, mistral, groq, deepseek, ollama)
3. Model selection (based on provider)
4. User profile setup (optional)
5. Timezone
6. Create example workflows (yes/no)

Create directory structure:
- .nika/config.toml
- .nika/user.yaml (if name provided)
- .nika/policies.yaml (with sensible defaults)
- .nika/memory.yaml
- .nika/agents/
- .nika/skills/
- .nika/context/
- .nika/memory/
- .nika/proposed/
- .nika/sessions/
- .nika/traces/
- .nika/cache/

Tests: 10 unit tests

### Phase B6: Schema @0.6 Integration (~3h)

Update src/ast/workflow.rs:
- Add memory: Option<MemoryConfig>
- Add agents: Option<HashMap<String, AgentRef>>
- Add skills: Option<HashMap<String, SkillRef>>
- AgentRef enum (Id, Inline)
- SkillRef enum (Id, Inline)

### Phase B7: Update lib.rs Exports (~1h)

Export new modules from lib.rs.

### Phase B8: Tests (~4h)

Total Plan B tests: 132

---

## Plan C: Heartbeat System

**Estimated Time:** ~12h
**New Tests:** 53

### Phase C1: Heartbeat AST (~2h)

#### C1.1: Heartbeat Definition (src/ast/heartbeat.rs)

- HeartbeatConfig struct (enabled, schedules, triggers)
- HeartbeatSchedule struct (name, cron, workflow, params, enabled)
- HeartbeatTrigger struct (name, on, workflow, params)
- TriggerType enum (WorkflowFailed, BudgetThreshold, FileCreated, McpEvent)
- Tests: 12 unit tests

### Phase C2: Cron Scheduler (~3h)

- CronScheduler struct (schedules)
- ScheduledJob struct (schedule, cron, last_run, next_run)
- Methods: new(), due_jobs(), next_run()
- Tests: 14 unit tests

### Phase C3: Heartbeat Daemon (~3h)

- HeartbeatDaemon struct (config, scheduler, watcher, shutdown_tx)
- Methods: start(), run_loop(), run_workflow(), stop()
- Background tokio task for checking schedules and triggers
- Tests: 12 unit tests

### Phase C4: Trigger System (~2h)

- TriggerWatcher struct (triggers, file_watcher, events_rx)
- TriggerEvent struct (trigger, context)
- Methods: new(), fired_triggers(), notify_workflow_failed(), notify_budget_threshold()
- File watching with notify crate
- Tests: 15 unit tests

### Phase C5: CLI Commands (~2h)

Heartbeat subcommands:
- nika heartbeat start
- nika heartbeat stop
- nika heartbeat status
- nika heartbeat list
- nika heartbeat run <name>
- nika heartbeat next [count]

---

## Test Summary

| Plan | New Tests | Cumulative |
|------|-----------|------------|
| Current (v0.12.x) | - | 2,997 |
| Plan B | 132 | 3,129 |
| Plan C | 53 | 3,182 |

### Test Files to Create

- tests/ast/memory_test.rs (12)
- tests/ast/agent_def_test.rs (18)
- tests/ast/skill_def_test.rs (15)
- tests/ast/user_test.rs (8)
- tests/ast/policies_test.rs (14)
- tests/ast/heartbeat_test.rs (12)
- tests/runtime/boot_test.rs (18)
- tests/runtime/memory_loader_test.rs (16)
- tests/runtime/agent_resolver_test.rs (12)
- tests/runtime/skill_loader_test.rs (14)
- tests/runtime/policy_enforcer_test.rs (15)
- tests/heartbeat/cron_test.rs (14)
- tests/heartbeat/daemon_test.rs (12)
- tests/heartbeat/triggers_test.rs (15)

---

## Pipeline Updates

### ARMADA Station Updates

Update Station 3 (Test): min_tests = 3182
Update Station 4 (Coverage): add heartbeat module

### CI/CD Changes

- Add AST tests step
- Add Runtime tests step
- Add Heartbeat tests step
- Update coverage threshold check

---

## Timeline

| Phase | Estimated | Description |
|-------|-----------|-------------|
| B1 | 4h | AST Types |
| B2 | 5h | Runtime Loaders |
| B3 | 4h | Boot Sequence |
| B4 | 3h | Policy Enforcement |
| B5 | 3h | Interactive nika init |
| B6 | 3h | Schema @0.6 Integration |
| B7 | 1h | lib.rs Exports |
| B8 | 4h | Tests |
| **Plan B Total** | **~27h** | |
| C1 | 2h | Heartbeat AST |
| C2 | 3h | Cron Scheduler |
| C3 | 3h | Heartbeat Daemon |
| C4 | 2h | Trigger System |
| C5 | 2h | CLI Commands |
| **Plan C Total** | **~12h** | |
| **Grand Total** | **~39h** | |

---

## Success Criteria

### v0.13.0 (Plan B Complete)

- All AST types implemented with 100% test coverage
- Boot sequence runs all 6 phases
- nika init --interactive works correctly
- Policy enforcement blocks disallowed actions
- Memory persistence works across sessions
- Agents discoverable from .nika/agents/
- Skills discoverable from .nika/skills/ (file and directory)
- 3,129 tests passing
- Zero clippy warnings
- ARMADA CI passes all 10 stations

### v0.14.0 (Plan C Complete)

- Heartbeat daemon starts and stops correctly
- Cron schedules run workflows on time
- Triggers fire on workflow_failed, budget_threshold, file_created
- nika heartbeat CLI commands work
- 3,182 tests passing
- Documentation updated

---

## .nika/ Directory Structure (Final)

```
.nika/
├── config.toml           # Main configuration
├── user.yaml             # User profile
├── memory.yaml           # Memory configuration
├── policies.yaml         # Security policies
├── heartbeat.yaml        # Cron schedules & triggers
├── agents/               # Agent definitions
│   ├── researcher.agent.yaml
│   └── writer.agent.yaml
├── skills/               # Skill definitions
│   ├── summarize.skill.yaml     # File format
│   └── generate-page/           # Directory format
│       ├── SKILL.yaml
│       └── templates/
│           └── page.md.hbs
├── context/              # Shared context files
│   └── project-context.md
├── memory/               # Persistent memory storage
│   └── conversation.json
├── proposed/             # Agent-proposed changes
│   └── 2026-02-26-feature.diff
├── sessions/             # TUI session state
│   └── <session-id>.json
├── traces/               # Execution traces
│   └── <trace-id>.ndjson
└── cache/                # Temporary cache
    └── mcp-schemas.json
```

---

## References

- v0.9.1 Consolidated Design
- Chat as Workflow DAG
- Memory & Agents Design
- v0.13 Chat-Workflow Unified Plan

---

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>
