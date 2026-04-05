# Daemon ↔ LSP Bridge — Mega Implementation Prompt

**Date**: 2026-03-27
**Target**: v0.50.0
**Estimated**: 5-8 sessions, ~20h total
**Style**: TDD + Parallel Agents + Code Review + E2E Verification

---

## PROMPT TO COPY-PASTE INTO NEXT SESSION

---

Tu vas implémenter le **Daemon ↔ LSP Bridge** et le **UX Polish** pour Nika, le workflow engine.
C'est LA feature différenciatrice — l'éditeur le plus intelligent pour les workflows AI.

## ═══════════════════════════════════════════════════
## PHASE 0 : LECTURE OBLIGATOIRE (NE CODE PAS ENCORE)
## ═══════════════════════════════════════════════════

### Plans à lire en premier

1. `docs/plans/2026-03-27-daemon-lsp-bridge-design.md` — Design détaillé (9 parts, 30 tasks)
2. `docs/plans/2026-03-27-handoff-daemon-lsp-bridge.md` — État actuel + ce qui a été fait
3. `docs/plans/2026-03-27-daemon-lsp-bridge-mega-prompt.md` — CE FICHIER (contexte enrichi)

### Exploration du code source

Lance **6 agents Explore en parallèle** pour comprendre le code AVANT de coder :

**Agent 1 — Daemon Protocol** : Lis `tools/nika-daemon/src/protocol.rs` (845 lignes).
- 23 `DaemonRequest` variants (Ping, Status, GetSecret, HasSecret, ListSecrets, SetSecret, DeleteSecret, JobSubmit, JobList, JobStatus, JobCancel, JobRetry, JobHistory, WatchStart, WatchStop, WatchStatus, CacheGet, CacheSet, CacheClear, CacheStats, EventSubscribe, Shutdown)
- 21 `DaemonResponse` variants (Ok, Error, Pong, StatusInfo, Secret, SecretExists, SecretList, SecretStored, SecretDeleted, AuthRequired, JobCreated, JobList, JobDetail, JobHistoryList, WatchActive, WatchInactive, CacheHit, CacheMiss, CacheStatsResult, Event, ShuttingDown)
- Serde dispatch : `#[serde(tag = "type")]` sur les deux enums
- Wire format : 4-byte BE u32 length + JSON payload, max 16MB
- 2 structs existants : `ProviderSecretInfo { provider, source }`, `SecretSource { Env, Keychain, NotFound }`
- 25 tests existants
- **EventSubscribe** : zero fields, réponse = streaming `Event { event: serde_json::Value }`

**Agent 2 — Daemon Server + Client** : Lis `tools/nika-daemon/src/server.rs` (1283 lignes) + `client.rs` (647 lignes).
- `ServerState` : secret_service, job_service (Option), cache_service, event_bus (broadcast), active_watch, shutdown_tx, auth_token
- `route_request()` : 21 match arms, async
- Auth : blake3 timing-safe XOR comparison, token in `~/.nika/daemon/.token`
- Read ops (GetSecret, HasSecret, ListSecrets, Cache reads) : **no auth required**
- Write ops (SetSecret, DeleteSecret, Shutdown) : **auth required**
- EventSubscribe streaming : 5-min idle timeout, handles Lagged/Closed
- `ConnectedClient` : reader/writer split, poisoning on error, 5s default timeout
- `DaemonClient::connect()` → `DaemonResult<ConnectedClient>`
- 18 server tests, 20 client tests

**Agent 3 — LSP Backend + Handlers** : Lis ces fichiers :
- `tools/nika-lsp/src/backend.rs` : `NikaBackend { client, documents: DashMap<Uri, DocumentState>, validation_tx, handler: DefaultHandler }`. 12 capabilities registered (completion, hover, definition, semantic_tokens, document_symbol, code_action, code_lens, inlay_hint, references, document_link, folding_range). Completion triggers: `:`, ` `, `-`, `.`, `{`, `@`
- `tools/nika-lsp/src/document.rs` : `DocumentState { rope: Rope, version: i32 }` — **NO last_valid_ast yet**
- `tools/nika-lsp-core/src/handlers/completion.rs` (1599 lines) : `completions(text, offset, context) -> Vec<CompletionItem>`. 15 completion functions, 40 tests. **Cost NOT used in completions** currently.
- `tools/nika-lsp-core/src/handlers/inlay_hints.rs` (227 lines) : 6 hint types (timeout, alias, depends_on, max_turns, concurrency, model). **Static cost table** hardcoded (20 model entries). 3 tests.
- `tools/nika-lsp-core/src/handlers/code_lens.rs` (89 lines) : 3 lens types (Run, Validate, TaskCount). 1 test.
- `tools/nika-lsp-core/src/handlers/hover.rs` : 13+ CursorContext variants handled, returns `HoverResult { contents: String, range: Option<(u32,u32)> }`. **No run history** currently.
- `tools/nika-lsp/src/diagnostics.rs` : 5 validation phases (parse → analyze → templates → empty tasks → provider key check). Provider key check is ENV ONLY (7 providers hardcoded). 7 tests.
- `tools/nika-lsp-core/src/handlers/references.rs` (751 lines) : `find_task_at_offset()` + `find_task_references()`. Covers 7 reference types (id def, inline deps, scalar deps, multi-line deps, with bindings, dollar templates, alias templates). 25 tests.

**Agent 4 — Extension + E2E Harness** : Lis :
- `editors/vscode/src/extension.ts` (645 lines) : 13 functions, 5 commands, binary auto-download, **NO status bar items**, **NO output channel** (only implicit LSP one). LSP transport: stdio, args: `['lsp', ...extraArgs]`
- `editors/vscode/snippets/nika.code-snippets` : 21 snippets existants. Tab stops format: `${1:placeholder}`, `${1|option1,option2|}`, `$0`
- `editors/vscode/package.json` : v0.42.0, 5 commands, 4 settings, 2 activation events
- `tools/nika-lsp/tests/e2e_harness.rs` (929 lines) : `LspClient` struct with `send_request()`, `read_message_with_timeout()`, 14 tests (all `#[ignore]`)

**Agent 5 — Storage + Catalogs** : Lis :
- `tools/nika-daemon/src/storage.rs` : `Job { id, name, workflow, args, cron, state, created_at, started_at, completed_at, exit_code, output, retry_count, max_retries }`. **NO cost/tokens/provider/model fields**. SQLite tables: `jobs` + `job_history`. Methods: insert_job, get_job, list_jobs, update_state, add_history, get_history, increment_retry. **NO list_jobs_for_workflow** method yet.
- `tools/nika-core/src/catalogs/providers.rs` : 19 `Provider` structs (7 LLM + 11 MCP + 1 Local). `find_provider(name)`, `provider_to_env_var(id)`, `providers_by_category(cat)`, `validate_key_format(provider, key)`.
- `tools/nika-core/src/catalogs/models.rs` : `KnownModel` with ModelType enum
- `tools/nika-daemon/Cargo.toml` : depends on `nika-core` ONLY. **NOT** nika-engine. deps: tokio, serde, blake3, chrono, uuid, rusqlite, notify, croner, nix

