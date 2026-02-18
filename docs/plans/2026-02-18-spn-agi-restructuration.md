# spn-agi Monorepo Restructuration — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reorganize novanet-hq and nika into a unified spn-agi workspace with consistent folder structures, pnpm workspace root, and shared documentation.

**Architecture:** spn-agi/ becomes the parent workspace containing two independent git repos (novanet-dev, nika-dev) with mirrored folder structures. Each has `tools/<name>/src/` with shared `core/` and `tui/` modules. A pnpm workspace at the root enables unified installation.

**Tech Stack:** pnpm workspaces, git, Rust (cargo workspaces), TypeScript

**Prerequisites:** None (this plan must be executed FIRST, before DX Setup and NovaNet Integration plans)

---

## Overview

```
BEFORE:                                 AFTER:
═══════                                 ══════
supernovae-st/                          supernovae-st/spn-agi/
├── novanet-hq/         (repo)          ├── .claude/              ← NEW: relation docs
│   ├── packages/                       ├── package.json          ← NEW: workspace root
│   └── tools/novanet/                  ├── pnpm-workspace.yaml   ← NEW
│                                       │
└── spn-agi/                            ├── novanet-dev/          ← MOVED from novanet-hq
    └── nika/           (repo)          │   ├── .git/             (separate repo)
        └── src/                        │   ├── packages/
                                        │   │   ├── core/
                                        │   │   └── db/
                                        │   └── tools/novanet/
                                        │       └── src/
                                        │           ├── core/     ← shared patterns
                                        │           └── tui/
                                        │
                                        └── nika-dev/             ← RESTRUCTURED from nika
                                            ├── .git/             (separate repo)
                                            ├── packages/
                                            │   └── core/         ← @nika/core (future)
                                            └── tools/nika/
                                                └── src/
                                                    ├── core/     ← mirrored patterns
                                                    └── tui/      ← future TUI
```

---

## Phase 1: Prepare spn-agi Workspace Root

### Task 1.1: Create spn-agi Workspace Structure

**Files:**
- Create: `~/supernovae-st/spn-agi/package.json`
- Create: `~/supernovae-st/spn-agi/pnpm-workspace.yaml`
- Create: `~/supernovae-st/spn-agi/README.md`
- Create: `~/supernovae-st/spn-agi/.gitignore`
- Create: `~/supernovae-st/spn-agi/docs/plans/` (for cross-project plans)
- Create: `~/supernovae-st/spn-agi/docs/research/` (for cross-project research)

**Step 1: Create workspace package.json**

```json
{
  "name": "spn-agi",
  "version": "0.1.0",
  "private": true,
  "description": "SuperNovae AGI - NovaNet + Nika unified workspace",
  "scripts": {
    "dev:novanet": "pnpm --filter @novanet/* dev",
    "dev:nika": "pnpm --filter @nika/* dev",
    "build": "pnpm -r build",
    "test": "pnpm -r test",
    "lint": "pnpm -r lint",
    "clean": "pnpm -r clean"
  },
  "keywords": ["novanet", "nika", "knowledge-graph", "workflow-engine"],
  "author": "SuperNovae Studio",
  "license": "MIT"
}
```

**Step 2: Create pnpm-workspace.yaml**

```yaml
packages:
  # NovaNet packages
  - 'novanet-dev/packages/*'
  - 'novanet-dev/apps/*'
  # Nika packages (future)
  - 'nika-dev/packages/*'
```

**Step 3: Create README.md**

```markdown
# spn-agi

SuperNovae AGI workspace - unified development environment for NovaNet and Nika.

## Projects

| Project | Description | Path |
|---------|-------------|------|
| **NovaNet** | Knowledge graph localization orchestrator | `novanet-dev/` |
| **Nika** | Semantic YAML workflow engine | `nika-dev/` |

## Architecture

```
NovaNet = The Brain (knowledge, context, memory)
Nika    = The Body (execution, actions, workflows)
```

NovaNet provides intelligent context via MCP protocol. Nika executes workflows using that context.

## Installation

```bash
# Install all packages
pnpm install

