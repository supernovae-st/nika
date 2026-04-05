Tu es l'orchestrateur autonome du projet **Nika** — un workflow engine YAML semantique pour l'IA (5 verbs, 9 providers, 39 builtin tools, 353k LOC Rust, 12 crates). Tu travailles sans intervention humaine. Commit, push, continue.

**IMPORTANT**: Ce prompt est base sur les findings de **31 agents specialises** (11 Opus + 20 Haiku) qui ont audite chaque aspect du codebase. Les chiffres sont VERIFIES, pas estimes.

---

# IDENTITE

| Cle | Valeur |
|-----|--------|
| Projet | Nika v0.51.0 → **v0.52.0** |
| LOC | 353,250 Rust, 12 crates |
| Tests | 8,888 (0 fail, 0 clippy) |
| Commits | 124 |
| Phase 1 | **100% COMPLETE** |
| Repertoire | `/Users/thibaut/dev/supernovae/nika/tools` |

# VERIFICATION INITIALE

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
```

---

# SKILLS OBLIGATOIRES

```
test-driven-development     → RED-GREEN-REFACTOR pour chaque fix
systematic-debugging        → Root cause AVANT fix
verification-before-completion → cargo test + clippy AVANT commit
dispatching-parallel-agents → Agents paralleles pour Wave 3
```

---

# PHILOSOPHIE: TEST → CASSE → FIX → VERIFIE

```
POUR CHAQUE PROVIDER (anthropic, openai, gemini, groq, mistral, deepseek, xai):
  1. Ecrire un workflow avec structured output (JSON schema)
  2. EXECUTER le workflow avec `nika run`
  3. Si ca CASSE → debugger, trouver le root cause dans le code engine
  4. FIXER le code (pas le test!) avec TDD
  5. RE-EXECUTER → verifier que ca passe
  6. Commit le fix + le test

ZERO FAUX POSITIF:
  - Si un test passe mais le output est mauvais → le test est FAUX → le fixer
  - Si un test echoue mais c'est un bug engine → fixer l'ENGINE
  - Si structured output echoue → traverser les 5 layers pour trouver ou ca casse
  - Si un provider timeout → verifier timeout config, pas just skip

LES 5 COUCHES STRUCTURED OUTPUT doivent TOUTES etre testees:
  Layer 0: Tool injection / response_format (provider-natif)
  Layer 2: Extract + Validate (post-processing JSON)
  Layer 3: Retry with Feedback (re-prompt avec erreurs)
  Layer 4: LLM Repair (cheap model repare le JSON)

  TOUS les providers DOIVENT produire du JSON valide. Pas d'exception.
  Le chemin interne differe (Layer 0 natif vs Layer 2-4 repair) mais
  le RESULTAT est identique: JSON conforme au schema.
  Si un provider echoue → c'est un BUG ENGINE a fixer, pas une limitation.

REGLE ABSOLUE STRUCTURED OUTPUT:
  Le prompt utilisateur ne doit JAMAIS mentionner le format JSON ou le schema.
  Le systeme de 5 layers s'en charge AUTOMATIQUEMENT.

  INTERDIT dans le prompt:
  ❌ "Reponds en JSON"
  ❌ "Le format attendu est: {name: string, age: number}"
  ❌ "Return a JSON object with fields..."
  ❌ "Output format: JSON"

  CORRECT — le prompt est NATUREL, le schema est dans structured::
  ✅ prompt: "Decris Alice, 30 ans, developpeuse Rust"
     structured:
       schema:
         type: object
         properties:
           name: { type: string }
           age: { type: number }
           skills: { type: array, items: { type: string } }
         required: [name, age, skills]

  C'est Layer 0 (tool injection) qui injecte le schema AUTOMATIQUEMENT.
  Si Layer 0 echoue → Layer 2 extrait le JSON → Layer 3 retry → Layer 4 repair.
  L'utilisateur ecrit un prompt NATUREL. Le systeme fait le reste.

  TESTER: prompt naturel + schema complexe sur CHAQUE provider.
  Si un provider ne produit pas du JSON valide → c'est un BUG ENGINE a fixer.
  Si le test met le schema dans le prompt → c'est un BUG TEST a fixer.
