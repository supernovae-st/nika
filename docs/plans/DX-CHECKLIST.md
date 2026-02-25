# Nika DX Checklist — Developer Experience Complete Audit

> **For Claude:** Ce document est une checklist OBLIGATOIRE à suivre. Utiliser TodoWrite pour tracker chaque item.

> **Skills requis:** `@test-driven-development`, `@verification-before-completion`, `@systematic-debugging`, `@requesting-code-review`

---

## Vue d'Ensemble

Cette checklist assure que la DX (Developer Experience) de Nika est complète et à jour avant chaque release majeure.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  DX CHECKLIST — À exécuter avant chaque release                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  □ CLAUDE.md files          — Contexte à jour pour Claude Code                ║
║  □ Skills                   — Skills Nika fonctionnels                        ║
║  □ Hooks                    — Pre-commit, format, lint                        ║
║  □ Scripts                  — npm/cargo scripts documentés                    ║
║  □ Tests                    — Coverage 80%+, TDD appliqué                     ║
║  □ CI/CD                    — GitHub Actions fonctionnel                      ║
║  □ Documentation            — README, CHANGELOG, ADRs à jour                  ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## 1. CLAUDE.md Files

### Checklist

| File | Status | Last Updated | Owner |
|------|--------|--------------|-------|
| `/CLAUDE.md` (root supernovae-agi) | □ | | |
| `/.claude/CLAUDE.md` | □ | | |
| `/nika/CLAUDE.md` | □ | | |
| `/nika/tools/nika/CLAUDE.md` | □ | | |

### Vérifications

```bash
# Vérifier que tous les CLAUDE.md existent
ls -la CLAUDE.md .claude/CLAUDE.md nika/CLAUDE.md nika/tools/nika/CLAUDE.md
```

- [ ] **Version actuelle** — Le CLAUDE.md reflète la version actuelle (v0.8.0 → v0.9.x)
- [ ] **Test counts** — Les nombres de tests sont à jour (1,902 → nouveau total)
- [ ] **Commands** — Toutes les commandes listées fonctionnent
- [ ] **File paths** — Les chemins de fichiers sont valides
- [ ] **Features** — Les nouvelles features sont documentées
- [ ] **ADR references** — Les ADRs cités existent
- [ ] **No broken links** — Aucun lien cassé

### Template de mise à jour

```markdown
## v0.X.X Changes

### New Features
- Feature 1: Description
- Feature 2: Description

### Breaking Changes
- Change 1: Migration path

### Test Counts
| Module | Tests |
|--------|-------|
| Total | X,XXX |
```

---

## 2. Skills

### Skills Nika existants

| Skill | Path | Status | Tests |
|-------|------|--------|-------|
| `/nika-run` | `.claude/skills/nika-run.md` | □ | □ |
| `/nika-debug` | `.claude/skills/nika-debug.md` | □ | □ |
| `/nika-spec` | `.claude/skills/nika-spec.md` | □ | □ |
| `/nika-yaml` | `.claude/skills/nika-yaml.md` | □ | □ |
| `/nika-binding` | `.claude/skills/nika-binding.md` | □ | □ |
| `/nika-arch` | `.claude/skills/nika-arch.md` | □ | □ |
| `/nika-diagnose` | `.claude/skills/nika-diagnose.md` | □ | □ |
| `/workflow-validate` | `.claude/skills/workflow-validate.md` | □ | □ |
| `/nika-sync` | `.claude/skills/nika-sync.md` | □ | □ |
| `/nika-deep-verify` | `.claude/skills/nika-deep-verify.md` | □ | □ |

### Vérifications par skill

Pour chaque skill:

- [ ] **Skill existe** — Le fichier `.md` existe
- [ ] **Syntax valide** — Frontmatter YAML correct
- [ ] **Description claire** — But du skill documenté
- [ ] **Étapes complètes** — Workflow step-by-step
- [ ] **Exemples** — Au moins un exemple d'utilisation
- [ ] **Error handling** — Gestion des erreurs documentée
- [ ] **Testé manuellement** — Exécution réussie

### Skills manquants à créer

