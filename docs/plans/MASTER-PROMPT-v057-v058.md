# MASTER PROMPT — Nika v0.56.2 → v0.58.0

> **Donne ce fichier tel quel à une session Claude Code.**
> Chaque phase est autonome. L'agent exécute de A à Z.
> Date: 2026-04-01 | Launch: 5 mai 2026

---

## ÉTAT ACTUEL (vérifié par 15 agents)

```
Version locale:  0.56.1 (Cargo.toml)
Tag v0.56.1:     14 commits en retard sur HEAD (stability fixes)
origin/main:     ~20 commits en retard (RIEN pushé)
Tests:           9,125 passent (cargo test --workspace --lib)
Clippy:          0 warnings
INTERNER:        ✅ DÉJÀ remplacé par Arc::from (commit b4c3920db)
keyring:         ✅ DÉJÀ supprimé 100% (commit f2fb52af1)
nika-serve V1:   4 endpoints, 21 tests, subprocess model
NikaVault V1:    XChaCha20Poly1305, BTreeMap<String,String>, 7 providers
Daemon:          20+ IPC commands, 4 services (secrets, jobs, cache, watch)
nika-storage:    SQLite, 2 tables, WAL mode, production ready
VPS nk-vps:      51.15.136.200, v0.51 (5 versions de retard!)
VPS jungo:       N'EXISTE PAS ENCORE
H100 GPU:        51.159.153.241 (UP, Qwen3.5-27B)
```

## RÈGLES ABSOLUES

1. `cargo test --workspace --lib` — TOUJOURS `--lib` (pas de keychain popup)
2. `cargo clippy --workspace -- -D warnings` — ZERO warnings
3. Commits: `type(scope): description` + co-authors
4. 1 fix = 1 commit. Pas de batch.
5. TDD: test AVANT fix (RED → GREEN → REFACTOR)
6. Lire le code AVANT de modifier

```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

# PHASE 1 — SHIP v0.56.2 (30 min)

> Push les 15 commits, bump version, tag, CI green.

```bash
# 1. Vérifier que tout passe
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings

# 2. Bump version
# tools/Cargo.toml : version = "0.56.1" → "0.56.2"

# 3. Supprimer ancien tag + créer nouveau
git tag -d v0.56.1
git add tools/Cargo.toml
git commit -m "chore: bump version to 0.56.2"
git tag -a v0.56.2 -m "v0.56.2: 15 stability fixes (27-agent audit)"

# 4. Push
git push origin main --tags

# 5. Vérifier CI
gh run list --limit 5
```

---

# PHASE 2 — ENGINE CLEANUP (demi-journée)

> INTERNER et keyring sont DÉJÀ FAITS. Il reste 3 items.

## 2.1 — Global task concurrency Semaphore(64)

**Fichier:** `nika-engine/src/runtime/runner.rs`
**Bug:** Un workflow avec 10000 for_each tasks peut spawner sans limite.

```rust
// RED: test
#[tokio::test]
async fn runner_limits_concurrent_tasks() {
    // Setup workflow with 100 parallel tasks
    // Verify max 64 run simultaneously via AtomicUsize counter
}

// GREEN: fix
// In Runner::new() or Runner::run():
let task_semaphore = Arc::new(Semaphore::new(64));
// Before each task spawn:
let permit = task_semaphore.clone().acquire_owned().await?;
// Drop permit when task completes
```

**Commit:** `feat(runtime): add global task concurrency limit (Semaphore 64)`

## 2.2 — Lockfile flock

**Fichier:** `nika-engine/src/registry/lockfile.rs`
**Bug:** 2 nika run en parallèle peuvent corrompre le lockfile.

```rust
// RED: test
#[test]
fn lockfile_prevents_concurrent_writes() {
    // Open lockfile twice, second should fail or block
}

// GREEN: fix
use std::fs::File;
use nix::fcntl::{flock, FlockArg};
let f = File::open(&lockfile_path)?;
flock(f.as_raw_fd(), FlockArg::LockExclusiveNonblock)
    .map_err(|_| NikaError::LockfileContention)?;