```

# 4 WAVES (15h total — donnees verifiees par 31 agents)

## WAVE 1: VRAIS BUGS (3h, 15 fixes chirurgicaux)

### 1A — Security (4 fixes)

| # | Bug | File:Line | Fix exact |
|---|-----|-----------|-----------|
| SEC-1 | IPv4-compatible IPv6 `::127.0.0.1` bypasses SSRF | `policy.rs:46-68` | Add `v6.to_ipv4()` check alongside `to_ipv4_mapped()` |
| SEC-2 | `/usr/bin/sudo` bypasses blocklist | `security.rs:28-137` | Extract basename from first token: `Path::new(token).file_name()` before match |
| SEC-3 | Symlink artifact escape | `io/writer.rs:229,311` | Treat `canonicalize()` Err as `ArtifactPathError` (not silent skip) |
| SEC-4 | canonicalize failure skips symlink check | `io/writer.rs:229` | `if let Ok(c) = parent.canonicalize()` → return Err on failure |

### 1B — High Bugs (6 fixes)

| # | Bug | File:Line | Fix exact |
|---|-----|-----------|-----------|
| BUG-1 | SpawnAgentTool disconnected CancellationToken | `rig_agent_loop/mod.rs:263` | Replace `CancellationToken::new()` with parent `cancel_token.child_token()` |
| BUG-2 | HashMap `depths[key]` can panic | `runner.rs:1481,1504` | `.get(key).copied().unwrap_or(0)` |
| BUG-3 | `println!` not guarded by `!self.quiet` | `runner.rs:1497-1499` | Wrap in `if !self.quiet { ... }` |
| BUG-4 | 9x streaming `try_send` silently drops chunks | `streaming.rs:128,148,160,176,481,504,536,569,593` | `if let Err(e) = tx.try_send(chunk) { tracing::debug!("stream send: {e}"); }` |
| BUG-5 | exit_code None treated as success (0) | `tui/widgets/task_box/exec.rs:326` | Use match on Option, not unwrap_or(0) |
| BUG-6 | size_bytes missing → silent 0 | `executor/invoke.rs:125` | Add `tracing::warn!` before unwrap_or(0) |

### 1C — Error Handling (5 fixes)

| # | Bug | File:Line | Fix exact |
|---|-----|-----------|-----------|
| ERR-1 | Workflow output defaults to "" silently | `runner.rs:2570` | `tracing::warn!("no final output")` before unwrap_or_default |
| ERR-2 | Structured output layers discard prev errors | `structured_output.rs:318,346,365` | Accumulate errors in Vec, include in NIKA-300 |
| ERR-3 | 5 JSONPath errors at debug! (should be warn!) | `run_context.rs:420,488,506,523,605` | Change to `tracing::warn!` |
| ERR-4 | Template malformed {{...}} silently passes through | `template.rs:474,1100` | Add `tracing::warn!("malformed template expression")` |
| ERR-5 | Binding NullInput catches ALL transform errors | `resolve.rs:571` | Match specifically on `TransformError::NullInput`, propagate others |

### 1D — Dead Code (3 deletions)

| # | What | File:Line | Action |
|---|------|-----------|--------|
| DEAD-1 | RecordSpec.retain field (parsed, never consumed) | `nika-core/ast/record.rs:28` | Delete field + tests |
| DEAD-2 | artifact_paths field (populated, never read) | `runner.rs:148` | Delete field + 8 population sites |
| DEAD-3 | RetryCondition enum (zero consumers) | `nika-core/ast/routing.rs:38` | Delete enum + re-export |

### 1E — Daemon + secrets architecture cleanup (1h)

Le daemon auto-start est implemente mais il faut:

1. **Supprimer le code legacy keyring** non utilise:
   - `keyring.rs`: les fonctions `NikaKeyring::get()`, `set()`, `delete()` restent pour le daemon
   - MAIS: `nika keys set` (CLI) doit stocker via daemon IPC, PAS keyring direct
   - Verifier que `nika keys set anthropic` passe par le daemon

2. **Tests daemon auto-start** (4 tests):
```rust
#[tokio::test]
async fn test_daemon_auto_starts_when_missing() {
    // Verify: daemon_available()=false → auto-fork → daemon_available()=true
}

#[tokio::test]
async fn test_daemon_not_started_if_nika_no_daemon() {
    // Set NIKA_NO_DAEMON=1 → verify daemon NOT started
}

#[tokio::test]
async fn test_secrets_from_env_without_daemon() {
    // Set env var → verify secret found without daemon
}

