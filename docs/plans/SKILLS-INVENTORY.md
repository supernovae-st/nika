# Nika Skills Inventory — Complete Reference

> **For Claude:** Ce document liste TOUS les skills disponibles dans l'environnement pour Nika v0.9.x

> **Commande rapide:** `/spn-powers:yo` pour voir l'inventaire dans le terminal

---

## Vue d'Ensemble

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  SKILLS INVENTORY — 40+ skills disponibles                                    ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  🔴 OBLIGATOIRES (3)     → Toujours utiliser                                  ║
║  🧪 TDD & Testing (5)    → Tests, coverage, anti-patterns                     ║
║  🔍 Debug & Review (5)   → Debugging, code review                             ║
║  📝 Planning (8)         → Brainstorming, plans, workflow                     ║
║  🦀 Rust (6)             → spn-rust plugin                                    ║
║  ✍️ Documentation (4)    → spn-writing plugin                                  ║
║  🔧 DevOps (3)           → Shell, CI/CD                                       ║
║  📚 Doc Generation (3)   → ADRs, CHANGELOG, OpenAPI                           ║
║  🤖 Agentic (3)          → Agent workflows                                    ║
║  💡 Avancés (5)          → Prompts, frontend, skills                          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## 🔴 1. Skills OBLIGATOIRES

Ces skills DOIVENT être utilisés dans chaque session.

### @using-superpowers
- **Path:** `spn-powers/skills/using-superpowers/SKILL.md`
- **Trigger:** Début de conversation (automatique)
- **Purpose:** Établir les workflows obligatoires
- **Key rule:** "If a skill for your task exists, you must use it"

### @test-driven-development
- **Path:** `spn-powers/skills/test-driven-development/SKILL.md`
- **Trigger:** TOUTE nouvelle feature ou bugfix
- **Purpose:** RED-GREEN-REFACTOR workflow
- **Key rule:** Write failing test FIRST, then minimal code to pass

### @verification-before-completion
- **Path:** `spn-powers/skills/verification-before-completion/SKILL.md`
- **Trigger:** AVANT tout commit ou claim de complétion
- **Purpose:** Vérifier que le travail est vraiment terminé
- **Key rule:** Run verification commands BEFORE claiming success

---

## 🧪 2. Skills TDD & Testing

### @test-driven-development
- **Path:** `spn-powers/skills/test-driven-development/SKILL.md`
- **Usage:** Nouvelle feature, bugfix
- **Workflow:**
  1. Write failing test
  2. Run test to see it fail
  3. Write minimal code to pass
  4. Run test to see it pass
  5. Refactor if needed
  6. Commit

### @testing-anti-patterns
- **Path:** `spn-powers/skills/testing-anti-patterns/SKILL.md`
- **Usage:** Review de tests, éviter les pièges
- **Anti-patterns:**
  - Testing mock behavior instead of real behavior
  - Adding test-only methods to production code
  - Mocking without understanding dependencies

### @testing-skills-with-subagents
- **Path:** `spn-powers/skills/testing-skills-with-subagents/SKILL.md`
- **Usage:** Création/modification de skills
- **Workflow:** Test skill with fresh subagent before deployment

### @condition-based-waiting
- **Path:** `spn-powers/skills/condition-based-waiting/SKILL.md`
- **Usage:** Tests avec race conditions, timing issues
- **Key rule:** Poll for actual state changes, not arbitrary timeouts

### @bats-testing-patterns (shell-scripting)
- **Path:** `shell-scripting/skills/bats-testing-patterns/SKILL.md`
- **Usage:** Tests de scripts shell, CI/CD
- **Framework:** Bash Automated Testing System (Bats)

---

## 🔍 3. Skills Debug & Review

### @systematic-debugging
- **Path:** `spn-powers/skills/systematic-debugging/SKILL.md`
- **Usage:** Bug, erreur, comportement inattendu
- **4 Phases:**
  1. Root cause investigation
  2. Pattern analysis
  3. Hypothesis testing
  4. Implementation

### @root-cause-tracing
- **Path:** `spn-powers/skills/root-cause-tracing/SKILL.md`
- **Usage:** Erreurs profondes dans l'exécution
- **Method:** Trace backward through call stack

### @requesting-code-review
- **Path:** `spn-powers/skills/requesting-code-review/SKILL.md`
- **Usage:** Après implémentation, avant merge
- **Action:** Dispatch code-reviewer subagent

### @receiving-code-review
- **Path:** `spn-powers/skills/receiving-code-review/SKILL.md`
- **Usage:** Recevoir feedback de review
- **Key rule:** Technical rigor, not performative agreement

### @defense-in-depth
- **Path:** `spn-powers/skills/defense-in-depth/SKILL.md`
- **Usage:** Validation multi-couche, sécurité
- **Principle:** Validate at every layer data passes through

---

## 📝 4. Skills Planning & Workflow

### @brainstorming
- **Path:** `spn-powers/skills/brainstorming/SKILL.md`
- **Usage:** Nouvelle idée, design
- **Method:** Questions one at a time, refine idea into design
- **Output:** Design document in `docs/plans/`