# Or install individually
cd novanet-dev && pnpm install
cd nika-dev && pnpm install
```

## Rust CLIs

Both projects have Rust CLI tools with matching structure:

```bash
# NovaNet CLI + TUI
cd novanet-dev/tools/novanet && cargo run -- tui

# Nika CLI + TUI
cd nika-dev/tools/nika && cargo run -- tui
```

## Relationship

See `.claude/CLAUDE.md` for detailed architecture documentation.
```

**Step 4: Create .gitignore**

```gitignore
# This is NOT a git repo - just workspace config
# Each sub-project has its own .git/

node_modules/
.turbo/
*.log
```

**Step 5: Verify structure**

Run: `ls -la ~/supernovae-st/spn-agi/`
Expected: package.json, pnpm-workspace.yaml, README.md, .gitignore, nika/ (existing)

**Step 6: Commit note**

No commit needed - spn-agi/ is not a git repo, just a workspace container.

---

### Task 1.2: Create spn-agi Claude Documentation

**Files:**
- Create: `~/supernovae-st/spn-agi/.claude/CLAUDE.md`
- Create: `~/supernovae-st/spn-agi/.claude/rules/architecture.md`

**Step 1: Create .claude directory**

```bash
mkdir -p ~/supernovae-st/spn-agi/.claude/rules
```

**Step 2: Create CLAUDE.md**

```markdown
# spn-agi — Claude Code Context

This workspace contains two complementary AI systems:

## The Brain & Body Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  spn-agi ARCHITECTURE                                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────┐         MCP Protocol        ┌─────────────────┐   │
│  │     NOVANET         │◄──────────────────────────►│      NIKA       │   │
│  │     (Brain)         │                             │     (Body)      │   │
│  ├─────────────────────┤                             ├─────────────────┤   │
│  │ • Knowledge Graph   │    novanet_generate         │ • YAML Workflows│   │
│  │ • Entity Memory     │    novanet_describe         │ • LLM Providers │   │
│  │ • Locale Context    │    novanet_traverse         │ • DAG Execution │   │
│  │ • SEO/GEO Intel     │◄────────────────────────────│ • Tool Calling  │   │
│  │ • ADR Decisions     │                             │ • State Machine │   │
│  └─────────────────────┘                             └─────────────────┘   │
│         │                                                     │            │
│         ▼                                                     ▼            │
│  novanet-dev/                                          nika-dev/           │
│  └── tools/novanet/                                    └── tools/nika/     │
│      └── src/                                              └── src/        │
│          ├── core/     ← shared patterns                       ├── core/   │
│          └── tui/      ← terminal UI                           └── tui/    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Projects

### NovaNet (`novanet-dev/`)

Knowledge graph localization orchestrator. Generates native content across 200+ locales.

**Key concepts:**
- **Entity** → Semantic concept (invariant)
- **EntityNative** → Locale-specific content (authored)
- **Page/Block** → Structure (defined) → PageNative/BlockNative (generated)
- **MCP Server** → Exposes knowledge via novanet_* tools

**Tech:** Neo4j, TypeScript, Rust CLI+TUI

### Nika (`nika-dev/`)

Semantic YAML workflow engine. Executes multi-step AI workflows.

**Key concepts:**
- **Workflow** → YAML definition with semantic verbs
- **Verbs** → `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:`
- **DAG** → Dependency resolution and execution
- **MCP Client** → Consumes NovaNet knowledge

**Tech:** Rust, tokio, MCP SDK

## Integration Pattern

```yaml
# Nika workflow using NovaNet context
workflow: generate-page
steps:
  - invoke: novanet_generate
    params:
      entity: "qr-code"
      locale: "fr-FR"
      forms: ["text", "title"]
    use.ctx: entity_context

  - infer: "Generate landing page"
    context: $entity_context
