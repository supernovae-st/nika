# Agent Prompt — nika serve V3 Bug Fixes + Hardening

> **Copie-colle ce fichier entier comme premier message a un agent Claude Code.**
> Working directory: `/Users/thibaut/dev/supernovae/nika`

---

## Ta mission

Tu dois fixer **11 bugs** dans `nika-serve` (le serveur HTTP de Nika) et verifier que tout marche de bout en bout. Les bugs sont documentes avec le fichier exact, la ligne, le code actuel, et le code fix.

## Fichiers a lire EN PREMIER

Lis ces 3 fichiers dans cet ordre avant de toucher au code :

1. **`docs/plans/2026-04-01-serve-bug-report.md`** — LE master plan. Contient les 11 bugs avec fichier:ligne et code fix exact. C'est ta source de verite. ~1100 lignes, lis TOUT.

2. **`CLAUDE.md`** — Le language reference de Nika + la structure du projet. Comprends les 5 verbs (infer, exec, fetch, invoke, agent) et le principe `.nika.yaml`.

3. **`docs/plans/2026-04-01-handoff.md`** — Le contexte complet de la session : ce qui a ete fait, ce qui reste, l'etat des VPS, les regles.

## Les 11 bugs a fixer (par ordre de priorite)

### Wave 1 — Ce que l'utilisateur voit (CRITIQUE)

**BUG-1 : Output pollue par le display CLI** (le plus visible)
- **Symptome :** GET /v1/status retourne Performance stats + Timeline + Provider Breakdown au lieu du resultat LLM
- **Fichier :** `tools/nika-serve/src/config.rs`
- **Fix :** Ligne 15-16 : deplacer `#[default]` de `Subprocess` a `Embedded`. Ligne 96 : changer `.unwrap_or("subprocess")` en `.unwrap_or("embedded")`
- **Aussi :** Update les test fixtures dans `tools/nika-serve/src/executor.rs` (~ligne 155) et `tools/nika-serve/src/lib.rs` (~ligne 329) : `ExecutorMode::Embedded`
- **Pourquoi :** Le subprocess capture tout stdout (display + output). L'embedded executor appelle `.quiet()` et recupere le resultat propre via `runner.run()` → `Result<String>`

