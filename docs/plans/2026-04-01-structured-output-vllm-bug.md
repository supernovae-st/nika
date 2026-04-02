# BUG REPORT + FIX PLAN — Structured Output Fails with vLLM/Qwen

> **Copie ce fichier entier comme premier message à un agent Claude Code.**
> Working directory: `/Users/thibaut/dev/supernovae/nika/tools`

---

## TL;DR

Les retries de structured output (L3 retry_with_feedback + L4 llm_repair) crashent quand le provider est vLLM/qwen parce que vLLM renvoie des champs JSON non-standard dans la réponse OpenAI-compat que rig-core 0.33 ne sait pas désérialiser.

**Impact** : Toute workflow avec `structured:` qui échoue en L2 (schema validation) ne peut pas se rattraper via les retries sur vLLM. 4/5 workflows qwen passent, mais le structured fail systématiquement.

---

## Symptôme

```
│ ⬡ L2: extract_validate ✗ ([NIKA-061] Schema validation failed: ...)
│ ⬡ L3: retry_with_feedback ✗ (retry 1: LLM call failed: [NIKA-031] Provider API error:
│   structured output retry failed: Completion error: CompletionError: JsonError:
│   invalid type: null, expected a map at line 1 column 3022)
│ ⬡ L4: llm_repair ✗ (same error)
│ error [NIKA-303] Structured output failed after 5 attempts
```

**Pattern** : L2 extrait bien le JSON mais la validation schema échoue (champ manquant ou type incorrect). Puis L3/L4 re-appellent le LLM via `infer()` (non-streaming) → le provider crash sur la **désérialisation de la réponse vLLM** avant même de pouvoir extraire le texte.

---

## Cause racine

### vLLM 0.18.0 renvoie des champs non-standard

Réponse brute de vLLM pour `qwen3.5-27b` :

```json
{
  "id": "chatcmpl-xxx",
  "object": "chat.completion",
  "model": "qwen3.5-27b",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "{\"name\": \"Rust\", ...}",
      "refusal": null,          // ← OpenAI field, géré par rig-core
      "annotations": null,      // ← NON-STANDARD, pas dans rig-core
      "audio": null,            // ← OpenAI field, géré par rig-core
      "function_call": null,    // ← DEPRECATED, pas dans rig-core
      "tool_calls": [],
      "reasoning": null         // ← NON-STANDARD (PAS reasoning_content)
    },
    "logprobs": null,
    "finish_reason": "stop",
    "stop_reason": null,        // ← NON-STANDARD
    "token_ids": null           // ← NON-STANDARD
  }],
  "usage": {
    "prompt_tokens": 80,
    "total_tokens": 95,
    "prompt_tokens_details": null  // ← null au lieu d'un objet (OpenAI renvoie un objet)
  },
  "service_tier": null,           // ← pas dans rig-core CompletionResponse
  "system_fingerprint": null,     // ← dans rig-core (Option<String>) ✅
  "prompt_logprobs": null,        // ← NON-STANDARD
  "prompt_token_ids": null,       // ← NON-STANDARD
  "kv_transfer_params": null      // ← NON-STANDARD
}
```

### rig-core 0.33 désérialise avec des structs strictes

**Fichier** : `~/.cargo/registry/src/.../rig-core-0.33.0/src/providers/openai/completion/mod.rs`

Les structs rig-core pour le non-streaming path :

```rust
// Ligne 772-781
#[derive(Debug, Deserialize, Serialize)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub system_fingerprint: Option<String>,  // ← gère null ✅
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}
// ❌ Pas de service_tier, prompt_logprobs, prompt_token_ids, kv_transfer_params
// MAIS : pas de #[serde(deny_unknown_fields)] → champs inconnus ignorés ✅

// Ligne 894-900
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Choice {
    pub index: usize,
    pub message: Message,
    pub logprobs: Option<serde_json::Value>,
    pub finish_reason: String,
}
// ❌ Pas de stop_reason, token_ids
// MAIS : pas de deny_unknown_fields → ignorés ✅

// Ligne 134-172 : Message enum (tagué par role)
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    Assistant {
        content: Vec<AssistantContent>,       // ← avec string_or_vec deserializer
        refusal: Option<String>,              // ← gère null ✅
        audio: Option<AudioAssistant>,        // ← gère null ✅
        name: Option<String>,
        tool_calls: Vec<ToolCall>,            // ← avec null_or_vec deserializer ✅
    },
    // ...
}
// ❌ Pas de reasoning, annotations, function_call
// Pour les enums tagués par serde, les champs inconnus DEVRAIENT être ignorés par défaut
```