#[tokio::test]
async fn test_secrets_fallback_clear_error() {
    // No env, no daemon → verify None returned (not panic, not popup)
}
```

3. **Nettoyer `nika provider list`**: ne JAMAIS toucher au keychain directement
   - Lister les providers depuis: env vars + daemon IPC uniquement
   - Afficher "(env)" ou "(daemon)" a cote de chaque provider

4. **Nettoyer les references legacy**:
   - Supprimer `NIKA_SKIP_KEYCHAIN` (plus necessaire, keyring direct supprime)
   - Supprimer `NIKA_KEYCHAIN_BOOT` references
   - Mettre a jour la doc: "API keys via env vars or daemon"

**Commits**:
```
test(daemon): add 4 auto-start + secrets resolution tests
refactor(secrets): clean provider list — env + daemon only, no keyring
docs: update secrets documentation — env vars or daemon, no direct keychain
```

### 1F — MSRV fix (1 line)

```
File: tools/Cargo.toml
Fix: rust-version = "1.86" → rust-version = "1.94"
Reason: div_ceil used 23x, stabilized in 1.94
```

**Wave 1 commits** (~6):
```
fix(security): IPv6 SSRF + path bypass + symlink + canonicalize
fix(runtime): SpawnAgent cancel token + HashMap panic + quiet guard
fix(runtime): streaming try_send logging + exit_code + size_bytes
fix(quality): error handling — warn for user errors, accumulate structured layers
refactor(core): remove 3 dead code items (retain, artifact_paths, RetryCondition)
chore: bump MSRV to 1.94 (div_ceil requirement)
```

---

## WAVE 2: E2E WORKFLOW TESTS — RUN + FIX + VERIFY (8h)

### 2A — Mock E2E workflows (3h)

Creer `tools/nika-engine/tests/e2e_workflows.rs` (ou enrichir existant):

```rust
// Test 1: Simple infer pipeline
#[tokio::test]
async fn test_e2e_simple_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: greet
    infer: "Say hello"
"#;
    let wf = parse_and_analyze(yaml);
    let mut runner = Runner::new(wf).unwrap();
    let result = runner.run().await;
    assert!(result.is_ok(), "Simple infer should succeed: {result:?}");
}

// Test 2: Fan-out / fan-in
#[tokio::test]
async fn test_e2e_fan_out_fan_in() { ... }

// Test 3: for_each with concurrency
#[tokio::test]
async fn test_e2e_for_each_concurrent() { ... }

// Test 4: Retry on failure
#[tokio::test]
async fn test_e2e_retry_succeeds_second_attempt() { ... }

// Test 5: Structured output with schema
#[tokio::test]
async fn test_e2e_structured_output_valid_json() { ... }

// Test 6: context_budget enforcement
#[tokio::test]
async fn test_e2e_context_budget_truncates() { ... }

// Test 7: Agent with tools (mock)
#[tokio::test]
async fn test_e2e_agent_with_builtin_tools() { ... }

// Test 8: Record compression
#[tokio::test]
async fn test_e2e_record_compress_creates_record() { ... }

// Test 9: Depends_on ordering
#[tokio::test]
async fn test_e2e_depends_on_ordering() { ... }

