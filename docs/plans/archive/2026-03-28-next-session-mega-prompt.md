# Nika v0.50.1 — Security Hardening + Integration Testing — Mega Prompt

> **Copy this entire prompt into a fresh Claude Code session at `/Users/thibaut/dev/supernovae/nika/tools/nika`**

---

## CONTEXT

Tu prends le relais d'une mega session de 50 commits qui a fixe ~30 bugs trouves par 15 review agents. Le code est sur `main`, tout pousse, 7178+ tests passent. Il reste **23 issues** documentees dans `docs/plans/2026-03-28-session-handoff.md`.

**Codebase**: Nika = Rust workflow engine, schema `nika/workflow@0.12`, workspace at `tools/`
**Version**: v0.50.0 (workspace), ~135k LOC engine, 10 crates
**Tests safe**: `cargo test --workspace --lib` (JAMAIS sans `--lib` — keychain popups)

---

## METHODOLOGIE OBLIGATOIRE

### 1. Research First (15 min)

Avant TOUT code:

```bash
# Etat du repo
git log --oneline -10
cargo test -p nika-engine --lib 2>&1 | grep "test result:"
cargo test -p nika-core --lib 2>&1 | grep "test result:"
cargo clippy --workspace -- -D warnings 2>&1 | tail -3

# Lire le handoff
cat docs/plans/2026-03-28-session-handoff.md
```

Verifier que les 50 commits precedents sont bien la et que les tests passent. Si des tests echouent AVANT tes changements, documenter et skipper.

### 2. Plan-Execute-Plan Cycle

Pour CHAQUE fix:
1. **PLAN**: Lire le fichier concerne, comprendre le bug, ecrire le plan de fix
2. **RED**: Ecrire le test FIRST qui expose le bug (doit echouer)
3. **GREEN**: Implementer le fix minimal qui fait passer le test
4. **REFACTOR**: Nettoyer si necessaire
5. **VERIFY**: `cargo test -p <crate> --lib` — TOUS les tests passent
6. **COMMIT**: 1 fix = 1 commit granulaire
7. **PUSH**: `git push origin main` apres chaque commit

### 3. Commit Format

```
fix(scope): description concise

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Scopes: `provider`, `runtime`, `security`, `ast`, `event`, `display`, `agent`, `media`, `schema`, `test`

### 4. Skills a Utiliser

- `rust-core` pour les patterns Rust (ownership, error handling)
- `rust-async` pour les timeouts et tokio
- `test-driven-development` pour le cycle RED-GREEN-REFACTOR
- `systematic-debugging` pour les bugs complexes
- `verification-before-completion` pour verifier AVANT de dire "done"

### 5. Review Waves

Apres chaque batch de 5+ fixes:
- Lancer 5 agents `code-reviewer` en parallele sur les zones modifiees
- Fixer tout ce qu'ils trouvent
- Relancer jusqu'a < 3 findings

---

## PHASE A: SECURITY FIXES (priorite absolue)

### A1. Template Injection via Cross-Pass Resolution (CRITICAL SECURITY)

**File**: `nika-engine/src/binding/template.rs` ~line 498-510 + line 1081

**Bug**: Quand le template original contient une ref `{{context.files.public}}` legit, Pass 2 est active (`has_context = true`). Si une valeur `with:` resolue en Pass 1 contient `{{context.files.secret}}`, Pass 2 resout AUSSI la ref injectee — exfiltration de fichiers contexte.

**Reproduction**:
```yaml
context:
  files:
    public: ./README.md
    secret: ./config/secrets.yaml

tasks:
  - id: llm_output
    infer: "Generate a template string"
    # LLM could return: "Result: {{context.files.secret}}"

  - id: use_output
    with:
      data: $llm_output
    infer: |
      Process: {{with.data}}
      Context: {{context.files.public}}
```

**Fix Strategy**: Au lieu de resoudre Pass 2 sur le resultat de Pass 1, extraire les positions des refs originales AVANT Pass 1. En Pass 2, ne resoudre QUE les refs qui existaient dans le template original.

```rust
// Before Pass 1: record original context/input ref positions
let original_refs: Vec<(usize, usize, String)> = TEMPLATE_RE
    .captures_iter(template)
    .filter_map(|cap| {
        let expr_str = cap.get(1)?.as_str().trim();
        if expr_str.starts_with("context.") || expr_str.starts_with("inputs.") {
            Some((cap.get(0)?.start(), cap.get(0)?.end(), expr_str.to_string()))
        } else {
            None
        }
    })
    .collect();

// Pass 1: resolve with.* aliases
// ...