```

**Commit:** `fix(registry): add flock to prevent concurrent lockfile writes`

## 2.3 — Package cache 5-min TTL

**Fichier:** `nika-engine/src/registry/operations.rs`
**Bug:** Re-fetch le registry à chaque run.

```rust
// RED: test
#[test]
fn cache_skips_fetch_within_ttl() {
    // Create cache file with recent mtime
    // Verify fetch is skipped
}

// GREEN: fix
const CACHE_TTL: Duration = Duration::from_secs(300);
if cache_path.exists() {
    let age = cache_path.metadata()?.modified()?.elapsed()?;
    if age < CACHE_TTL { return Ok(cached_data); }
}
```

**Commit:** `perf(registry): add 5-minute TTL to package cache`

---

# PHASE 3 — NIKA SERVE V2 (1-2 semaines)

> De subprocess à embedded Runner. Production-grade.

## Sous-phase 3A — WorkflowExecutor trait (2 jours)

### 3A.1 — Définir le trait

**Fichier:** `nika-serve/src/executor.rs` (nouveau)

```rust
use async_trait::async_trait;

#[async_trait]
pub trait WorkflowExecutor: Send + Sync {
    async fn execute(&self, job: &Job, cancel: CancellationToken) -> Result<JobResult>;
    async fn cancel(&self, job_id: &str) -> Result<()>;
}

