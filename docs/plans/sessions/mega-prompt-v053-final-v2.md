# Nika v0.53.0 — Final Master Prompt v2 (8-Agent Enriched)

Tu es l'orchestrateur autonome du projet **Nika**. Tu travailles sans intervention humaine. Commit, push, continue. TDD obligatoire. Code review par agent avant chaque push.

**IMPORTANT**: Ce prompt integre les findings de **8 agents supplementaires** lances apres Sprint 1. Corrections critiques par rapport au prompt v1:
- ModelResolver N'EST PAS encore wire dans infer.rs et agent.rs (20+ hardcoded models restants)
- P-ORCHESTRATE: confidence_target est IGNORE (jamais lu), events JAMAIS emis
- OOM possible sur task output >100MB (pas de size limit)
- Pas de workflow timeout global
- Token overflow (u64) sur agents 100+ turns
- MCP reconnection absente

---

# ETAT ACTUEL (verifie 2026-03-30 23:00)

| Cle | Valeur |
|-----|--------|
| Version | v0.52.0 + 6 commits |
| Tests | **8,968** (0 fail, 0 clippy) |
| Dernier commit | `8ad7c41b7` feat(orchestrate) |
| Branch | main |
| Repertoire | `/Users/thibaut/dev/supernovae/nika/tools` |

## Providers

| Provider | Env Key | Status |
|----------|---------|--------|
| OpenAI | OPENAI_API_KEY | **OK** |
| xAI | XAI_API_KEY | **OK** |
| Gemini | GEMINI_API_KEY | **RATE LIMITED** (free tier) |
| Anthropic | ANTHROPIC_API_KEY | **NO CREDITS** |
| Groq | daemon | **OK** |
| Mistral | daemon | **OK** |
| DeepSeek | daemon | **OK** |

---

# CE QUI A ETE FAIT (Sprint 1 complete)

| Commit | Description |
|--------|-------------|
| `cbfb90aaf` | output_scanner wire + empty provider chain panic fix |
| `27925163f` | ModelResolver cree dans nika-core (11 tests) |
| `440b40f01` | L0 structured output context wire + retry delay |
| `8ad7c41b7` | P-ORCHESTRATE wire + alias blocklist + redaction patterns |

---

# CORRECTIONS CRITIQUES (8-Agent Enrichment)

## CC-1: ModelResolver PAS ENCORE wire dans executor (20+ hardcoded restants)

Le ModelResolver existe dans `nika-core/src/catalogs/resolver.rs` mais n'est utilise que dans:
- `RigProvider::default_model()` (delegates)
- `nika-tui/src/app/routing.rs` (1 site)
- `runner.rs` compressor (1 site)

**PAS encore wire dans:**

| Fichier | Ligne | Code hardcode |
|---------|-------|---------------|
| `executor/infer.rs` | 369 | `model.unwrap_or_else(\|\| provider.default_model())` — 7 sites identiques |
| `executor/agent.rs` | 233 | `resolved_model.or_else(\|\| self.default_model...)` |
| `executor_compressor.rs` | 76-78 | `"claude-haiku-4-5"`, `"gpt-4.1-mini"` hardcodes |
| `tui/app/mod.rs` | 616-620 | 5 hardcoded provider→model mappings |
| `tui/app/lifecycle.rs` | 66-72 | 7 hardcoded models |
| `tui/state/chat_overlay.rs` | 84,86 | `"claude-sonnet-4"`, `"gpt-4o"` |
| `tui/views/chat/mod.rs` | 331-367 | 5 hardcoded models |

**Action Sprint 2:** Wire `default_model_for_provider()` dans TOUS ces sites. Total: ~20 edits.

## CC-2: P-ORCHESTRATE — confidence_target IGNORE + events JAMAIS emis

| Gap | Detail | Fix |
|-----|--------|-----|
| confidence_target | Le champ existe (default 0.85) mais JAMAIS LU pendant l'execution | Verifier completion.confidence >= config.confidence_target dans RigAgentLoop |
| OrchestratorStarted | Event defini mais JAMAIS emit | Emettre dans Runner avant execution du __orchestrator__ task |
| OrchestratorRound | Event defini mais JAMAIS emit | Emettre dans RigAgentLoop apres chaque tour d'agent |
| OrchestratorCompleted | Event defini mais JAMAIS emit | Emettre quand agent appelle nika:complete |