// Pass 2: only resolve refs that were in the ORIGINAL template
// Replace by position tracking, not by scanning the post-Pass-1 string
```

**TDD**:
```rust
#[test]
fn test_template_injection_blocked() {
    // Template with legit context ref + injected ref in binding
    let template = "Result: {{with.data}} Context: {{context.files.public}}";
    let mut bindings = ResolvedBindings::new();
    bindings.insert("data", Value::String("{{context.files.secret}}".into()));
    let context = Context { files: {"public": "public content", "secret": "SECRET DATA"} };

    let result = resolve_with(template, &bindings, &context);
    assert!(!result.contains("SECRET DATA"), "Injected context ref must not resolve");
    assert!(result.contains("public content"), "Legit context ref should resolve");
}
```

### A2. Binary Fetch OOM Vector (HIGH)

**File**: `nika-engine/src/runtime/executor/fetch.rs` ~line 450

**Bug**: `response: binary` uses `response.bytes().await` (unbounded allocation). Chunked transfer encoding bypasses Content-Length pre-check. Can OOM with ~100MB allocation.

**Fix**: Use the existing `read_body_with_limit()` streaming function for binary too:
```rust
// Instead of:
let body = response.bytes().await?;
// Use:
let body = read_body_with_limit(response, MAX_RESPONSE_SIZE).await?;
```

### A3. ProviderCalled Before Budget Reserve (HIGH)

**File**: `nika-engine/src/runtime/executor/infer.rs` ~line 276-300

**Bug**: `ProviderCalled` event emitted BEFORE `reserve_tokens()`. If budget fails, orphaned event with no ProviderResponded.

**Fix**: Move the emit to AFTER the budget check succeeds (after line 300).

### A4. Vision Cost Heuristic (HIGH)

**File**: `nika-engine/src/runtime/executor/infer.rs` ~line 1150

**Bug**: `est_in = estimate_tokens(prompt.len())` ignores image data entirely.

**Fix**: Add image bytes to estimate:
```rust
let est_in = estimate_tokens(prompt.len()) + (total_bytes / 750) as u64;
```
(750 bytes/token is Anthropic's high-res image approximation)

---

## PHASE B: INTEGRATION TESTS WITH REAL PROVIDERS (2h)

### B1. Setup

```bash
# Verify all API keys
nika provider list

# Verify custom endpoint config
cat ~/.config/nika/config.toml

# Should show:
# [endpoints.h100]
# base_url = "http://51.159.167.12:8000/v1"
# api_key = "..."
# model = "Qwen/Qwen3.5-27B"
```

### B2. Test Matrix

Creer un workflow de test qui exerce chaque feature:

```yaml
# File: test-integration-v050.nika.yaml
schema: "nika/workflow@0.12"
workflow: integration-test-v050
provider: anthropic
model: claude-haiku-4-5

inputs:
  topic: "Rust workflow engines"

tasks:
  # 1. Basic infer
  - id: basic_infer
    infer: "In one sentence, what is {{inputs.topic}}?"

  # 2. Structured output
  - id: structured
    depends_on: [basic_infer]
    with:
      summary: $basic_infer
    infer:
      prompt: "Extract 3 keywords from: {{with.summary}}"
    structured:
      schema:
        type: object
        properties:
          keywords: { type: array, items: { type: string } }
        required: [keywords]

  # 3. Exec verb
  - id: exec_test
    exec: "echo 'Hello from exec'"

  # 4. Fetch verb
  - id: fetch_test
    fetch:
      url: "https://httpbin.org/json"
      extract: jsonpath
      selector: "$.slideshow.title"

  # 5. For each with concurrency
  - id: parallel_process
    depends_on: [structured]
    with:
      items: $structured
    for_each:
      items: "{{with.items.keywords}}"
      as: keyword
      concurrency: 3
    infer: "Define '{{with.keyword}}' in 10 words"

  # 6. Multi-provider (OpenAI)
  - id: openai_test
    provider: openai
    model: gpt-4o-mini
    depends_on: [basic_infer]
    with:
      data: $basic_infer
    infer: "Rephrase: {{with.data}}"

  # 7. Retry test
  - id: retry_test
    retry:
      max_attempts: 2
      delay_ms: 1000
    infer: "Say 'hello'"
```

### B3. Run Matrix

```bash
# Test with Anthropic (default)
nika run test-integration-v050.nika.yaml

# Test with mock (no API calls)
nika run test-integration-v050.nika.yaml --provider mock

# Test dry-run
nika run test-integration-v050.nika.yaml --dry-run

# Test custom endpoint (if configured)
nika run test-integration-v050.nika.yaml --provider h100 --model "Qwen/Qwen3.5-27B"

# Test with verbose output
nika run test-integration-v050.nika.yaml --detail max

# Test TUI
nika ui test-integration-v050.nika.yaml
```

### B4. What to Verify

Pour CHAQUE run, checker:
- [ ] Tous les tasks completent sans erreur
- [ ] Le cost summary montre des valeurs non-zero
- [ ] Les for_each items produisent un array (pas null)
- [ ] Le structured output contient les keywords attendues
- [ ] Le retry event apparait dans le log si applicable
- [ ] Pas de `<think>` tags dans les outputs (si Qwen/DeepSeek)
- [ ] L'endpoint_url apparait dans les traces pour custom endpoints

### B5. Test GEO-SEO Workflow (stress test)

Si les tests simples passent, lancer le workflow GEO-SEO complet:

```bash
# Le workflow existe a ~/Desktop/aaayayaa/ (54 tasks, 174 edges)
# Utiliser mock d'abord pour valider la structure
nika run ~/Desktop/aaayayaa/geo-seo-audit.nika.yaml --provider mock --dry-run