// Test 10: Workflow with context files
#[tokio::test]
async fn test_e2e_context_files_loaded() { ... }
```

### 2B — Real API tests: TOUS les providers (3h, budget illimite)

**IMPORTANT**: Tester CHAQUE provider avec un VRAI appel API. Utiliser les modeles rapides/cheap.
Toutes les cles API sont disponibles en env vars. Ne PAS skipper si la cle existe.

Creer `tools/nika-engine/tests/e2e_real_api.rs`:

```rust
macro_rules! provider_test {
    ($name:ident, $provider:expr, $model:expr, $env_key:expr) => {
        #[tokio::test]
        async fn $name() {
            if std::env::var($env_key).is_err() {
                eprintln!("Skipping {}: no {}", stringify!($name), $env_key);
                return;
            }
            let yaml = format!(r#"
schema: "nika/workflow@0.12"
provider: {}
model: {}
tasks:
  - id: hello
    infer: "Reply with exactly one word: NIKA"
"#, $provider, $model);
            let result = run_workflow(&yaml).await;
            assert!(result.is_ok(), "{} failed: {:?}", $provider, result.err());
            let output = result.unwrap();
            assert!(!output.is_empty(), "{} returned empty output", $provider);
        }
    };
}

// === TEST EVERY SINGLE PROVIDER ===

// Anthropic — Claude
provider_test!(test_anthropic_haiku, "anthropic", "claude-haiku-4-5", "ANTHROPIC_API_KEY");
provider_test!(test_anthropic_sonnet, "anthropic", "claude-sonnet-4-20250514", "ANTHROPIC_API_KEY");

// OpenAI — GPT
provider_test!(test_openai_mini, "openai", "gpt-4.1-mini", "OPENAI_API_KEY");
provider_test!(test_openai_gpt4, "openai", "gpt-4.1", "OPENAI_API_KEY");

// Google Gemini
provider_test!(test_gemini_flash, "gemini", "gemini-2.5-flash", "GEMINI_API_KEY");
provider_test!(test_gemini_pro, "gemini", "gemini-2.5-pro", "GEMINI_API_KEY");

// Groq — ultra fast
provider_test!(test_groq_llama, "groq", "llama-3.3-70b-versatile", "GROQ_API_KEY");
provider_test!(test_groq_mixtral, "groq", "mixtral-8x7b-32768", "GROQ_API_KEY");

// Mistral
provider_test!(test_mistral_small, "mistral", "mistral-small-latest", "MISTRAL_API_KEY");
provider_test!(test_mistral_large, "mistral", "mistral-large-latest", "MISTRAL_API_KEY");

// DeepSeek
provider_test!(test_deepseek_chat, "deepseek", "deepseek-chat", "DEEPSEEK_API_KEY");

// xAI — Grok
provider_test!(test_xai_grok, "xai", "grok-3", "XAI_API_KEY");

// === STRUCTURED OUTPUT: VALIDATION PROGRAMMATIQUE PAR PROVIDER ===
//
// ARCHITECTURE DU TEST:
// 1. Prompt NATUREL (jamais mentionner JSON)
// 2. Schema complexe (nested, enums, arrays, constraints)
// 3. Executer le workflow
// 4. Parser le JSON output
// 5. Valider CHAQUE champ contre le schema (type + constraints)
// 6. Verifier QUEL layer a reussi (via EventLog)
// 7. Si echec → c'est un BUG ENGINE, pas un "provider limitation"

const STRUCTURED_SCHEMA: &str = r#"
type: object
properties:
  person:
    type: object
    properties:
      name: { type: string }
      age: { type: number, minimum: 0, maximum: 150 }
      role: { type: string, enum: ["developer", "designer", "manager"] }
    required: [name, age, role]
  skills: { type: array, items: { type: string }, minItems: 2 }
  active: { type: boolean }
required: [person, skills, active]
"#;

// Prompt NATUREL — ne mentionne JAMAIS JSON, schema, format, structure
const NATURAL_PROMPT: &str = "Parle-moi d'Alice, 30 ans, developpeuse senior qui maitrise Rust, Python et TypeScript";

fn validate_structured_output(output: &str, provider: &str) {
    // 1. Must be valid JSON
    let parsed: serde_json::Value = serde_json::from_str(output)
        .unwrap_or_else(|e| panic!("{provider}: output is not valid JSON: {e}\nRaw: {output}"));

    // 2. Required top-level fields exist
    assert!(parsed.get("person").is_some(), "{provider}: missing 'person' field");
    assert!(parsed.get("skills").is_some(), "{provider}: missing 'skills' field");
    assert!(parsed.get("active").is_some(), "{provider}: missing 'active' field");

    // 3. Nested object validation
    let person = &parsed["person"];
    assert!(person["name"].is_string(), "{provider}: person.name is not string");
    assert!(person["age"].is_number(), "{provider}: person.age is not number");
    let age = person["age"].as_f64().unwrap();
    assert!((0.0..=150.0).contains(&age), "{provider}: age {age} out of range 0-150");

    // 4. Enum validation
    let role = person["role"].as_str().unwrap_or("");
    assert!(
        ["developer", "designer", "manager"].contains(&role),
        "{provider}: role '{role}' not in enum"
    );

    // 5. Array with minItems
    let skills = parsed["skills"].as_array()
        .unwrap_or_else(|| panic!("{provider}: skills is not an array"));
    assert!(skills.len() >= 2, "{provider}: skills has {} items, need >= 2", skills.len());

    // 6. Boolean
    assert!(parsed["active"].is_boolean(), "{provider}: active is not boolean");
}

macro_rules! structured_test {
    ($name:ident, $provider:expr, $model:expr, $env_key:expr) => {
        #[tokio::test]
        async fn $name() {
            if std::env::var($env_key).is_err() {
                eprintln!("Skip {}: no {}", stringify!($name), $env_key);
                return;
            }
            let yaml = format!(r#"
schema: "nika/workflow@0.12"
provider: {provider}
model: {model}
tasks:
  - id: extract
    infer: "{prompt}"
    structured:
      schema:
{schema}
      enable_repair: true
      max_retries: 3
"#, provider=$provider, model=$model, prompt=NATURAL_PROMPT,
    schema=STRUCTURED_SCHEMA.lines().map(|l| format!("        {l}")).collect::<Vec<_>>().join("\n"));

            let (output, events) = run_workflow_with_events(&yaml).await
                .unwrap_or_else(|e| panic!("{} workflow failed: {e}", $provider));

            // Validate output programmatically
            validate_structured_output(&output, $provider);

            // Verify which layer succeeded (observability)
            let layer_events: Vec<_> = events.iter()
                .filter(|e| matches!(&e.kind, EventKind::StructuredOutputSuccess { .. }))
                .collect();
            assert!(!layer_events.is_empty(),
                "{}: no StructuredOutputSuccess event — 5-layer system did not activate", $provider);
        }
    };
}

// Test CHAQUE provider avec le MEME schema
structured_test!(test_structured_anthropic, "anthropic", "claude-haiku-4-5", "ANTHROPIC_API_KEY");
structured_test!(test_structured_openai, "openai", "gpt-4.1-mini", "OPENAI_API_KEY");
structured_test!(test_structured_gemini, "gemini", "gemini-2.5-flash", "GEMINI_API_KEY");
structured_test!(test_structured_groq, "groq", "llama-3.3-70b-versatile", "GROQ_API_KEY");
structured_test!(test_structured_mistral, "mistral", "mistral-small-latest", "MISTRAL_API_KEY");
structured_test!(test_structured_deepseek, "deepseek", "deepseek-chat", "DEEPSEEK_API_KEY");
structured_test!(test_structured_xai, "xai", "grok-3", "XAI_API_KEY");

// === FETCH (no API key needed) ===

#[tokio::test]
async fn test_fetch_markdown() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: markdown
"#;
    let result = run_workflow(yaml).await.unwrap();
    assert!(result.contains("Example Domain"), "Should extract content");
}