### Le streaming path gère reasoning_content mais PAS le non-streaming

**Fichier** : `~/.cargo/registry/src/.../rig-core-0.33.0/src/providers/openai/completion/streaming.rs`

```rust
// Ligne 34-42
#[derive(Deserialize, Debug)]
struct StreamingDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,  // ← GÉRÉ dans streaming ✅
    #[serde(default, deserialize_with = "json_utils::null_or_vec")]
    tool_calls: Vec<StreamingToolCall>,
}
```

Le streaming delta a `reasoning_content: Option<String>` mais le `Message::Assistant` non-streaming n'a rien.

### Hypothèse du crash

Le crash `invalid type: null, expected a map` à colonne ~3000 indique que :

1. Quand qwen fait du thinking sur les retries, le `content` est très long (~3000 chars de `<think>...</think>` + JSON)
2. Le `content` field dans `Message::Assistant` utilise `string_or_vec` deserializer
3. Le deserializer essaie de parser le contenu comme un `Vec<AssistantContent>` — `AssistantContent` est un enum avec des variants qui sont des structs (maps)
4. Si le contenu est un mix de text et null, le deserializer échoue

**OU** (plus probable) :

1. Le champ `audio: null` est `Option<AudioAssistant>` mais `AudioAssistant` pourrait ne pas gérer `null` correctement si c'est un struct wrapper
2. Ou `prompt_tokens_details: null` dans Usage quand Usage est `Some(Usage { prompt_tokens_details: Option<PromptTokensDetails> })` — si vLLM renvoie `"usage": {"prompt_tokens": 80, "total_tokens": 95, "prompt_tokens_details": null}` ça devrait marcher... SAUF si `PromptTokensDetails` est deserializé depuis le parent context

**L'investigation exacte du champ fautif nécessite un test avec `RUST_LOG=trace`** ou un test unitaire qui essaie de désérialiser la réponse vLLM brute dans le struct rig-core.

---

## Reproduction

### Sur le VPS nk-jungo-vps (51.159.87.214)

```bash
# Le workflow test
cat /opt/nika/nk-jungo/workflows/test-qwen-structured.nika.yaml

# Lancer
curl -s -X POST http://51.159.87.214:3000/v1/run \
  -H "Authorization: Bearer 936f35a85300cc0aa64fb0978bba506f9f87902561bcf52e892a90eec6ec04dd" \
  -H "Content-Type: application/json" \
  -d '{"workflow":"test-qwen-structured.nika.yaml","inputs":{"topic":"Python"}}' | jq .

# Poll (attendre ~60-90s pour que les retries timeout)
curl -s "http://51.159.87.214:3000/v1/status/<JOB_ID>" \
  -H "Authorization: Bearer 936f35a85300cc0aa64fb0978bba506f9f87902561bcf52e892a90eec6ec04dd" | jq .
```

### En local avec test unitaire

```rust
// Créer un test qui désérialise une réponse vLLM brute
#[test]
fn test_vllm_response_deserialization() {
    let vllm_response = r#"{
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1712000000,
        "model": "qwen3.5-27b",
        "system_fingerprint": null,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "{\"name\":\"Rust\",\"category\":\"Systems\"}",
                "refusal": null,
                "annotations": null,
                "audio": null,
                "function_call": null,
                "tool_calls": [],
                "reasoning": null
            },
            "logprobs": null,
            "finish_reason": "stop",
            "stop_reason": null,
            "token_ids": null
        }],
        "usage": {
            "prompt_tokens": 80,
            "total_tokens": 95,
            "prompt_tokens_details": null
        },
        "prompt_logprobs": null,
        "prompt_token_ids": null,
        "kv_transfer_params": null
    }"#;

    // Ceci devrait passer si rig-core gère bien les champs inconnus
    let result: Result<rig::providers::openai::CompletionResponse, _> =
        serde_json::from_str(vllm_response);
    assert!(result.is_ok(), "Failed to deserialize vLLM response: {:?}", result.err());
}
```

