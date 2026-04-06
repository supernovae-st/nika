# Nika IDE — Session Autonome Prompt

> **Copie ce prompt dans un nouveau terminal Claude Code pour lancer la session.**

---

## Prompt

```
Lis ces 3 documents dans l'ordre :

1. docs/plans/2026-04-06-nika-ide-plan.md — Le plan complet (800 lignes, 8 phases, 27 tasks)
2. docs/plans/2026-04-06-nika-ide-mega-session.md — Le handoff autonome (400 lignes, context, diffs, skills)
3. docs/plans/2026-04-06-nika-ide-roadmap.md — La roadmap structurée (index de tous les documents)

Puis lis les 4 rapports de recherche :
4. docs/research/2026-04-06-vscode-platform-binary-bundling.md
5. docs/research/2026-04-06-vscode-dag-webview-research.md
6. docs/research/2026-04-06-mcp-integration-cursor-windsurf-research.md
7. docs/research/2026-04-06-vscode-extension-tdd-research.md

Exécute le plan phase par phase avec TDD.

SKILLS OBLIGATOIRES :
- superpowers:executing-plans (pour l'exécution)
- superpowers:test-driven-development (pour chaque task)
- superpowers:verification-before-completion (avant de dire "done")
- superpowers:requesting-code-review (après chaque phase)
- spn-rust:rust-core (pour le code Rust)
- spn-rust:rust-async (pour DaemonProvider)

ORDRE D'EXÉCUTION :
Phase 0 → Phase 6 → Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 7

COMMIT CONVENTION :
type(scope): description
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

GUARDRAILS après chaque phase :
- cargo test --workspace --lib (Rust, toujours --lib)
- cd editors/vscode && npm run compile (TypeScript)
- cargo tree -p nika-lsp | grep nika-engine → MUST BE 0 (après Phase 6)

SCAFFOLD DAG WEBVIEW :
Le code webview complet est déjà dans tools/nika-vscode/ :
- esbuild.mjs, src/dagPanel.ts, src/webview/dag.ts, src/webview/dag.css
Copier et adapter dans editors/vscode/ pendant Phase 4.

Commence par Phase 0 (30 min, 8 bug fixes). Go.
```