**Action Sprint 3:** Implementer la boucle confidence + events.

## CC-3: OOM sur gros outputs (HIGH)

Pas de limite de taille sur les task outputs. `Arc<serde_json::Value>` stocke en memoire. Un `for_each` de 100K items × 1KB = 100MB en memoire sans protection.

**Fix:** Ajouter `MAX_TASK_OUTPUT_SIZE = 50MB` dans runner.rs avant stockage dans RunContext.

## CC-4: Pas de workflow timeout global (HIGH)

Seuls les task-level timeouts existent. Un workflow peut tourner indefiniment.

**Fix:** Ajouter `max_duration_secs` optionnel au workflow header. Default: 3600s (1h). Enforcer via `tokio::time::timeout` dans `runner.run()`.

## CC-5: Token overflow u64 (MEDIUM)

`total_tokens` dans agent loop accumule sans `saturating_add`. Sur 100+ turns avec gros outputs, u64 peut overflow.

**Fix:** Remplacer `+=` par `saturating_add()` dans `providers.rs:364-366`.

## CC-6: MCP reconnection absente (MEDIUM)

Si un serveur MCP crash mid-workflow, pas de reconnection. Le tool call echoue et l'agent recoit une erreur.

**Fix Sprint future:** Implementer retry avec exponential backoff dans `McpClientPool::get_or_connect()`.

## CC-7: for_each concurrency:0 silencieusement → 1

`.max(1)` dans runner.rs:2193 transforme silencieusement `concurrency: 0` en 1. Pas d'erreur.

**Fix:** Valider dans l'analyzer que `concurrency >= 1` avec erreur claire.

## CC-8: Fetch — HTTPS→HTTP downgrade pas bloque

Redirects de HTTPS vers HTTP ne sont pas bloquees. Standard HTTP mais risque securite.

**Fix Sprint future:** Option `require_https: true` dans policy config.

---

# SECURITE CONFIRMEE PAR AUDIT

| Composant | Status | Details |
|-----------|--------|---------|
| Template injection | **SAFE** | 3-pass isolation, injection references bloquees |
| Shell injection (exec) | **SAFE** | \|shell modifier, NFKC, blocklist |
| SSRF | **SAFE** | IPv4/IPv6/mapped/compatible, DNS rebinding, post-redirect check |
| CRLF in headers | **SAFE** | \r\n rejection |
| Path traversal (artifacts) | **SAFE** | sanitize_for_path(), canonicalize fail-closed |
| CSS selector ReDoS | **SAFE** | cssparser linear-time |
| JSONPath deep nesting | **SAFE** | serde_json depth limit ~128 |
| CAS hash collision | **SAFE** | BLAKE3 256-bit |
| Cost tracking accuracy | **CORRECT** | Tous providers prices, cached tokens discounts corrects |
| Fetch size limits | **SAFE** | 50MB text, 100MB binary, streaming check |

---

# SPRINTS RESTANTS (3.5 sprints)

## SPRINT 2: "ModelResolver Wire Complete + Quick Fixes" (1 jour)

### 2.1 — Wire ModelResolver dans executor/infer.rs (2h)

Remplacer les 7 sites de `model.unwrap_or_else(|| provider.default_model())` par:
```rust
let resolved = nika_core::catalogs::ModelResolver::resolve(
    resolved_model.as_deref(),
    self.default_model.as_deref(),
    provider_name,
    provider_idx,
    resolved_model.as_deref(),
);
let model_id = resolved.model_id.as_str();
```

Utiliser `model_id` partout dans la fonction (cost, events, structured output).

**Commit:** `refactor(infer): wire ModelResolver — eliminate 7 model fallback sites`

### 2.2 — Wire ModelResolver dans TUI (1h)

