# Session 15 — Mega-prompt Handoff (draft)

> ⚠️ **SUPERSEDED 2026-04-11** — voir `23-session15-mega-prompt.md` pour
> le mega-prompt post-review enrichi.
>
> Ce doc est le DRAFT v1 + l'enrichissement Wave C de Sonnet. Le post-S14
> review (4 agents : code-reviewer, rust-architect ×2, code-explorer) a
> ensuite produit un mega-prompt complet `23-session15-mega-prompt.md`
> qui intègre :
> - L'état réel post-S14 + S14.5 hotfix (HEAD `144f5abeb`)
> - Le 8-commit sequence McpPool (S15-A0 → A7)
> - Les DTOs kernel ready-to-paste
> - Les top 3 traps + anti-goals
> - Les invariants #23/#24/#25 codifiés
> - Les debts #1–#7 (McpPool, infer 7 sites, reqwest leak, fractional backoff,
>   finish_reason_raw dead, Wave C blocked, Wave D dispatch blocked)
>
> Ce doc et `22-agent-v2-design.md` restent référence pour la partie
> Wave C / agent v2 (separate concern from S15 McpPool).
>
> ---
>
> **Auteur / date:** Claude Opus 4.6, 2026-04-11 (after S14 execution)
> **Enrichi:** Claude Sonnet 4.6, 2026-04-11 (brainstorm agent v2 + Wave C architecture)
> **Source:** Phase 1 review S14 (4 agents) + S14 execution postmortem + brainstorm 2 agents (Explore haiku + WebResearcher sonnet)
> **Gitignored** — local only, ne jamais push
> **Baseline attendue:** post-S14, HEAD ≈ eaa7f16c2 + 5 commits (S14-α à ε)
>
> **Companion doc:** `22-agent-v2-design.md` — architecture complète Wave C + features agent v2

---

## 0. Orientation

```
Working dir : tools/   (submodule public dans un monorepo privé)
Tests       : cargo test --workspace --lib   (jamais --test — Keychain macOS)
Co-author   : Co-Authored-By: Nika 🦋 <nika@supernovae.studio>   (JAMAIS Claude)
Launch      : 2026-05-05  — refactor partiel = état valide
Langue      : Franglais conversation, EN code/docs/commits
```

---

## 1. Baseline attendue (post-S14)

```
HEAD expected : ~eaa7f16c2 + 5 S14 commits (α/β/γ/δ/ε)
Crates        : 33 (inchangé)
Engine LOC    : ~146,220 (−253 vs S14 début, grâce à fetch helpers migration)
Tests         : ~10,850+ lib
Clippy        : 0
```

