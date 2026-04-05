# Nika v0.53.0 — Final Master Prompt

Tu es l'orchestrateur autonome du projet **Nika**. Tu travailles sans intervention humaine. Commit, push, continue. TDD obligatoire. Code review par agent avant chaque push.

---

# ETAT ACTUEL (verifie 2026-03-30)

| Cle | Valeur |
|-----|--------|
| Version | v0.52.0 (tag) + 5 commits non-tagges |
| Tests | **8,968** (0 fail, 0 clippy) |
| LOC | 353k Rust, 12 crates |
| Branch | main |
| Repertoire | `/Users/thibaut/dev/supernovae/nika/tools` |
| Dernier commit | `8ad7c41b7` feat(orchestrate) |

## Providers (env vars disponibles)

| Provider | Key | Infer | Structured | Status |
|----------|-----|-------|-----------|--------|
| OpenAI | OPENAI_API_KEY | PASS | PASS | **OK** |
| Gemini | GEMINI_API_KEY | PASS | FAIL (quota) | **RATE LIMITED** |
| xAI | XAI_API_KEY | PASS | PASS | **OK** |
| Anthropic | ANTHROPIC_API_KEY | FAIL (billing) | FAIL (billing) | **NO CREDITS** |
| Groq | daemon only | PASS | PASS | **OK (daemon)** |
| Mistral | daemon only | PASS | PASS | **OK (daemon)** |
| DeepSeek | daemon only | PASS | PASS | **OK (daemon)** |

## Ce qui a ete fait dans cette session (Sprint 1 + Sprint 2 partiel)

| Commit | Description |
|--------|-------------|
| `cbfb90aaf` | Output scanner wire + panic fix: `scan_output()` maintenant appele apres chaque reponse LLM. `SecurityScanFinding` event cree. `last_error.unwrap()` → `unwrap_or_else` |
| `27925163f` | **ModelResolver**: `nika-core/src/catalogs/resolver.rs` — PROVIDER_DEFAULTS, resolve(), validate(), ModelSource. 11 tests. Wire dans RigProvider, TUI routing, runner compressor |
| `440b40f01` | L0 structured output: `with_original_prompt()` + `with_provider_context()` wires sur les 2 safety-net engines. 200ms retry delay. Fix "unknown" → provider.default_model() |
| `8ad7c41b7` | P-ORCHESTRATE wire (1 ligne dans Runner). Shell alias blocklist. Groq/Gemini/xAI redaction patterns |

## Ce qui reste (3.5 sprints)

### SPRINT 2 RESTANT (3h)

| # | Tache | Effort | Detail |
|---|-------|--------|--------|
| 2.2 | DNS rebinding pin | 2h | Apres `resolve_and_check_ssrf()`, utiliser `reqwest::Client::builder().resolve(host, validated_addr)` dans `executor/fetch.rs` pour empecher TOCTOU. Tester avec un nom d'hote qui resolve vers localhost |
| 2.5 | Structured output aggregate timeout | 30min | Wrapper `engine.validate()` avec `tokio::time::timeout(600s)` dans `structured_output.rs`. L'erreur remonte `StructuredOutputAllLayersFailed` avec message "validation timed out (600s)" |
| 2.6 | MCP tool result size limit | 30min | Dans `executor/invoke.rs`, verifier `result_str.len() > 50MB` avant deserialization. Nouveau `NikaError::McpToolResultTooLarge` si necessaire, sinon utiliser variant existant |

**TDD pour chaque:**
- 2.2: Test qui fait un fetch vers un hostname avec resolve verifiee
- 2.5: Test unitaire qui mock un engine.validate qui ne retourne jamais, verifier timeout
- 2.6: Test qui passe un result de 50MB+1 byte, verifier erreur

**Commits:** 3 commits separes, 1 par tache.

---

### SPRINT 3: "Mock + E2E" (1 jour)

**Objectif:** Rendre possible le test de structured output et des features avancees SANS cles API.

#### 3.1 — Mock Structured Output (3h)

**Probleme actuel:** Le mock provider retourne `{"response": "Mock response from rig agent", "completed": true}` — du texte fixe, pas du JSON conforme au schema. Impossible de tester structured output sans vrais appels API.