**Agent 6 — LSP Architecture** : Lis :
- `tools/nika-lsp/src/main.rs` : thin 50-line async server, `LspService::build(NikaBackend::new)`
- `tools/nika-lsp-core/src/lib.rs` : 745+ tests, modules: analysis, db, document, handler, handlers, parse, position
- `tools/nika-lsp-core/src/handlers/mod.rs` : 11 handler modules
- `tools/nika-lsp/Cargo.toml` : deps on tower-lsp-server 0.23, nika-engine (AST only), nika-lsp-core, ropey, dashmap, parking_lot

### Recherche externe (lance en parallèle)

Lance **3 agents de recherche** :
- **Perplexity** : "tower-lsp-server 0.23 custom request handler Rust example 2025" — pour savoir comment implémenter `nika/daemonStatus` custom request
- **Perplexity** : "VS Code extension StatusBarItem API best practices 2025 polling interval" — pour la status bar
- **Context7** : `ctx7 library tower-lsp-server "custom request handling"` — docs à jour

### Enrichissement du plan

Après avoir lu tout ça, **enrichis le plan** avec ces corrections CRITIQUES :

1. **Le design dit `nika_engine::provider::cost::estimate_cost()`** — FAUX. nika-daemon ne dépend PAS de nika-engine. Le cost catalog est dans `nika-core/src/catalogs/`. Le static cost table est dans `inlay_hints.rs`. Tu dois créer une fonction de cost lookup dans **nika-core** ou réutiliser les données hardcodées.

2. **Le design dit `storage.list_jobs_for_workflow(path)`** — cette méthode N'EXISTE PAS. Tu dois la créer dans storage.rs. Le champ s'appelle `workflow` (pas `workflow_path`). La requête SQL doit filtrer sur `workflow = ?`.

3. **Le design dit `DaemonClient::connect(&socket)`** — la vraie signature est `DaemonClient::connect() -> DaemonResult<ConnectedClient>` (pas de param socket, il utilise `daemon_socket_path()`).

4. **Job struct n'a PAS de champs cost/tokens** — le design suppose `json_extract(output, '$.cost_usd')` qui ne marchera que si `output` contient du JSON avec ces clés. Vérifie ce que le runner met dans `output`.

5. **Les handlers nika-lsp-core sont des fonctions PURES** — pas d'async, pas de state. Le daemon data doit être passé en **paramètre**, pas requêté à l'intérieur des handlers. C'est une contrainte architecturale non-négociable.

6. **`ProviderSecretInfo`** existe déjà dans protocol.rs avec `{ provider: String, source: SecretSource }`. Le design parle de `ProviderInfo { name, has_key, source, category }` — c'est un TYPE DIFFÉRENT. Tu dois décider : étendre `ProviderSecretInfo` ou créer un nouveau type.

7. **EventSubscribe** utilise `serde_json::Value` pour les events — il n'y a PAS de typed event enum dans protocol.rs. Les events sont dans `tools/nika-daemon/src/events.rs`. Vérifie la structure de `DaemonEvent`.

8. **Le LSP standalone (nika-lsp)** et le LSP embarqué (dans le binary nika) sont DEUX binaires séparés. Le plan ne parle que de nika-lsp. L'embarqué utilise aussi `NikaBackend` dans `tools/nika/src/lsp/`. Vérifie si tu dois toucher les deux.

9. **completion.rs a 1599 lignes** — c'est GROS. Quand tu ajoutes le provider filtering, fais-le dans `provider_completions()` qui existe déjà. Ne crée pas de nouvelles fonctions si tu peux enrichir l'existante.

10. **Le test harness e2e** requiert `cargo build -p nika-lsp` avant de lancer les tests.

## ═══════════════════════════════════════════════════
## MÉTHODOLOGIE STRICTE
## ═══════════════════════════════════════════════════

### Skills OBLIGATOIRES à utiliser

```
/spn-powers:test-driven-development        — RED → GREEN → REFACTOR pour chaque feature
/spn-powers:verification-before-completion  — vérifie que tout passe avant de dire "c'est fini"
/spn-powers:dispatching-parallel-agents     — lance des agents parallèles pour les domaines indépendants
/spn-powers:requesting-code-review          — code review après chaque batch de 3 tasks
/spn-powers:systematic-debugging            — si un test fail, 4 phases (root cause, hypothèse, fix, verify)
/spn-powers:executing-plans                 — batch de 3 tasks, report entre les batches
/spn-powers:root-cause-tracing              — si un bug apparaît, trace jusqu'à la source
```

### TDD strict — le cycle pour CHAQUE feature

```
1. ÉCRIS LE TEST D'ABORD (dans le bon fichier de tests)
2. cargo test -p <crate> --lib -- <test_name>  →  DOIT ÉCHOUER (RED)
3. Implémente le minimum pour passer le test
4. cargo test -p <crate> --lib -- <test_name>  →  DOIT PASSER (GREEN)
5. Refactor si nécessaire (noms, abstractions, DRY)
6. cargo test -p <crate> --lib  →  TOUS les tests du crate passent
7. Passe au test suivant
```

### Vérification entre chaque batch

Après chaque batch de 3 tasks :
```bash
cargo test --workspace --lib                    # TOUT passe
cargo clippy --workspace -- -D warnings         # ZÉRO warnings
```

Si un test fail :
1. NE CODE PAS PLUS
2. Utilise /spn-powers:systematic-debugging
3. Trouve le root cause
4. Fix
5. Re-run les tests
6. Seulement ENSUITE continue

### Code review après chaque phase

Après chaque phase majeure (1-9), lance un agent `code-reviewer` :
```
Lance un agent spn-powers:code-reviewer qui review :
- Les fichiers modifiés dans cette phase
- Contre le plan original
- Contre les conventions Nika (NikaError, NIKA-XXX codes, Option<T> pour daemon data)
- Security: pas de .unwrap() sur daemon results, timeouts sur tous les await
```

## ═══════════════════════════════════════════════════
## PHASE 1 : DAEMON PROTOCOL EXTENSION
## ═══════════════════════════════════════════════════

### Task 1.1 : Nouveaux types dans protocol.rs

**Fichier** : `tools/nika-daemon/src/protocol.rs`

**PATTERN EXACT à suivre** (copié du code existant) :

