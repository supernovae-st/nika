# Nika v0.53.0 — Master Prompt (5 Sprints)

Tu es l'orchestrateur autonome du projet **Nika** — un workflow engine YAML semantique pour l'IA (5 verbs, 9 providers, 30+ builtin tools, 353k LOC Rust, 12 crates). Tu travailles sans intervention humaine. Commit, push, continue.

**IMPORTANT**: Ce prompt est base sur un **deep audit de 10 agents specialises** (rust-architect, rust-pro, rust-async-expert, rust-perf, rust-security, 5 explorers). Tous les chiffres sont VERIFIES contre le code. Les 8 erreurs du plan precedent ont ete corrigees.

---

# IDENTITE

| Cle | Valeur |
|-----|--------|
| Projet | Nika v0.52.0 → **v0.53.0** |
| LOC | 353,250 Rust, 12 crates |
| Tests | 8,914 (0 fail, 0 clippy) |
| Phase | Production readiness |
| Repertoire | `/Users/thibaut/dev/supernovae/nika/tools` |

# VERIFICATION INITIALE

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
```

---

# SKILLS OBLIGATOIRES

```
rust-core                   → Ownership, error handling patterns
rust-async                  → Tokio, channels, timeouts
test-driven-development     → RED-GREEN-REFACTOR pour chaque fix
systematic-debugging        → Root cause AVANT fix
verification-before-completion → cargo test + clippy AVANT commit
```

---

# ARCHITECTURE NIKA (pour contexte)

```
YAML → Parse (nika-core) → Analyze → DAG (nika-engine) → Execute → Result
                                        |
                                   TaskExecutor
                                   dispatches verb
                                        |
                              +---------+---------+
                              |    |    |    |    |
                           infer exec fetch invoke agent
                              |
                        5-Layer Structured Output
                        L0a: response_format (OpenAI/Groq/DeepSeek/xAI)
                        L0b: DynamicSubmitTool + tool_choice:Required (Anthropic/Mistral/Gemini)
                        L2: Extract JSON + validate schema
                        L3: Retry with validation feedback (infer callback)
                        L4: LLM repair (cheap model)
                        L5: Accumulated error
```

**FAIT VERIFIE**: Le structured output est du VRAI tool injection natif, PAS du prompt engineering. L0a envoie `response_format: { type: "json_schema" }` dans les params API. L0b cree un `DynamicSubmitTool` avec `tool_choice: Required` au niveau API.

---

# 5 SPRINTS

## SPRINT 1: "Model Routing + Critical Fixes" (1 jour)

### 1.1 — ModelResolver (4h, 1 new file + 7 modified)

**Probleme**: Model resolution scattered in 6+ files. 12 issues identifiees.

**Creer** `nika-core/src/catalogs/resolver.rs`:

```rust
/// Single source of truth for provider default models.
pub static PROVIDER_DEFAULTS: &[(&str, &str)] = &[
    ("anthropic", "claude-sonnet-4-6"),
    ("openai", "gpt-4o"),
    ("mistral", "mistral-large-latest"),
    ("groq", "llama-3.3-70b-versatile"),
    ("deepseek", "deepseek-chat"),
    ("gemini", "gemini-2.0-flash"),
    ("xai", "grok-3-fast"),
    ("native", "native-model"),
];

pub struct ResolvedModel {
    pub model_id: String,
    pub provider_id: String,
    pub source: ModelSource,
}

pub enum ModelSource {
    Task,
    Workflow,
    ProviderDefault,
    FallbackSubstituted { original_model: String, chain_position: usize },
    Config,
}

pub enum ModelCompatibility {
    Compatible,
    Incompatible { model: String, provider: String, reason: String },
    Unknown,
}

impl ModelResolver {
    pub fn resolve(
        task_model: Option<&str>,
        workflow_model: Option<&str>,
        config_model: Option<&str>,
        provider: &str,
        fallback_position: usize,
        original_model: Option<&str>,
    ) -> ResolvedModel;

    pub fn validate(provider: &str, model: &str) -> ModelCompatibility;
}