### @writing-plans
- **Path:** `spn-powers/skills/writing-plans/SKILL.md`
- **Usage:** Avant implémentation
- **Output:** Detailed plan with bite-sized tasks (2-5 min each)
- **Rule:** Assume engineer has zero codebase context

### @executing-plans
- **Path:** `spn-powers/skills/executing-plans/SKILL.md`
- **Usage:** Exécuter un plan task-by-task
- **Method:** Batch execution with review checkpoints

### @subagent-driven-development
- **Path:** `spn-powers/skills/subagent-driven-development/SKILL.md`
- **Usage:** Plusieurs tâches indépendantes
- **Method:** Fresh subagent per task + code review between tasks

### @dispatching-parallel-agents
- **Path:** `spn-powers/skills/dispatching-parallel-agents/SKILL.md`
- **Usage:** 3+ tâches indépendantes
- **Method:** Multiple Claude agents investigating concurrently

### @using-git-worktrees
- **Path:** `spn-powers/skills/using-git-worktrees/SKILL.md`
- **Usage:** Feature work needing isolation
- **Action:** Create isolated git worktree

### @finishing-a-development-branch
- **Path:** `spn-powers/skills/finishing-a-development-branch/SKILL.md`
- **Usage:** Implémentation complète, prêt à merge
- **Options:** Merge, PR, or cleanup

### @file-organizer
- **Path:** `spn-powers/skills/file-organizer/SKILL.md`
- **Usage:** Structure projet, organisation fichiers
- **Methods:** PARA Method, Johnny Decimal

---

## 🦀 5. Skills Rust (spn-rust)

### @rust
- **Path:** `spn-rust/skills/rust/SKILL.md`
- **Usage:** Master skill pour tout code Rust
- **Routes to:** rust-core, rust-async, rust-ai, rust-agentic

### @rust-core
- **Path:** `spn-rust/skills/rust-core/SKILL.md`
- **Usage:** Ownership, borrowing, error handling
- **References:**
  - `serde-patterns.md` — Serialization
  - `thiserror-anyhow.md` — Error types
  - `error-handling.md` — Patterns

### @rust-async
- **Path:** `spn-rust/skills/rust-async/SKILL.md`
- **Usage:** Tokio, async/await, concurrency
- **References:**
  - `tokio-patterns.md` — Tokio patterns
  - `joinset-patterns.md` — JoinSet usage
  - `sync-collections.md` — DashMap, parking_lot
  - `axum-patterns.md` — Web server

### @rust-agentic
- **Path:** `spn-rust/skills/rust-agentic/SKILL.md`
- **Usage:** Multi-agent, DAG workflows, LLM
- **Patterns:** Supervisor-worker, petgraph DAG, RAG pipelines

### @rust-ai
- **Path:** `spn-rust/skills/rust-ai/SKILL.md`
- **Usage:** MCP, ML integration
- **References:**
  - `mcp-protocol.md` — MCP client/server
  - `candle-ml.md` — Candle ML
  - `onnx-runtime.md` — ONNX

### @rust-tauri
- **Path:** `spn-rust/skills/rust-tauri/SKILL.md`
- **Usage:** Desktop apps, TUI
- **References:**
  - `security-examples.md`
  - `threat-model.md`
  - `advanced-patterns.md`

---

## ✍️ 6. Skills Documentation (spn-writing)

### @writing
- **Path:** `spn-writing/skills/writing/SKILL.md`
- **Usage:** Master skill pour toute documentation
- **Combines:** Markdown + Mermaid seamlessly
- **Auto-creates:** Diagrams where appropriate

### @markdown
- **Path:** `spn-writing/skills/markdown/SKILL.md`
- **Usage:** README, docs, tutorials
- **References:**
  - `markdown-syntax.md` — CommonMark
  - `gfm-extensions.md` — GitHub Flavored
- **Templates:** readme.md, tutorial.md, documentation.md

### @mermaid
- **Path:** `spn-writing/skills/mermaid/SKILL.md`
- **Usage:** Diagrammes (22 types)
- **Types:** flowchart, sequence, ER, class, state, gantt, gitGraph, etc.
- **Theme:** Tailwind-Solarized colors

### @color-system
- **Path:** `spn-writing/skills/color-system/SKILL.md`
- **Usage:** Couleurs cohérentes
- **Palette:** Tailwind-Solarized (VS Code + GitHub Dark compatible)

---

## 🔧 7. Skills DevOps & Shell

### @bash-defensive-patterns
- **Path:** `shell-scripting/skills/bash-defensive-patterns/SKILL.md`
- **Usage:** Scripts shell robustes, CI/CD
- **Patterns:** set -euo pipefail, trap, error handling

### @shellcheck-configuration
- **Path:** `shell-scripting/skills/shellcheck-configuration/SKILL.md`
- **Usage:** Lint shell scripts
- **Tool:** ShellCheck static analysis