# Puis avec un vrai provider
nika run ~/Desktop/aaayayaa/geo-seo-audit.nika.yaml --provider anthropic
```

---

## PHASE C: FIX REMAINING HIGHs (post-integration)

### C1. Preset Not Applied During Structured Retry

**File**: `runner.rs:626-643`

`get_retry_config` constructs InferParams with `task.provider` and `task.model` directly, BEFORE preset resolution. Retries use wrong provider/model.

**Fix**: Pass `effective_provider` and `effective_model` to `get_retry_config`.

### C2. MaxTurnsReached Dead Code

**File**: `rig_agent_loop/types.rs:33`

`MaxTurnsReached` variant is never produced. rig-core silently truncates at max_turns.

**Fix**: Track turn count in `stream_with_tools`. When FinalResponse arrives and turns == max_turns, return MaxTurnsReached instead of NaturalCompletion.

### C3. for_each Items Pipe Transforms Ignored

**File**: `runner.rs:1923-1966`

`for_each: items: "{{inputs.xxx | flatten}}"` — pipe transforms silently fail.

**Fix**: Use the full TransformExpr pipeline for for_each item resolution.

### C4. Template Validation Gaps

**Files**: `dag/validate.rs:62` and `dag/validate.rs:72`

- `validate_template_refs` doesn't check context/inputs refs
- `extract_templates_from_action` misses fetch.headers, agent.system

**Fix**: Add extraction for all template-bearing fields.

---

## PHASE D: REVIEW WAVE + RELEASE (1h)

### D1. Review Wave

```
Lance 5 agents code-reviewer en parallele:
1. Security sweep (template.rs, policy.rs, safety.rs, security.rs)
2. Provider + cost (rig.rs, cost.rs, endpoints.rs)
3. Runtime (runner.rs, infer.rs, fetch.rs)
4. Agent loop (all rig_agent_loop/ files)
5. Schema + AST (schema.json, analyzer, parser)
```

### D2. Release v0.50.1

```bash
# Bump version
# tools/Cargo.toml → version = "0.50.1"
# (workspace.package.version)

# Update CHANGELOG
# docs/CHANGELOG.md

# Tag
git tag v0.50.1
git push origin main --tags

# Verify CI
gh run list --limit 5
```

---

## REGLES ABSOLUES

1. **`cargo test --lib` TOUJOURS** — jamais sans `--lib` (keychain)
2. **1 fix = 1 commit** — pas de batching
3. **TDD RED-GREEN-REFACTOR** — le test AVANT le code
4. **Push apres chaque commit** — pas d'accumulation locale
5. **Review apres chaque batch** — lancer les agents code-reviewer
6. **Verify before claiming done** — `cargo test && cargo clippy`
7. **Ne JAMAIS skipper les hooks** — si le hook echoue, fixer la cause
8. **Ne PAS toucher du code non lie** — pas de refactor opportuniste
9. **Documenter les bugs trouves** — meme si pas fixes dans cette session
10. **Sauver en memoire** les decisions importantes pour les sessions futures

---

## COMMANDS REFERENCE

```bash
# Tests
cargo test --workspace --lib                    # ALL crates (safe)
cargo test -p nika-engine --lib                 # Engine only (3800+)
cargo test -p nika-engine --lib -- provider     # Filter by name
cargo clippy --workspace -- -D warnings         # Zero warnings

# Run workflows
nika run workflow.nika.yaml                     # Execute
nika run workflow.nika.yaml --dry-run           # Validate only
nika run workflow.nika.yaml --provider mock     # No API calls
nika check workflow.nika.yaml                   # Syntax + DAG check
nika provider list                              # API key status

# Git
git push origin main                            # Push
git log --oneline -10                           # Recent commits
git stash list                                  # Check stashes (10 old ones exist)
```

---

## ARBRE DE DECISION

```
START
  │
  ├─ Tests passent? ──NO──→ FIX TESTS FIRST (ne pas commencer les features)
  │
  YES
  │
  ├─ Phase A (security) ──→ Template injection (#A1) est le PLUS IMPORTANT
  │                          car c'est la seule vuln security restante
  │
  ├─ Phase B (integration) ──→ OBLIGATOIRE avant release
  │                             Tester avec vrais providers
  │                             Si un workflow echoue: documenter le bug, fixer
  │
  ├─ Phase C (remaining HIGHs) ──→ Fixes logiques
  │
  └─ Phase D (review + release) ──→ Ne releaser que si:
                                     - 0 test failures
                                     - 0 clippy warnings
                                     - Integration tests OK
                                     - Review wave < 3 findings
```
