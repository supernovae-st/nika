# Engine Fixes — Master Prompt for Executing Agent

> **Copy-paste this entire prompt into a fresh Claude Code session opened at `/Users/thibaut/dev/supernovae/nika/tools`**

---

## MISSION

Tu es charge de fixer 5 bugs critiques dans le moteur Nika (v0.49.0). Chaque bug est documente avec les fichiers exacts, numeros de ligne, et le code a ecrire. Tu DOIS utiliser TDD strict: test qui fail d'abord, puis implementation minimale, puis verification.

**Plan detaille:** `docs/plans/2026-03-27-structured-output-and-engine-fixes.md`

**Codebase:** Nika est un workflow engine YAML. Le workspace Cargo est dans `tools/`. Les crates principales: `nika-engine` (135k lignes), `nika-core` (23k lignes).

**Tests:** TOUJOURS `cargo test --workspace --lib` — JAMAIS sans `--lib` (declenche des popups macOS Keychain sinon). Test count actuel: ~8500 tests.

**Git workflow:** 1 fix = 1 commit. Format: `type(scope): description`. Co-authors obligatoires:
```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**Skills a utiliser:** `@rust-core`, `@test-driven-development`, `@spn-powers:executing-plans`

---

## PHASE 0: RECHERCHE PREALABLE (OBLIGATOIRE)

Avant de toucher au code, tu DOIS lancer ces recherches:

### 0.1 — Comprendre rig-core 0.32

```bash
# rig-core est dans le cargo registry
RIG_SRC="$HOME/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rig-core-0.32.0/src"

# API cle: AgentBuilder.additional_params()
cat "$RIG_SRC/agent/builder.rs" | head -300

# API cle: OpenAI completion — comment response_format est gere
cat "$RIG_SRC/providers/openai/completion/mod.rs" | grep -A 30 "response_format"

# API cle: CompletionRequest.additional_params
cat "$RIG_SRC/completion/request.rs" | head -100
```

**DECOUVERTE CRITIQUE:** rig-core 0.32 a DEJA le support `response_format` pour OpenAI. Dans `providers/openai/completion/mod.rs` lignes 1102-1127, il merge `output_schema` en `response_format` dans `additional_params`. ET `AgentBuilder.additional_params(json!({...}))` est une methode publique (ligne 194 de builder.rs).

Le fix est donc: passer `additional_params` avec `response_format: { type: "json_schema", json_schema: { name: "output", strict: true, schema: <our_schema> } }` quand on appelle l'AgentBuilder pour les providers OpenAI-compatibles.

### 0.2 — Comprendre le code Nika actuel

```bash
# Comment Nika appelle rig-core pour les infer avec tools
grep -n "infer_with_tools\|AgentBuilder\|additional_params\|agent_builder" \
  tools/nika-engine/src/provider/rig.rs | head -30

# Le Layer 0 (tool injection) dans l'executor
sed -n '280,552p' tools/nika-engine/src/runtime/executor/infer.rs

# Le StructuredOutputEngine
sed -n '1,100p' tools/nika-engine/src/runtime/structured_output.rs

# Le DynamicSubmitTool
cat tools/nika-engine/src/runtime/submit_tool.rs | head -120

# Les tests existants
grep -c "#\[test\]\|#\[tokio::test\]" tools/nika-engine/src/runtime/structured_output.rs
grep -c "#\[test\]\|#\[tokio::test\]" tools/nika-engine/src/runtime/executor/tests.rs
grep -c "#\[test\]\|#\[tokio::test\]" tools/nika-engine/src/runtime/executor/tests_wiremock.rs
```

### 0.3 — Comprendre le systeme d'imports/include

```bash
# Parser YAML — cherche "imports" et "include"
grep -n '"imports"\|"include"\|known_workflow_keys' \
  tools/nika-core/src/ast/raw/parser.rs | head -20

# Le loader qui expand les imports
wc -l tools/nika-engine/src/ast/import_loader.rs
grep -c "#\[test\]" tools/nika-engine/src/ast/import_loader.rs

# Tous les fichiers a renommer
grep -rn "import_loader\|expand_imports\|parse_imports\|RawImportSpec" \
  tools/nika-core/src/ tools/nika-engine/src/ | grep -v target | grep -v ".rs:" | wc -l