#[tokio::test]
async fn test_fetch_metadata() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: meta
    fetch:
      url: "https://example.com"
      extract: metadata
"#;
    let result = run_workflow(yaml).await.unwrap();
    assert!(result.contains("title"), "Should have title in metadata");
}

// === MULTI-STEP REAL PIPELINES ===

#[tokio::test]
async fn test_real_research_pipeline_anthropic() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() { return; }
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: anthropic
model: claude-haiku-4-5
tasks:
  - id: research
    infer: "List 3 interesting facts about the Rust programming language"
  - id: summarize
    depends_on: [research]
    with: { data: $research }
    infer: "Summarize in exactly one sentence: {{with.data}}"
  - id: format
    depends_on: [summarize]
    with: { summary: $summarize }
    infer: "Add a markdown heading '## Summary' before: {{with.summary}}"
"#;
    let result = run_workflow(yaml).await.unwrap();
    assert!(result.contains("Summary") || result.contains("summary"), "Should have heading");
}

#[tokio::test]
async fn test_real_scrape_and_summarize() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() { return; }
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: anthropic
model: claude-haiku-4-5
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: article
  - id: summarize
    depends_on: [scrape]
    with: { content: $scrape }
    infer: "Summarize this webpage in 2 sentences: {{with.content}}"
"#;
    let result = run_workflow(yaml).await.unwrap();
    assert!(!result.is_empty(), "Pipeline should produce summary");
}

// === PROVIDER FALLBACK ===

#[tokio::test]
async fn test_provider_fallback_chain() {
    if std::env::var("GROQ_API_KEY").is_err() || std::env::var("ANTHROPIC_API_KEY").is_err() { return; }
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: [groq, anthropic]
model: llama-3.3-70b-versatile
tasks:
  - id: test
    infer: "Say NIKA"
"#;
    let result = run_workflow(yaml).await.unwrap();
    assert!(!result.is_empty(), "Fallback should work");
}
```

### 2C — Structured output par provider: RUN + DEBUG + FIX (2h)

L'objectif N'EST PAS juste d'ecrire des tests. C'est de les EXECUTER et de FIXER ce qui casse.

```bash
# Pour CHAQUE provider disponible, executer:
nika run test-structured-anthropic.nika.yaml
nika run test-structured-openai.nika.yaml
nika run test-structured-gemini.nika.yaml
nika run test-structured-groq.nika.yaml      # CRITIQUE: pas de Layer 0
nika run test-structured-mistral.nika.yaml
nika run test-structured-deepseek.nika.yaml
nika run test-structured-xai.nika.yaml
```

Si un workflow ECHOUE:
1. Lire le trace: `nika trace show <generation_id>`
2. Identifier la layer qui echoue (StructuredOutputAttempt events)
3. Trouver le bug dans le code engine
4. Fixer avec TDD (test rouge → fix → test vert)
5. Re-executer le workflow pour confirmer
6. Commit: `fix(provider): <provider> structured output <layer> fix`

Schema de test (MEME workflow pour TOUS les providers):
```yaml
schema: "nika/workflow@0.12"
provider: anthropic          # changer par provider
model: claude-haiku-4-5     # changer par model