Remplacer les hardcoded models dans:
- `tui/app/mod.rs:616-620`
- `tui/app/lifecycle.rs:66-72`
- `tui/state/chat_overlay.rs:84,86`
- `tui/views/chat/mod.rs:331-367`

Tous → `nika_core::catalogs::default_model_for_provider(provider_name)`

**Commit:** `refactor(tui): wire ModelResolver — eliminate 20 hardcoded model strings`

### 2.3 — Wire ModelResolver dans executor_compressor.rs (15min)

Remplacer lignes 76-78 hardcoded par `default_model_for_provider()`.

**Commit:** `refactor(compressor): wire ModelResolver for cheap model selection`

### 2.4 — Token overflow fix (15min)

Dans `providers.rs:364-366`, remplacer:
```rust
total_input_tokens += input_tokens;
```
par:
```rust
total_input_tokens = total_input_tokens.saturating_add(input_tokens);
```

**Commit:** `fix(agent): use saturating_add for token accumulation — prevent u64 overflow`

### 2.5 — Task output size limit (30min)

Dans `runner.rs`, avant stockage dans RunContext:
```rust
const MAX_OUTPUT_SIZE: usize = 50 * 1024 * 1024;
if output.len() > MAX_OUTPUT_SIZE {
    tracing::warn!(task_id = %task_id, size = output.len(), "Task output exceeds 50MB limit — truncating");
    output.truncate(MAX_OUTPUT_SIZE);
}
```

**Commit:** `fix(runtime): add 50MB output size limit — prevent OOM on large outputs`

### 2.6 — for_each concurrency validation (15min)

Dans l'analyzer (nika-core), valider `concurrency >= 1`:
```rust
if concurrency == Some(0) {
    return Err(AnalyzerError::InvalidConcurrency { task_id, value: 0 });
}
```

**Commit:** `fix(analyzer): reject concurrency: 0 with clear error instead of silent .max(1)`

### 2.7 — DNS rebinding pin (2h)

Apres `resolve_and_check_ssrf()`, creer un client temporaire:
```rust
let mut builder = reqwest::Client::builder();
for addr in resolved_addrs {
    builder = builder.resolve(host, addr);
}
let pinned_client = builder.build()?;
```

**Commit:** `fix(security): pin DNS resolution via reqwest .resolve() — prevent TOCTOU rebinding`

### 2.8 — Structured output aggregate timeout (30min)

```rust
// structured_output.rs
const ENGINE_TIMEOUT: Duration = Duration::from_secs(600);
pub async fn validate(...) -> Result<...> {
    tokio::time::timeout(ENGINE_TIMEOUT, self.validate_inner(...))
        .await
        .map_err(|_| NikaError::StructuredOutputAllLayersFailed { ... })?
}
```

**Commit:** `fix(structured): add 600s aggregate timeout on validation engine`

### 2.9 — MCP tool result size limit (15min)

```rust
// executor/invoke.rs
const MAX_MCP_RESULT: usize = 50 * 1024 * 1024;
if result_str.len() > MAX_MCP_RESULT {
    return Err(NikaError::ExecutionError { ... });
}
```

**Commit:** `fix(security): add 50MB size limit on MCP tool results`

---

## SPRINT 3: "Mock + E2E + P-ORCHESTRATE Fix" (1 jour)

### 3.1 — Mock structured output (3h)

Quand task a `structured:`, generer JSON valide depuis schema.

### 3.2 — Mock failure simulation (1h)

`NIKA_MOCK_FAIL_COUNT=N` env var.

### 3.3-3.7 — E2E Tests (4h)

- Vision E2E (real OpenAI)
- Artifact E2E (verify on disk)
- Agent guardrails E2E
- Provider fallback E2E
- Retry E2E

### 3.8 — P-ORCHESTRATE confidence_target + events (2h)

1. Emit `OrchestratorStarted` dans Runner quand `__orchestrator__` task demarre
2. Emit `OrchestratorRound` dans RigAgentLoop apres chaque tour
3. Emit `OrchestratorCompleted` quand nika:complete est appele
4. Verifier `completion.confidence >= config.confidence_target` — si non, retry

**Commit:** `feat(orchestrate): implement confidence_target checking + emit orchestrator events`