**Solution:** Quand la tache a un `structured: { schema: ... }`, generer du JSON valide:

```rust
// Dans nika-engine/src/runtime/rig_agent_loop/providers.rs — run_mock()
// ET dans provider/rig/mod.rs — le mock infer path

fn generate_mock_json(schema: &serde_json::Value) -> serde_json::Value {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("object") => {
            let mut obj = serde_json::Map::new();
            if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                for (key, prop_schema) in props {
                    obj.insert(key.clone(), generate_mock_json(prop_schema));
                }
            }
            serde_json::Value::Object(obj)
        }
        Some("string") => {
            // Respect enum constraint if present
            if let Some(enums) = schema.get("enum").and_then(|e| e.as_array()) {
                enums.first().cloned().unwrap_or(json!("mock"))
            } else {
                json!("mock_string")
            }
        }
        Some("number" | "integer") => {
            let min = schema.get("minimum").and_then(|m| m.as_f64()).unwrap_or(0.0);
            json!(min)
        }
        Some("boolean") => json!(true),
        Some("array") => {
            let item_schema = schema.get("items").cloned().unwrap_or(json!({"type": "string"}));
            let min_items = schema.get("minItems").and_then(|m| m.as_u64()).unwrap_or(1);
            let items: Vec<_> = (0..min_items).map(|_| generate_mock_json(&item_schema)).collect();
            json!(items)
        }
        _ => json!("mock"),
    }
}
```

**Fichiers a modifier:**
1. `nika-engine/src/provider/rig/mod.rs` — dans le mock infer path, detecter structured spec et retourner JSON genere
2. `nika-engine/src/runtime/rig_agent_loop/providers.rs` — `run_mock()` meme chose

**Tests TDD:**
```rust
#[tokio::test]
async fn e2e_mock_structured_output_generates_valid_json() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: extract
    infer: "Describe Alice"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          age: { type: number, minimum: 0 }
          skills: { type: array, items: { type: string }, minItems: 2 }
          active: { type: boolean }
        required: [name, age, skills, active]
"#;
    let result = run_workflow(yaml).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .expect("Mock structured output should be valid JSON");
    assert!(parsed["name"].is_string());
    assert!(parsed["age"].is_number());
    assert!(parsed["skills"].as_array().unwrap().len() >= 2);
    assert!(parsed["active"].is_boolean());
}

#[tokio::test]
async fn e2e_mock_structured_output_respects_enum() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: classify
    infer: "Classify this item"
    structured:
      schema:
        type: object
        properties:
          category: { type: string, enum: ["hardware", "software", "service"] }
        required: [category]
"#;
    let result = run_workflow(yaml).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let cat = parsed["category"].as_str().unwrap();
    assert!(["hardware", "software", "service"].contains(&cat));
}
```

**Commit:** `feat(mock): generate schema-conforming JSON for structured output`

#### 3.2 — Mock Failure Simulation (1h)

**Probleme:** Impossible de tester retry/fallback sans vrais echecs API.

**Solution:** Variable d'env `NIKA_MOCK_FAIL_COUNT=N` — les N premiers appels mock echouent avant de reussir.

**Test TDD:**
```rust
#[tokio::test]
async fn e2e_mock_retry_succeeds_after_failure() {
    std::env::set_var("NIKA_MOCK_FAIL_COUNT", "2");
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: resilient
    retry:
      max_attempts: 3
      delay_ms: 50
    infer: "Generate something"
"#;
    let result = run_workflow(yaml).await;
    assert!(result.is_ok(), "Should succeed after 2 failures: {result:?}");
    std::env::remove_var("NIKA_MOCK_FAIL_COUNT");
}
```

**Commit:** `feat(mock): NIKA_MOCK_FAIL_COUNT for retry/fallback testing`

#### 3.3 — Vision E2E Test (1h)

**Prerequis:** OPENAI_API_KEY disponible.

**Test:** Envoyer une image reelle a OpenAI, verifier la description:

```rust
#[tokio::test]
async fn e2e_vision_openai_describes_image() {
    if std::env::var("OPENAI_API_KEY").is_err() { return; }

    let yaml = r#"
schema: "nika/workflow@0.12"
provider: openai
model: gpt-4o
tasks:
  - id: import_img
    invoke:
      tool: "nika:import"
      params:
        path: "tests/fixtures/test-image.png"
  - id: describe
    depends_on: [import_img]
    with: { img: $import_img }
    provider: openai
    model: gpt-4o
    infer:
      content:
        - type: image
          source: "{{with.img.hash}}"
        - type: text
          text: "Describe this image in one sentence"
"#;
    let result = run_workflow(yaml).await;
    assert!(result.is_ok(), "Vision should work: {result:?}");
    let output = result.unwrap();
    assert!(!output.is_empty(), "Should describe the image");
}
```

**Prerequis:** Creer `tests/fixtures/test-image.png` (un petit PNG 1x1 pixel suffit).

**Commit:** `test(e2e): vision multimodal with real OpenAI API`

#### 3.4 — Artifact E2E Test (1h)

```rust
#[tokio::test]
async fn e2e_artifact_creates_file_on_disk() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let artifact_dir = tmp_dir.path().join(".nika/artifacts");

    let yaml = format!(r#"
schema: "nika/workflow@0.12"
provider: mock
artifacts:
  dir: "{}"
tasks:
  - id: generate
    infer: "Generate a report"
    artifact:
      path: "report.txt"
      format: text
"#, artifact_dir.display());

    let workflow = parse_analyzed(&yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap().quiet()
        .with_base_path(tmp_dir.path().to_path_buf());
    runner.run().await.unwrap();

    let report_path = artifact_dir.join("report.txt");
    assert!(report_path.exists(), "Artifact file should exist on disk");
    let content = std::fs::read_to_string(&report_path).unwrap();
    assert!(!content.is_empty(), "Artifact should have content");
}
```

**Commit:** `test(e2e): artifact writing verification on disk`

#### 3.5 — Agent Guardrails E2E (1h)

```rust
#[tokio::test]
async fn e2e_agent_guardrail_length_rejects_long_output() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: constrained_agent
    agent:
      prompt: "Write a very long essay about everything"
      max_turns: 1
      guardrails:
        - type: length
          max_words: 5
          on_failure: fail
"#;
    let result = run_workflow(yaml).await;
    // Mock returns fixed text > 5 words → guardrail should trigger
    assert!(result.is_err() || result.unwrap().split_whitespace().count() <= 5);
}
```

**Commit:** `test(e2e): agent guardrail length enforcement`

#### 3.6 — Provider Fallback E2E (1h)

```rust
#[tokio::test]
async fn e2e_provider_fallback_chain() {
    if std::env::var("OPENAI_API_KEY").is_err() { return; }

    let yaml = r#"
schema: "nika/workflow@0.12"
routing:
  fallback: [nonexistent_provider, openai]
model: gpt-4.1-mini
tasks:
  - id: test
    infer: "Say NIKA"
"#;
    let result = run_workflow(yaml).await;
    assert!(result.is_ok(), "Fallback to openai should succeed: {result:?}");
}
```

**Commit:** `test(e2e): provider fallback chain with real API`

#### 3.7 — Retry E2E (30min)

```rust
#[tokio::test]
async fn e2e_retry_with_mock_failure() {
    // Use NIKA_MOCK_FAIL_COUNT from 3.2
    unsafe { std::env::set_var("NIKA_MOCK_FAIL_COUNT", "1"); }
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: retry_task
    retry:
      max_attempts: 3
      delay_ms: 50
      backoff: 2.0
    infer: "Generate data"
"#;
    let result = run_workflow(yaml).await;
    assert!(result.is_ok(), "Retry should succeed after 1 failure");
    unsafe { std::env::remove_var("NIKA_MOCK_FAIL_COUNT"); }
}
```

**Commit:** `test(e2e): retry with exponential backoff via mock failure`

---

### SPRINT 4: "Performance + Mass Validation" (1 jour)

#### 4.1 — Value Clone Elimination (2h)

**Probleme:** `get_resolved()` dans `binding/resolve.rs:328` clone `serde_json::Value` pour CHAQUE lookup d'alias eager. Pour un `for_each` de 50 items avec 3 templates = 150 deep clones inutiles.