```rust
// Les enums utilisent #[serde(tag = "type")]
// Chaque variant est un objet JSON avec "type": "VariantName"
// Exemple existant :
//   GetSecret { provider: String }
//   → {"type":"GetSecret","provider":"anthropic"}
```

**4 nouveaux DaemonRequest variants** à ajouter (après EventSubscribe, avant Shutdown) :

```rust
/// LSP: list all known providers with key status
ListProviderStatus,

/// LSP: estimate cost for a model invocation
EstimateCost {
    provider: String,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
},

/// LSP: get recent runs for a workflow file
GetWorkflowHistory {
    workflow: String,  // ⚠️ Pas "workflow_path" — le champ Job s'appelle "workflow"
},

/// LSP: get daemon capabilities and stats
GetDaemonCapabilities,
```

**4 nouveaux DaemonResponse variants** :

```rust
ProviderStatusList {
    providers: Vec<ProviderStatusInfo>,
},

CostEstimateResult {
    estimate: CostEstimate,
},

WorkflowHistoryResult {
    runs: Vec<WorkflowRunInfo>,
},

DaemonCapabilitiesResult {
    capabilities: DaemonCapabilities,
},
```

**5 nouveaux structs** à créer (dans protocol.rs, après ProviderSecretInfo) :

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderStatusInfo {
    pub id: String,
    pub name: String,
    pub has_key: bool,
    pub source: SecretSource,
    pub category: ProviderCategory,
    pub env_var: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProviderCategory {
    Llm,
    Mcp,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostEstimate {
    pub usd: f64,
    pub input_rate_per_million: f64,
    pub output_rate_per_million: f64,
    pub model: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowRunInfo {
    pub job_id: String,
    pub state: String,
    pub workflow: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonCapabilities {
    pub version: String,
    pub uptime_secs: u64,
    pub cache_entries: usize,
    pub cache_hit_rate: f64,
    pub active_jobs: usize,
    pub watch_active: bool,
    pub total_cost_saved: f64,
}
```

### Task 1.2 : Tests TDD pour le protocol

**ÉCRIS CES TESTS D'ABORD** (avant d'implémenter) :

```rust
#[test]
fn request_serialize_list_provider_status() {
    let req = DaemonRequest::ListProviderStatus;
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"type":"ListProviderStatus"}"#);
    let back: DaemonRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back, req);
}

#[test]
fn request_serialize_estimate_cost() {
    let req = DaemonRequest::EstimateCost {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        input_tokens: 1000,
        output_tokens: 500,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains(r#""type":"EstimateCost"#));
    let back: DaemonRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back, req);
}

#[test]
fn request_serialize_get_workflow_history() {
    let req = DaemonRequest::GetWorkflowHistory {
        workflow: "test.nika.yaml".into(),
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains(r#""workflow":"test.nika.yaml"#));
    let back: DaemonRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back, req);
}

#[test]
fn request_serialize_get_daemon_capabilities() {
    let req = DaemonRequest::GetDaemonCapabilities;
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"type":"GetDaemonCapabilities"}"#);
}

#[test]
fn response_serialize_provider_status_list() {
    let resp = DaemonResponse::ProviderStatusList {
        providers: vec![ProviderStatusInfo {
            id: "anthropic".into(),
            name: "Anthropic Claude".into(),
            has_key: true,
            source: SecretSource::Env,
            category: ProviderCategory::Llm,
            env_var: "ANTHROPIC_API_KEY".into(),
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DaemonResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back, resp);
}

#[test]
fn response_serialize_cost_estimate() {
    let resp = DaemonResponse::CostEstimateResult {
        estimate: CostEstimate {
            usd: 0.018,
            input_rate_per_million: 3.0,
            output_rate_per_million: 15.0,
            model: "claude-sonnet-4-20250514".into(),
            provider: "anthropic".into(),
        },
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DaemonResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back, resp);
}

// AUSSI : mettre à jour roundtrip_request_all_variants() et roundtrip_response_all_variants()
// pour inclure les 4 nouveaux variants !
```

**Cycle** : tests RED → implémente → tests GREEN → `cargo test -p nika-daemon --lib`

### Task 1.3 : Handlers dans server.rs

**Fichier** : `tools/nika-daemon/src/server.rs`

Ajoute 4 match arms dans `route_request()` :

```rust
DaemonRequest::ListProviderStatus => {
    // Utilise nika_core::catalogs::providers::KNOWN_PROVIDERS
    // Pour chaque provider, check secret_service.has_secret(&provider.id)
    // Retourne ProviderStatusList avec Vec<ProviderStatusInfo>
}

DaemonRequest::EstimateCost { provider, model, input_tokens, output_tokens } => {
    // ⚠️ PAS nika-engine ! Utilise le cost data de nika-core ou hardcode ici
    // Le static cost table de inlay_hints.rs a 20 modèles
    // Crée une fonction cost_per_million_tokens(model) dans un module approprié
    // Calcul : usd = (input_tokens * input_rate + output_tokens * output_rate) / 1_000_000
}

DaemonRequest::GetWorkflowHistory { workflow } => {
    // Appelle storage.list_jobs_for_workflow(&workflow)
    // ⚠️ Cette méthode N'EXISTE PAS encore — tu dois la créer (Task 1.4)
    // Retourne WorkflowHistoryResult
}

DaemonRequest::GetDaemonCapabilities => {
    // Agrège : version, uptime, cache_service.stats(), job count, watch status
}
```

**ATTENTION** : `ListProviderStatus` et `EstimateCost` ne nécessitent PAS d'auth (ce sont des lectures).

### Task 1.4 : Méthode storage

**Fichier** : `tools/nika-daemon/src/storage.rs`

Ajoute cette méthode publique :

```rust
pub async fn list_jobs_for_workflow(&self, workflow: &str) -> DaemonResult<Vec<Job>> {
    // SQL: SELECT * FROM jobs WHERE workflow = ? ORDER BY created_at DESC LIMIT 10
    // Même pattern que list_jobs() mais filtré par workflow
}
```

**Test TDD** :
```rust
#[tokio::test]
async fn list_jobs_for_workflow_returns_matching() {
    let storage = Storage::open_memory().unwrap();
    // Insert 3 jobs: 2 for "a.nika.yaml", 1 for "b.nika.yaml"
    // list_jobs_for_workflow("a.nika.yaml") → 2 results
    // list_jobs_for_workflow("c.nika.yaml") → 0 results
}
```

### Task 1.5 : Cost lookup (nouveau)

**⚠️ ENRICHISSEMENT DU PLAN** : Le design original suppose `nika_engine::provider::cost::estimate_cost()`. Ça n'existe pas dans nika-daemon (il ne dépend pas de nika-engine).

**Solution** : Crée un module `tools/nika-daemon/src/cost.rs` avec les données de pricing hardcodées (copiées de `inlay_hints.rs`). Ou mieux : déplace le cost table dans `nika-core/src/catalogs/cost.rs` pour que les deux crates puissent l'utiliser.

**Décision architecturale** : Vérifie si `nika-core/src/catalogs/` a déjà un `cost.rs`. Si non, crée-le. Si oui, utilise-le.

```rust
// Dans nika-core/src/catalogs/cost.rs (ou nika-daemon/src/cost.rs)
pub struct ModelPricing {
    pub provider: &'static str,
    pub model_pattern: &'static str,  // substring match
    pub input_per_million: f64,
    pub output_per_million: f64,
}

pub fn estimate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> Option<f64> {
    let pricing = find_pricing(model)?;
    Some(
        (input_tokens as f64 * pricing.input_per_million
            + output_tokens as f64 * pricing.output_per_million)
            / 1_000_000.0,
    )
}
```

### Checkpoint Phase 1

```bash
cargo test -p nika-daemon --lib    # 155+ existants + ~10 nouveaux = tous passent
cargo test -p nika-core --lib      # Si cost.rs ajouté ici
cargo clippy --workspace -- -D warnings
```

**Lance un agent code-reviewer** sur protocol.rs + server.rs + storage.rs.

## ═══════════════════════════════════════════════════
## PHASE 2 : LSP DAEMON BRIDGE
## ═══════════════════════════════════════════════════

### Task 2.1 : Créer daemon_bridge.rs

**Fichier** : `tools/nika-lsp/src/daemon_bridge.rs` (NOUVEAU, ~300 lignes)

**TESTS D'ABORD** :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bridge_returns_none_when_daemon_not_running() {
        // Pointe vers un socket inexistant
        let bridge = DaemonBridge::try_connect_to("/tmp/nonexistent-nika-test.sock").await;
        assert!(bridge.is_none());
    }

    #[tokio::test]
    async fn bridge_provider_status_returns_empty_when_disconnected() {
        let bridge = DaemonBridge::disconnected();
        let status = bridge.provider_status().await;
        assert!(status.is_empty());
    }

    #[tokio::test]
    async fn bridge_estimate_cost_returns_none_when_disconnected() {
        let bridge = DaemonBridge::disconnected();
        let cost = bridge.estimate_cost("anthropic", "claude-sonnet-4", 1000, 500).await;
        assert!(cost.is_none());
    }

    #[tokio::test]
    async fn bridge_workflow_history_returns_empty_when_disconnected() {
        let bridge = DaemonBridge::disconnected();
        let history = bridge.workflow_history("test.nika.yaml").await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn bridge_is_connected_false_when_disconnected() {
        let bridge = DaemonBridge::disconnected();
        assert!(!bridge.is_connected());
    }

    #[tokio::test]
    async fn bridge_capabilities_returns_none_when_disconnected() {
        let bridge = DaemonBridge::disconnected();
        let caps = bridge.capabilities().await;
        assert!(caps.is_none());
    }
}
```

**Implémentation** :

```rust
use nika_daemon::protocol::*;
use nika_daemon::{ConnectedClient, DaemonClient};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const CACHE_TTL: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub struct DaemonBridge {
    client: Arc<RwLock<Option<ConnectedClient>>>,
    providers_cache: Arc<RwLock<(Vec<ProviderStatusInfo>, Instant)>>,
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl DaemonBridge {
    /// Try to connect to running daemon. Returns None if daemon not running.
    pub async fn try_connect() -> Option<Self> { ... }

    /// Create a disconnected bridge (for testing / graceful degradation)
    pub fn disconnected() -> Self { ... }

    /// Is daemon currently connected?
    pub fn is_connected(&self) -> bool { ... }

    /// Get provider status (cached 60s). Returns empty vec if disconnected.
    pub async fn provider_status(&self) -> Vec<ProviderStatusInfo> { ... }

    /// Estimate cost. Returns None if disconnected or model unknown.
    pub async fn estimate_cost(&self, provider: &str, model: &str,
                                input: u64, output: u64) -> Option<CostEstimate> { ... }

    /// Get workflow run history. Returns empty vec if disconnected.
    pub async fn workflow_history(&self, workflow: &str) -> Vec<WorkflowRunInfo> { ... }

    /// Get daemon capabilities. Returns None if disconnected.
    pub async fn capabilities(&self) -> Option<DaemonCapabilities> { ... }

    /// Background reconnection loop
    async fn reconnect_loop(/* ... */) { ... }
}
```

**CONTRAINTES ABSOLUES** :
- Tous les `await` vers le daemon doivent avoir `tokio::time::timeout(REQUEST_TIMEOUT, ...)`
- JAMAIS de `.unwrap()` sur un résultat daemon — toujours `.ok()` ou `match`
- Reconnexion avec backoff exponentiel (1s, 2s, 4s, 8s, 16s, max 30s)
- `provider_status()` utilise le cache si `Instant::now() - last_refresh < 60s`

### Task 2.2 : Intégrer dans NikaBackend

**Fichier** : `tools/nika-lsp/src/backend.rs`

1. Ajoute le champ :
```rust
pub struct NikaBackend {
    client: Client,
    documents: DashMap<Uri, DocumentState>,
    validation_tx: mpsc::Sender<ValidationRequest>,
    handler: DefaultHandler,
    daemon: Arc<RwLock<Option<DaemonBridge>>>,  // ← NOUVEAU
}
```

2. Dans `new()`, spawn la connexion daemon en background :
```rust
let daemon = Arc::new(RwLock::new(None));
let daemon_clone = daemon.clone();
tokio::spawn(async move {
    if let Some(bridge) = DaemonBridge::try_connect().await {
        *daemon_clone.write().await = Some(bridge);
        tracing::info!("Connected to Nika daemon");
    } else {
        tracing::debug!("Daemon not running — LSP features degraded");
    }
});
```

3. Ajoute `daemon_bridge.rs` dans `src/main.rs` :
```rust
mod daemon_bridge;
```

4. **VÉRIFIE** : `cargo test -p nika-lsp --lib` → tous les tests existants passent encore

### Task 2.3 : Ajouter nika-daemon en dépendance de nika-lsp

**Fichier** : `tools/nika-lsp/Cargo.toml`

```toml
# ATTENTION: seulement si unix (le daemon est unix-only)
[target.'cfg(unix)'.dependencies]
nika-daemon = { path = "../nika-daemon", version = "0.49.0", default-features = false }
```

**⚠️** Vérifie que ça ne crée pas de circular dependency. Le graphe doit être :
```
nika-core ← nika-daemon ← (client module only)
nika-core ← nika-engine ← nika-lsp
                            nika-lsp → nika-daemon (client only)
```

### Checkpoint Phase 2

```bash
cargo test -p nika-lsp --lib       # Existants + bridge tests passent
cargo test --workspace --lib       # Tout le workspace
cargo clippy --workspace -- -D warnings
```

## ═══════════════════════════════════════════════════
## PHASE 3 : WIRE DAEMON DATA INTO HANDLERS
## ═══════════════════════════════════════════════════

**C'est le CŒUR de la feature.** Les handlers nika-lsp-core sont des **fonctions pures**.
Le daemon data est passé en **paramètre**, jamais requêté à l'intérieur.

### Task 3.1 : Enrichir les completions (providers)

**Fichier** : `tools/nika-lsp-core/src/handlers/completion.rs`

La fonction `provider_completions()` existe déjà. Modifie-la pour accepter un paramètre optionnel :

```rust
pub fn provider_completions(
    prefix: &str,
    current_provider: Option<&str>,
    daemon_providers: Option<&[ProviderStatusInfo]>,  // ← NOUVEAU
) -> Vec<CompletionItem>
```

**Tests TDD** :

```rust
#[test]
fn provider_completion_with_daemon_shows_key_status() {
    let providers = vec![
        ProviderStatusInfo {
            id: "anthropic".into(),
            name: "Anthropic Claude".into(),
            has_key: true,
            source: SecretSource::Env,
            category: ProviderCategory::Llm,
            env_var: "ANTHROPIC_API_KEY".into(),
        },
        ProviderStatusInfo {
            id: "openai".into(),
            name: "OpenAI".into(),
            has_key: false,
            source: SecretSource::NotFound,
            category: ProviderCategory::Llm,
            env_var: "OPENAI_API_KEY".into(),
        },
    ];
    let items = provider_completions("", None, Some(&providers));
    let anthropic = items.iter().find(|i| i.label == "anthropic").unwrap();
    assert!(anthropic.detail.as_ref().unwrap().contains("✓"));
    let openai = items.iter().find(|i| i.label == "openai").unwrap();
    assert!(openai.detail.as_ref().unwrap().contains("no API key"));
}

#[test]
fn provider_completion_without_daemon_shows_all() {
    let items = provider_completions("", None, None);
    assert!(items.len() >= 8); // All providers listed
    // No key status shown
}
```

**⚠️ Attention** : `ProviderStatusInfo` est défini dans nika-daemon. Mais nika-lsp-core ne dépend PAS de nika-daemon. Tu as 2 options :
- Option A : Déplace `ProviderStatusInfo` dans nika-core (meilleur — c'est un type de données pur)
- Option B : Crée un trait/type miroir dans nika-lsp-core

**Recommandation** : Option A. Déplace les structs `ProviderStatusInfo`, `CostEstimate`, etc. dans `nika-core::catalogs::` et réexporte depuis nika-daemon.

### Task 3.2 : Enrichir les completions (models)

Dans la même fonction `provider_completions()`, quand `current_provider` est Some :
- Filtre les modèles pour ne montrer que ceux du provider actif
- Ajoute le prix dans `detail` : `"claude-sonnet-4 · $3/$15 per 1M tokens"`

### Task 3.3 : Enrichir le hover

**Fichier** : `tools/nika-lsp-core/src/handlers/hover.rs`

Modifie la signature :
```rust
pub fn hover(
    text: &str,
    offset: u32,
    context: &CursorContext,
    daemon_data: Option<&DaemonHoverData>,  // ← NOUVEAU
) -> Option<HoverResult>
```

Crée un struct pour le daemon data :
```rust
pub struct DaemonHoverData {
    pub workflow_history: Vec<WorkflowRunInfo>,
    pub provider_status: Vec<ProviderStatusInfo>,
}
```

Quand on hover sur `workflow:` et que `daemon_data` contient de l'historique :
```markdown
## my-workflow

**Last runs:**
- ✓ 12s ago — 2.3s, exit 0
- ✗ 1h ago — failed, exit 1
- ✓ 3h ago — 1.9s, exit 0
```

### Task 3.4 : Enrichir les inlay hints

**Fichier** : `tools/nika-lsp-core/src/handlers/inlay_hints.rs`

Modifie la signature :
```rust
pub fn inlay_hints(
    text: &str,
    start_offset: u32,
    end_offset: u32,
    daemon_cost: Option<&dyn Fn(&str) -> Option<(f64, f64)>>,  // ← NOUVEAU : model → (input_rate, output_rate)
) -> Vec<InlayHintEntry>
```

Quand `daemon_cost` est Some, utilise-le au lieu du static cost table.
Quand None, fallback sur le static cost table (comportement actuel).

### Task 3.5 : Enrichir le code lens

**Fichier** : `tools/nika-lsp-core/src/handlers/code_lens.rs`

Ajoute un nouveau variant `LensCommand::LastRun` :
```rust
pub enum LensCommand {
    Run,
    Validate,
    TaskCount(usize),
    LastRun { status: String, duration: String, ago: String },  // ← NOUVEAU
}
```

Modifie `code_lenses()` pour accepter un historique optionnel :
```rust
pub fn code_lenses(
    text: &str,
    last_run: Option<&WorkflowRunInfo>,  // ← NOUVEAU
) -> Vec<CodeLensEntry>
```

### Task 3.6 : Enrichir les diagnostics

**Fichier** : `tools/nika-lsp/src/diagnostics.rs`

Dans Phase 5 (provider key check), ajoute le check daemon :

```rust
// AVANT (env var seulement) :
if env_var_missing(provider) { warn NIKA-031 }

// APRÈS (daemon + env var) :
if let Some(daemon_providers) = daemon_provider_status {
    // Check daemon first (covers keychain)
    if let Some(info) = daemon_providers.iter().find(|p| p.id == provider) {
        if !info.has_key {
            warn NIKA-031 with "Set via `nika provider set {provider}` or {env_var}"
        }
    }
} else {
    // Fallback: env var check (current behavior)
    if env_var_missing(provider) { warn NIKA-031 }
}
```

### Checkpoint Phase 3

```bash
cargo test -p nika-lsp-core --lib    # 745+ tests passent
cargo test -p nika-lsp --lib         # Bridge + handler wiring tests
cargo test --workspace --lib         # Tout le workspace
cargo clippy --workspace -- -D warnings
```

**Lance le code-reviewer agent** sur les modifications de completion.rs, hover.rs, inlay_hints.rs, code_lens.rs, diagnostics.rs.

## ═══════════════════════════════════════════════════
## PHASE 4 : EVENT SUBSCRIPTION
## ═══════════════════════════════════════════════════

### Task 4.1 : Subscribe aux daemon events

**Fichier** : `tools/nika-lsp/src/daemon_bridge.rs`

1. Lis `tools/nika-daemon/src/events.rs` pour comprendre la structure de `DaemonEvent`
2. Ajoute `subscribe_events()` au bridge :

```rust
pub async fn subscribe_events(
    &self,
    tx: tokio::sync::mpsc::Sender<DaemonEvent>,
) {
    // Si connecté : spawn background task qui lit les events
    // Sur erreur : log warning, ne crash pas
    // Sur déconnexion : tente reconnexion
}
```

### Task 4.2 : Réagir aux events dans le backend

**Fichier** : `tools/nika-lsp/src/backend.rs`

Dans `initialized()` (appelé après le handshake LSP) :
```rust
// Spawn event listener
if let Some(bridge) = self.daemon.read().await.as_ref() {
    let (tx, mut rx) = mpsc::channel(32);
    bridge.subscribe_events(tx).await;

    let client = self.client.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                DaemonEvent::WatchTriggered { path } => {
                    // Revalidate le document si ouvert
                    tracing::debug!("Watch triggered: {}", path);
                }
                DaemonEvent::JobCompleted { .. } => {
                    // Rafraîchir les code lens
                    // client.send_request("workspace/codeLens/refresh") si supporté
                }
                _ => {} // Ignore les autres events
            }
        }
    });
}
```

### Checkpoint Phase 4

```bash
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings
```

## ═══════════════════════════════════════════════════
## PHASE 5 : RENAME HANDLER
## ═══════════════════════════════════════════════════

### Task 5.1 : Créer rename.rs

**Fichier** : `tools/nika-lsp-core/src/handlers/rename.rs` (NOUVEAU)

**Réutilise** `references.rs` — il fait déjà tout le travail de recherche.

**TESTS D'ABORD** (6 minimum) :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_rename_on_task_id() {
        let text = "tasks:\n  - id: fetch_data\n    infer: \"hello\"";
        let offset = text.find("fetch_data").unwrap() as u32 + 2;
        let result = prepare_rename(text, offset);
        assert!(result.is_some());
        let range = result.unwrap();
        assert_eq!(&text[range.start as usize..range.end as usize], "fetch_data");
    }

    #[test]
    fn prepare_rename_on_non_identifier_returns_none() {
        let text = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: test";
        let offset = text.find("schema").unwrap() as u32;
        assert!(prepare_rename(text, offset).is_none());
    }

    #[test]
    fn rename_updates_all_references() {
        let text = r#"tasks:
  - id: step1
    infer: "hello"
  - id: step2
    depends_on: [step1]
    with:
      data: $step1
    infer: "{{with.data}}"
"#;
        let offset = text.find("step1").unwrap() as u32;
        let edits = rename(text, offset, "fetch_data");
        // Should find: id definition, depends_on ref, with $ref
        assert!(edits.len() >= 3);
        // Apply edits and verify
        let result = apply_edits(text, &edits);
        assert!(!result.contains("step1"));
        assert!(result.contains("fetch_data"));
    }

    #[test]
    fn rename_invalid_name_returns_error() {
        let text = "tasks:\n  - id: step1\n    infer: \"test\"";
        let offset = text.find("step1").unwrap() as u32;
        // Names with spaces are invalid
        let result = try_rename(text, offset, "invalid name");
        assert!(result.is_err());
    }

    #[test]
    fn rename_in_for_each_context() {
        let text = r#"tasks:
  - id: urls
    infer: "list urls"
  - id: scrape
    depends_on: [urls]
    with:
      data: $urls
    for_each: "{{with.data}}"
    fetch: "{{with.item}}"
"#;
        let offset = text.find("urls").unwrap() as u32;
        let edits = rename(text, offset, "sources");
        assert!(edits.len() >= 3); // id + depends_on + with $ref
    }

    #[test]
    fn rename_template_dollar_refs() {
        let text = "tasks:\n  - id: a\n    infer: \"{{$a.data}}\"";
        let offset = text.find(": a").unwrap() as u32 + 2;
        let edits = rename(text, offset, "b");
        // Should update $a → $b in template
        let result = apply_edits(text, &edits);
        assert!(result.contains("{{$b.data}}"));
    }
}
```

**Implémentation** :

```rust
use super::references::{find_task_at_offset, find_task_references, ReferenceEntry};

pub struct RenameRange {
    pub start: u32,
    pub end: u32,
}

pub struct TextEdit {
    pub start: u32,
    pub end: u32,
    pub new_text: String,
}

/// Check if cursor is on a renameable identifier. Returns range if yes.
pub fn prepare_rename(text: &str, offset: u32) -> Option<RenameRange> {
    let task_id = find_task_at_offset(text, offset)?;
    // Find the exact range of the task_id at cursor position
    // ...
    Some(RenameRange { start, end })
}

/// Rename a task ID. Returns text edits for all references.
pub fn rename(text: &str, offset: u32, new_name: &str) -> Vec<TextEdit> {
    let task_id = match find_task_at_offset(text, offset) {
        Some(id) => id,
        None => return vec![],
    };
    let refs = find_task_references(text, &task_id);
    // Convert ReferenceEntry to TextEdit, replacing old name with new name
    // ⚠️ Apply edits in REVERSE ORDER (highest offset first) to preserve offsets
    refs.into_iter()
        .map(|r| TextEdit {
            start: r.start_offset,
            end: r.end_offset,
            new_text: new_name.to_string(),
        })
        .collect()
}
```

### Task 5.2 : Register rename dans backend.rs

**Fichier** : `tools/nika-lsp/src/backend.rs`

Dans `initialize()` capabilities :
```rust
rename_provider: Some(OneOf::Left(true)),
```

Implémente `prepare_rename()` et `rename()` dans `NikaBackend`.

### Checkpoint Phase 5

```bash
cargo test -p nika-lsp-core --lib -- rename    # Les 6+ tests passent
cargo test --workspace --lib
```

## ═══════════════════════════════════════════════════
## PHASE 6 : AST CACHE + UX POLISH
## ═══════════════════════════════════════════════════

### Task 6.1 : Last-valid-AST caching

**Fichier** : `tools/nika-lsp/src/document.rs`

```rust
pub struct DocumentState {
    pub rope: Rope,
    pub version: i32,
    pub last_valid_raw: Option<Arc<nika_core::ast::raw::RawWorkflow>>,      // ← NOUVEAU
    pub last_valid_analyzed: Option<Arc<nika_core::ast::analyzed::AnalyzedWorkflow>>,  // ← NOUVEAU
}
```

**Fichier** : `tools/nika-lsp/src/backend.rs`

Dans le validation flow :
```rust
// On parse success:
doc.last_valid_raw = Some(Arc::new(raw_workflow.clone()));
doc.last_valid_analyzed = Some(Arc::new(analyzed.clone()));

// On parse failure:
// Keep existing cache — don't clear it
// Use cached AST for hover/goto-def when current parse fails
```

Dans les handlers hover/completion/goto-def :
```rust
// Si parse du document actuel échoue, utilise le cache
let ast = current_parse.unwrap_or_else(|| doc.last_valid_raw.clone());
```

**Test** :
```rust
#[test]
fn hover_works_on_broken_yaml_with_cache() {
    // 1. Parse a valid workflow → cache is populated
    // 2. Break the YAML (remove closing bracket)
    // 3. Call hover → should still return results from cache
}
```

### Task 6.2 : Status bar (extension.ts)

**Fichier** : `editors/vscode/src/extension.ts`

Dans `activate()` :

```typescript
// Status bar item
const statusBar = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100
);
statusBar.command = 'nika.showOutput';
statusBar.text = '$(butterfly) Nika: Starting...';
statusBar.tooltip = 'Nika Language Server';
statusBar.show();
context.subscriptions.push(statusBar);

// Output channel
const output = vscode.window.createOutputChannel('Nika Language Server');
context.subscriptions.push(output);

function log(level: string, msg: string) {
    output.appendLine(`[${new Date().toISOString()}] [${level}] ${msg}`);
}

// Register show output command
context.subscriptions.push(
    vscode.commands.registerCommand('nika.showOutput', () => output.show())
);

log('INFO', `Nika extension v${context.extension.packageJSON.version} activating`);
log('INFO', `Platform: ${process.platform}/${process.arch}`);
```

Après le démarrage du LSP client :
```typescript
// Poll daemon status every 30s
const statusInterval = setInterval(async () => {
    if (!client || !client.isRunning()) {
        statusBar.text = '$(butterfly) Nika: LSP $(x)';
        statusBar.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
        return;
    }
    try {
        const status = await client.sendRequest('nika/daemonStatus');
        if (status.connected) {
            statusBar.text = `$(butterfly) Nika: LSP $(check) | Daemon $(check)`;
            statusBar.backgroundColor = undefined;
        } else {
            statusBar.text = `$(butterfly) Nika: LSP $(check) | Daemon $(x)`;
            statusBar.backgroundColor = undefined;
        }
    } catch {
        statusBar.text = '$(butterfly) Nika: LSP $(check)';
    }
}, 30000);
context.subscriptions.push({ dispose: () => clearInterval(statusInterval) });

// Initial status update
statusBar.text = '$(butterfly) Nika: LSP $(check)';
```

**Fichier** : `tools/nika-lsp/src/backend.rs`

Ajoute le custom request handler pour `nika/daemonStatus` :
```rust
// Dans la méthode qui gère les requêtes custom (ou ajoute-en une)
// tower-lsp-server 0.23 utilise la méthode `request` custom
```

⚠️ **Recherche nécessaire** : Vérifie comment tower-lsp-server 0.23 gère les custom requests. C'est peut-être via `#[tower_lsp::rpc]` ou via un trait method override. Lance une recherche Perplexity si besoin.

### Task 6.3 : Nouveaux snippets

**Fichier** : `editors/vscode/snippets/nika.code-snippets`

Ajoute 7 snippets (suis le format existant avec tab stops) :

1. **artifact** : `["artifact", "output"]`
2. **limits** : `["limits", "budget"]`
3. **context** : `["context", "files"]`
4. **imports** : `["imports", "include", "partial"]`
5. **content-vision** : `["contentvision", "multimodal"]`
6. **for-each-fetch** : `["foreachfetch", "scrapeall"]`
7. **fan-out-fan-in** : `["fanout", "parallel", "mapreduce"]`

### Task 6.4 : Output channel logging

Déjà couvert dans Task 6.2. Assure-toi de logger :
- Activation : version, platform
- Binary discovery : path trouvé
- LSP start/fail
- Daemon connection status
- Erreurs

### Checkpoint Phase 6

```bash
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings
cd editors/vscode && npm run compile   # Extension compile sans erreur
```

## ═══════════════════════════════════════════════════
## PHASE 7 : VÉRIFICATION FINALE + E2E
## ═══════════════════════════════════════════════════

### Task 7.1 : Nouveaux tests e2e

**Fichier** : `tools/nika-lsp/tests/e2e_harness.rs`

Ajoute ces tests e2e :

```rust
#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_completion_provider_shows_items() {
    let mut client = LspClient::new();
    client.initialize();
    client.open_document("file:///test.nika.yaml", "schema: \"nika/workflow@0.12\"\nprovider: ");
    let resp = client.send_request("textDocument/completion", json!({
        "textDocument": { "uri": "file:///test.nika.yaml" },
        "position": { "line": 1, "character": 10 }
    }));
    // Verify provider items present
    let items = resp["result"]["items"].as_array().unwrap_or(&vec![]);
    assert!(!items.is_empty(), "Should have provider completions");
    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_rename_task_id() {
    let mut client = LspClient::new();
    client.initialize();
    let text = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: old_name\n    infer: \"test\"\n  - id: step2\n    depends_on: [old_name]";
    client.open_document("file:///test.nika.yaml", text);
    let resp = client.send_request("textDocument/rename", json!({
        "textDocument": { "uri": "file:///test.nika.yaml" },
        "position": { "line": 2, "character": 10 },
        "newName": "new_name"
    }));
    // Verify edits returned
    assert!(resp["result"].is_object(), "Should return workspace edit");
    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_inlay_hints_present() {
    let mut client = LspClient::new();
    client.initialize();
    let text = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: gen\n    infer:\n      prompt: \"test\"\n      model: claude-sonnet-4-20250514";
    client.open_document("file:///test.nika.yaml", text);
    let resp = client.send_request("textDocument/inlayHint", json!({
        "textDocument": { "uri": "file:///test.nika.yaml" },
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 6, "character": 0 }
        }
    }));
    // May or may not have hints depending on model line
    assert!(!resp.get("error").is_some(), "Should not error");
    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_daemon_status_custom_request() {
    let mut client = LspClient::new();
    client.initialize();
    let resp = client.send_request("nika/daemonStatus", json!({}));
    // Should return { connected: false } when daemon not running
    assert!(resp["result"]["connected"].is_boolean());
    client.shutdown();
}
```

### Task 7.2 : Run ALL tests

```bash
# Unit tests (TOUS les crates)
cargo test --workspace --lib

# E2E tests (nécessite build d'abord)
cargo build -p nika-lsp
cargo test -p nika-lsp --test e2e_harness -- --ignored

# Clippy
cargo clippy --workspace -- -D warnings

# Extension
cd editors/vscode && npm run compile
```

### Task 7.3 : Code review finale

Lance **3 agents en parallèle** :

1. **Code reviewer** : Review tous les fichiers modifiés contre le plan et les conventions Nika
2. **Security reviewer** : Review daemon_bridge.rs — IPC, socket, timeouts, pas de .unwrap(), pas de leak de secrets
3. **Rust reviewer** : Review les patterns async, les Arc/RwLock, les potential deadlocks

## ═══════════════════════════════════════════════════
## PHASE 8 : COMMITS
## ═══════════════════════════════════════════════════

**1 commit par domaine logique** (pas un mega commit) :

```
feat(daemon): add LSP query protocol — ListProviderStatus, EstimateCost, GetWorkflowHistory, GetDaemonCapabilities

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

```
feat(lsp): create daemon bridge with graceful degradation

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

```
feat(lsp): wire daemon data into completions, hover, inlay hints, code lens, diagnostics

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

```
feat(lsp): event subscription for live updates

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

```
feat(lsp-core): rename handler for task ID refactoring

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

```
feat(lsp): last-valid-AST caching for broken YAML resilience

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

```
feat(extension): status bar + output channel + 7 new snippets

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

```
test(lsp): daemon bridge tests + e2e daemon-powered tests

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**Chaque commit doit passer** : `cargo test --workspace --lib` + `cargo clippy --workspace -- -D warnings`

## ═══════════════════════════════════════════════════
## ERREURS CRITIQUES À ÉVITER
## ═══════════════════════════════════════════════════

| # | Erreur | Conséquence | Comment éviter |
|---|--------|-------------|----------------|
| 1 | Ajouter nika-engine en dep de nika-daemon | Circular dependency, build cassé | nika-daemon → nika-core SEULEMENT |
| 2 | .unwrap() sur résultat daemon | Crash du LSP si daemon down | Toujours .ok() ou match, retourne Option/Vec vide |
| 3 | Oublier timeout sur await daemon | LSP hang si daemon freeze | `tokio::time::timeout(5s, ...)` sur TOUT |
| 4 | Modifier handler signature sans adapter les deux LSP | Embedded LSP cassé | Vérifie `tools/nika/src/lsp/` aussi |
| 5 | Supposer qu'une fonction existe | Compile error | Lis le fichier AVANT de l'appeler |
| 6 | Hardcoder le socket path | Casse sur custom NIKA_HOME | Utilise `nika_daemon::daemon_socket_path()` |
| 7 | Merger des tests qui fail | Régression | Fix d'abord, merge ensuite |
| 8 | Oublier les co-author lines | Convention violée | Template dans chaque commit |
| 9 | Cost data dans nika-daemon mais pas nika-lsp-core | Types non partagés | Mets les types dans nika-core |
| 10 | Async dans les handlers nika-lsp-core | Casse l'architecture pure | Passe le data en param, pas de fetch |
| 11 | Ne pas tester le cas "daemon down" | Le LSP crash en prod | Test explicite pour CHAQUE méthode |
| 12 | Oublier de register rename dans capabilities | Feature invisible | `rename_provider: Some(OneOf::Left(true))` |
| 13 | Ne pas mettre à jour roundtrip tests | Serde bug non détecté | Ajoute les 4 nouveaux variants aux roundtrip tests |
| 14 | provider_completions change la signature mais pas les appelants | Compile error | Grep pour tous les call sites |

## ═══════════════════════════════════════════════════
## RÉSULTAT ATTENDU
## ═══════════════════════════════════════════════════

À la fin, je dois pouvoir :

1. **Ouvrir VS Code/Cursor** sur un `.nika.yaml`
2. **Status bar** : `🦋 Nika: LSP ✓ | Daemon ✓` (ou `Daemon ✗` si pas lancé)
3. **Taper `provider: `** → voir les providers avec `✓` / `no key`
4. **Taper `model: `** → voir seulement les modèles du provider avec pricing
5. **Hover sur `model:`** → voir `Anthropic · $3/$15 per 1M tokens`
6. **Hover sur `workflow:`** → voir les 3 derniers runs
7. **Inlay hints** → coût estimé `~$0.02`
8. **Code lens** → `✓ Last: 2.3s, $0.004 (12s ago)`
9. **Renommer un task ID** → toutes les refs mises à jour (id, depends_on, with, templates)
10. **Casser le YAML** → hover fonctionne encore (AST cache)
11. **Output channel** → logs structurés
12. **7 nouveaux snippets** → artifact, limits, context, imports, vision, for-each-fetch, fan-out-fan-in

**Tests** : 8500+ passent, clippy clean, extension compile.

## ═══════════════════════════════════════════════════
## ARCHITECTURE DE TYPES — DÉCISION À PRENDRE
## ═══════════════════════════════════════════════════

Le plus gros choix architectural : **où mettre les types partagés ?**

```
Option A (RECOMMANDÉE) :
  nika-core/src/catalogs/lsp_types.rs
    → ProviderStatusInfo, CostEstimate, WorkflowRunInfo, DaemonCapabilities
    → Réexportés par nika-daemon et nika-lsp-core

Option B :
  nika-daemon/src/protocol.rs (types définis ici)
  nika-lsp-core (duplice les types ou utilise un trait)
    → Plus de duplication, moins clean

Option C :
  Nouveau crate nika-protocol
    → Overkill pour 4 structs
```

**Choisis Option A** sauf si tu as une meilleure idée après exploration.

## ═══════════════════════════════════════════════════
## PLANNING PAR BATCH (executing-plans)
## ═══════════════════════════════════════════════════

```
BATCH 1 (Phase 1) — 5 tasks :
  1.1 Protocol types
  1.2 Protocol tests
  1.3 Server handlers
  1.4 Storage method
  1.5 Cost lookup
  → CHECKPOINT + CODE REVIEW

BATCH 2 (Phase 2) — 3 tasks :
  2.1 daemon_bridge.rs
  2.2 Integrate into NikaBackend
  2.3 Add nika-daemon dependency
  → CHECKPOINT + CODE REVIEW

BATCH 3 (Phase 3) — 6 tasks :
  3.1 Provider completions
  3.2 Model completions
  3.3 Hover enrichment
  3.4 Inlay hints enrichment
  3.5 Code lens enrichment
  3.6 Diagnostics enrichment
  → CHECKPOINT + CODE REVIEW

BATCH 4 (Phase 4+5) — 4 tasks :
  4.1 Event subscription
  4.2 Event handling in backend
  5.1 Rename handler
  5.2 Register rename
  → CHECKPOINT + CODE REVIEW

BATCH 5 (Phase 6) — 4 tasks :
  6.1 AST cache
  6.2 Status bar
  6.3 Snippets
  6.4 Output channel
  → CHECKPOINT + CODE REVIEW

BATCH 6 (Phase 7+8) — 5 tasks :
  7.1 New e2e tests
  7.2 Run ALL tests
  7.3 Code review finale
  8.1 Commits (8 commits logiques)
  8.2 Final verification
  → DONE
```

Total : **27 tasks** en **6 batches** avec code review entre chaque batch.