```

### 0.4 — Perplexity / Web Research

Utilise `perplexity_search_web` ou `firecrawl_search` pour:
1. **"rig-core rust additional_params openai response_format structured output"** — verifier la doc officielle rig-core
2. **"openai api response_format json_schema strict mode 2025"** — verifier le format exact attendu par l'API OpenAI
3. **"rust cargo rename module file without breaking"** — best practices pour renommer un module Rust

### 0.5 — Swarm d'agents analyse

Lance 3 agents en parallele pour analyser le code:

**Agent 1 — Structured Output Pipeline:**
> "Read ALL of tools/nika-engine/src/runtime/structured_output.rs, tools/nika-engine/src/runtime/executor/infer.rs lines 250-600, tools/nika-engine/src/runtime/submit_tool.rs, and tools/nika-engine/src/provider/rig.rs lines 750-900. Map the COMPLETE data flow from when structured: is declared in YAML to when the validated JSON is returned. Identify every decision point where OpenAI vs Claude diverges."

**Agent 2 — Import System:**
> "Read ALL of tools/nika-engine/src/ast/import_loader.rs and tools/nika-core/src/ast/raw/parser.rs lines 1280-1600. List EVERY function, struct, field, string literal, and test that references 'import' or 'imports'. I need the COMPLETE rename list."

**Agent 3 — Template Resolution Gaps:**
> "Read tools/nika-engine/src/runtime/executor/infer.rs and tools/nika-engine/src/runtime/executor/agent.rs. Find EVERY field that should support templates but doesn't. Check: model, provider, cwd, base_url, env values. For each, show the line where the field is used and whether template_resolve() is called."

---

## PHASE 1: FIX STRUCTURED OUTPUT (OpenAI) — TASK 1

### Contexte

Le probleme: quand `structured:` est utilise avec `provider: openai`, le Layer 0 (DynamicSubmitTool + tool_choice: Required) echoue avec `MaxTurnError(0)`. Layers 2-4 tentent de valider/retry/repair via texte-only, mais c'est insuffisant.

La solution: rig-core 0.32 a `AgentBuilder.additional_params()`. On peut injecter `response_format: { type: "json_schema" }` pour les providers OpenAI-compatibles. Ca force OpenAI a retourner du JSON conforme au schema.

### Fichiers a modifier

1. `tools/nika-engine/src/provider/rig.rs` — Ajouter methode `infer_with_structured()` ou modifier `infer_with_tools()` pour passer `additional_params` avec `response_format`
2. `tools/nika-engine/src/runtime/executor/infer.rs` — Modifier le Layer 0 path pour utiliser `response_format` quand le provider est OpenAI-compatible
3. `tools/nika-engine/src/provider/endpoints.rs` — Si les custom endpoints (vLLM, Ollama) supportent aussi `response_format`

### TDD Steps

**Step 1:** Ecrire un test qui verifie que `infer_with_tools()` passe `additional_params` quand un schema est fourni et que le provider est openai. Utiliser wiremock pour intercepter le HTTP request et verifier le body.

**Step 2:** Run test → FAIL

**Step 3:** Implementer — ajouter un parametre `schema: Option<&Value>` a `infer_with_tools()` dans rig.rs. Quand `schema` est `Some` et le provider est OpenAI/Groq/DeepSeek/xAI, passer `additional_params`:

```rust
let agent_builder = agent_builder.additional_params(json!({
    "response_format": {
        "type": "json_schema",
        "json_schema": {
            "name": "structured_output",
            "strict": true,
            "schema": schema
        }
    }
}));
```

**Step 4:** Run test → PASS

**Step 5:** Ecrire test de non-regression: Claude ne doit PAS recevoir `response_format` (il utilise tool_choice)

**Step 6:** Modifier `infer.rs` Layer 0 pour passer le schema a la nouvelle signature

**Step 7:** Run `cargo test -p nika-engine --lib` → ALL PASS

**Step 8:** Commit

### Providers qui supportent response_format

| Provider | Supporte response_format | Notes |
|----------|--------------------------|-------|
| openai | OUI | Native support |
| groq | OUI | OpenAI-compatible |
| deepseek | OUI | OpenAI-compatible |
| xai | OUI | OpenAI-compatible |
| gemini | NON | Utilise tool injection |
| claude | NON | Utilise tool injection |
| mistral | PARTIEL | Verifier via web research |
| native | NON | GGUF local |

---

## PHASE 2: RENAME imports → include — TASK 2

### Regle absolue

`imports:` est MORT. On le SUPPRIME. Pas de backward compat. Pas de "accept both". Le YAML key est `include:`, point final.

### Plan de rename

Le rename touche ~10 fichiers source + ~45 fonctions de test. L'approche:

1. **Renommer le fichier**: `import_loader.rs` → `include_loader.rs` (via `git mv`)
2. **Renommer les fonctions**: `expand_imports` → `expand_include`, `parse_imports` → `parse_include`
3. **Renommer les types**: `RawImportSpec` → `RawIncludeSpec`, `AnalyzedImportSpec` → `AnalyzedIncludeSpec` (optionnel — le type interne peut rester si trop risque)
4. **Changer les string literals**: `"imports"` → `"include"` dans le parser et known_workflow_keys
5. **Mettre a jour les tests**: tous les YAML embedded dans les tests doivent passer de `imports:` a `include:`
6. **Verifier le JSON schema**: `tools/nika/schemas/nika-workflow.schema.json` utilise deja `include:` — confirmer

### Fichiers exhaustifs

```
# nika-core (parser + AST)
tools/nika-core/src/ast/raw/workflow.rs          # ligne 51: champ imports → include
tools/nika-core/src/ast/raw/parser.rs            # lignes 1296,1345,1487-1543: parse_imports → parse_include, "imports" → "include"
tools/nika-core/src/ast/schema.rs                # lignes 135,138,198,284: commentaires
tools/nika-core/src/ast/analyzed/workflow.rs      # ligne 57,94: champ imports → include