---

## SPRINT 4: "Performance + 502 Workflows" (1 jour)

### 4.1 — Value clone elimination (get_ref) (2h)
### 4.2 — TransformExpr pre-parsing (2h)
### 4.3 — DAG compute_depths Kahn's (1h)
### 4.4 — Run 502 example workflows (4h)

---

## SPRINT 5: "Release v0.53.0" (demi-jour)

### 5.1 — Version bump + CHANGELOG
### 5.2 — Code review agent
### 5.3 — Tag + push

---

# BUGS DECOUVERTS PAR 8 AGENTS (a integrer dans les sprints)

| # | Bug | Severity | Sprint | Fix |
|---|-----|----------|--------|-----|
| B1 | 20+ hardcoded model strings (not wired to ModelResolver) | HIGH | S2 | Wire all sites |
| B2 | confidence_target ignored in P-ORCHESTRATE | HIGH | S3 | Implement checking |
| B3 | Orchestrator events never emitted | MEDIUM | S3 | Emit in Runner/RigAgentLoop |
| B4 | OOM on >100MB task output | HIGH | S2 | 50MB limit |
| B5 | No global workflow timeout | HIGH | S4 | max_duration_secs header |
| B6 | Token overflow u64 in agent loop | MEDIUM | S2 | saturating_add |
| B7 | No MCP reconnection | MEDIUM | Future | Exponential backoff |
| B8 | concurrency:0 silently becomes 1 | LOW | S2 | Validator error |
| B9 | HTTPS→HTTP downgrade allowed | LOW | Future | require_https option |
| B10 | Nested for_each no flatten | DOC | S4 | Document limitation |
| B11 | stop_sequences provider normalization | LOW | Future | Use ProviderName enum |
| B12 | Chat overlay hardcoded models | MEDIUM | S2 | Wire ModelResolver |

# SECURITE CONFIRMEE (pas de fix necessaire)

| Vector | Status | Evidence |
|--------|--------|---------|
| Template injection 3-pass | SAFE | 15+ tests, trusted_context block |
| $env in LLM output | SAFE | Not a template syntax |
| for_each item injection | SAFE | No double-resolution |
| Path traversal artifacts | SAFE | sanitize_for_path() |
| Shell exec injection | MITIGATED | \|shell modifier |
| CRLF headers | SAFE | \r\n rejected |
| CSS ReDoS | SAFE | cssparser linear |
| CAS BLAKE3 collision | SAFE | 256-bit |
| JSON context escaping | SAFE | Context-aware |
| Prototype pollution | IMMUNE | Rust type system |
| Cost tracking | ACCURATE | All 7 providers, cached discounts |
| Fetch size limits | SAFE | Streaming + Content-Length |
| SSRF (all vectors) | SAFE | Defense-in-depth |

# METRIQUES DE SUCCES

| Metrique | Actuel | Cible v0.53 |
|----------|--------|-------------|
| Tests lib | 8,968 | **9,300+** |
| Tests E2E | 40 | **55+** |
| Panics production | 0 | **0** |
| Hardcoded models | 20+ | **0** |
| P-ORCHESTRATE | wired (no confidence) | **confidence + events** |
| Security findings | 4 open | **1** (SEC-05 known) |
| OOM protection | none | **50MB limit** |
| Token overflow | possible | **saturating_add** |
| Mock structured | NO | **YES** |
| Vision E2E | NO | **YES** |
| Workflow timeout | none | **max_duration_secs** |
| Example workflows | 0/502 | **400+/502** |

# REGLES

```
1. cargo test --workspace --lib AVANT chaque commit
2. cargo clippy --workspace --all-targets --all-features -- -D warnings
3. TDD: test ROUGE → fix → test VERT → commit
4. 1 fix = 1 commit
   Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
6. Push toutes les 2-3 commits
7. JAMAIS marquer done sans test
```

# CONTEXT WINDOW HANDOFF

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v053-final-v2.md)"
```

# GO

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
# SPRINT 2 → SPRINT 3 → SPRINT 4 → SPRINT 5 → v0.53.0
```
