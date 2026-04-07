# Prompt Instruction — Nika Next Session

> Copier-coller ce bloc ENTIER dans un nouveau Claude Code.
> Dernière mise à jour : 2026-04-07 | HEAD: `a5dfecae1` | Tag: `v0.76.0`

---

## INSTRUCTIONS

```
Tu travailles sur Nika, un workflow engine YAML pour l'IA.
Repo: /Users/thibaut/dev/supernovae/nika (submodule dans supernovae-hq)
Repo PUBLIC sur GitHub: supernovae-st/nika

=== ÉTAT ACTUEL ===

Version: v0.76.0 TAGGED + PUSHED
HEAD: a5dfecae1
Tests: 10,411 GREEN (cargo test --workspace --lib depuis tools/)
LOC: 547K | Crates: 18 | Schema: nika/workflow@0.12
Launch: May 5, 2026 (28 jours)

Tout est PRÊT pour le launch. Rien ne bloque.

=== CE QUI EST SHIPPED (ne pas toucher sauf bug) ===

1. Engine: 5 verbes, 64 transforms, 63 builtins, DAG, for_each, retry, on_error
2. Providers: 18 (9 cloud + 7 OpenAI-compat + native GGUF + mock)
3. Zero Ambiguity Design (v0.74):
   - model: est primary, provider auto-inféré du prefix (claude→anthropic, gpt→openai)
   - Slash syntax: model: groq/llama-3.3-70b (split au 1er /)
   - [endpoints.*] dans nika.toml (project > user priority)
   - base_url: SUPPRIMÉ du YAML (erreur de deprecation claire)
   - native-inference bundled (1 binaire, zero opt-in sauf media-provenance)
4. Scheduling: nika every, nika schedule, cron loop, serve reconciliation, 24h timeline
5. S10 Auth: V6 schema, BLAKE3 TokenStore, moka cache, AuthMode (Legacy+MultiKey),
   CLI (nika serve token add/list/revoke), middleware, WWW-Authenticate
6. LSP: decoupled de nika-engine, ls-types lightweight, 15s compile
7. IDE Extension: 8/8 phases, DAG webview (ELK.js+D3), sidebar, MCP auto-config,
   5-platform VSIX, extension decomposée en 5 modules
8. Distribution: npm, VSIX, Homebrew, Docker, crates.io, GitHub Releases, SLSA
9. Stabilization: 15/15 fixes, 4/4 tests, zero clippy

=== CE QUI RESTE (optionnel, pas bloquant) ===

P1 — Should ship:
  - S11 PostgreSQL store (~600 LOC, ~3h) — multi-server nika serve
    - V6 schema déjà en place, juste switch SQLite→PostgreSQL
    - Blueprint: docs/plans/2026-04-06-s9-s10-handoff.md section S11
  - Auth L2 scope enforcement (~100 LOC) — glob matching sur Principal.scope
  - Auth L3 RBAC (~300 LOC) — admin/viewer roles

P2 — Nice to have:
  - nika check faux warnings sur slash syntax (analyzer ne parse pas le /)
  - Double find_project_root_from dans main.rs
  - runner.rs encore 5000+ LOC (déjà task_dispatch + structured_retry extraits)

P3 — Post-launch:
  - nika memory (Egghead) — 22 mechanisms, design bible 1400 lines
  - nika bench — provider benchmarking
  - nika serve webhooks — HTTP trigger externe
  - nika studio web — browser workflow editor
  - Smart routing — cost/latency provider selection

=== RÈGLES ABSOLUES ===

1. Tests: cargo test --workspace --lib depuis tools/ (ALWAYS --lib, jamais keychain)
2. Commits: 1 fix = 1 commit. Push après chaque commit.
3. Co-author: UNIQUEMENT `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`
   JAMAIS Claude, JAMAIS Anthropic. C'est ABSOLU.
4. License: AGPL-3.0-or-later (tous les crates)
5. Backward compat: ZERO (v0.x, zero users externes)
6. Pre-commit: cargo fmt + clippy doivent passer (hook automatique)
7. Dead code: ZERO tolérance. Supprimer sans demander.
8. Questions: AskUserQuestion avec 2-4 options pour archi/security/multi-composant
9. Public/Private: nika/ = PUBLIC GitHub. Research/strategy/brand → supernovae/docs/ ONLY

=== SKILLS À UTILISER ===

AVANT TOUTE TÂCHE, check les skills disponibles. Mandatory.

Workflow: Question → Research → Skills → Test → Code → Verify → Commit

Skills clés:
  - /spn-powers:brainstorming — AVANT de coder, raffiner l'idée
  - /spn-powers:test-driven-development — TDD: test first, fail, implement, pass
  - /spn-powers:verification-before-completion — AVANT de claim "done"
  - /spn-powers:requesting-code-review — APRÈS un chunk de travail
  - /spn-powers:systematic-debugging — pour les bugs
  - /spn-rust:rust — pour tout code Rust
  - /spn-powers:git:commit — pour commit+push avec Conventional Commits

=== ORGANISATION ===

1. TaskCreate pour chaque tâche (multi-step = todo list)
2. TaskUpdate in_progress AVANT de commencer, completed APRÈS
3. Agents en parallèle quand les tâches sont indépendantes
4. Worktrees (isolation: "worktree") pour les gros refactors
5. Commit + push GRANULAIRE — jamais attendre d'avoir 5 fichiers
6. Mémoire: /Users/thibaut/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/
   → Lire MEMORY.md pour le contexte complet
   → Écrire un memory file après chaque session significative

=== ARCHITECTURE ===

tools/
├── nika/                CLI binary (2K)
├── nika-engine/         Execution engine (135K) — LE cœur
│   ├── src/provider/    18 providers (rig-core + OpenAI-compat + native + mock)
│   ├── src/runtime/     Runner, executor, agent loop, 63 builtins, security
│   ├── src/ast/         Lower (Analyzed → Runtime)
│   ├── src/binding/     64 transforms, templates, JSONPath
│   └── src/display/     CLI renderers (live + classic)
├── nika-core/           AST + catalogs (23K) — pure, zero I/O
├── nika-vault/          Secrets (1.2K) — XChaCha20 + Argon2i
├── nika-daemon/         Daemon (5K) — secrets, jobs, cache, cron
├── nika-init/           Scaffolding (21K) — init wizard + 12-level course
├── nika-event/          EventLog (4K)
├── nika-mcp/            MCP client+server (9K)
├── nika-media/          CAS + processor (13K) — 24 media tools
├── nika-storage/        SQLite V6 (2K) — serve_tokens, schedules, jobs
├── nika-cli/            CLI commands (8K) — verbs, schedule, token, model
├── nika-tui/            TUI (86K) — ratatui
├── nika-serve/          HTTP server (4K) — axum, SSE, auth, rate limit
├── nika-sdk/            SDK (3K)
├── nika-display/        Display crate (4K)
├── nika-lsp-core/       LSP intelligence (9K)
└── nika-lsp/            LSP binary (2.5K)

editors/vscode/          VSCode extension — TypeScript + DAG webview

=== AUTH SYSTEM (S10) ===

nika serve token add --name "prod"
  → nk_<48hex> (BLAKE3 hash → serve_tokens table V6)
  → startup: count_tokens() → AuthMode::MultiKey
  → HTTP: Bearer nk_... → moka cache (60s) → DB → Principal

Files:
  nika-storage/src/lib.rs      — V6, TokenEntry, 7 CRUD
  nika-serve/src/token_store.rs — AuthMode, Principal, TokenStore
  nika-serve/src/auth.rs        — middleware + WWW-Authenticate
  nika-cli/src/token.rs          — CLI add/list/revoke (353 LOC)

=== PROVIDER RESOLUTION ===

1. provider: explicit → WINS
2. model: has / → split (groq/llama → provider=groq, model=llama)
3. model: prefix → auto-infer (claude→anthropic, gpt→openai)
4. nika.toml [provider] default
5. detect_first_configured() → scan API keys

=== DOCS CLÉS ===

- Mega handoff: docs/sprints/SESSION-HANDOFF-2026-04-07.md
- Stabilization: docs/sprints/SESSION-MEGA-FINAL.md
- S10 auth blueprint: docs/plans/2026-04-06-s10-multi-tenant-auth-blueprint.md
- Model/provider plan: supernovae/docs/plans/2026-04-06-model-provider-refactor-plan.md
- IDE roadmap: docs/plans/2026-04-06-nika-ide-roadmap.md
- Egghead design: docs/plans/2026-03-31-egghead-design-bible.md (1400 lines)
- Language bible: docs/language-bible.md

=== COMMANDES ===

# Build & test
cd tools && cargo test --workspace --lib     # 10,411 tests
cargo clippy --workspace -- -D warnings      # zero warnings
cargo fmt --all --check                      # clean

# Run
nika run workflow.nika.yaml                  # execute
nika check workflow.nika.yaml                # validate
nika ui                                      # TUI
nika serve                                   # HTTP API
nika every 6h report.nika.yaml              # schedule

# Release
git tag v0.77.0 && git push --tags          # triggers CI for all channels
```