# nika-engine (loader + lower + runner)
tools/nika-engine/src/ast/import_loader.rs       # RENAME FILE → include_loader.rs, fonctions, ~45 tests
tools/nika-engine/src/ast/mod.rs                 # lignes 101,134,189,201,248,269: module + exports
tools/nika-engine/src/ast/lower.rs               # lignes 466,644: lower_imports → lower_include
tools/nika-engine/src/runtime/runner.rs          # 6 locations: imports: vec![] → include: vec![]
```

### TDD Steps

**Step 1:** Ecrire test `test_parse_include_not_imports` dans parser.rs — YAML avec `include:` doit parser, YAML avec `imports:` doit FAIL

**Step 2:** Run → FAIL (parser cherche encore "imports")

**Step 3:** Renommer tout (batch — c'est un refactor pur)

**Step 4:** Run `cargo test --workspace --lib` → ALL PASS

**Step 5:** Commit

---

## PHASE 3: TEMPLATE RESOLVE SUR model: — TASK 3

### Le bug

`model: "{{inputs.deep_model}}"` dans un task n'est PAS resolu. Le string literal est passe tel quel au provider.

### Fichiers

- `tools/nika-engine/src/runtime/executor/infer.rs:229` — ajouter `template_resolve()` sur le model
- `tools/nika-engine/src/runtime/executor/agent.rs:126` — meme chose

### TDD Steps

**Step 1:** Ecrire test unitaire qui cree un TaskExecutor mock, appelle run_infer avec `model: "{{inputs.fast_model}}"` et des bindings contenant `inputs.fast_model = "gpt-4o-mini"`, verifie que le model passe au provider est "gpt-4o-mini"

**Step 2:** Run → FAIL

**Step 3:** Ajouter template_resolve:
```rust
// infer.rs ~line 229
let resolved_model = match infer.model.as_deref() {
    Some(m) if m.contains("{{") => {
        Some(template_resolve(m, bindings, datastore)?.into_owned())
    }
    Some(m) => Some(m.to_string()),
    None => None,
};
let model = resolved_model.as_deref().or(self.default_model.as_deref());
```

**Step 4:** Run → PASS

**Step 5:** Meme fix dans agent.rs

**Step 6:** Commit

---

## PHASE 4: EXTENDED_THINKING GRACEFUL DEGRADATION — TASK 4

### Le bug

`extended_thinking: true` avec un provider non-Claude CRASH avec `ValidationError` au lieu d'emettre un WARN et continuer.

### Comportement souhaite

```
provider: openai + extended_thinking: true
→ WARN: "extended_thinking: true ignored — only supported by Claude"
→ Continue l'inference normalement sans thinking
→ PAS de crash
```

### Fichiers

- `tools/nika-core/src/ast/analyzer/analyze.rs` — si validation parse-time existe, la changer en warning
- `tools/nika-engine/src/runtime/executor/infer.rs` — ajouter check post-resolution du provider
- `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs` — meme chose pour agent

### TDD Steps

**Step 1:** Ecrire test: workflow avec `provider: openai` + `extended_thinking: true` → PAS d'erreur, juste un WARN

**Step 2:** Run → FAIL (crash actuel)

**Step 3:** Changer l'erreur en warning + clear du flag

**Step 4:** Run → PASS

**Step 5:** Commit

---

## PHASE 5: TELEMETRIE + ERROR REPORTING — TASK 5

### Le probleme

Quand structured output echoue, l'erreur `NIKA-061` ne dit pas QUEL layer a echoue, COMBIEN de retries ont ete tentes, ni QUEL etait l'output du LLM.

### Ameliorations

1. Chaque attempt de layer emet un event avec: layer number, success/fail, error details, output preview (200 chars)
2. L'erreur finale NIKA-061 inclut: nombre total d'attempts, statut de chaque layer, suggestion
3. Le display CLI montre le progression des layers:
   ```
   ⬡ L0: response_format ✓ (OpenAI native)
   ⬡ L2: validate ✗ (missing: "brand")
   ⬡ L3: retry 1/3 ✓
   ```

### Fichiers

- `tools/nika-engine/src/runtime/structured_output.rs` — enrichir les events
- `tools/nika-engine/src/display/format_event.rs` — formatter les events
- `tools/nika-engine/src/event.rs` — si besoin d'ajouter des variantes EventKind

---

## PHASE 6: VERIFICATION FINALE

### 6.1 — Tests

```bash
cd tools
cargo test --workspace --lib        # Tous les tests
cargo clippy --workspace -- -D warnings  # Zero warnings
```

### 6.2 — Test d'integration avec le workflow GEO-SEO

```bash
cd /Users/thibaut/Desktop/aaayayaa
nika check main.nika.yaml                    # Doit passer
nika run main.nika.yaml \
  --input site_url="https://qrcode-ai.com" \
  --input brand="QR Code AI" \
  --input output_dir="/Users/thibaut/Desktop/aaayayaa" \
  --provider openai \
  --no-live 2>&1 | head -100