### Vérification rapide depuis la H100

```bash
# Réponse brute de vLLM
curl -s http://51.159.153.241:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"qwen3.5-27b","messages":[{"role":"user","content":"Return JSON: {\"name\":\"test\"}"}],"temperature":0.1,"max_tokens":50}' | python3 -m json.tool

# Vérifier que le content est bien du JSON propre (sans fences quand system prompt demande du JSON)
curl -s http://51.159.153.241:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"qwen3.5-27b","messages":[{"role":"system","content":"Return ONLY valid JSON matching: {\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\"}},\"required\":[\"name\"]}"}, {"role":"user","content":"Topic: Rust"}],"temperature":0.1,"max_tokens":100}' | python3 -c "import sys,json; print(json.load(sys.stdin)['choices'][0]['message']['content'])"
```

---

## Architecture du structured output (pour contexte)

```
LLM response (raw text)
    │
    ▼
validate() [600s timeout]
    │
    ├─→ Layer 2: extract_json() + validate_schema_ref()
    │   File: structured_output.rs:507 (try_layer_2)
    │   JSON extraction: output.rs:80 (extract_json_from_output, 4 strategies)
    │   └→ Success? Return. Fail? → L3
    │
    ├─→ Layer 3: retry_with_feedback (loop, max_retries times)
    │   File: structured_output.rs:560 (try_layer_3)
    │   Calls: infer_fn(retry_prompt, max_tokens)  ← L3 RE-CALLS THE LLM
    │   infer_fn is: make_infer_callback() at executor/infer.rs:72
    │     → provider.infer_with_options() → rig-core agent.prompt() → DESERIALIZE RESPONSE ← CRASH HERE
    │   └→ Success? Return. Fail? → L4
    │
    ├─→ Layer 4: llm_repair
    │   File: structured_output.rs:741 (try_layer_4)
    │   Same callback → same crash
    │
    └─→ All fail: NIKA-303 StructuredOutputAllLayersFailed
```

**Le crash arrive quand L3/L4 re-appellent le LLM** (pas L2 qui utilise la réponse déjà parsée du premier infer).

Le callback est créé par `make_infer_callback()` (`executor/infer.rs:72-96`) qui appelle `provider.infer_with_options()` (`provider/rig/mod.rs:946`) qui finit dans `agent.prompt()` de rig-core qui désérialise la réponse via `CompletionResponse` struct.

---

## Plan de fix

### Option A : Intercepter la réponse vLLM avant rig-core (RECOMMANDÉ)

**Idée** : Ajouter un HTTP middleware/interceptor qui nettoie les champs non-standard de vLLM avant que rig-core les parse.

**Fichier** : `nika-engine/src/provider/rig/mod.rs`

Pour `OpenAiCompat`, au lieu de passer directement par `agent.prompt()`, intercepter la réponse HTTP brute :

```rust
// Option: custom HTTP client wrapper that strips non-standard fields
// Or: use reqwest directly, clean the JSON, then deserialize manually
```

**Pros** : Fix propre sans toucher à rig-core. Supporte tous les providers OpenAI-compat qui ajoutent des champs.
**Cons** : Faut parser le JSON deux fois (une pour nettoyer, une pour rig-core).

### Option B : Fork/patch rig-core CompletionResponse

**Idée** : Dans le fork ou patch de rig-core, ajouter `#[serde(flatten)] extra: HashMap<String, Value>` au `CompletionResponse` struct, ou rendre tous les champs plus permissifs.

**Pros** : Fix à la source.
**Cons** : Maintenance d'un fork.

### Option C : Utiliser le streaming path pour L3/L4