pub fn default_model_for_provider(provider: &str) -> Option<&'static str>;
```

**Fichiers a modifier:**

| Fichier | Changement |
|---------|-----------|
| `nika-core/src/catalogs/mod.rs` | `pub mod resolver;` + re-exports |
| `nika-engine/src/runtime/executor/infer.rs` | Remplacer 7x `model.unwrap_or_else(\|\| provider.default_model())` par `resolved.model_id` |
| `nika-engine/src/runtime/executor/agent.rs` | Meme pattern |
| `nika-engine/src/provider/rig/mod.rs` | `default_model()` delegue a `default_model_for_provider()` |
| `nika-tui/src/app/routing.rs:311-318` | Remplacer 5-arm match par `default_model_for_provider()` |
| `nika-engine/src/runtime/runner.rs:1284` | Remplacer hardcode "claude-haiku-4-5" |
| `nika-engine/src/display/renderer.rs` | Warning au lieu de "unknown" |

**Tests**: 10 tests unitaires (task priority, workflow fallback, alias resolution, fallback substitution, cross-provider validation, every provider has default, aliases resolve to same default).

**Commit**: `refactor(core): centralize model routing via ModelResolver — 12 issues fixed`

### 1.2 — Structured Output L0 Context (1h)

**Probleme**: L0 safety-net `StructuredOutputEngine` manque `with_original_prompt()`, `with_provider_context()`, `with_repair_callback()`. Le streaming-path engine les a (lines 1041-1048).

**Fix**: `infer.rs:557-561` et `infer.rs:737-741` — ajouter les 3 wiring calls.

**Aussi**: Ajouter `tokio::time::timeout(600s)` autour de `engine.validate()` dans `structured_output.rs`.

**Aussi**: Ajouter `tokio::time::sleep(Duration::from_millis(200))` dans le structured output retry loop de `runner.rs:642-816` quand `attempts > 1`.

**Commit**: `fix(structured): wire L0 safety-net context + aggregate timeout + retry delay`

### 1.3 — Output Scanner Wiring (30min)

**Probleme**: `scan_output()` et `sanitize_output()` dans `output_scanner.rs` sont implementes et testes mais JAMAIS appeles. Zero call sites.

**Fix**: Dans `run_infer()` (infer.rs), apres la reponse provider:
```rust
// After provider response, before storing in RunContext
let findings = crate::runtime::output_scanner::scan_output(&output);
if !findings.is_empty() {
    for finding in &findings {
        self.event_log.emit(EventKind::SecurityScanFinding {
            task_id: Arc::clone(task_id),
            category: finding.category.clone(),
            detail: finding.detail.clone(),
        });
    }
}
```

**Verifier**: L'EventKind `SecurityScanFinding` existe-t-il? Sinon, le creer.

**Commit**: `fix(security): wire output_scanner into infer pipeline — LLM injection audit trail`

### 1.4 — Empty Provider Chain Panic (10min)

**Fichier**: `infer.rs:224`

```rust
// BEFORE:
return Err(last_error.unwrap());

// AFTER:
return Err(last_error.unwrap_or_else(|| {
    NikaError::ProviderNotConfigured {
        provider: "none (empty provider chain)".to_string(),
    }
}));
```

**Commit**: `fix(runtime): handle empty provider chain without panic`

---

## SPRINT 2: "P-ORCHESTRATE + Security" (1 jour)

### 2.1 — Wire P-ORCHESTRATE (15min + 1h test)

**Fichier**: `runner.rs` dans `Runner::with_event_log()` ou `Runner::new()`.

```rust
// Apres creation du workflow, avant DAG construction:
let workflow = if workflow.goal.is_some() {
    crate::runtime::orchestrate::wrap_as_orchestrator(workflow)
} else {
    workflow
};
```

**E2E test**:
```yaml
schema: "nika/workflow@0.12"
provider: mock
goal: "Produce a summary of the research"
orchestrate:
  max_rounds: 3
  confidence_target: 0.8
tasks:
  - id: research
    infer: "Research the topic of AI safety"
  - id: analyze
    depends_on: [research]
    with: { data: $research }
    infer: "Analyze: {{with.data}}"