| Skill | Purpose | Priority |
|-------|---------|----------|
| `/nika-tdd` | TDD workflow pour Nika | HIGH |
| `/nika-review` | Code review checklist | HIGH |
| `/nika-perf` | Performance audit | MEDIUM |
| `/nika-release` | Release checklist | HIGH |
| `/nika-migration` | Version migration guide | MEDIUM |

### Template de skill

```yaml
---
name: nika-xxx
description: Description courte
---

# Nika XXX

## Overview
Ce skill fait X.

## When to Use
- Situation 1
- Situation 2

## Steps

### Step 1: Name
Description

### Step 2: Name
Description

## Examples

### Example 1
```bash
# Command
```

## Troubleshooting
- Error 1: Solution
```

---

## 3. Hooks

### Hooks existants

| Hook | Trigger | Status | Tests |
|------|---------|--------|-------|
| Pre-commit | `git commit` | □ | □ |
| Pre-push | `git push` | □ | □ |
| Format | On save | □ | □ |
| Lint | On save | □ | □ |

### Configuration

```bash
# Vérifier .claude/settings.json
cat .claude/settings.json | jq '.hooks'
```

### Hooks recommandés

```json
{
  "hooks": {
    "preCommit": [
      "cargo fmt --check",
      "cargo clippy -- -D warnings",
      "cargo test --lib"
    ],
    "prePush": [
      "cargo test",
      "cargo doc --no-deps"
    ],
    "onSave": [
      "cargo fmt"
    ]
  }
}
```

### Vérifications

- [ ] **Pre-commit fonctionne** — `git commit` lance les checks
- [ ] **Format on save** — Fichiers formatés automatiquement
- [ ] **Clippy warnings** — Zéro warning
- [ ] **Tests passent** — Avant chaque commit

---

## 4. Scripts

### Cargo scripts

| Script | Command | Status |
|--------|---------|--------|
| Test all | `cargo test` | □ |
| Test lib | `cargo test --lib` | □ |
| Test integration | `cargo test --test '*'` | □ |
| Clippy | `cargo clippy -- -D warnings` | □ |
| Format | `cargo fmt` | □ |
| Doc | `cargo doc --no-deps --open` | □ |
| Bench | `cargo bench` | □ |
| Coverage | `cargo llvm-cov` | □ |

### Makefile / Justfile

```makefile
# Makefile recommandé

.PHONY: test lint fmt doc bench coverage release

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

check: fmt lint test
	@echo "All checks passed!"

doc:
	cargo doc --no-deps --open

bench:
	cargo bench

coverage:
	cargo llvm-cov --html
	open target/llvm-cov/html/index.html

release:
	@echo "Running release checklist..."
	$(MAKE) check
	cargo build --release
	@echo "Release build complete!"
```

### Vérifications

- [ ] **Tous les scripts fonctionnent** — Exécution sans erreur
- [ ] **Documentation** — Scripts documentés dans README
- [ ] **CI utilise les mêmes scripts** — Cohérence local/CI

---

## 5. Tests

### Test Coverage

```bash
# Générer rapport de coverage
cargo llvm-cov --html
open target/llvm-cov/html/index.html
```

| Module | Target | Current | Status |
|--------|--------|---------|--------|
| `ast/` | 90% | □ | |
| `dag/` | 85% | □ | |
| `runtime/` | 80% | □ | |
| `mcp/` | 75% | □ | |
| `binding/` | 85% | □ | |
| `tui/` | 70% | □ | |
| `event/` | 80% | □ | |
| **TOTAL** | **80%** | □ | |

### Types de tests

| Type | Count | Status |
|------|-------|--------|
| Unit tests | □ | |
| Integration tests | □ | |
| Property tests (proptest) | □ | |
| Snapshot tests (insta) | □ | |
| Benchmark tests (criterion) | □ | |

### TDD Checklist

Pour chaque nouvelle feature:

- [ ] **Write failing test FIRST** — Test écrit avant le code
- [ ] **Run test to see it fail** — Vérifier le message d'erreur
- [ ] **Write minimal code** — Juste assez pour passer
- [ ] **Run test to see it pass** — Vérifier le succès
- [ ] **Refactor if needed** — Améliorer sans casser
- [ ] **Commit** — Atomic commit