```

## Shared Patterns

Both Rust CLIs follow identical structure:

| Module | Purpose | NovaNet | Nika |
|--------|---------|---------|------|
| `core/config.rs` | Root discovery | ✓ | ✓ |
| `core/error.rs` | Error handling | ✓ | ✓ |
| `core/output.rs` | Format abstraction | ✓ | ✓ |
| `tui/` | Terminal UI | ✓ | ✓ (planned) |

## Commands

```bash
# Workspace-level
pnpm install          # Install all packages
pnpm build            # Build all
pnpm test             # Test all

# Project-level
cd novanet-dev && pnpm dev
cd nika-dev && cargo run
```
```

**Step 3: Create architecture.md rules**

```markdown
# spn-agi Architecture Rules

## Rule 1: Mirrored Structure

NovaNet and Nika MUST have identical folder structure for Rust CLIs:

```
tools/<name>/
├── src/
│   ├── core/           # Shared patterns
│   │   ├── config.rs   # Root discovery
│   │   ├── error.rs    # Error handling
│   │   └── output.rs   # Output formatting
│   ├── tui/            # Terminal UI
│   ├── commands/       # CLI commands
│   └── main.rs         # Entry point
├── Cargo.toml
└── CLAUDE.md
```

## Rule 2: MCP Integration

Nika connects to NovaNet via MCP protocol ONLY. No direct Neo4j access from Nika.

```
Nika → MCP Client → NovaNet MCP Server → Neo4j
```

## Rule 3: Independent Repos

novanet-dev/ and nika-dev/ are SEPARATE git repos. They share:
- Folder structure (mirrored)
- Design patterns (core/)
- Documentation style (CLAUDE.md)

They do NOT share:
- Git history
- Cargo workspace
- Direct code imports

## Rule 4: Zero Cypher in Nika

Nika workflows NEVER use raw Cypher. Use semantic MCP tools:

```yaml
# WRONG
- exec: "MATCH (e:Entity) RETURN e"

# RIGHT
- invoke: novanet_traverse
  params:
    start: "entity:qr-code"
    arc: "HAS_NATIVE"
```
```

**Step 4: Verify**

Run: `ls -la ~/supernovae-st/spn-agi/.claude/`
Expected: CLAUDE.md, rules/

---

## Phase 2: Move NovaNet

### Task 2.1: Move novanet-hq to spn-agi/novanet-dev

**Files:**
- Move: `~/supernovae-st/novanet-hq/` → `~/supernovae-st/spn-agi/novanet-dev/`

**Step 1: Verify no uncommitted changes in novanet-hq**

```bash
cd ~/supernovae-st/novanet-hq && git status
```

Expected: Clean working tree (or commit pending changes first)

**Step 2: Move the directory**

```bash
mv ~/supernovae-st/novanet-hq ~/supernovae-st/spn-agi/novanet-dev
```

**Step 3: Verify git history preserved**

```bash
cd ~/supernovae-st/spn-agi/novanet-dev && git log --oneline -5
```

Expected: Recent commits visible (git history intact)

**Step 4: Update any absolute paths in configs**

Check these files for hardcoded paths:
- `.claude/settings.json` (if exists)
- `.env` files
- Any symlinks

**Step 5: Test that novanet still works**

```bash
cd ~/supernovae-st/spn-agi/novanet-dev
pnpm install
cargo build --manifest-path tools/novanet/Cargo.toml
cargo test --manifest-path tools/novanet/Cargo.toml
```

Expected: Build succeeds, tests pass

**Step 6: Commit in novanet-dev noting the move**