**BUG-2 : Env allowlist leak de secrets** (le plus dangereux)
- **Fichier :** `tools/nika-serve/src/worker.rs` lignes 288-297
- **Fix :** Remplacer `k.ends_with("_KEY")` (trop large, match DEPLOY_KEY, SSH_HOST_KEY) par une liste explicite : `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `XAI_API_KEY`, `GEMINI_API_KEY`, `MISTRAL_API_KEY`, `GROQ_API_KEY`, `DEEPSEEK_API_KEY`. SUPPRIMER `NIKA_VAULT_PASSPHRASE` de la liste (c'est la cle de dechiffrement du vault).

**BUG-3 : Shutdown signal fire trop tard**
- **Fichier :** `tools/nika-serve/src/lib.rs` lignes 196-207
- **Fix :** Deplacer `shutdown_tx.send(true)` dans la closure du signal (avant que Axum finish), pas apres `axum::serve().await`

### Wave 2 — Robustesse

**BUG-4 : active_jobs counter leak**
- **Fichier :** `tools/nika-serve/src/routes/workflows.rs` ligne ~92
- **Fix :** Si `storage.create_job()` echoue apres `try_acquire_job_slot()`, decrementer `active_jobs.fetch_sub(1, Relaxed)` sur le path d'erreur

**BUG-5 : SSE orphan channels**
- **Fichier :** `tools/nika-serve/src/events.rs` ligne ~86
- **Fix :** Avant `subscribe()`, verifier que le job existe dans le storage. Retourner 404 sinon.

**BUG-6 : Input key injection**
- **Fichier :** `tools/nika-serve/src/worker.rs` ligne ~275 + `routes/workflows.rs`
- **Fix :** Valider que chaque input key match `^[a-zA-Z_][a-zA-Z0-9_]*$`. Limiter a 64 keys max.

**BUG-7 : /metrics sans auth**
- **Fichier :** `tools/nika-serve/src/routes/mod.rs` lignes 23-38 + `lib.rs` ~117
- **Fix :** Ajouter `require_auth` middleware au metrics router, ou documenter que c'est intentionnellement public.

**BUG-8 : WebhookConfig reload + SSRF**
- **Fichier :** `tools/nika-serve/src/worker.rs` ligne ~144
- **Fix :** Charger WebhookConfig UNE FOIS au startup dans AppState, pas par job. Ajouter validation URL (bloquer IPs privees).

**BUG-9 : Embedded + panic=abort = crash serveur**
- **Fichier :** `tools/nika-serve/src/executor.rs` lignes 65-125
- **Fix :** Ajouter un `warn!()` au startup si embedded mode. Documenter le tradeoff crash isolation vs performance.

**BUG-10 : Polling race condition (cote client)**
- **Fichier :** `/Users/thibaut/dev/supernovae/test-jungo/pages/index.vue`
- **Fix :** Remplacer `setInterval(tick, 2000)` par un `setTimeout` chain avec un `pollAborted` guard. Le code exact est dans le bug report section BUG-10.

**BUG-11 : --quiet supprime aussi le resultat**
- Pas de fix code — c'est un design issue resolu par BUG-1 (embedded mode). Documenter dans un commentaire dans worker.rs.

## Comment travailler

### Regles absolues

```
cargo test --workspace --lib    ← TOUJOURS --lib (evite les keychain popups macOS)
cargo clippy --workspace        ← doit etre clean
```

- **1 fix = 1 commit**. Pas de batch. Chaque bug dans un commit separe.
- **Test BEFORE commit.** `cargo test --workspace --lib` doit passer.
- **Commit format :** `fix(serve): description courte`
- **Co-authors :**
  ```
  Co-Authored-By: Claude <noreply@anthropic.com>
  Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
  ```

### Ordre d'execution

```
1. Lis les 3 fichiers (bug report, CLAUDE.md, handoff)
2. BUG-1 → commit → cargo test
3. BUG-2 → commit → cargo test
4. BUG-3 → commit → cargo test
5. BUG-4 → commit → cargo test
6. BUG-5 → commit → cargo test
7. BUG-6 → commit → cargo test
8. BUG-7 → commit → cargo test (ou documenter si choix "public intentionnel")
9. BUG-8 → commit → cargo test
10. BUG-9 → commit → cargo test
11. BUG-10 → commit (dans test-jungo, pas nika)
12. E2E test final (voir section ci-dessous)
13. Tag v0.58.1
```

### E2E test final

Apres tous les fixes, tester le flow complet :

```bash
# Terminal 1 — lancer nika serve (embedded mode maintenant default)
cd /Users/thibaut/dev/supernovae/nika/tools
export NIKA_SERVE_TOKEN=test-token-1234567890123456
export NIKA_SERVE_WORKFLOWS=/Users/thibaut/dev/supernovae/test-jungo/workflows
export NIKA_SERVE_BIND=0.0.0.0:3000
target/release/nika serve

# Terminal 2 — tester l'API
curl -s http://localhost:3000/health | jq .
# Attendu : { "status": "ok", "version": "0.58.1", "service": "nika-serve" }

curl -s -X POST http://localhost:3000/v1/run \
  -H "Authorization: Bearer test-token-1234567890123456" \
  -H "Content-Type: application/json" \
  -d '{"workflow": "test-hello.nika.yaml", "inputs": {"topic": "AI"}}' | jq .
# Attendu : { "job_id": "...", "status": "pending" }

# Poll avec le job_id
curl -s http://localhost:3000/v1/status/<JOB_ID> \
  -H "Authorization: Bearer test-token-1234567890123456" | jq .