### Test Naming Convention

```rust
#[test]
fn test_<function>_<scenario>_<expected_outcome>() {
    // arrange
    // act
    // assert
}

// Examples:
fn test_parse_workflow_valid_yaml_returns_workflow()
fn test_parse_workflow_missing_schema_returns_error()
fn test_execute_task_infer_calls_provider()
```

### Vérifications

- [ ] **Tests passent localement** — `cargo test`
- [ ] **Tests passent en CI** — GitHub Actions vert
- [ ] **Coverage ≥ 80%** — Pas de régression
- [ ] **Pas de tests flaky** — Stable sur 10 runs
- [ ] **Tests documentés** — Docstrings sur tests complexes

---

## 6. CI/CD GitHub

### GitHub Actions

| Workflow | File | Triggers | Status |
|----------|------|----------|--------|
| CI | `.github/workflows/ci.yml` | push, PR | □ |
| Release | `.github/workflows/release.yml` | tag | □ |
| Docs | `.github/workflows/docs.yml` | push main | □ |

### CI Workflow recommandé

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable
        with:
          components: clippy, rustfmt

      - name: Cache
        uses: Swatinem/rust-cache@v2

      - name: Format check
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy -- -D warnings

      - name: Test
        run: cargo test

      - name: Doc
        run: cargo doc --no-deps

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install llvm-cov
        uses: taiki-e/install-action@cargo-llvm-cov

      - name: Coverage
        run: cargo llvm-cov --lcov --output-path lcov.info

      - name: Upload coverage
        uses: codecov/codecov-action@v4
        with:
          files: lcov.info