**Idée** : Le streaming path gère déjà `reasoning_content`. Modifier `make_infer_callback()` pour utiliser `infer_stream()` au lieu de `infer()` pour les retries.

**Fichier** : `executor/infer.rs:72-96`

```rust
fn make_infer_callback(provider: &RigProvider, model: Option<&str>) -> InferCallback {
    // ...
    Arc::new(move |retry_prompt, max_tokens| {
        // Use infer_stream instead of infer for L3/L4 retries
        // The streaming path handles vLLM's non-standard fields
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        provider.infer_stream(&retry_prompt, tx, model, max_tokens).await?;
        // Collect all chunks into a single string
        // ...
    })
}
```

**Pros** : Streaming already handles vLLM fields. Quick fix.
**Cons** : Performance overhead pour les retries. Weird pattern.

### Option D : Raw HTTP call for OpenAiCompat (MEILLEUR)

**Idée** : Pour `OpenAiCompat` (vLLM, custom endpoints), bypasser rig-core complètement et faire un `reqwest` call direct. Extraire le `content` du JSON brut sans tenter de désérialiser dans les structs rig-core.

**Fichier** : `provider/rig/mod.rs:548-564` (le block `OpenAiCompat` dans `infer()`)

```rust
RigProvider::OpenAiCompat { client, timeout_secs, .. } => {
    // Instead of: agent.prompt(prompt) which goes through rig-core deserialization
    // Do: raw HTTP POST, extract choices[0].message.content from JSON Value
    let url = format!("{}/chat/completions", client.base_url());
    let body = json!({
        "model": model_id,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": effective_max_tokens,
    });
    let resp: Value = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .timeout(compat_timeout)
        .send().await?.json().await?;
    let content = resp["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| ...)?
        .to_string();
    Ok(content)
}
```

**Pros** : Bypass complet du problème. Compatible avec TOUS les providers OpenAI-compat (vLLM, LiteLLM, Ollama, etc.). Pas de fork rig-core. Robuste.
**Cons** : Duplique un peu la logique HTTP. Perd le streaming/tool handling de rig-core (pas grave pour L3/L4 retries).

---

## Fix recommandé : Option D pour le `infer()` non-streaming

### Étape 1 : Identifier le scope
- Seul le path non-streaming (`infer()` et `infer_with_options()`) est affecté
- Le streaming (`infer_stream()`) fonctionne déjà (il a `reasoning_content: Option<String>`)
- Seul `OpenAiCompat` variant est affecté (les vrais providers OpenAI/Claude/xAI renvoient des réponses propres)

### Étape 2 : Implémenter le raw HTTP path
**Fichier** : `nika-engine/src/provider/rig/mod.rs`

Dans les méthodes `infer()` (ligne 548) et `infer_with_options()` (chercher le block `OpenAiCompat`), remplacer `agent.prompt()` par un call HTTP direct.

### Étape 3 : Ajouter `strip_think_tags` au résultat
Qwen met parfois du `<think>...</think>` dans le content. Le callback fait déjà `strip_think_tags()` (infer.rs:90). Vérifier que le raw HTTP path aussi.

### Étape 4 : Test
```bash
# Sur le VPS
~/.nika/bin/nika run /opt/nika/workflows/test-qwen-structured.nika.yaml

# Ou via serve
curl -s -X POST http://51.159.87.214:3000/v1/run \
  -H "Authorization: Bearer 936f35a85300cc0aa64fb0978bba506f9f87902561bcf52e892a90eec6ec04dd" \
  -H "Content-Type: application/json" \
  -d '{"workflow":"test-qwen-structured.nika.yaml","inputs":{"topic":"Rust"}}'
```

### Étape 5 : Vérifier les regressions
```bash
cd tools && cargo test --workspace --lib
# Tester aussi avec les vrais providers (OpenAI, xAI) pour vérifier que rien ne casse
```

---

## Infra (pour les tests)