**Fix:** Ajouter `get_ref()` pour emprunter sans cloner:

```rust
// binding/resolve.rs
pub fn get_ref(&self, alias: &str) -> Option<&Value> {
    match self.bindings.get(alias)? {
        LazyBinding::Resolved(v) => Some(v),
        _ => None,
    }
}
```

Puis dans `template.rs`, utiliser `get_ref()` pour les eager bindings:
```rust
let base_value_owned;
let base_value: &Value = if let Some(v) = bindings.get_ref(alias) {
    v  // zero-cost borrow for eager bindings
} else {
    base_value_owned = bindings.get_resolved(alias, datastore)?;
    &base_value_owned  // clone only for lazy bindings
};
```

**Test:** Benchmark avec `for_each` de 100 items — mesurer temps avant/apres.

**Commit:** `perf(binding): zero-clone for eager binding lookups`

#### 4.2 — TransformExpr Pre-Parsing (2h)

**Probleme:** `TransformExpr::parse()` appele a chaque match de template dans `for_each`. La meme expression "upper | trim" parsee N fois.

**Fix:** Parser les transforms une seule fois dans `parse_template_expr()`:
```rust
// AVANT:
pub enum TemplateExpr {
    Alias { path: String, transforms: Vec<String> },
}

// APRES:
pub enum TemplateExpr {
    Alias { path: String, transforms: Option<TransformExpr> },
}
```

**Attention:** Verifier que `TransformExpr` est `Clone` (necessaire pour le enum). Si pas Clone, utiliser `Arc<TransformExpr>`.

**Test:** Verifier que les transforms fonctionnent toujours via les tests existants (regression).

**Commit:** `perf(template): pre-parse transform expressions in AST`

#### 4.3 — DAG compute_depths Kahn's (1h)

**Fichier:** `dag/flow.rs:267-318`

**Probleme:** `compute_depths()` utilise `while !remaining` iteratif O(V^2). `compute_layers()` utilise deja Kahn's O(V+E).

**Fix:** Implementer Kahn's BFS topological sort comme `compute_layers()`:
```rust
fn compute_depths(&self) -> FxHashMap<Arc<str>, usize> {
    let mut depths = FxHashMap::default();
    let mut in_degree: FxHashMap<&str, usize> = FxHashMap::default();
    let mut queue: VecDeque<Arc<str>> = VecDeque::new();

    for id in &self.task_ids {
        let preds = self.get_dependencies(id.as_ref());
        in_degree.insert(id.as_ref(), preds.len());
        if preds.is_empty() {
            depths.insert(Arc::clone(id), 0);
            queue.push_back(Arc::clone(id));
        }
    }

    while let Some(node) = queue.pop_front() {
        let node_depth = depths[&node];
        for succ in self.get_successors(node.as_ref()) {
            let new_depth = node_depth + 1;
            let entry = depths.entry(Arc::clone(succ)).or_insert(0);
            *entry = (*entry).max(new_depth);
            let deg = in_degree.get_mut(succ.as_ref()).unwrap();
            *deg -= 1;
            if *deg == 0 {
                queue.push_back(Arc::clone(succ));
            }
        }
    }
    depths
}
```

**Test:** Verifier que `compute_depths()` retourne les memes valeurs qu'avant (regression).

**Commit:** `perf(dag): compute_depths uses Kahn's algorithm O(V+E)`

#### 4.4 — Run 502 Example Workflows (4h)

**C'est LE test de realite.** Script automatise:

```bash
#!/bin/bash
# run-all-examples.sh
PASS=0
FAIL=0
SKIP=0
ERRORS=""

for f in examples/gates/feature/*.nika.yaml examples/gates/complex/*.nika.yaml examples/dag-patterns/*.nika.yaml; do
  result=$(cargo run --bin nika --release -- run "$f" --provider mock --no-live 2>&1 | tail -1)
  if echo "$result" | grep -q "DONE"; then
    PASS=$((PASS + 1))
  elif echo "$result" | grep -q "NIKA-"; then
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS\n$f: $result"
  else
    SKIP=$((SKIP + 1))
  fi
done

echo "Results: $PASS pass, $FAIL fail, $SKIP skip"
echo -e "$ERRORS"
```