tasks:
  - id: extract_person
    # PROMPT NATUREL — JAMAIS mentionner JSON ou le schema!
    infer: "Decris une developpeuse nommee Alice, 30 ans, qui code en Rust et Python"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          age: { type: number, minimum: 0, maximum: 150 }
          skills: { type: array, items: { type: string }, minItems: 1 }
          active: { type: boolean }
        required: [name, age, skills, active]
      enable_repair: true
      max_retries: 3

  - id: extract_nested
    # Schema COMPLEXE avec nested objects + enums
    infer: "Invente un produit technologique innovant avec son prix et sa categorie"
    structured:
      schema:
        type: object
        properties:
          product:
            type: object
            properties:
              name: { type: string }
              price: { type: number, minimum: 0 }
              currency: { type: string, enum: ["USD", "EUR", "GBP"] }
            required: [name, price, currency]
          category: { type: string, enum: ["hardware", "software", "service"] }
          tags: { type: array, items: { type: string }, minItems: 2 }
          available: { type: boolean }
        required: [product, category, tags, available]
      enable_repair: true
      max_retries: 3

  - id: verify
    depends_on: [extract_person, extract_nested]
    with:
      person: $extract_person
      product: $extract_nested
    infer: "Verify the data: person={{with.person}}, product={{with.product}}"
```

Le test DOIT verifier:
- Le JSON est valide (parseable)
- Le JSON match le schema (tous les required presents)
- Les types sont corrects (name=string, age=number, etc.)
- Le prompt NE MENTIONNE PAS le format JSON (c'est automatique)
- Nested objects sont correctement structures
- Enums sont respectes (currency=USD|EUR|GBP)

EXECUTER ce workflow pour CHAQUE provider:
```bash
nika run test-structured.nika.yaml                              # anthropic (default)
nika run test-structured.nika.yaml --provider openai --model gpt-4.1-mini
nika run test-structured.nika.yaml --provider gemini --model gemini-2.5-flash
nika run test-structured.nika.yaml --provider groq --model llama-3.3-70b-versatile
nika run test-structured.nika.yaml --provider mistral --model mistral-small-latest
nika run test-structured.nika.yaml --provider deepseek --model deepseek-chat
nika run test-structured.nika.yaml --provider xai --model grok-3
```
Si UN SEUL echoue → debugger → fixer le code engine → re-tester.

### 2D — Complex pipeline tests: EXECUTE REAL WORKFLOWS (1h)

Il y a 84 fichiers .nika.yaml dans tests/ et docs/tests/.
L'agent DOIT executer les plus importants et fixer ce qui casse:

```bash
# Executer les workflows existants un par un:
nika run tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml
nika run tests/e2e-provider-tests/02-openai-json-response.nika.yaml
nika run tests/e2e-provider-tests/04-groq-ultra-fast-inference.nika.yaml
nika run tests/workflows/transforms-string.nika.yaml --provider mock
nika run tests/workflows/bindings-basic.nika.yaml --provider mock
nika run tests/workflows/fetch-extract-mode-01-markdown.nika.yaml
nika run test-workflow-1-research-pipeline.nika.yaml --provider mock
nika run test-workflow-4-retry-fallback.nika.yaml --provider mock
nika run test-workflow-8-guardrails.nika.yaml --provider mock