```

**Note:** Le workflow utilise actuellement `imports:` — apres le rename en Phase 2, il faudra changer en `include:` dans `main.nika.yaml`.

### 6.3 — Swarm de review final

Lance 3 agents code-reviewer en parallele:
1. Review des changements structured output
2. Review du rename imports → include
3. Review des template resolve + extended_thinking

---

## REGLES ABSOLUES

1. **TDD STRICT** — Jamais de code sans test qui fail d'abord
2. **`--lib` TOUJOURS** — JAMAIS `cargo test` sans `--lib`
3. **1 fix = 1 commit** — Pas de mega-commit
4. **Pas de backward compat** — `imports:` est mort, on ne le supporte plus
5. **Pas de workaround** — Si une feature ne marche pas, on la FIX, on ne la contourne pas
6. **extended_thinking ne crash JAMAIS** — WARN + continue
7. **structured: output marche avec TOUS les providers** — C'est la promesse de Nika
8. **Telemetrie partout** — L'utilisateur doit TOUJOURS savoir ce qui se passe

---

## LIENS UTILES

| Ressource | Chemin |
|-----------|--------|
| Plan detaille | `docs/plans/2026-03-27-structured-output-and-engine-fixes.md` |
| rig-core 0.32 source | `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rig-core-0.32.0/src/` |
| rig-core AgentBuilder | `~/.cargo/registry/src/.../rig-core-0.32.0/src/agent/builder.rs` |
| rig-core OpenAI completion | `~/.cargo/registry/src/.../rig-core-0.32.0/src/providers/openai/completion/mod.rs` |
| Nika structured output | `tools/nika-engine/src/runtime/structured_output.rs` |
| Nika infer executor | `tools/nika-engine/src/runtime/executor/infer.rs` |
| Nika provider/rig | `tools/nika-engine/src/provider/rig.rs` |
| Nika import loader | `tools/nika-engine/src/ast/import_loader.rs` |
| Nika raw parser | `tools/nika-core/src/ast/raw/parser.rs` |
| Nika JSON schema | `tools/nika/schemas/nika-workflow.schema.json` |
| GEO-SEO workflow | `/Users/thibaut/Desktop/aaayayaa/main.nika.yaml` |
| CLAUDE.md (dev ref) | `tools/nika/CLAUDE.md` |
| Architecture rules | `../../dx/.claude/rules/architecture.md` |
| Git workflow rules | `../../dx/.claude/rules/git-workflow.md` |