```bash
cd ~/supernovae-st/spn-agi/novanet-dev
git add -A
git commit -m "chore: move to spn-agi/novanet-dev

Part of spn-agi workspace unification.
Previous path: ~/supernovae-st/novanet-hq

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Phase 3: Restructure Nika

### Task 3.1: Create nika-dev Structure

**Files:**
- Create: `~/supernovae-st/spn-agi/nika-dev/`
- Create: `~/supernovae-st/spn-agi/nika-dev/tools/`
- Move: `~/supernovae-st/spn-agi/nika/*` → `~/supernovae-st/spn-agi/nika-dev/tools/nika/`

**Step 1: Create nika-dev directory structure**

```bash
mkdir -p ~/supernovae-st/spn-agi/nika-dev/tools
mkdir -p ~/supernovae-st/spn-agi/nika-dev/packages/core/src
mkdir -p ~/supernovae-st/spn-agi/nika-dev/docs
```

**Step 2: Move nika content to tools/nika (preserving git)**

```bash
cd ~/supernovae-st/spn-agi/nika

# Move docs to nika-dev root level
git mv docs ../nika-dev/docs 2>/dev/null || mv docs ../nika-dev/

# Move everything else to tools/nika
mkdir -p ../nika-dev/tools/nika
git mv .claude ../nika-dev/tools/nika/ 2>/dev/null || mv .claude ../nika-dev/tools/nika/
git mv src ../nika-dev/tools/nika/
git mv tests ../nika-dev/tools/nika/ 2>/dev/null || true
git mv examples ../nika-dev/tools/nika/
git mv spec ../nika-dev/tools/nika/
git mv Cargo.toml ../nika-dev/tools/nika/
git mv Cargo.lock ../nika-dev/tools/nika/
git mv README.md ../nika-dev/tools/nika/
git mv LICENSE ../nika-dev/tools/nika/
git mv CHANGELOG.md ../nika-dev/tools/nika/
git mv .gitignore ../nika-dev/tools/nika/
git mv .env* ../nika-dev/tools/nika/ 2>/dev/null || true
git mv .github ../nika-dev/ 2>/dev/null || true

# Move .git to nika-dev root
mv .git ../nika-dev/
```

**Step 3: Remove old nika directory**

```bash
cd ~/supernovae-st/spn-agi
rm -rf nika
```

**Step 4: Verify structure**

```bash
ls -la ~/supernovae-st/spn-agi/nika-dev/
ls -la ~/supernovae-st/spn-agi/nika-dev/tools/nika/
```

Expected:
```
nika-dev/
├── .git/
├── .github/
├── docs/
├── packages/
│   └── core/
└── tools/
    └── nika/
        ├── .claude/
        ├── src/
        ├── examples/
        ├── Cargo.toml
        └── ...
```

**Step 5: Test that nika still works**

```bash
cd ~/supernovae-st/spn-agi/nika-dev/tools/nika
cargo build
cargo test
```

Expected: Build succeeds, tests pass

**Step 6: Commit the restructure**

```bash
cd ~/supernovae-st/spn-agi/nika-dev
git add -A
git commit -m "refactor: restructure to nika-dev/tools/nika pattern

Aligns with novanet-dev structure for consistency:
- tools/nika/ contains Rust CLI
- docs/ at project root
- packages/ ready for @nika/core

Part of spn-agi workspace unification.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3.2: Create nika-dev Root Files

**Files:**
- Create: `~/supernovae-st/spn-agi/nika-dev/package.json`
- Create: `~/supernovae-st/spn-agi/nika-dev/README.md`
- Create: `~/supernovae-st/spn-agi/nika-dev/.claude/CLAUDE.md`
- Create: `~/supernovae-st/spn-agi/nika-dev/packages/core/package.json`

**Step 1: Create root package.json**

```json
{
  "name": "nika-dev",
  "version": "0.2.0",
  "private": true,
  "description": "Nika - Semantic YAML workflow engine",
  "workspaces": [
    "packages/*"
  ],
  "scripts": {
    "build": "pnpm -r build",
    "test": "pnpm -r test && cargo test --manifest-path tools/nika/Cargo.toml",
    "lint": "pnpm -r lint",
    "clean": "pnpm -r clean && cargo clean --manifest-path tools/nika/Cargo.toml"
  },
  "keywords": ["nika", "workflow", "yaml", "llm", "ai"],
  "author": "SuperNovae Studio",
  "license": "MIT"
}
```

**Step 2: Create root README.md**

```markdown
# Nika

Semantic YAML workflow engine for AI-powered automation.

## Structure

```
nika-dev/
├── docs/           # Documentation & plans
├── packages/
│   └── core/       # @nika/core - shared types (future)
└── tools/
    └── nika/       # Rust CLI + TUI
        └── src/
```

## Installation

```bash
# Install npm packages
pnpm install

# Build Rust CLI
cd tools/nika && cargo build --release
```

## Usage

```bash
# Run a workflow
nika run workflow.yaml

# Interactive TUI (coming soon)
nika tui
```

## Part of spn-agi

Nika is the "body" that executes workflows. NovaNet is the "brain" that provides context.

See `../README.md` for the unified architecture.
```

**Step 3: Create .claude/CLAUDE.md at nika-dev root**

```markdown
# nika-dev — Claude Code Context

Nika development workspace.

## Structure

```
nika-dev/
├── docs/                    # Plans, ADRs, research
├── packages/
│   └── core/                # @nika/core (future TypeScript types)
└── tools/
    └── nika/                # Rust CLI + TUI
        ├── src/
        │   ├── ast/         # Workflow AST
        │   ├── dag/         # Execution engine
        │   ├── provider/    # LLM clients
        │   ├── runtime/     # State machine
        │   ├── core/        # Shared patterns (planned)
        │   └── tui/         # Terminal UI (planned)
        └── CLAUDE.md        # CLI-specific context
```

## Key Commands

```bash
# Rust CLI
cd tools/nika
cargo build
cargo test
cargo run -- run examples/basic.yaml

# Future TUI
cargo run -- tui
```

## Related

- **NovaNet**: `../novanet-dev/` - Knowledge graph (the brain)
- **spn-agi**: `../` - Unified workspace documentation
```

**Step 4: Create packages/core/package.json placeholder**

```json
{
  "name": "@nika/core",
  "version": "0.1.0",
  "private": true,
  "description": "Nika core types and utilities",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "scripts": {
    "build": "echo 'No build yet'",
    "test": "echo 'No tests yet'"
  }
}
```

**Step 5: Create packages/core/src/index.ts placeholder**

```typescript
// @nika/core - Shared types for Nika workflows
// This package will contain TypeScript types that mirror Rust types

export interface Workflow {
  name: string;
  version?: string;
  steps: Step[];
}

export interface Step {
  id?: string;
  // Verb type determined by which key is present
  infer?: string;
  exec?: string;
  fetch?: string;
  invoke?: string;
  agent?: string;
}

// Placeholder - will be expanded as needed
```

**Step 6: Commit**

```bash
cd ~/supernovae-st/spn-agi/nika-dev
git add -A
git commit -m "chore: add nika-dev root structure

- package.json with pnpm workspaces
- README.md with project overview
- .claude/CLAUDE.md for Claude Code context
- packages/core/ placeholder for @nika/core

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Phase 4: Verify & Document

### Task 4.1: Verify Workspace Installation

**Step 1: Test pnpm install from spn-agi root**

```bash
cd ~/supernovae-st/spn-agi
pnpm install
```

Expected: Installs packages from both novanet-dev and nika-dev

**Step 2: Test individual project installs**

```bash
cd ~/supernovae-st/spn-agi/novanet-dev && pnpm install
cd ~/supernovae-st/spn-agi/nika-dev && pnpm install
```

Expected: Both work independently

**Step 3: Test Rust builds**

```bash
cd ~/supernovae-st/spn-agi/novanet-dev/tools/novanet && cargo build
cd ~/supernovae-st/spn-agi/nika-dev/tools/nika && cargo build
```

Expected: Both compile successfully

---

### Task 4.2: Update Existing Plans

**Files:**
- Modify: `~/supernovae-st/spn-agi/nika-dev/docs/plans/2026-02-18-nika-v02-dx-setup.md`
- Modify: `~/supernovae-st/spn-agi/nika-dev/docs/plans/2026-02-18-nika-v02-novanet-integration.md`

**Step 1: Update file paths in DX Setup plan**

Replace all occurrences of:
- `crates/` → `tools/nika/crates/`
- `src/` → `tools/nika/src/`
- `tests/` → `tools/nika/tests/`
- `Cargo.toml` → `tools/nika/Cargo.toml`

**Step 2: Update file paths in Integration plan**

Same replacements as above.

**Step 3: Add prerequisite note to both plans**

Add to top of each plan:

```markdown
> **PREREQUISITE:** Execute `2026-02-18-spn-agi-restructuration.md` first!
```

**Step 4: Commit updates**

```bash
cd ~/supernovae-st/spn-agi/nika-dev
git add docs/plans/
git commit -m "docs: update plans with new paths after restructuration

Paths now reflect tools/nika/ structure.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4.3: Move Restructuration Plan to spn-agi/docs/

**Files:**
- Move: `~/supernovae-st/spn-agi/nika-dev/docs/plans/2026-02-18-spn-agi-restructuration.md` → `~/supernovae-st/spn-agi/docs/plans/`

**Step 1: Create spn-agi/docs structure**

```bash
mkdir -p ~/supernovae-st/spn-agi/docs/plans
mkdir -p ~/supernovae-st/spn-agi/docs/research
```

**Step 2: Move this plan (it's cross-project)**

```bash
mv ~/supernovae-st/spn-agi/nika-dev/docs/plans/2026-02-18-spn-agi-restructuration.md \
   ~/supernovae-st/spn-agi/docs/plans/
```

**Step 3: Commit in nika-dev (removing the file)**

```bash
cd ~/supernovae-st/spn-agi/nika-dev
git add -A
git commit -m "docs: move restructuration plan to spn-agi/docs/

Cross-project plans belong at workspace level.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

**Rationale:** The restructuration plan concerns BOTH projects, so it belongs at spn-agi/ level, not inside nika-dev/.

---

## Docs Placement Summary

```
spn-agi/
├── docs/                          ← Cross-project documentation
│   ├── plans/
│   │   └── 2026-02-18-spn-agi-restructuration.md  ← THIS PLAN
│   └── research/
│       └── (future cross-project research)
│
├── novanet-dev/
│   └── docs/                      ← NovaNet-specific
│       ├── plans/                 (30+ existing plans)
│       ├── research/              (15+ existing research)
│       └── archive/
│
└── nika-dev/
    └── docs/                      ← Nika-specific
        ├── plans/
        │   ├── 2026-02-18-nika-v02-dx-setup.md
        │   └── 2026-02-18-nika-v02-novanet-integration.md
        └── research/
            └── 2026-02-18-nika-novanet-integration.md
```

---

## Summary

After completing this plan:

```
~/supernovae-st/spn-agi/
├── .claude/CLAUDE.md           ← Relationship docs
├── docs/                       ← Cross-project documentation
│   ├── plans/                  ← This plan lives here
│   └── research/
├── package.json                ← Workspace root
├── pnpm-workspace.yaml         ← Unified packages
├── README.md
│
├── novanet-dev/                ← Git repo (moved from novanet-hq)
│   ├── .git/
│   ├── docs/                   ← NovaNet-specific docs (30+ plans, 15+ research)
│   ├── packages/
│   │   ├── core/               ← @novanet/core
│   │   └── db/                 ← @novanet/db
│   └── tools/novanet/
│       └── src/
│
└── nika-dev/                   ← Git repo (restructured from nika)
    ├── .git/
    ├── docs/                   ← Nika-specific docs
    │   └── plans/              ← DX Setup, NovaNet Integration plans
    ├── packages/
    │   └── core/               ← @nika/core (placeholder)
    └── tools/nika/
        └── src/
```

**Next steps:**
1. Execute `2026-02-18-nika-v02-dx-setup.md` (creates core/, tui/, hooks, skills)
2. Execute `2026-02-18-nika-v02-novanet-integration.md` (adds invoke:, agent: verbs)