**Commits S14 attendus sur main** (si l'exécution est complète):
```
S14-α  feat(kernel): enrich InferEvent::Done as struct variant with request_id + finish_reason_raw
S14-β  refactor(verb-fetch): migrate pure helpers (backoff, parse_retry_after, hreflang) from engine
S14-γ  feat(verb-fetch): add RetryExhausted + DeadlineExceeded error variants (S15 prep)
S14-δ  test(verb-infer): golden oracle asserts all ProviderResponded fields (S12-G2)
S14-ε  feat(verb-exec): pre-spawn cancellation check + test
```

**Action pré-flight #0**: `git log --oneline -15` pour confirmer les 5 commits landed sans drift.

---

## 2. Carte extraction (expected state post-S14)

```
VERBE       CRATE              BRIDGE ENGINE         DISPATCH ARM
─────────────────────────────────────────────────────────────────
exec    ✅ S13-B1 + S14-ε     ✅ S13-B2              ✅ S13-B4
fetch   ✅ S13-D1 + S14-β/γ   ❌ NON BRIDGÉ          ✅ S13-D2 (NotImpl)
invoke  ✅ S13-C1 (builtins)  ✅ S13-C2 (partial)    ✅ S13-C3
infer   ✅ W14-B1 + S14-α/δ   ❌ NON BRIDGÉ          ✅ W14-B3 (NotImpl)
agent   ❌ N'EXISTE PAS       ❌                      ❌ (S15 target)
─────────────────────────────────────────────────────────────────
```

S14 a consolidé les 4 verb crates existantes. S15 doit:
1. Faire tomber les bridges `infer` + `fetch` (flip live path)
2. Créer nika-verb-agent
3. Activer dispatch() live pour les verbs bridgés

---

## 3. Ce qui est bloqué — les vraies dettes architecturales

### Debt #1 — McpPool trait trop mince

**Constat** (code-explorer S14 Phase 1):

| Surface manquante | Impact | Bloquant pour |
|---|---|---|
| `call_tool` retourne `serde_json::Value` au lieu d'un struct kernel-local `McpToolResult` avec `is_error`, `was_cached`, `content_size_bytes()`, `has_media()` | Engine bridge perd tool-level errors, cache flags, 50MB cap | McpPoolAdapter, W14-B0 réel |
| `read_resource` retourne `String` au lieu de `McpResourceContent { text, blob, mime_type, uri }` | Perd le blob pour pipeline media | W14-B4 |
| Pas de `call_tool_with_retry_events(task_id, event_log)` surface | McpRetry events émis par McpClient, impossible à reproduire derrière trait mince | McpRetry event ordering preservation |
| Pas de cancel token dans les signatures trait | Adapter ne peut pas threader cancellation pendant `get_or_connect()` | Cancel semantics lors de server spawn |

**Action S15**:
1. Ajouter `nika-kernel::mcp::McpToolResult { content, is_error, was_cached, size_bytes }`
2. Ajouter `nika-kernel::mcp::McpResourceContent { text: Option<String>, blob: Option<Vec<u8>>, mime_type: Option<String>, uri: String }`
3. Modifier `McpPool::call_tool` signature: `-> Result<McpToolResult, McpError>`
4. Modifier `McpPool::read_resource` signature: `-> Result<McpResourceContent, McpError>`
5. **Décision S15**: event emission dans trait ou dans adapter?
   - Option A — trait `call_tool(&self, server, tool, args, task_id, event_emitter: &dyn EventEmitter)`
   - Option B — adapter wraps `call_tool_with_retry_events` internally, trait reste mince, adapter gère events + retry
   - **Recommandation Phase 1**: Option B. Le trait reste mince, l'adapter dans nika-engine garde la responsabilité d'emission + retry. Verb crate appelle `caps.mcp_pool.call_tool()` et voit seulement le résultat final.
6. Ajouter cancellation: soit `cancel: Option<&CancellationToken>` dans les méthodes, soit structure `CallOptions { cancel, timeout }` comme param additionnel.

### Debt #2 — fetch retry loop orchestration

**Constat** (rust-pro S14 Phase 1):

Le retry loop dans `fetch.rs:477-1142` (665 LOC) est interleaved avec 6 response-handling modes:
- `slim` (body only, no parse)
- `full` (body + headers + redirect chain via `response: full`)
- `binary` (CAS store via `BlobStore`)
- `llm_txt` (sub-requests for `/llms.txt`, `/.well-known/llms.txt`)
- `304 cache` (`fetch_cache.rs` ETag check)
- `default + extract` (markdown/article/text/selector/metadata/links/jsonpath/feed/llm_txt modes via nika-extract)

Chacun touche: `datastore: &RunContext`, `fetch_cache`, `self.cas`, `template_resolve()`, `MediaRef`, et la closure SSRF dans `redirect::Policy::custom()` qui capture `allowed_hosts`.

**Décision**: **pas possible d'extraire le retry loop sans déplacer simultanément**:
- `fetch_cache` (L2, sits in engine)
- `template_resolve` + `RunContext` (couplage profond au runtime)
- `MediaRef` construction (nika-media déjà L2)
- SSRF post-redirect hook (nika-policy L1 mais la closure est baked dans reqwest::Client)

**Action S15 (proposed)**:
1. Créer `nika-verb-fetch::run_with_retry(input, caps, event_log) -> Result<VerbFetchResponse, VerbFetchError>`
2. Déplacer uniquement le retry control flow (~160 LOC) + les branches 503/429/network-error
3. **Garder en bridge**: response body handling (slim/full/binary/llm_txt/304-cache/extract) comme un post-processor callback
4. Pattern: le verb crate retourne `reqwest::Response` (ou un owned clone), le bridge fait le switch sur `extract:` et écrit dans `datastore`.
5. Alternative plus propre: introduire un `FetchResponseHandler` trait dans le kernel avec default impl qui fait passthrough.

**Risque**: cette extraction est isomorphe à W14-B2 (infer.rs bridge surgery). Peut-être que S15 est trop ambitieux et qu'il faut séparer:
- S15a: McpPool trait expansion + McpPoolAdapter
- S15b: fetch retry orchestration
- S16: infer bridge W14-B2 + agent Wave C

### Debt #3 — W14-B2 infer.rs bridge surgery

**Constat** (rust-async-expert S14 Phase 1):

`infer.rs` actuel (2157 LOC) contient 2 sites d'emission `ProviderResponded`:
- L621 (mock path)
- L1136 (streaming path)

Quand le bridge flip pour déléguer à `nika_verb_infer::run()`, **les deux sites doivent être supprimés atomiquement** ou le golden oracle verra 2 `ProviderResponded` par infer call.

**Autres bloqueurs W14-B2** (détaillés dans `14-session12-handoff-postmortem.md`):
- `SkillInjector` (compile-time skill resolution)
- `StructuredOutputEngine` (5-layer defense pipeline + LLM repair)
- Spotlight + canary wrapping
- Vision content blocks
- Streaming with event emission ordering
- `InferCallback` signature

**Action S15**:
1. Décider si W14-B2 se fait en S15 ou S16
2. Si S15: séquence recommandée est (a) expand `nika-kernel::provider` surface pour accepter skill/canary/spotlight caps via `InferRequest.extras`, (b) réécrire `nika-verb-infer::run_full()` qui consomme streaming + structured output, (c) flip bridge avec les 2 sites supprimés dans le même commit

### Debt #4 — NoopMcpPool shim removal (W14-E0)

**Sites**:
- `nika-engine/src/runtime/executor/invoke.rs:342` (builtin path, jamais appelé par builtins)
- `nika-runtime/src/dispatch.rs:144` (test helper `VerbCapabilities::for_tests()`)

**Bloqué sur**: Debt #1 (McpPoolAdapter doit exister avant que le shim soit removable).

**Action S15**: après McpPoolAdapter landed, remplacer les 2 sites par `McpPoolAdapter::new()` ou équivalent. Supprimer `NoopMcpPool` struct.

### Debt #5 — Wave C: nika-verb-agent

> **Architecture complète dans `22-agent-v2-design.md`.**
> Cette section résume les décisions clés ; le design doc a les specs Rust complètes.

#### 5.1 Décision architecturale : frontière rig

**On ne forke pas rig.** On bypasse son haut niveau (AgentBuilder, MultiTurnChat, ToolDyn).
On garde son bas niveau (CompletionModel, clients HTTP providers) via RigProvider.

```
AUJOURD'HUI   TaskExecutor → rig AgentBuilder → rig MultiTurnChat (loop) → rig CompletionModel → HTTP
CIBLE S15     nika-verb-agent (notre loop) → Provider::infer (kernel) → RigProvider → rig CompletionModel → HTTP
```

rig reste notre couche transport. La boucle multi-tour nous appartient.

#### 5.2 Extensions kernel requises (réalité post-vérification code)

> ⚠️ **Correction post-review code** — La version initiale proposait des types qui
> n'existent pas et ratait des types existants. Vérification faite contre provider.rs,
> builtin.rs, mcp.rs, caps.rs. Voir `22-agent-v2-design.md` section 3 pour le détail complet.

**CE QUI EXISTE DÉJÀ (aucun travail S15 requis) :**
```rust
// provider.rs — TOUS présents post-S14
pub struct ToolDef { name, description, parameters: Value }  // PAS ToolDefinition !
pub enum ToolChoice { Auto, Required, None, Specific(String) } // PAS Tool(String) !
pub enum StopReason { EndTurn, MaxTokens, StopSequence, ToolUse, ContentFilter, Unknown }
// InferRequest.tools: Vec<ToolDef> — DÉJÀ LÀ ✓
// InferRequest.tool_choice: ToolChoice — DÉJÀ LÀ ✓
// ContentBlock::ToolUse { id, name, input: Value } — tool calls dans content
// ContentBlock::ToolResult { tool_use_id, content: String, is_error } — résultats
// Message { role: Role, content: Vec<ContentBlock> } — PAS un enum InferMessage !
```

**CE QUI MANQUE RÉELLEMENT (vrai travail Wave C) :**
```rust
// BuiltinRouter (builtin.rs) — AJOUTER:
fn tool_definitions(&self) -> Vec<ToolDef>;   // sync — catalogue statique
// Existant: fn dispatch(tool, args: String) -> Pin<Box<...>>  ← notre call_tool
// Existant: fn knows(tool: &str) -> bool  ← PAS has_tool() !

// McpPool (mcp.rs) — AJOUTER (combiné avec S15-A0/A1):
fn tool_definitions<'a>(&'a self, servers: &'a [String])
    -> Pin<Box<dyn Future<Output = Result<Vec<ToolDef>, McpError>> + Send + 'a>>;
// Existant: call_tool(server, tool, args: Value) -> Result<Value, McpError>
// Existant: has_server(&str) -> bool  ← PAS has_tool() !

// AgentCaps (caps.rs) — AJOUTER UN CHAMP au struct existant (11 → 12):
pub record_query: &'a dyn RecordQuery,  // pour inject_records
// Le struct existant utilise &'a dyn Trait (pas Arc<dyn>) — respecter le pattern !
```

#### 5.3 nika-verb-agent — loop minimum (W15-CA0)

Pattern identique à nika-verb-infer (W14-B1). Minimum viable :

```rust
pub struct AgentInput {
    pub prompt: String,
    pub system: Option<String>,
    pub max_turns: u32,               // default: 10
    pub tools: Vec<String>,           // noms tools autorisés
    pub tool_choice: ToolChoice,
    pub task_id: Arc<str>,
    // Features v2 (S16)
    pub inject_records: Option<InjectRecordsConfig>,
    pub reflection: Option<ReflectionConfig>,
    pub session_id: Option<String>,
    pub resume_session: Option<String>,
    pub parallel_tools: bool,         // default: true
    pub concurrency_limit: usize,     // default: 8
}

// AgentCaps EXISTE DÉJÀ dans nika-kernel::caps avec 11 champs (pattern &'a dyn)
// nika-verb-agent importe AgentCaps depuis nika-kernel — NE PAS redéfinir localement.
// Seule extension : AgentCaps::new() doit accepter record_query comme 12e argument.
// PAS Arc<dyn> — le pattern kernel est &'a dyn (sauf provider: Arc<dyn Provider>).
// Voir caps.rs pour le struct complet : provider(Arc), builtin_router/mcp_pool/...(&'a dyn)

pub async fn run(input: &AgentInput, caps: &AgentCaps<'_>, event_log: &EventLog)
    -> Result<AgentOutput, VerbAgentError>
```

Le loop : `Provider::infer` avec tools → si `tool_calls` non vide → dispatch → append history → répéter.
Pas de rig::AgentBuilder. Pas de rig::MultiTurnChat. Pas de rig::ToolDyn.

#### 5.4 Features agent v2 (S16 — après extraction minimum S15)

Backing research dans `22-agent-v2-design.md` section 1 :

| Feature | YAML | Research | Gain mesuré |
|---|---|---|---|
| Parallel tool dispatch | `parallel_tools: true` | M1-Parallel (ICML 2025) | 1.6-10× latency |
| Karpathy pattern | `inject_records:` | Karpathy AutoResearch 2026 | +11% efficiency |
| Reflexion gate | `reflection:` | Shinn et al. NeurIPS 2023 | 91% AlfWorld (vs 25%) |
| Session CAS | `session_id:` / `resume_session:` | OpenAI Threads model | continuité cross-run |
| Sleep consolidation | agent `depends_on:` | Letta arXiv 2504.13171 | +13-18% stateful |

#### 5.5 TEMP deps du rig_agent_loop actuel

Les 10 TEMP deps existantes restent dans le bridge engine (agent.rs ~26KB). Elles ne
migrent PAS dans nika-verb-agent minimum. Pattern = même que nika-verb-infer W14-B1 :
extraction minimum prouvée compile + tests, bridge surgery (W16-CA6) dans la session suivante.

```
1. SkillInjector          → TEMP dans bridge, migre S16
2. LimitTracker           → TEMP dans bridge, migre S16
3. DynamicSubmitTool      → TEMP dans bridge, supprimé quand structured: migre
4. NikaMcpTool            → remplacé par McpPool::call_tool dans verb crate
5. ProviderKind           → Provider trait, pas besoin dans verb crate
6. STREAM_CHUNK_TIMEOUT   → TEMP dans bridge, migre S16
7. EngineRunExecutor      → remplacé par RunExecutor trait kernel (S16)
8. KernelToolAdapter      → remplacé par BuiltinRouter::call_tool
9. SecurityContext        → PolicyChecker via caps (S16)
10. Guardrails runner     → reflection: config dans AgentInput (S16)
```

---

## 4. Scope proposé S15

### Option A — "Conservateur, focus McpPool"

Commits:
1. `S15-A0` Kernel: `McpToolResult` + `McpResourceContent` types
2. `S15-A1` Kernel: `McpPool` trait signatures updated
3. `S15-B0` Engine: `McpPoolAdapter` in `kernel_bridge::mcp_pool_adapter`
4. `S15-B1` nika-verb-invoke: wire real adapter + remove NoopMcpPool from invoke.rs
5. `S15-B2` nika-runtime: remove NoopMcpPool from dispatch.rs test helper
6. `S15-C0` W14-B4: resource reads via McpPool (blocked on A1)
7. `S15-E0` Engine: delete NoopMcpPool struct
8. `S15-E1` docs ARCHITECTURE.md update

**Pros**: scope contrôlé, déroule proprement. Clears Debts #1, #4.
**Cons**: ne touche pas fetch retry ni infer bridge. Engine LOC unchanged (~0).

### Option B — "Ambitieux, deux bridges en 1 session"

En plus de Option A:
9. `S15-F0` nika-verb-fetch: retry loop orchestration (~160 LOC)
10. `S15-F1` Engine: flip fetch bridge to `nika_verb_fetch::run_with_retry()`
11. `S15-G0` nika-verb-infer: run_full() with structured output support
12. `S15-G1` Engine: flip infer bridge atomic double-emit cleanup

**Pros**: engine LOC drop significatif (~−2000 probablement). Debts #1-#3 addressed.
**Cons**: très gros scope, risque de session overrun. Chaque flip = risque P0 si golden oracle pas preventive.

### Option C — "Realistic, McpPool + fetch retry"

Option A + items 9 + 10 seulement. Infer bridge (W14-B2) pushed to S16.

**Recommandation Phase 1**: **Option C**. McpPool est le plus bloquant (4 debts en dépendent). Fetch retry est isomorphe mais sans les caps kernel extensions donc plus simple que infer. Infer = biggest beast, mérite sa propre session.

### Option D — "Wave C foundations + McpPool" ← NOUVEAU (post-brainstorm 2026-04-11)

**Motivation** : les extensions kernel requises pour Wave C (W15-CK0 à CK5) sont
**compatibles et complémentaires** avec McpPool expansion (S15-A0/A1). McpPool.tool_definitions
et BuiltinRouter.call_tool sont des surfaces nécessaires pour nika-verb-agent AND pour McpPoolAdapter.
Faire les deux en S15 n'ajoute pas de risque — les kernel types sont additifs.

Commits (Option A + kernel extensions + verb-agent min) :
> ⚠️ Correction post-review : CK0/1/2/3 étaient "à créer" — ils EXISTENT DÉJÀ.
> Scope réel S15 Wave C = seulement CK4/CK5 + CP0 + CA0/CA1. Gain de ~4 commits.

1. `S15-A0` Kernel: `McpToolResult` + `McpResourceContent` types
2. `S15-A1` Kernel: `McpPool` trait signatures + `tool_definitions` (Pin<Box<...>> pattern)
            ~~CK0~~ DÉJÀ FAIT: ToolDef, ToolChoice::Specific, InferRequest.tools, StopReason::ToolUse
            ~~CK1~~ DÉJÀ FAIT: InferRequest.tools + tool_choice déjà présents
            ~~CK2~~ DÉJÀ FAIT: ContentBlock::ToolUse/ToolResult déjà là, StopReason::ToolUse déjà là
            ~~CK3~~ N/A: InferMessage n'existe pas — kernel utilise Message + ContentBlock
3. `S15-CK4` Kernel: `BuiltinRouter` += `tool_definitions() -> Vec<ToolDef>` (sync)
             Kernel: `AgentCaps` += `record_query: &'a dyn RecordQuery` (12e champ)
4. `S15-CK5` Kernel: `McpPool` += `tool_definitions()` — combiné avec S15-A1
5. `S15-B0`  Engine: `McpPoolAdapter` in `kernel_bridge::mcp_pool_adapter`
6. `S15-B1`  nika-verb-invoke: wire real adapter + remove NoopMcpPool from invoke.rs
7. `S15-B2`  nika-runtime: remove NoopMcpPool from dispatch.rs test helper
8. `S15-CP0` Engine: `RigProvider` tool translation (Vec<ToolDef> → rig format)
             + extract `ContentBlock::ToolUse` depuis réponse rig
9. `S15-CA0` feat(verb-agent): nika-verb-agent crate — loop minimum, sequential tools
             Uses: Vec<Message>, ContentBlock::ToolUse/ToolResult, ToolDef, ToolChoice::Specific
             5 tests minimum (cancel, no-tools, tool-call, max-turns, stop-reason)
10. `S15-CA1` feat(runtime): verb_agent adapter + AgentCaps::new(12 args) in VerbCapabilities
11. `S15-C0`  W14-B4: resource reads via McpPool
12. `S15-E0`  Engine: delete `NoopMcpPool` struct
13. `S15-E1`  docs: ARCHITECTURE.md update (invariants #23-#31)

**Pros**: kernel extensions sont fondationnelles — elles débloqueront S16 features sans
session de kernel setup dédiée. nika-verb-agent minimum (S15-CA0) = même pattern que
W14-B1, prouvé faisable. Pas de bridge surgery engine en S15 — juste l'extraction.
**Cons**: scope plus large que Option C (~16 commits vs ~8). Mais les CK commits sont
petits et additifs — risque réel limité.

**Recommandation révisée** : **Option D si session S15 est longue, Option C si contrainte de temps.**
Décider en Phase 1 review après audit des callsites McpPool et BuiltinRouter.

---

## 5. Sacred invariants hérités (à respecter en S15)

Tous les invariants #1-#22 de S12/S13/S14 s'appliquent. Nouveaux en S15 probablement:

**Invariant #23 (proposed)**: **Trait expansion commits before adapter impls.** Never land an adapter that would need a trait method that doesn't exist yet — split into 2 commits minimum (trait signature change + adapter impl).

**Invariant #24 (proposed)**: **Event emission sites are singletons.** For any `EventKind` variant, there must be exactly one code path that emits it. Double-emission (like `ProviderResponded` at `infer.rs:621` + `:1136`) is a latent bug that must be resolved before flipping bridges.

**Invariant #25 (proposed)**: **Cancel semantics are documented at the verb-crate error boundary.** When a `Cancelled` error is returned, the doc comment on the variant MUST specify whether side effects (MCP tool call, HTTP request, subprocess) completed or were aborted. Silent "cancel = nothing happened" assumptions are bugs in waiting.

**Invariant #26 (proposed — Wave C)**: **nika-verb-agent n'importe jamais de types rig-core.** `Provider` trait est la seule interface LLM dans le verb crate. Si du code dans nika-verb-agent référence `AgentBuilder`, `MultiTurnChat`, `ToolDyn`, ou `CompletionModel`, c'est une violation de layering. rig-core reste interne à RigProvider (L2).

**Invariant #27 (proposed — Wave C)**: **Parallel tool dispatch est bounded.** `concurrency_limit` (default: 8) appliqué via `futures::stream::buffer_unordered()`. `join_all()` non borné sur une liste user-contrôlée = DoS vector. Interdit.

**Invariant #28 (proposed — Wave C)**: **Reflection MUST use structured output.** Schema minimum `{score: number, issues: array}` via `Provider::infer`. Jamais de regex sur free-form text pour extraire un score — parsing non-robuste masqué.

**Invariant #29 (proposed — Wave C)**: **`session_id` / `resume_session` scopés au projet nika.** La clé CAS inclut le project fingerprint. Sessions cross-projet = contamination de contexte, bug de correctness.

**Invariant #30 (proposed — Wave C)**: **Events `AgentTurnStarted` / `AgentTurnCompleted` émis uniquement dans `nika-verb-agent::run()`.** Pas dans les helpers dispatch. Un point d'émission par event kind (généralise invariant #24).

**Invariant #31 (proposed — Wave C)**: **`agent:` est genuinement itératif.** `agent:` sans tool calls ET sans reflection = `infer:` dégradé. Linter rule `L-AGT-001` doit le détecter. (Source : Agentless paper, FSE 2025.)

---

## 6. Pré-flight ritual S15

```bash
cd tools/
git log --oneline -25                                          # verify S14 landed
cargo check --workspace 2>&1 | grep "^error"                   # 0
cargo check --workspace --no-default-features 2>&1 | grep "^error"  # 0
cargo test --workspace --lib 2>&1 | tail -3                    # ok. 10850+ passed
cargo clippy --workspace --all-targets 2>&1 | grep "^warning"  # 0

# Phase 0: verify Debt status
grep -n "NoopMcpPool" tools/                                   # 2 sites expected
wc -l tools/nika-engine/src/runtime/executor/infer.rs          # ~2157 expected
wc -l tools/nika-engine/src/runtime/executor/fetch.rs          # ~1160 expected (post S14-β)
```

---

## 7. Phase 1 review dispatch (S15 opening move)

Ne pas répéter l'erreur S14 — dispatch les agents AVANT d'écrire du code:

**Agents à dispatcher en parallèle** (si Option D retenu, sinon 1-4 seulement pour Option C):
1. **rust-architect** — valider le design `McpToolResult` / `McpResourceContent` + `ToolDefinition` / `ToolCall` et décider Option A/C/D
2. **rust-pro** — mapper les dependencies de `call_tool_with_retry_events` (quels fields de `ToolCallResult` survivent, cache semantics, error propagation)
3. **code-explorer** — trace toutes les callsites de `McpClient::call_tool` dans engine **ET** les callsites de `rig::AgentBuilder` / `rig::MultiTurnChat` pour Wave C import surgery
4. **rust-async-expert** — valider les cancel semantics du nouvel adapter (pre-connect, during-call, post-result). Audit !Send guards dans `McpClientPool`.
5. **rust-architect** (second agent) — valider l'architecture `nika-verb-agent::run()` depuis `22-agent-v2-design.md` : boucle correcte ? RigProvider tool translation correcte ? AgentCaps complet ?

**Temps estimé Phase 1**: ~20 min. **Temps économisé**: probablement 3-4h de code qui serait à refaire.

---

## 8. Anti-goals pour S15

**Ne PAS faire en S15**:
- ❌ Nouvelle trait method `infer_with_vision` / `infer_with_tools` / `infer_agent` (invariant #17)
- ❌ "Moderniser" ou "simplifier" du code qui fonctionne déjà
- ❌ Extraire du code sans avoir une migration path testable
- ❌ Toucher à des TEMP deps sans comment `# TEMP: clears when X` explicit
- ❌ Commit avec `cargo check` green mais `cargo test --lib` failing (invariant S14)
- ❌ Supprimer `NoopMcpPool` AVANT que l'adapter réel soit wired dans les 2 sites
- ❌ Importer `rig-core` types dans nika-verb-agent (invariant #26)
- ❌ `join_all()` non borné sur tool_calls (invariant #27)
- ❌ Implémenter `inject_records` / `reflection` / `resume_session` en S15 — features v2 = S16
- ❌ Faire la bridge surgery agent.rs en S15 — minimum extraction seulement, bridge surgery = S16
- ❌ Forker rig — contribuer upstream si besoin, jamais fork

---

## 9. Feedback loop au user

Après Phase 1 review, présenter au user:
1. Les findings des 5 agents
2. Le choix Option A / C / D avec tradeoffs (D inclut Wave C kernel foundations)
3. Les GATE items (si découvertes architecturales émergent)
4. Un ordre de commits précis avec verification ritual

**Pattern à suivre**: exactement ce qu'a fait Phase 1 S14. Ne jamais exécuter sans user sign-off sur les GATE decisions.

---

## 10. S16 preview (post-S15 Option D)

Si S15 livre Option D (McpPool + kernel extensions + verb-agent minimum) :

```
S16-A  feat(verb-agent): parallel tool dispatch (buffer_unordered)
S16-B  feat(verb-agent): inject_records via RecordQuery
S16-C  feat(verb-agent): reflection gate (Reflexion paper pattern)
S16-D  feat(verb-agent): resume_session / session_id via BlobStore
S16-E  feat(engine): agent.rs bridge surgery (26KB → ~300 LOC bridge)
S16-F  feat(verb-fetch): retry loop orchestration (si pas fait en S15)
S16-G  feat(verb-infer): run_full() bridge surgery (W14-B2, infer.rs ~1980 LOC)
S16-H  feat(runtime): activate dispatch() as live path (AMEND-4 flip)
S16-I  feat(engine): TaskExecutor dissolution (bloquait sur agent extraction)
S16-J  docs: ARCHITECTURE.md final update + invariants #26-#31 merged
```

S16 = session la plus impactante du Constellation refactor : engine passe de ~146K à <120K LOC,
dispatch() devient le chemin live, TaskExecutor disparaît, agent v2 features sont en prod.

**S17** : Runner migration vers nika-runtime, engine ≤100K LOC target atteint, fin Constellation V2.3.