pub struct SubprocessExecutor { /* V1 — existant */ }
pub struct EmbeddedExecutor { /* V2 — nouveau */ }
```

### 3A.2 — Refactorer worker.rs vers SubprocessExecutor

**Fichier:** `nika-serve/src/worker.rs` → extraire dans `SubprocessExecutor`
Le code de spawn_worker() actuel devient `SubprocessExecutor::execute()`.
Aucun changement de comportement — juste refactor.

**Commit:** `refactor(serve): extract SubprocessExecutor trait from worker`

### 3A.3 — Implémenter EmbeddedExecutor

**Fichier:** `nika-serve/src/executor/embedded.rs`

```rust
impl WorkflowExecutor for EmbeddedExecutor {
    async fn execute(&self, job: &Job, cancel: CancellationToken) -> Result<JobResult> {
        let workflow = parse_and_analyze(&job.workflow)?;
        let runner = Runner::new(workflow, self.provider_pool.clone());

        tokio::select! {
            result = runner.run() => {
                let output = result?;
                Ok(JobResult { output: serde_json::to_string(&output)?, exit_code: 0 })
            }
            _ = cancel.cancelled() => {
                Ok(JobResult { output: "Cancelled".into(), exit_code: 130 })
            }
        }
    }
}
```

**Key:** Partager un `reqwest::Client` pool entre les workflows (pas un client par subprocess).

**Commit:** `feat(serve): implement EmbeddedExecutor with shared provider pool`

### 3A.4 — Config pour choisir l'executor

```rust
// nika-serve/src/config.rs
pub enum ExecutorMode {
    Subprocess, // V1 legacy (safe fallback)
    Embedded,   // V2 (default)
}
// Env var: NIKA_SERVE_EXECUTOR=embedded|subprocess
```

**Commit:** `feat(serve): add executor mode config (embedded default, subprocess fallback)`

### 3A.5 — Tests EmbeddedExecutor

Au minimum **30 tests** pour nika-serve (actuellement 21, objectif 51+) :
- Embedded executor runs workflow successfully
- Embedded executor cancellation works
- Embedded executor respects timeout
- Concurrent embedded jobs respect semaphore
- Shared provider pool reused across jobs
- Job output persisted correctly
- Error handling (bad workflow, provider down)
- Memory: no leak after 100 sequential jobs

**Commit:** `test(serve): add 30+ tests for EmbeddedExecutor`

---

## Sous-phase 3B — SSE + Rate Limiting (3 jours)

### 3B.1 — SSE endpoint `GET /v1/events/{id}`

**Fichier:** `nika-serve/src/routes/events.rs` (nouveau)

```rust
async fn stream_events(
    Path(job_id): Path<String>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event>>> {
    let rx = state.event_bus.subscribe(job_id);
    let stream = ReceiverStream::new(rx).map(|event| {
        Ok(Event::default()
            .event(event.kind.as_str())
            .json_data(&event)
            .unwrap())
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

**Route:** `router.route("/v1/events/:id", get(stream_events))`
**Auth:** Bearer token (même middleware)

**Commit:** `feat(serve): add SSE streaming endpoint /v1/events/{id}`

### 3B.2 — Rate limiting (tower-governor)

**Fichier:** `nika-serve/src/rate_limit.rs` (nouveau)

```toml
# Cargo.toml
governor = "0.7"
tower_governor = "0.4"
```

```rust
use tower_governor::{GovernorLayer, GovernorConfigBuilder};

let governor_conf = GovernorConfigBuilder::default()
    .per_second(10)      // 10 req/s per token
    .burst_size(30)      // burst de 30
    .key_extractor(BearerTokenExtractor)
    .finish()
    .unwrap();

// Ajouter au router
.layer(GovernorLayer { config: governor_conf })
```

**Headers de réponse:** `X-RateLimit-Remaining`, `X-RateLimit-Reset`

**Commit:** `feat(serve): add per-token rate limiting via tower-governor`

### 3B.3 — Request ID middleware

```rust
// Middleware qui ajoute X-Request-Id à chaque réponse
// UUID v4 si pas fourni par le client
async fn request_id_middleware(req: Request, next: Next) -> Response {
    let id = req.headers().get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut resp = next.run(req).await;
    resp.headers_mut().insert("x-request-id", id.parse().unwrap());
    resp
}
```

**Commit:** `feat(serve): add X-Request-Id header middleware`

---

## Sous-phase 3C — Observability + Performance (3 jours)

### 3C.1 — Prometheus metrics

```toml
# Cargo.toml
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
```

```rust
// GET /metrics → prometheus text format
// Métriques:
// nika_jobs_total{status="completed|failed|cancelled"}
// nika_jobs_active (gauge)
// nika_job_duration_seconds (histogram)
// nika_tokens_total{provider="anthropic|openai|..."}
// nika_requests_total{method="POST|GET", path="/v1/run|..."}
```

**Commit:** `feat(serve): add Prometheus metrics endpoint`

### 3C.2 — Performance stack

```toml
# Cargo.toml
mimalloc = { version = "0.1", default-features = false }

# tools/nika/src/main.rs (1 ligne)
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```

**Commit:** `perf: switch to mimalloc allocator (+5-10% throughput)`

### 3C.3 — Job GC (cleanup vieux jobs)

```rust
// Background task dans serve
async fn job_gc(storage: Storage, interval: Duration, max_age: Duration) {
    loop {
        tokio::time::sleep(interval).await;
        storage.delete_jobs_older_than(max_age).await;
    }
}
// Default: run toutes les heures, supprime les jobs > 7 jours
```

**Commit:** `feat(serve): add automatic job garbage collection (7-day retention)`

### 3C.4 — HMAC webhook signatures

```rust
// Quand un job termine, POST vers callback URL avec HMAC signature
// Header: X-Nika-Signature: sha256=<hmac>
// Body: { "job_id": "...", "status": "completed", "output": "..." }
// Secret: NIKA_WEBHOOK_SECRET env var
```

**Commit:** `feat(serve): add HMAC webhook signatures for job callbacks`

### 3C.5 — API schema enrichi (POST /v1/run)

Étendre le body de POST /v1/run avec des champs optionnels :

```rust
// nika-serve/src/routes/workflows.rs
#[derive(Deserialize)]
struct RunRequest {
    workflow: String,
    #[serde(default)]
    inputs: serde_json::Value,
    // v0.57 additions:
    #[serde(default)]
    timeout: Option<u64>,            // Override timeout (seconds)
    #[serde(default)]
    dry_run: Option<bool>,           // Validate without executing
    #[serde(default)]
    idempotency_key: Option<String>, // Prevent double execution
    #[serde(default)]
    callback_url: Option<String>,    // Webhook URL on completion
    #[serde(default)]
    priority: Option<String>,        // "low" | "normal" | "high"
    #[serde(default)]
    tags: Option<serde_json::Value>, // Free-form metadata
}
```

**Idempotency** : Si `idempotency_key` fourni, checker dans SQLite si déjà reçu. Si oui, renvoyer le même job_id.

**Callback webhook** : Quand le job finit, si `callback_url` set :
```rust
// POST vers callback_url avec signature HMAC
let body = serde_json::to_string(&WebhookPayload {
    event: "job.completed",
    job_id, status, output, duration_ms, tokens_used, cost_usd,
})?;
let sig = hmac_sha256(webhook_secret, &body);
client.post(callback_url)
    .header("X-Nika-Signature", format!("sha256={sig}"))
    .header("Content-Type", "application/json")
    .body(body)
    .send().await?;
```

**Dry run** : Parse + validate YAML + DAG sans exécuter. Retourne immédiatement :
```json
{ "job_id": null, "status": "valid", "tasks": ["research", "summarize"], "dag": "research→summarize" }
```

**Priority** : Triée dans la queue SQLite (`ORDER BY priority DESC, created_at ASC`).

**Commit:** `feat(serve): add timeout, dry_run, idempotency_key, callback_url, priority, tags to /v1/run`

### 3C.6 — Réponse status enrichie

Étendre GET /v1/status/{id} avec plus de détails :

```json
{
  "job_id": "abc123",
  "status": "completed",
  "workflow": "seo.nika.yaml",
  "output": "...",
  "created_at": "2026-04-01T00:00:00Z",
  "started_at": "2026-04-01T00:00:01Z",
  "completed_at": "2026-04-01T00:00:15Z",
  "duration_ms": 14200,
  "tokens_used": 3400,
  "cost_usd": 0.012,
  "provider": "anthropic",
  "model": "claude-sonnet-4-20250514",
  "tags": { "customer": "acme" },
  "priority": "normal",
  "tasks": [
    { "id": "research", "status": "completed", "duration_ms": 8000 },
    { "id": "summarize", "status": "completed", "duration_ms": 6200 }
  ]
}
```

**Commit:** `feat(serve): add duration, tokens, cost, tasks detail to status response`

### Release v0.57.0

```bash
# Après tous les commits de Phase 3
# Bump tools/Cargo.toml → 0.57.0
# Tag + push
git tag -a v0.57.0 -m "v0.57.0: nika serve V2 — embedded Runner, SSE, webhooks, rate limiting, Prometheus"
git push origin main --tags
```

---

# PHASE 4 — NIKAVAULT V2 (1-2 semaines)

> De 7 clés API à vault universel de credentials.

## 4.1 — VaultEntry v2 enum

**Fichier:** `nika-core/src/vault.rs`

```rust
// Étendre le payload existant
#[derive(Serialize, Deserialize)]
struct VaultPayload {
    version: u32,  // 1 → 2
    secrets: BTreeMap<String, VaultEntry>,  // était BTreeMap<String, String>
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum VaultEntry {
    // V1 backward compat: "anthropic" → "sk-ant-..."
    Key(String),
    // V2: multi-field credentials
    Credential {
        fields: BTreeMap<String, String>,
        service_url: Option<String>,
        category: String,
        created_at: Option<String>,
        expires_at: Option<String>,
    },
}
```

**Migration:** Si `version == 1`, convertir chaque `String` en `VaultEntry::Key(s)`, bump à 2.

**Tests:**
```rust
#[test] fn migrate_v1_to_v2() { /* old format → new format */ }
#[test] fn credential_set_and_get() { /* multi-field */ }
#[test] fn backward_compat_key_still_works() { /* Key variant */ }
```

**Commit:** `feat(core): extend NikaVault to v2 with multi-field credentials`

## 4.2 — $vault.SERVICE.FIELD binding source

**Fichier:** `nika-engine/src/binding/resolve.rs`

```rust
// Nouveau BindingSource
enum BindingSource {
    Task(String),
    Input(String),
    Env(String),
    Vault { service: String, field: String },  // NEW
    Context(String),
}

// Resolution: "$vault.stripe.secret" → vault.get_credential("stripe", "secret")
```

**Template:** `{{vault.stripe.webhook}}` dans tous les contextes.
**Null safety:** `{{vault.stripe.webhook ?? ""}}`
**Redaction:** Les valeurs vault sont TOUJOURS redactées dans logs/traces/events.

**Tests:**
```rust
#[test] fn vault_binding_resolves() { /* $vault.x.y → value */ }
#[test] fn vault_binding_redacted_in_events() { /* never in logs */ }
#[test] fn vault_binding_null_with_default() { /* ?? fallback */ }
```

**Commit:** `feat(engine): add $vault.SERVICE.FIELD binding source`

## 4.3 — CLI `nika vault`

**Fichier:** `nika-cli/src/vault.rs` (nouveau)

```rust
#[derive(Subcommand)]
enum VaultAction {
    Set { service: String, #[arg(long)] field: Vec<String> },
    List,
    Check,
    Export { #[arg(long)] format: ExportFormat },
    Import { #[arg(long)] from: ImportSource },
}

// nika vault set stripe --field secret=sk_live_xxx --field webhook=whsec_xxx
// nika vault list → [stripe (2 fields), github (2 fields), anthropic (key)]
// nika vault check → test each credential
// nika vault export --format env → STRIPE_SECRET=sk_live_xxx
// nika vault import --from env → auto-detect *_API_KEY, *_TOKEN
```

**Commit:** `feat(cli): add nika vault subcommand for credential management`

## 4.4 — Daemon vault IPC extension

**Fichier:** `nika-daemon/src/protocol.rs`

```rust
// Nouveaux messages IPC
SetCredential { service: String, fields: BTreeMap<String, String>, auth_token: String },
GetCredential { service: String, field: String },
ListCredentials,
DeleteCredential { service: String, auth_token: String },

// Réponses
CredentialField { value: Option<String> },
CredentialList { services: Vec<CredentialInfo> },
```

**Commit:** `feat(daemon): extend IPC protocol for multi-field credentials`

## 4.5 — Import depuis env/Doppler

```rust
// nika vault import --from env
// Scanne les env vars: *_API_KEY, *_TOKEN, *_SECRET, *_KEY
// Propose de les importer dans le vault groupés par service

// nika vault import --from doppler
// Requiert: doppler CLI installé + projet configuré
// Exécute: doppler secrets download --format json
// Parse et importe dans le vault
```

**Commit:** `feat(cli): add vault import from env and doppler`

## 4.6 — Doppler runtime backend (NIKA_VAULT_BACKEND=doppler)

**Fichier:** `nika-core/src/vault.rs` + `nika-engine/src/secrets/fallback.rs`

Nicolas utilise Doppler pour Jungo. Au lieu de juste importer, on peut lire
directement depuis Doppler au runtime — zéro copie locale des secrets.

```rust
// nika-core/src/vault.rs
#[derive(Debug, Clone)]
enum VaultBackend {
    Local,    // Default — vault.enc local (XChaCha20Poly1305)
    Doppler,  // Runtime sync — `doppler secrets get KEY --plain`
}

impl VaultBackend {
    fn from_env() -> Self {
        match std::env::var("NIKA_VAULT_BACKEND").as_deref() {
            Ok("doppler") => Self::Doppler,
            _ => Self::Local,
        }
    }
}

// Doppler backend
struct DopplerBackend;

impl DopplerBackend {
    /// Read a single secret from Doppler via CLI
    fn get(&self, key: &str) -> Result<Option<String>> {
        let output = std::process::Command::new("doppler")
            .args(["secrets", "get", key, "--plain"])
            .output()?;
        if output.status.success() {
            let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if val.is_empty() { Ok(None) } else { Ok(Some(val)) }
        } else {
            Ok(None) // Secret not found in Doppler
        }
    }

    /// List all secrets from Doppler (names only)
    fn list(&self) -> Result<Vec<String>> {
        let output = std::process::Command::new("doppler")
            .args(["secrets", "--json"])
            .output()?;
        let map: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        Ok(map.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default())
    }
}
```

**Resolution chain étendue :**
```
NIKA_VAULT_BACKEND=doppler :
  1. Env var (toujours priorité)
  2. Doppler CLI (`doppler secrets get KEY --plain`)
  3. Vault local (fallback si Doppler indisponible)
  4. None

NIKA_VAULT_BACKEND=local (default) :
  1. Env var
  2. Daemon IPC
  3. Vault local
  4. None
```

**Config Doppler sur jungo-vps :**
```bash
# Nicolas configure Doppler une fois :
doppler setup  # choisit projet + environment
# Nika lit les secrets directement :
NIKA_VAULT_BACKEND=doppler nika run seo-pipeline.nika.yaml
# Ou dans le .env de systemd :
echo "NIKA_VAULT_BACKEND=doppler" >> /home/nika/.nika/.env
```

**Tests :**
```rust
#[test] fn doppler_backend_selected_from_env() {
    temp_env::with_var("NIKA_VAULT_BACKEND", Some("doppler"), || {
        assert!(matches!(VaultBackend::from_env(), VaultBackend::Doppler));
    });
}

#[test] fn doppler_backend_falls_back_to_local() {
    // Si doppler CLI absente, fallback local sans crash
}

#[test] fn local_backend_is_default() {
    temp_env::with_var_unset("NIKA_VAULT_BACKEND", || {
        assert!(matches!(VaultBackend::from_env(), VaultBackend::Local));
    });
}
```

**Commit:** `feat(core): add Doppler runtime vault backend (NIKA_VAULT_BACKEND=doppler)`

## 4.7 — Audit log

**Fichier:** `nika-core/src/vault.rs`

```rust
// Chaque opération vault est loguée
struct VaultAuditEntry {
    timestamp: String,
    operation: String,    // "get", "set", "delete"
    service: String,
    field: Option<String>,
    source: String,       // "cli", "daemon", "workflow:task_id"
}
// Stocké dans ~/.nika/secrets/audit.jsonl (append-only)
```

**Commit:** `feat(core): add vault audit log for credential access tracking`

### Release v0.58.0

```bash
git tag -a v0.58.0 -m "v0.58.0: NikaVault Universal Identity — multi-field credentials, $vault bindings, Doppler backend, audit log"
git push origin main --tags
```

---

# PHASE 5 — INFRASTRUCTURE (parallèle aux phases 3-4)

> Exécuter manuellement sur Scaleway + SSH. Pas dans Claude Code.

## 5.1 — Deploy v0.56.2 sur nk-vps

```bash
ssh root@51.15.136.200 << 'EOF'
  RELEASE_URL=$(curl -s https://api.github.com/repos/SuperNovae-studio/nika/releases/tags/v0.56.2 \
    | grep "browser_download_url.*linux.*x86_64.*gnu" | head -1 | cut -d '"' -f 4)
  curl -fsSL "$RELEASE_URL" -o /tmp/nika && chmod +x /tmp/nika
  /tmp/nika --version
  cp ~/.nika/bin/nika ~/.nika/bin/nika.bak
  mv /tmp/nika ~/.nika/bin/nika
  systemctl --user restart nika-daemon && sleep 3
  ~/.nika/bin/nika --version && ~/.nika/bin/nika daemon status
EOF
```

## 5.2 — Créer jungo-vps

```bash
# Scaleway console → PLAY2-NANO, fr-par-1, Ubuntu Noble
# Security group: SSH 22 + HTTP 3000
# Private network: nk-internal (pour accès H100)
```

## 5.3 — Installer Nika sur jungo-vps

```bash
ssh root@<JUNGO_IP> << 'EOF'
  useradd -m -s /bin/bash nika && loginctl enable-linger nika
  mkdir -p /home/nika/.nika/bin
  curl -fsSL https://github.com/SuperNovae-studio/nika/releases/latest/download/nika-linux-x86_64 \
    -o /home/nika/.nika/bin/nika && chmod +x /home/nika/.nika/bin/nika
  su - nika -c 'nika provider set anthropic'

  # systemd daemon + serve
  su - nika -c 'nika daemon install'
  cat > /home/nika/.config/systemd/user/nika-serve.service << 'SVC'
[Unit]
Description=Nika HTTP API
After=nika-daemon.service
Requires=nika-daemon.service
[Service]
ExecStart=/home/nika/.nika/bin/nika serve --port 3000 --bind 0.0.0.0 --workflows /opt/nika/workflows
Restart=always
EnvironmentFile=/home/nika/.nika/.env
[Install]
WantedBy=default.target
SVC

  # Token + env
  TOKEN=$(openssl rand -hex 24)
  echo "NIKA_SERVE_TOKEN=$TOKEN" > /home/nika/.nika/.env
  chmod 600 /home/nika/.nika/.env

  su - nika -c 'systemctl --user daemon-reload && systemctl --user enable --now nika-daemon nika-serve'
  sleep 5 && curl -s http://localhost:3000/health | python3 -m json.tool
  echo "TOKEN: $TOKEN"
EOF
```

## 5.4 — Doc Nicolas (copier-coller)

```
URL: http://<JUNGO_IP>:3000
Auth: Bearer <TOKEN>

POST /v1/run         — Lance un workflow
GET  /v1/status/{id} — Vérifie le status
GET  /health         — Health check

const { data } = await axios.post('http://<IP>:3000/v1/run',
  { workflow: 'seo-pipeline.nika.yaml', inputs: { topic: 'AI' } },
  { headers: { Authorization: 'Bearer <TOKEN>' } }
);
```

---

# PHASE 6 — MEGA STABILITY AUDIT (parallèle)

> Lancer sur vrais providers. Plan complet : `docs/plans/2026-04-01-mega-stability-audit.md`

6 phases :
1. Structured Output × 7 providers (8 torture cases)
2. E2E complex workflows (fan-out, multi-provider, for_each chains)
3. Bindings & 38 transforms torture
4. Security (SSRF, exec injection, template injection)
5. Agent verb + 4 guardrails + completion modes
6. Socratic loop (fix → test → repeat → 0 failures)

---

# PHASE 7 — LEGAL (avant 5 mai)

- [ ] e-Soleau INPI (15€) — URGENT, AVANT toute publication
- [ ] Trademark "Nika" — consulter avocat PI
- [ ] Privacy Policy + Mentions Légales LCEN
- [ ] AGPL source link dans UI
- [ ] Plausible Analytics (pas de cookie banner)

---

# RÉSUMÉ EXÉCUTIF

| Phase | Quoi | Durée | Bloque |
|-------|------|-------|--------|
| **1** | Ship v0.56.2 (push + deploy) | 30 min | Tout |
| **2** | Engine cleanup (semaphore, flock, cache) | 1 jour | Phase 3 |
| **3** | nika serve V2 (embedded, SSE, rate limit, metrics) → v0.57 | 1-2 sem | Phase 4 |
| **4** | NikaVault V2 ($vault, multi-field, import, audit) → v0.58 | 1-2 sem | Launch |
| **5** | Infra (VPS deploy, jungo-vps, Nicolas) | parallèle | — |
| **6** | Stability audit (7 providers, 6 phases) | parallèle | — |
| **7** | Legal (e-Soleau, trademark) | parallèle | Launch |

```
Avril 1-2    → Phase 1 + 2
Avril 3-16   → Phase 3 (serve V2) + Phase 5 (infra) + Phase 6 (audit)
Avril 17-30  → Phase 4 (vault V2) + Phase 7 (legal)
Mai 5        → LAUNCH 🚀
```

**Métriques cibles v0.58:**
- 10,000+ tests
- 0 clippy warnings, 0 unsafe, 0 FIXME
- 7/7 providers testés
- jungo-vps UP avec nika serve
- Nicolas onboardé
- e-Soleau déposé