```
PAR 2 (fr-par-2) — Private Network — nk-sg
┌────────────────────────────────────────────────────────┐
│  nk-dev-vps    PLAY2-MICRO  51.159.167.12  v0.58.1   │
│  nk-jungo-vps  PLAY2-MICRO  51.159.87.214  v0.58.1   │
│  nk-h100-gpu   H100-1-80G   51.159.153.241 vLLM      │
│    └── qwen3.5-27b (GPTQ-Int4, 64K context, :8000)   │
└────────────────────────────────────────────────────────┘

Tokens:
  nk-dev-vps:   nk-dev-token-a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6
  nk-jungo-vps: 936f35a85300cc0aa64fb0978bba506f9f87902561bcf52e892a90eec6ec04dd

SSH: ssh root@51.159.87.214 / ssh root@51.159.167.12 / ssh root@51.159.153.241
```

### Workflows de test sur les VPS

```
/opt/nika/workflows/
├── test-mock.nika.yaml            ✅ (0 API, mock provider)
├── test-hello.nika.yaml           ✅ (OpenAI gpt-4.1-mini)
├── test-chain.nika.yaml           ✅ (OpenAI, depends_on)
├── test-exec.nika.yaml            ✅ (exec verb)
├── test-verify-transforms.nika.yaml ✅ (exec, 0 LLM)
├── test-qwen-h100.nika.yaml      ✅ (qwen simple infer)
├── test-qwen-chain.nika.yaml     ✅ (qwen 2-step chain)
├── test-qwen-parallel.nika.yaml  ✅ (qwen fan-out 3→merge)
├── test-qwen-fallback.nika.yaml  ✅ (qwen H100 → OpenAI cloud)
└── test-qwen-structured.nika.yaml ❌ (qwen structured — LE BUG)
```

### Upgrade script

```bash
# Après le fix, release v0.58.2 :
./scripts/release.sh nika 0.58.2

# Puis upgrade les VPS :
cat docs/05-operations/stack/scaleway/nika-upgrade.sh | ssh root@51.159.87.214 'bash -s 0.58.2'
cat docs/05-operations/stack/scaleway/nika-upgrade.sh | ssh root@51.159.167.12 'bash -s 0.58.2'
# Note: il faut pkill -9 nika AVANT le cp (Text file busy error)
```

---

## Fichiers clés

| Fichier | Rôle |
|---------|------|
| `nika-engine/src/provider/rig/mod.rs:449-578` | `infer()` — le non-streaming path qui crash |
| `nika-engine/src/provider/rig/mod.rs:946+` | `infer_with_options()` — même crash |
| `nika-engine/src/runtime/executor/infer.rs:72-96` | `make_infer_callback()` — crée le callback pour L3/L4 |
| `nika-engine/src/runtime/structured_output.rs:560` | Layer 3 — appelle `infer_fn()` |
| `nika-engine/src/runtime/structured_output.rs:741` | Layer 4 — appelle `infer_fn()` |
| `nika-engine/src/runtime/output.rs:80` | `extract_json_from_output()` — extraction JSON (fonctionne) |
| `~/.cargo/.../rig-core-0.33.0/src/providers/openai/completion/mod.rs:772` | CompletionResponse struct |
| `~/.cargo/.../rig-core-0.33.0/src/providers/openai/completion/streaming.rs:34` | StreamingDelta (a `reasoning_content`) |

## Contexte additionnel

- vLLM version : 0.18.0
- rig-core version : 0.33.0
- Le streaming path fonctionne (test-qwen-h100.nika.yaml passe) car il utilise `StreamingDelta` qui a `reasoning_content: Option<String>` et `#[serde(default)]` sur tous les champs
- Le non-streaming path (utilisé par L3/L4 retries) crash car `CompletionResponse` / `Message::Assistant` n'a pas tous les champs vLLM
- Les 4 autres workflows qwen passent car ils n'ont pas `structured:` → pas de retry → pas de non-streaming call

---

## Critères de succès

1. `test-qwen-structured.nika.yaml` passe (status: completed)
2. Les workflows existants (OpenAI, xAI, mock) ne régressent pas
3. `cargo test --workspace --lib` passe
4. `cargo clippy --workspace -- -D warnings` passe