### @git-advanced-workflows
- **Path:** `developer-essentials/skills/git-advanced-workflows/SKILL.md`
- **Usage:** Git avancé, rebase, cherry-pick

---

## 📚 8. Skills Documentation Generation

### @architecture-decision-records
- **Path:** `documentation-generation/skills/architecture-decision-records/SKILL.md`
- **Usage:** Documenter décisions architecture
- **Format:** ADR template

### @changelog-automation
- **Path:** `documentation-generation/skills/changelog-automation/SKILL.md`
- **Usage:** Générer CHANGELOG automatiquement
- **Format:** Keep a Changelog

### @openapi-spec-generation
- **Path:** `documentation-generation/skills/openapi-spec-generation/SKILL.md`
- **Usage:** Documentation API
- **Format:** OpenAPI 3.1

---

## 🤖 9. Skills Agentic (spn-agentic)

### @agentic-engine
- **Path:** `spn-agentic/skills/agentic-engine/SKILL.md`
- **Usage:** Moteur d'exécution agentic

### @agentic-generate
- **Path:** `spn-agentic/skills/agentic-generate/SKILL.md`
- **Usage:** Génération de contenu agentic
- **Templates:** design-document.md, subagent-specialist.md

### @agentic-evaluate
- **Path:** `spn-agentic/skills/agentic-evaluate/SKILL.md`
- **Usage:** Évaluation de workflows agents

---

## 💡 10. Skills Avancés

### @prompt-engineering-patterns
- **Path:** `spn-powers/skills/prompt-engineering-patterns/SKILL.md`
- **Usage:** Optimiser prompts LLM
- **References:**
  - `chain-of-thought.md`
  - `few-shot-learning.md`
  - `system-prompts.md`
  - `prompt-templates.md`
  - `prompt-optimization.md`

### @claude-code-docs
- **Path:** `spn-powers/skills/claude-code-docs/SKILL.md`
- **Usage:** Questions Claude Code
- **Coverage:** 270 docs officielles

### @frontend-design
- **Path:** `spn-powers/skills/frontend-design/SKILL.md`
- **Usage:** Design UI production
- **Avoids:** Generic AI aesthetics

### @sharing-skills
- **Path:** `spn-powers/skills/sharing-skills/SKILL.md`
- **Usage:** Contribuer skills upstream
- **Method:** PR to upstream repository

### @writing-skills
- **Path:** `spn-powers/skills/writing-skills/SKILL.md`
- **Usage:** Créer nouveaux skills
- **Method:** TDD for process documentation

---

## Agents Disponibles

### spn-rust Agents

| Agent | Quand | Usage |
|-------|-------|-------|
| `rust-pro` | Code Rust général | Implémentation |
| `rust-perf` | Performance | Optimisation, profiling |
| `rust-async-expert` | Async/Tokio | Concurrency patterns |
| `rust-security` | Sécurité | Audit, vulnérabilités |
| `rust-architect` | Architecture | Design système |
| `rust-ml` | ML/AI | Intégration ML |

### feature-dev Agents

| Agent | Quand | Usage |
|-------|-------|-------|
| `code-reviewer` | Après code | Review qualité |
| `code-architect` | Architecture | Design features |
| `code-explorer` | Exploration | Comprendre codebase |

### Nika Agents

| Agent | Quand | Usage |
|-------|-------|-------|
| `nika-sync` | Alignement | Spec/code/docs sync |
| `nika-deep-verify` | Fin de version | 6 agents vérification |

---

## Commands Rapides

```bash
# Inventaire complet
/spn-powers:yo

# Skills Rust
/spn-rust:rust

# Skills Documentation
/spn-writing:writing

# Vérification Nika
/nika-deep-verify
/nika-sync

# Git workflow
/spn-powers:git:commit
/spn-powers:git:push
```

---

## Quand Utiliser Quel Skill

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  DECISION TREE — Quel skill utiliser ?                                        ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Nouvelle idée ?                                                              ║
║  └─→ @brainstorming → @writing-plans                                          ║
║                                                                               ║
║  Implémenter feature ?                                                        ║
║  └─→ @test-driven-development + @rust-core / @rust-async                      ║
║                                                                               ║
║  Bug ou erreur ?                                                              ║
║  └─→ @systematic-debugging → @root-cause-tracing                              ║
║                                                                               ║
║  Prêt à commit ?                                                              ║
║  └─→ @verification-before-completion (OBLIGATOIRE)                            ║
║                                                                               ║
║  Après feature ?                                                              ║
║  └─→ @requesting-code-review                                                  ║
║                                                                               ║
║  Documentation ?                                                              ║
║  └─→ @writing / @markdown / @mermaid                                          ║
║                                                                               ║
║  Release ?                                                                    ║
║  └─→ @changelog-automation + /nika-deep-verify                                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Mise à Jour

| Date | Version | Changes |
|------|---------|---------|
| 2026-02-25 | v1.0 | Initial inventory (40+ skills) |

---

**Ce document est la référence complète des skills disponibles pour Nika.**