```

**Commit**: `feat(orchestrate): wire wrap_as_orchestrator into Runner + E2E test`

### 2.2 — DNS Rebinding Pin (2h)

**Probleme**: TOCTOU entre `resolve_and_check_ssrf()` et reqwest connect.

**Fix**: Apres validation DNS, utiliser `reqwest::Client::builder().resolve(host, validated_addr)`.

**Fichier**: `executor/fetch.rs` — creer un client temporaire avec `.resolve()` pour chaque fetch.

**Commit**: `fix(security): pin DNS resolution to prevent TOCTOU rebinding`

### 2.3 — Shell Alias Bypass (30min)

**Fix**: Ajouter `"alias "` au BLOCKLIST dans `security.rs:28`. Aussi `"function "` et `"declare -f"`.

**Tests**: `test_blocklist_rejects_alias_definition`, `test_blocklist_rejects_function_definition`.

**Commit**: `fix(security): block alias/function definitions in shell mode`

### 2.4 — API Key Redaction (30min)

**Fichier**: `executor/verbs.rs` — `redact_for_event()`.

Ajouter patterns: `gsk_[a-zA-Z0-9]{20,}` (Groq), `AIza[a-zA-Z0-9_-]{30,}` (Google), `xai-[a-zA-Z0-9]{20,}` (xAI).

**Commit**: `fix(security): add Groq/Gemini/xAI patterns to API key redaction`

### 2.5 — Structured Output Aggregate Timeout (30min)

```rust
// structured_output.rs — wrap validate() inner
pub async fn validate(&mut self, task_id: &str, raw_output: &str) -> Result<...> {
    const ENGINE_TOTAL_TIMEOUT: Duration = Duration::from_secs(600);
    tokio::time::timeout(ENGINE_TOTAL_TIMEOUT, self.validate_inner(task_id, raw_output))
        .await
        .map_err(|_| NikaError::StructuredOutputAllLayersFailed { ... })?
}
```

**Commit**: `fix(structured): add 600s aggregate timeout on validation engine`

### 2.6 — MCP Tool Result Size Limit (30min)

**Fichier**: `executor/invoke.rs` — ajouter verification taille apres deserialization.

```rust
const MAX_MCP_RESULT_SIZE: usize = 50 * 1024 * 1024; // 50 MB
if result_str.len() > MAX_MCP_RESULT_SIZE {
    return Err(NikaError::McpToolResultTooLarge { ... });
}
```

**Commit**: `fix(security): add 50MB size limit on MCP tool results`

---

## SPRINT 3: "Mock + E2E Tests" (1 jour)

### 3.1 — Mock Structured Output (3h)

**Probleme**: Mock provider retourne du texte fixe, pas du JSON. Impossible de tester structured output sans API keys.

**Fix**: Dans le mock provider, quand la task a un `structured:` spec:
1. Lire le schema
2. Generer du JSON valide conforme (valeurs par defaut: string→"mock", number→0, boolean→true, array→["mock"])
3. Retourner ce JSON

**Fichier**: `rig_agent_loop/providers.rs` — `run_mock()` + `provider/rig/mod.rs` — mock infer path.

**Commit**: `feat(mock): generate schema-conforming JSON for structured output`

### 3.2 — Mock Failure Simulation (1h)

Ajouter un mecanisme pour simuler des erreurs:

```rust
pub struct MockConfig {
    pub fail_first_n: usize,  // Fail N times before succeeding
    pub error: Option<NikaError>,
}
```

Utile pour tester retry sans vrais appels API.

**Commit**: `feat(mock): add failure simulation for retry/fallback testing`

### 3.3 — Vision E2E Test (1h)

Test avec image reelle envoyee a OpenAI (API key dispo):

```rust
#[tokio::test]
async fn e2e_vision_openai() {
    // 1. Import image to CAS: invoke nika:import
    // 2. Send to OpenAI with content: [{type: image}]
    // 3. Verify description contains expected keywords
}
```

**Commit**: `test(e2e): vision multimodal test with real OpenAI API`

### 3.4 — Artifact E2E Test (1h)

```rust
#[tokio::test]
async fn e2e_artifact_creates_file() {
    // 1. Run workflow with artifact: { path: "test-output.json", format: json }
    // 2. Verify file exists on disk
    // 3. Verify JSON content is valid
    // 4. Cleanup
}
```

**Commit**: `test(e2e): artifact writing verification on disk`

### 3.5 — Agent Guardrails E2E Test (1h)

```rust
#[tokio::test]
async fn e2e_agent_guardrail_length_violation() {
    // agent with max_words: 5 guardrail
    // Verify NIKA-112 error on long response
}
```

**Commit**: `test(e2e): agent guardrail violation triggers NIKA-112`

---

## SPRINT 4: "Performance + Mass Validation" (1 jour)

### 4.1 — Value Clone Elimination (2h)

**Fichier**: `binding/resolve.rs:328`

```rust
// BEFORE:
Some(LazyBinding::Resolved(value)) => Ok(value.clone()),