# Attendu : { "status": "completed", "output": "Hello! ..." }
# PAS de Performance/Timeline/Provider Breakdown dans output
```

**Criteres de succes :**
- [ ] `cargo test --workspace --lib` passe (0 failures)
- [ ] `cargo clippy --workspace` clean
- [ ] Health endpoint retourne ok
- [ ] POST /v1/run retourne un job_id
- [ ] GET /v1/status retourne "completed" avec un output PROPRE (texte LLM seul, pas de display CLI)
- [ ] L'output ne contient PAS "Performance", "Timeline", "Provider Breakdown", ni de box drawing chars (┌─┐)
- [ ] Pas de `NIKA_VAULT_PASSPHRASE` dans l'env du subprocess (BUG-2)
- [ ] Le serveur ne leak pas `_KEY` vars generiques (BUG-2)

## Architecture du code (pour comprendre ou tu travailles)

```
nika/tools/
├── nika/src/main.rs              ← CLI entry point (nika run, nika serve, etc.)
├── nika-serve/src/
│   ├── lib.rs                    ← Server bootstrap, run_server(), shutdown
│   ├── config.rs                 ← ServeConfig, ExecutorMode (BUG-1)
│   ├── auth.rs                   ← Bearer token, constant-time comparison
│   ├── error.rs                  ← ServeError enum
│   ├── state.rs                  ← AppState (storage, workers, config, event_bus)
│   ├── executor.rs               ← EmbeddedExecutor + SubprocessExecutor (BUG-9)
│   ├── worker.rs                 ← spawn_worker, run_subprocess, env allowlist (BUG-2, BUG-6, BUG-8)
│   ├── events.rs                 ← EventBus, SSE streaming (BUG-5)
│   ├── metrics.rs                ← Prometheus /metrics
│   ├── rate_limit.rs             ← Governor 10 req/s/token
│   ├── request_id.rs             ← X-Request-Id middleware
│   ├── webhook.rs                ← HMAC-SHA256 webhooks
│   └── routes/
│       ├── mod.rs                ← Router construction (BUG-7)
│       ├── health.rs             ← GET /health
│       └── workflows.rs          ← POST /v1/run, GET /v1/status, POST /v1/cancel (BUG-4)
├── nika-storage/src/lib.rs       ← SQLite actor, jobs table, create/complete/fail/cancel
├── nika-engine/src/
│   ├── runtime/runner.rs         ← Runner.run(), get_final_output(), render_summary()
│   └── display/                  ← CliRenderer, LiveRenderer, summary, detail levels
└── nika-core/src/
    └── ast/                      ← YAML parsing, schema validation
```

## Ce que tu NE dois PAS faire

- NE PAS toucher a `nika-engine` ou `nika-core` (sauf si un test casse)
- NE PAS changer la logique du runner ou du display
- NE PAS ajouter de dependances crates
- NE PAS modifier le schema SQLite (ca c'est pour v0.59)
- NE PAS push sur GitHub (juste commit local + tag)
- NE PAS modifier les workflows dans test-jungo (sauf BUG-10 dans index.vue)

## Contexte supplementaire

- **Nika** est un workflow engine AI en Rust. Schema `nika/workflow@0.12`. 5 verbs, 7 LLM providers, DAG execution.
- **nika serve** est le serveur HTTP (Axum) qui execute les workflows via subprocess ou embedded Runner.
- **Nicolas** est un dev Node.js qui appelle nika serve depuis son backend. Il ne connait pas Rust.
- **Le dashboard test-jungo** est un site Nuxt 3 qui teste l'API. BUG-10 est dedans.
- **L'embedded executor** (BUG-1 fix) appelle `runner.quiet().run()` qui retourne le texte LLM propre sans display.
- **Le subprocess executor** (le bug) capture tout stdout incluant le display CLI.
- **AGPL-3.0-or-later** pour tous les crates.
- Toujours `cargo test --workspace --lib` (jamais sans --lib, ca trigger des keychain popups macOS).