**Pour chaque echec:**
1. Lire le workflow YAML
2. Determiner si c'est un bug YAML (fixer le workflow) ou un bug ENGINE (fixer le code)
3. Fixer avec TDD
4. Commit: `fix(parser|runtime): <description>` ou `fix(examples): <workflow> <description>`

**Objectif:** 400+/502 passent (80%+). Les echecs restants sont soit des workflows qui necessitent des inputs specifiques, soit des features non supportees en mock.

---

### SPRINT 5: "Release v0.53.0" (demi-jour)

#### 5.1 — Version Bump

```bash
# Dans tools/Cargo.toml, remplacer TOUTES les occurrences:
sed -i '' 's/0.52.0/0.53.0/g' tools/Cargo.toml
```

**Verifier:** `cargo check --workspace` compile sans erreur.

#### 5.2 — CHANGELOG

Ajouter section `[0.53.0] — 2026-03-31` dans CHANGELOG.md avec:
- **Added:** ModelResolver, SecurityScanFinding event, P-ORCHESTRATE wiring, mock structured output
- **Fixed:** Empty provider chain panic, L0 safety-net context, structured output retry delay, "unknown" model, shell alias bypass, API key redaction
- **Changed:** RigProvider::default_model() delegates to resolver, TUI routing uses catalog
- **Performance:** Value clone elimination, TransformExpr pre-parsing, DAG Kahn's algorithm
- **Security:** output_scanner wired, DNS rebinding pin, alias/function blocked, MCP size limit

#### 5.3 — Final Verification

```bash
# TOUT doit passer:
cargo test --workspace --lib                                    # 9200+ tests
cargo clippy --workspace --all-targets --all-features -- -D warnings  # 0 warnings
cargo test --test e2e_workflow_test                              # 40+ E2E tests
cargo test --test e2e_workflow_test -- e2e_structured_openai    # Real structured output
cargo test --test e2e_workflow_test -- e2e_real_openai_mini     # Real provider
```

#### 5.4 — Code Review Agent

```
Avant le tag final, lancer l'agent code-reviewer sur TOUS les fichiers modifies:

1. git diff v0.52.0..HEAD --name-only | grep "\.rs$" → liste des fichiers
2. Pour chaque fichier: verifier coherence, securite, tests
3. Verifier que chaque commit a des co-authors
4. Verifier que Cargo.lock est a jour
```

#### 5.5 — Tag + Push

```bash
git tag v0.53.0
git push && git push --tags
```

---

# REGLES STRICTES

```
1. cargo test --workspace --lib AVANT chaque commit (--lib = pas de keychain popup)
2. cargo clippy --workspace --all-targets --all-features -- -D warnings AVANT chaque commit
3. TDD: ecrire le test ROUGE d'abord → fix → test VERT → commit
4. 1 fix = 1 commit (sauf si vraiment couple)
5. Conventional commits: type(scope): description
6. Co-authors OBLIGATOIRES:
   Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
7. Push toutes les 2-3 commits
8. Si bloque 3x sur le meme probleme → skip + note dans CHANGELOG
9. JAMAIS marquer done sans test qui prouve que ca marche
10. Lire master-plan-v2-definitive.md pour le contexte complet
11. Les tests structured output doivent:
    - Utiliser un prompt NATUREL (JAMAIS mentionner JSON)
    - Valider PROGRAMMATIQUEMENT (type, enum, range, required)
    - Fonctionner sur le mock ET sur les vrais providers
12. Les tests E2E doivent capturer les events pour le diagnostic
    (utiliser EventLog::new_with_broadcast + event loop pattern)
```