// AFTER: ajouter get_ref()
pub fn get_ref(&self, alias: &str) -> Option<&Value> {
    match self.bindings.get(alias)? {
        LazyBinding::Resolved(v) => Some(v),
        _ => None,
    }
}
```

Puis dans `template.rs` resolution, utiliser `get_ref()` pour eager bindings (zero clone).

**Commit**: `perf(binding): zero-clone for eager binding lookups`

### 4.2 — TransformExpr Pre-Parsing (2h)

**Fichier**: `binding/template.rs`

Changer `TemplateExpr::Alias { transforms: Vec<String> }` en `transforms: Option<TransformExpr>`.

Elimine le re-parsing dans chaque iteration de `for_each`.

**Commit**: `perf(template): pre-parse transform expressions in AST`

### 4.3 — DAG compute_depths Kahn's (1h)

**Fichier**: `dag/flow.rs:267-318`

Remplacer la boucle `while !remaining` par Kahn's BFS topological sort. `compute_layers()` le fait deja.

**Commit**: `perf(dag): compute_depths uses Kahn's algorithm O(V+E)`

### 4.4 — Run 502 Example Workflows (4h)

```bash
# Script automatise
for f in examples/gates/feature/*.nika.yaml; do
  result=$(cargo run --bin nika -- run "$f" --provider mock --no-live 2>&1 | tail -1)
  echo "$f: $result"
done | tee /tmp/gate-results.txt

# Compter succes/echecs
grep -c "DONE" /tmp/gate-results.txt
grep -c "error\|FAILED" /tmp/gate-results.txt
```

Pour chaque echec: determiner si c'est un bug YAML (fixer le workflow) ou un bug ENGINE (fixer le code). Commit chaque fix separement.

---

## SPRINT 5: "Release v0.53.0" (demi-jour)

### 5.1 — Version Bump

```bash
sed -i '' 's/version = "0.52.0"/version = "0.53.0"/' tools/Cargo.toml
```

### 5.2 — CHANGELOG

Documenter tout depuis v0.52.0.

### 5.3 — Final Verification

```bash
cargo test --workspace --lib
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --test e2e_workflow_test
```

### 5.4 — Tag + Push

```bash
git tag v0.53.0
git push && git push --tags
```

---

# REGLES

```
1. cargo test --workspace --lib TOUJOURS (--lib = pas de keychain)
2. TDD: test FAIL → fix → PASS → suite → commit
3. 1 fix = 1 commit (sauf si vraiment couple)
4. Co-authors:
   Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
5. Push toutes les 2-3 commits
6. clippy ZERO warnings (--all-targets --all-features)
7. Si bloque 3x → skip + note
8. JAMAIS marquer done sans test
9. Lire le master plan v2: docs/plans/master-plan-v2-definitive.md
```

# METRIQUES DE SUCCES

| Metrique | v0.52 | Cible v0.53 |
|----------|-------|-------------|
| Tests | 8,914 | **9,200+** |
| Panics production | 1 | **0** |
| Model routing issues | 12 | **0** |
| Structured output L0 context | missing | **complete** |
| output_scanner | dead code | **wired** |
| P-ORCHESTRATE | 40% | **100%** |
| Security findings open | 7 | **2** (SEC-05, SEC-06 by design) |
| Mock structured output | NO | **YES** |
| Vision E2E | NO | **YES** |
| Perf: Value clones in for_each | O(N*M) | **O(lazy only)** |
| Example workflows validated | 0/502 | **400+/502** |

# CONTEXT WINDOW HANDOFF

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v053.md)"
```

# GO

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
# SPRINT 1 → SPRINT 2 → SPRINT 3 → SPRINT 4 → SPRINT 5 → v0.53.0
```