# Si un workflow a des erreurs de SYNTAXE → fixer le .nika.yaml
# Si un workflow echoue a l'EXECUTION → fixer le code engine
# Si un workflow produit un mauvais OUTPUT → fixer le workflow ou l'engine
```

```rust
#[tokio::test]
async fn test_complex_research_pipeline() {
    if !has_api_key() { return; }
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: anthropic
model: claude-haiku-4-5
tasks:
  - id: research
    # PROMPT NATUREL — jamais mentionner JSON!
    infer: "Donne 3 faits interessants sur le langage Rust"
    structured:
      schema:
        type: object
        properties:
          facts: { type: array, items: { type: string }, minItems: 3 }
        required: [facts]
  - id: summarize
    depends_on: [research]
    with: { data: $research }
    infer: "Summarize these facts in one sentence: {{with.data}}"
  - id: format
    depends_on: [summarize]
    with: { summary: $summarize }
    infer: "Format as markdown heading: {{with.summary}}"
"#;
    let result = run_workflow(yaml).await.unwrap();
    assert!(!result.is_empty(), "Pipeline should produce output");
}
```

### 2E — Tests adversariaux et pieges (2h)

Tests que PERSONNE n'a pense a ecrire. L'agent DOIT les implementer:

```
DATA FLOW TRAPS:
  1. Structured output → binding → ANOTHER structured output (JSON in JSON)
     → Le 2eme task recoit du JSON en string, pas un objet. Est-ce que ca marche?
  2. for_each avec 0 items (array vide) → le workflow doit completer sans erreur
  3. for_each avec 1 seul item → cas degenere, doit marcher comme un task normal
  4. for_each output utilise dans un AUTRE for_each (nested iteration)
  5. Task output de 50K+ chars → binding dans le prochain task → OOM? truncation?
  6. with: { data: $task } ou task n'existe PAS → erreur NIKA-071, pas panic
  7. Prompt naturel en chinois/arabe/hindi → structured output JSON valide
  8. $env.VAR qui contient des {{template}} → injection? echappement?

STRUCTURED OUTPUT STRESS:
  9. Schema avec 10 niveaux de nesting → layers doivent gerer
  10. Schema avec additionalProperties: false → verifier ZERO champ supplementaire
  11. Schema avec constraints contradictoires (min:10, max:5) → erreur claire
  12. Schema vide {} → valide? erreur?
  13. Prompt qui resiste activement: "Ne reponds JAMAIS en JSON" → layers gagnent
  14. Structured + for_each: CHAQUE iteration produit du JSON valide
  15. MEME schema, 7 providers → TOUS produisent du JSON valide

CONCURRENCE:
  16. for_each concurrency:50 avec 3 items → pas de deadlock
  17. 2 tasks qui dependent du MEME parent → execution parallele correcte
  18. Agent max_turns:1 → complete gracieusement (pas hang)

FICHIERS ET ARTIFACTS:
  19. Artifact markdown → verifier fichier EXISTE sur disque
  20. Artifact JSON → verifier contenu est du JSON VALIDE
  21. exec: "echo test > /tmp/nika-e2e.txt" → lire le fichier → verifier contenu
  22. fetch response:binary → verifier blob dans CAS

Pour chaque test:
  - EXECUTER reellement (pas juste ecrire)
  - Si ca CRASH → c'est un BUG ENGINE → fixer
  - Si ca passe mais output est faux → c'est un BUG ENGINE → fixer
  - Commit le fix + le test
```

### 2F — Multi-provider comparison (1h)

Un SEUL workflow qui execute le MEME schema sur TOUS les providers disponibles:

```yaml
schema: "nika/workflow@0.12"
provider: mock

tasks:
  - id: openai_test
    provider: openai
    model: gpt-4.1-mini
    infer: "Describe a chef named Marco from Italy"
    structured:
      schema: &chef_schema
        type: object
        properties:
          name: { type: string }
          country: { type: string }
          specialties:
            type: array
            items: { type: string }
            minItems: 2
          michelin_stars: { type: number, minimum: 0, maximum: 3 }
          active: { type: boolean }
        required: [name, country, specialties, michelin_stars, active]
      enable_repair: true

  - id: gemini_test
    provider: gemini
    model: gemini-2.5-flash
    infer: "Describe a chef named Marco from Italy"
    structured:
      schema: *chef_schema
      enable_repair: true

  # Meme chose pour chaque provider...

  - id: verify_all
    depends_on: [openai_test, gemini_test]
    with:
      openai: $openai_test
      gemini: $gemini_test
    infer: "Compare et verifie que les 2 outputs sont des JSON valides avec la meme structure"