---

## COMMENT UTILISER CE PROMPT

1. Ouvre un nouveau Claude Code dans `/Users/thibaut/dev/supernovae/nika`
2. Copie-colle le bloc entre les \`\`\` ci-dessus
3. Ajoute ta demande spécifique après

### Exemples de demandes :

**Pour S11 PostgreSQL :**
```
Implémente S11 — remplace SQLite par PostgreSQL pour nika serve.
Blueprint: docs/plans/2026-04-06-s9-s10-handoff.md section S11.
TDD: test first, implement, verify.
```

**Pour un bug fix :**
```
Fix: nika check donne un faux warning quand model: groq/llama-3.3-70b
est utilisé. L'analyzer ne parse pas la slash syntax. Le runtime la parse
correctement (infer.rs parse_model_slash). Ajoute le parsing dans
nika-core/src/ast/analyzer/analyze.rs fonction check_model_name.
```

**Pour une nouvelle feature :**
```
Brainstorm: nika serve webhooks — déclencher un workflow via HTTP POST
depuis un bot Telegram ou un cron externe. Utilise le skill brainstorming
AVANT de coder. Puis TDD.
```

**Pour un audit :**
```
Lance 3 agents en parallèle :
1. Code reviewer sur les 10 derniers commits
2. Security audit sur nika-serve (auth, rate limit, SSRF)
3. Performance audit sur runner.rs (5000+ LOC, hotpath)
Rapport avec HIGH/MEDIUM/LOW + file:line.
```