# VERIFICATION INITIALE

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
```

# METRIQUES DE SUCCES

| Metrique | Actuel | Cible v0.53 |
|----------|--------|-------------|
| Tests lib | 8,968 | **9,300+** |
| Tests E2E | 40 | **55+** |
| Panics production | 0 | **0** |
| Model routing issues | 0 (resolved) | **0** |
| Structured output L0 context | complete | **complete** |
| output_scanner | wired | **wired** |
| P-ORCHESTRATE | wired | **tested E2E** |
| Security findings | 4 open | **1** (SEC-05 known) |
| Mock structured output | NO | **YES** |
| Vision E2E | NO | **YES** |
| Artifact E2E | NO | **YES** |
| Retry E2E | NO | **YES** |
| Fallback E2E | NO | **YES** |
| Example workflows run | 0/502 | **400+/502** |
| Perf: Value clones | O(N*M) | **O(lazy only)** |

# FICHIERS CLE

| Fichier | Role |
|---------|------|
| `nika-core/src/catalogs/resolver.rs` | ModelResolver — source de verite modeles |
| `nika-engine/src/runtime/executor/infer.rs` | Infer executor — L0a/L0b/streaming |
| `nika-engine/src/runtime/executor/mod.rs` | Task dispatch + output scanner wiring |
| `nika-engine/src/runtime/structured_output.rs` | 5-layer structured output engine |
| `nika-engine/src/runtime/runner.rs` | DAG runner + retry + orchestrate wiring |
| `nika-engine/src/runtime/output_scanner.rs` | LLM injection scanner |
| `nika-engine/src/runtime/orchestrate.rs` | wrap_as_orchestrator() |
| `nika-engine/src/runtime/security.rs` | Command blocklist |
| `nika-engine/src/runtime/executor/fetch.rs` | Fetch + SSRF + DNS rebinding |
| `nika-engine/src/provider/rig/mod.rs` | Provider abstraction + default_model |
| `nika-engine/src/binding/resolve.rs` | Binding resolution + get_resolved |
| `nika-engine/src/binding/template.rs` | Template engine + transforms |
| `nika-engine/src/dag/flow.rs` | DAG computation + compute_depths |
| `nika/tests/e2e_workflow_test.rs` | E2E tests (mock + real providers) |
| `docs/plans/master-plan-v2-definitive.md` | Plan detaille avec 10-agent audit |

# ARCHITECTURE NIKA

```
                    .nika.yaml
                        |
                        v
              Parse (nika-core/ast/raw)
                        |
                        v
              Analyze (nika-core/ast/analyzer)
                        |
                  AnalyzedWorkflow
                        |
         if goal: → wrap_as_orchestrator()
                        |
                        v
                 Runner::with_event_log()
                        |
                   DAG construction
                        |
                        v
              ┌────────────────────┐
              │  execute_single_task │ × N tasks (parallel when deps met)
              │         |           │
              │    retry loop       │ ← max_attempts, backoff
              │         |           │
              │  execute_with_routing │ ← fallback chain
              │         |           │
              │     execute()       │
              │         |           │
              │  ┌──────┼──────┐   │
              │ infer  exec fetch  │
              │  |     invoke agent │
              │  |                  │
              │  5-Layer Structured │
              │  L0a: response_format │ (OpenAI/Groq/DeepSeek/xAI)
              │  L0b: tool_choice   │ (Anthropic/Mistral/Gemini)
              │  L2: extract+validate │
              │  L3: retry+feedback │
              │  L4: LLM repair    │
              │         |           │
              │  output_scanner()  │ ← NEW: injection audit
              │         |           │
              │  store in RunContext │
              └────────────────────┘
                        |
                        v
                  WorkflowCompleted
                  + artifact writing
                  + record compression
                  + NDJSON trace
```

# STRUCTURED OUTPUT — FAIT VERIFIE

Le structured output est du **VRAI tool injection natif**, PAS du prompt engineering:
- **L0a** (OpenAI, Groq, DeepSeek, xAI): `response_format: { type: "json_schema" }` dans `additional_params`
- **L0b** (Anthropic, Mistral, Gemini): `DynamicSubmitTool` + `tool_choice: Required` au niveau API
- Le schema est envoye au PROVIDER, pas injecte dans le prompt utilisateur
- Les tests valident 5/7 providers (Anthropic: billing, Gemini: quota)

# CONTEXT WINDOW HANDOFF

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v053-final.md)"
```

# GO

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
# SPRINT 2 (remaining) → SPRINT 3 → SPRINT 4 → SPRINT 5 → v0.53.0
```