```

### Vérifications

- [ ] **CI existe** — `.github/workflows/ci.yml` présent
- [ ] **CI passe** — Badge vert sur main
- [ ] **PR checks** — CI bloque les PR avec erreurs
- [ ] **Coverage reported** — Codecov ou similaire
- [ ] **Release workflow** — Tags déclenchent release

---

## 7. Documentation

### Fichiers à vérifier

| File | Status | Last Check |
|------|--------|------------|
| `README.md` | □ | |
| `CHANGELOG.md` | □ | |
| `CONTRIBUTING.md` | □ | |
| `docs/plans/VISION.md` | □ | |
| `docs/plans/*/README.md` | □ | |
| ADRs | □ | |

### README Checklist

- [ ] **Description** — Ce que fait Nika
- [ ] **Installation** — Comment installer
- [ ] **Quick start** — Premier workflow
- [ ] **Usage** — Commandes principales
- [ ] **Configuration** — Options de config
- [ ] **Contributing** — Comment contribuer
- [ ] **License** — MIT/Apache

### CHANGELOG Checklist

- [ ] **Format** — Keep a Changelog format
- [ ] **Unreleased section** — Pour les changements en cours
- [ ] **Version sections** — Pour chaque release
- [ ] **Categories** — Added, Changed, Deprecated, Removed, Fixed, Security
- [ ] **Links** — Liens vers PRs/issues

### ADR Checklist

| ADR | Title | Status |
|-----|-------|--------|
| ADR-001 | 5 Semantic Verbs | □ Reviewed |
| ADR-002 | YAML-First | □ Reviewed |
| ADR-003 | MCP-Only | □ Reviewed |
| ADR-004 | spawn_agent | □ Reviewed |
| ADR-005 | decompose | □ Reviewed |
| ADR-006 | Lazy Bindings | □ Reviewed |

---

## 8. Vérification Étape par Étape

### Workflow de vérification

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  VERIFICATION WORKFLOW                                                         ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  1. FORMAT CHECK                                                              ║
║     cargo fmt --check                                                         ║
║     └─ Si échec: cargo fmt && recommit                                        ║
║                                                                               ║
║  2. LINT CHECK                                                                ║
║     cargo clippy -- -D warnings                                               ║
║     └─ Si échec: Fix warnings && recommit                                     ║
║                                                                               ║
║  3. TEST CHECK                                                                ║
║     cargo test                                                                ║
║     └─ Si échec: Debug avec @systematic-debugging                             ║
║                                                                               ║
║  4. DOC CHECK                                                                 ║
║     cargo doc --no-deps                                                       ║
║     └─ Si warnings: Fix docstrings                                            ║
║                                                                               ║
║  5. COVERAGE CHECK                                                            ║
║     cargo llvm-cov                                                            ║
║     └─ Si < 80%: Add tests                                                    ║
║                                                                               ║
║  6. INTEGRATION CHECK                                                         ║
║     /nika-deep-verify                                                         ║
║     └─ Si échec: Review avec @requesting-code-review                          ║
║                                                                               ║
║  7. MANUAL CHECK                                                              ║
║     cargo run -- chat                                                         ║
║     └─ Tester les features clés manuellement                                  ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Commandes de vérification rapide

```bash
# Full check (copier-coller)
cargo fmt --check && \
cargo clippy -- -D warnings && \
cargo test && \
cargo doc --no-deps && \
echo "✅ All checks passed!"
```

---

## 9. Skills Claude Code à utiliser

### Skills obligatoires

| Skill | Quand | Usage |
|-------|-------|-------|
| `@test-driven-development` | Nouvelle feature | TDD workflow |
| `@verification-before-completion` | Avant commit | Vérifier le travail |
| `@systematic-debugging` | Bug/erreur | Debug méthodique |
| `@requesting-code-review` | Après feature | Demander review |
| `@brainstorming` | Nouvelle idée | Raffiner le design |
| `@writing-plans` | Avant implémentation | Plan détaillé |

### Agents à utiliser

| Agent | Quand | Usage |
|-------|-------|-------|
| `rust-pro` | Code Rust | Implémentation |
| `rust-async-expert` | Async/Tokio | Patterns async |
| `feature-dev:code-reviewer` | Après code | Review |
| `feature-dev:code-architect` | Architecture | Design |
| `nika-deep-verify` | Fin de version | Vérification complète |

### Commands à utiliser

| Command | Usage |
|---------|-------|
| `/nika-sync` | Vérifier alignement spec/code/docs |
| `/nika-deep-verify` | 6 agents parallèles de vérification |
| `/spn-powers:commit` | Commit avec checks |
| `/spn-powers:push` | Push avec validation |

---

## 10. Checklist Finale Pre-Release

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  PRE-RELEASE CHECKLIST                                                         ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  □ 1. CLAUDE.md files à jour (toutes les versions)                            ║
║  □ 2. Skills testés et fonctionnels                                           ║
║  □ 3. Hooks configurés (pre-commit, format)                                   ║
║  □ 4. Scripts documentés (Makefile/Justfile)                                  ║
║  □ 5. Tests passent (cargo test)                                              ║
║  □ 6. Coverage ≥ 80%                                                          ║
║  □ 7. Clippy clean (0 warnings)                                               ║
║  □ 8. CI/CD GitHub fonctionnel                                                ║
║  □ 9. README à jour                                                           ║
║  □ 10. CHANGELOG à jour                                                       ║
║  □ 11. ADRs reviewed                                                          ║
║  □ 12. Version bump (Cargo.toml)                                              ║
║  □ 13. Tag créé (git tag vX.Y.Z)                                              ║
║  □ 14. Release notes écrites                                                  ║
║  □ 15. /nika-deep-verify passé                                                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Exécution de cette Checklist

### Avec TodoWrite

```
Créer un todo pour CHAQUE section:
1. □ Audit CLAUDE.md files
2. □ Audit Skills
3. □ Audit Hooks
4. □ Audit Scripts
5. □ Audit Tests
6. □ Audit CI/CD
7. □ Audit Documentation
8. □ Run verification workflow
9. □ Pre-release checklist
```

### Timing recommandé

| Phase | Quand | Durée estimée |
|-------|-------|---------------|
| Quick check | Avant chaque commit | 2 min |
| Full check | Avant chaque PR | 15 min |
| DX Audit | Avant chaque release | 2-4 heures |
| Complete audit | Trimestriel | 1 jour |

---

## Historique des Audits

| Date | Version | Auditor | Status | Notes |
|------|---------|---------|--------|-------|
| 2026-02-25 | v0.8.0 | — | Pending | Créer baseline |

---

**Cette checklist est obligatoire avant chaque release majeure.**
