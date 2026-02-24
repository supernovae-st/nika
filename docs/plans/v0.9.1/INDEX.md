# Nika v0.9.1 Plans Index

**Target Version:** v0.9.1
**Codename:** "File-First Agentic Architecture"
**Status:** In Development
**Prerequisite:** v0.9.0 (foundation release)

---

## Core Plans

| File | Description | Lines | Status |
|------|-------------|-------|--------|
| `v091-consolidated-design.md` | Full specification (Schema v0.6, Context, Agents, Skills, Boot) | ~2,100 | Draft |
| `v091-implementation-plan.md` | 9-sprint breakdown with dependencies | ~2,000 | Approved |
| `chat-as-workflow-dag.md` | Chat-as-DAG design (@mentions, //, bindings) | ~370 | Draft |
| `chat-dag-implementation-plan.md` | Implementation details for Chat-as-DAG | ~600 | Draft |

## Supporting Plans

| File | Description | Status |
|------|-------------|--------|
| `memory-and-agents-design.md` | Agent SOUL pattern, memory.yaml, policies.yaml | Draft |
| `nika-project-structure.md` | .nika/ directory structure, new YAML files | Draft |
| `nika-meta-execution-plan.md` | Meta-plan for executing v0.9.1 | Draft |

---

## Key Features Summary

```
v0.9.1 — File-First Agentic Architecture
├── Schema v0.6
│   ├── context: (files + session)
│   ├── agents: (3-modes + SOUL)
│   └── skills: (3-modes + composition)
├── Boot Sequence (6 phases)
│   ├── Identity → Memory → Policies
│   └── Tools → Persona → Ready
├── New YAML Files
│   ├── user.yaml (operator profile)
│   ├── memory.yaml (long-term facts)
│   ├── policies.yaml (guardrails)
│   └── heartbeat.yaml (cron automation)
├── Chat-as-DAG
│   ├── Messages as Tasks
│   ├── @mentions for bindings
│   ├── // for parallel fork
│   └── YAML export
└── Unified Runtime
    └── Workflow + Chat + Heartbeat → FlowGraph → Executor
```

---

## Sprint Overview

| Sprint | Focus | Dependencies |
|--------|-------|--------------|
| 1 | Context System | None |
| 2 | Agent 3-Modes | Sprint 1 |
| 3 | Skill 3-Modes | Sprint 1 |
| 4 | New YAML Files | Sprint 1 |
| 5 | Boot Sequence | Sprints 2, 3, 4 |
| 6 | Chat-as-DAG Core | Sprint 1 |
| 7 | DAG Panel Widget | Sprint 6 |
| 8 | YAML Export | Sprints 6, 7 |
| 9 | Integration + Polish | All |

**Parallelizable:** Sprints 2, 3, 4 can run in parallel. Sprints 6, 7, 8 can overlap.

---

## Metrics

| Metric | Current (v0.9.0) | Target (v0.9.1) |
|--------|------------------|-----------------|
| Tests | 1,902 | 2,200+ |
| LOC | ~25,000 | ~29,000 |
| New code | - | ~4,500 lines |
| TUI Views | 4 | 4 (6 in v0.10) |

---

## Related Documents

- **v0.10+ Plans:** `../v0.10+/` (6-Views, Provider Modal v2)
- **v0.8 Archive:** `../archive-v0.8/` (completed work)
- **ADRs:** `../../tools/nika/.claude/rules/adr/`