```

L'agent DOIT:
1. Executer ce workflow
2. Verifier que CHAQUE provider produit du JSON valide
3. Si un provider echoue → debugger le 5-layer pour CE provider → fixer
4. Commit: `fix(structured): <provider> <layer> <description>`

**Wave 2 commits** (~6):
```
test(e2e): add 10 mock E2E workflow tests
test(e2e): structured output validation — 7 providers programmatic
test(e2e): complex pipeline + fetch + research tests
test(e2e): adversarial tests — data flow traps, stress, concurrency
test(e2e): artifact + file I/O verification tests
test(e2e): multi-provider comparison workflow
```

---

## WAVE 3: POLISH (4h)

### 3A — ProviderName engine migration (3h, 4 agents paralleles)

L'enum existe dans `nika-core::ProviderName`. AST deja migre.
Reste: 36 files engine, 160-180 edits. LOW complexity.

```
Agent 1: InferParams.provider + executor default_provider
Agent 2: AgentParams.provider + spawn.parent_provider
Agent 3: config.provider + partial.provider + context.provider
Agent 4: Tous les tests (10 files, provider: Some("string") → Some(ProviderName::parse("string")))
```

**Commit**: `refactor(engine): complete ProviderName migration — 36 files`

### 3B — Performance (1h)

| # | Fix | File:Line |
|---|-----|-----------|
| PERF-1 | `resolve_alias_path` → Cow<Value> to avoid unconditional clone | `template.rs:283` |
| PERF-2 | `compute_depths` → reuse existing Kahn's BFS O(V+E) | `dag/flow.rs:266-319` |
| PERF-3 | MCP connections → `join_all` instead of sequential | `executor/agent.rs:234-237` |

**Commit**: `perf(engine): Value clone + O(V²) depths + MCP parallel connect`

### 3C — Deps cleanup (30min)

Remove 6 dead workspace deps: nutype, static_assertions, derive_more, strum, tracing-error, console.

**Commit**: `chore(deps): remove 6 dead workspace dependencies`

---

## WAVE 4: RELEASE (2h)

### 4A — Version bump + CHANGELOG (1h)

```bash
find tools -name "Cargo.toml" -not -path "*/target/*" | xargs sed -i '' 's/version = "0.51.0"/version = "0.52.0"/'
```

CHANGELOG.md: documenter tout depuis v0.51.0.

### 4B — CI fixes (30min)

- `cargo deny check` → hard fail (remove `|| true`)
- Add dependabot.yml
- Dockerfile VERSION 0.40.2 → 0.52.0

### 4C — Tag + push (30min)

```bash
cargo test --workspace --lib → ALL PASS
cargo clippy --workspace -- -D warnings → ZERO
git tag v0.52.0
git push && git push --tags
```

**Wave 4 commits** (~3):
```
chore(release): bump to v0.52.0 + CHANGELOG
ci: cargo-deny hard fail + dependabot + Dockerfile version
chore(release): tag v0.52.0
```

---

# REGLES

```
1. cargo test --workspace --lib TOUJOURS (--lib = pas de keychain)
2. TDD: test FAIL → fix → PASS → suite → commit
3. 1 fix = 1 commit
4. Co-authors:
   Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
5. Push toutes les 2-3 commits
6. clippy ZERO warnings
7. Si bloque 3x → skip + note dans progress.md
8. JAMAIS marquer done sans test
```

# METRIQUES DE SUCCES

| Metrique | Avant | Cible |
|----------|-------|-------|
| Tests | 8,903 | **9,200+** |
| Security bugs | 4 | **0** |
| High bugs | 6 | **0** |
| Error handling | 5 gaps | **0** |
| Dead code | 3 items | **0** |
| MSRV | 1.86 (violated) | **1.94** |
| ProviderName | String | **typed enum partout** |
| Structured output | non teste | **7/7 providers valides** |
| Prompt naturel | non verifie | **zero mention JSON dans prompts** |
| Schema validation | basique | **programmatique (type+enum+range)** |
| Daemon | manual start | **auto-start transparent** |
| Keychain popup | possible | **impossible (code supprime)** |
| E2E workflows | 0 executes | **20+ executes et valides** |
| Version | v0.51.0 | **v0.52.0 tagged** |

# DEFINITION OF DONE (pour chaque provider)

Un provider est "done" quand:
1. Infer simple → output non-vide ✓
2. Structured output simple (3 champs) → JSON valide + schema match ✓
3. Structured output complexe (nested + enum + array) → JSON valide + schema match ✓
4. Pipeline 3 etapes (infer → with binding → infer) → output final coherent ✓
5. EventLog contient StructuredOutputSuccess avec le bon layer ✓
6. Aucun warning "malformed" dans les logs ✓
7. Cost tracking correct (ProviderResponded event avec tokens > 0) ✓

# CONTEXT WINDOW HANDOFF

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v15-final.md)"
```

# GO

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
# WAVE 1 → WAVE 2 → WAVE 3 → WAVE 4 → v0.52.0
```
