# Agent v2 — Design Document (Wave C + Feature Roadmap)

> **Auteur / date:** Claude Sonnet 4.6, 2026-04-11
> **Enrichi + corrigé:** Claude Sonnet 4.6, 2026-04-11
>   — 4 agents parallèles (code-reviewer, rust-architect, explore haiku, web-researcher)
>   — vérification code réel post-brainstorm (caps.rs, provider.rs, builtin.rs, mcp.rs)
> **Source:** Brainstorm session + synthèse architecture Constellation V2.3
> **Gitignored** — local uniquement, ne jamais push
> **Session cible:** Wave C = S15 (kernel extensions + loop min) → features S16
>
> ✅ **CORRECTIONS POST-REVIEW APPLIQUÉES** — Le design doc initial proposait des types
> qui n'existent pas (`InferMessage`, `ToolDefinition`) et ratait des types existants
> (`ToolDef`, `ToolChoice::Specific`, `InferRequest.tools`, `StopReason::ToolUse`).
> Le doc a été réécrit contre le code réel de nika-kernel.

---

## 0. Pourquoi ce document existe

Le plan S15 (`21-session15-handoff.md`) liste Wave C comme "Debt #5" sans détail architectural.
Ce document capture la décision de design complète issue du brainstorm 2026-04-11 :

1. La frontière rig — où s'arrête rig, où commence Nika
2. Les extensions kernel requises (ce qui existe déjà vs ce qui manque)
3. L'architecture de la boucle nika-verb-agent (types réels)
4. Les nouvelles features YAML (inject_records, reflection, resume_session, parallel_tools, planning)
5. Les invariants #26-#31 à ajouter à architecture.md post-S15

---

## 1. Research Backing

| Papier / Source | Finding clé | Impact sur nika-verb-agent |
|---|---|---|
| **Karpathy AutoResearch** (mars 2026) | Simple loop + métrique vérifiable + logs passés = 700 expériences/2 jours, +11% efficiency | `inject_records:` — lire ses propres runs avant tour 1 |
| **Reflexion** (Shinn et al., NeurIPS 2023) | Critique verbale stockée en mémoire épisodique = 91% AlfWorld (vs 25%), 91% HumanEval (vs 80% GPT-4) | `reflection:` — gate entre tours avec score + retry |
| **M1-Parallel** (ICML 2025) | Parallel tool dispatch = 1.6-2.2× latency reduction, jusqu'à 10× sur vrais workloads | `parallel_tools:` + futures::stream::buffer_unordered |
| **Letta sleep-time** (arXiv 2504.13171) | Background consolidation = +13% Stateful GSM-Symbolic, +18% Stateful AIME, ~5× less test-time compute | pattern `depends_on:` avec agent consolidateur |
| **Agentless** (FSE 2025) | Pipeline sans boucle bat agents complexes sur tâches bornées | `agent:` doit être genuinement itératif — pas un `infer:` glorifié |
| **OpenAI Agents SDK** | 4-branch while loop (final/handoff/run-again/interrupt) = state of art | Notre loop = même pattern, plus propre |
| **ReWOO** (Xu et al., 2023) | Plan upfront + exécution parallèle = -67% LLM calls, -90% sur certains benchmarks | `planning:` mode — LLM planifie, puis dispatch parallèle |

**Leçon transversale** : la simplicité gagne. Une boucle while avec 4 branches + des tools fiables + une métrique vérifiable > orchestration complexe. Nika a déjà la bonne architecture de base.

---

## 2. La frontière rig — décision architecturale

### Aujourd'hui (rig possède la boucle)

```
TaskExecutor::run_agent()
  → RigAgentLoop::new_with_shield()
    → rig::AgentBuilder               ← haut niveau rig
      → rig::MultiTurnChat             ← boucle possédée par rig
        → rig::CompletionModel         ← client HTTP rig
          → HTTP (Anthropic/OpenAI/…)
```

Problème : on est **observateur** des tool calls, pas **exécuteur**. rig contrôle le parallélisme, les retries, l'ordering des résultats. On ne peut pas injecter de reflection gate, de mémoire, de parallel dispatch.

### Cible (Nika possède la boucle)

```
nika-verb-agent::run()               ← NOTRE boucle
  → caps.provider.infer(             ← kernel trait Provider (async_trait)
      InferRequest { tools, … }
    )
    → RigProvider::infer()           ← bridge existant (L2)
      → rig::CompletionModel         ← CLIENT HTTP SEULEMENT
        → HTTP
```

**On ne forke pas rig.** On bypasse son haut niveau (AgentBuilder, MultiTurnChat, ToolDyn). On garde son bas niveau (CompletionModel, clients HTTP providers) via RigProvider. rig reste notre couche transport — on ne réécrit pas curl.

### Pourquoi ne pas forker

```
Fork rig = maintenir patches sur 9 providers
         = rater les API changes (Anthropic tool_use v2, etc.)
         = diverger des nouveaux providers upstream
         = zéro avantage — on veut seulement posséder le LOOP
```

rig est MIT, Nika est AGPL. Si on a besoin d'un fix en amont : on contribue upstream.

---

## 3. Extensions kernel requises — réalité vs doc initial

> ⚠️ Cette section a été entièrement réécrite après lecture de provider.rs, builtin.rs,
> mcp.rs et caps.rs. Le doc initial proposait des types incorrects.

### 3.1 Ce qui existe DÉJÀ (pas de travail requis)

```rust
// nika-kernel/src/provider.rs — TOUS déjà présents post-S14

// Types de base — NOMS RÉELS
pub struct ToolDef {               // PAS ToolDefinition !
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

pub enum ToolChoice {              // PAS Tool(String) pour le cas specific !
    Auto,                          // défaut
    Required,
    None,
    Specific(String),              // PAS Tool(String) !
}

pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,                       // DÉJÀ PRÉSENT ✓
    ContentFilter,
    Unknown(String),
}

// InferRequest — tools + tool_choice DÉJÀ PRÉSENTS ✓
pub struct InferRequest {
    pub model: String,
    pub messages: Vec<Message>,    // system prompt = Message { role: Role::System, ... }
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolDef>,       // [] = plain infer. Non-vide = tool use. DÉJÀ LÀ ✓
    pub tool_choice: ToolChoice,   // DÉJÀ LÀ ✓
    pub response_format: ResponseFormat,
    pub stop_sequences: Vec<String>,
    pub thinking_budget: Option<u32>,
    pub extra: ProviderExtras,
}

// InferResponse — tool calls DANS content, PAS un champ séparé
pub struct InferResponse {
    pub content: Vec<ContentBlock>,  // ContentBlock::ToolUse = tool call
    pub usage: TokenUsage,           // .input_tokens, .output_tokens (u64!)
    pub stop_reason: StopReason,     // ToolUse quand tools appelés
    pub ttft_ms: Option<u64>,
    pub cached_tokens: Option<u32>,
    pub request_id: Option<String>,
    pub cost_usd: Option<f64>,
    // PAS de tool_calls: Vec<ToolCall> — les tool calls sont dans content !
}

// Message — PAS un enum InferMessage, une struct avec ContentBlock
pub struct Message {
    pub role: Role,                  // System | User | Assistant | Tool
    pub content: Vec<ContentBlock>,
}

// ContentBlock — où vivent les tool calls
pub enum ContentBlock {
    Text { text: String },                                    // NOTE: struct variant, pas tuple
    Image { source: String, detail: Option<String> },
    ToolUse { id: String, name: String, input: Value },       // tool call du LLM
    ToolResult { tool_use_id: String, content: String, is_error: bool }, // String PAS Value !
    Thinking { text: String },
}
```

### 3.2 Ce qui manque dans BuiltinRouter

```rust
// nika-kernel/src/builtin.rs — trait actuel

pub trait BuiltinRouter: Send + Sync {
    // EXISTANT — dispatch via JSON string args (pattern Pin<Box<...>> pour dyn-safety)
    fn dispatch<'a>(
        &'a self,
        tool: &'a str,    // sans préfixe "nika:" — "log", "read", etc.
        args: String,     // JSON-encoded args
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>>;

    // EXISTANT — PAS has_tool() ! Le vrai nom est knows()
    fn knows(&self, tool: &str) -> bool;

    // NOUVEAU pour Wave C — nécessaire pour construire le tool menu du provider
    fn tool_definitions(&self) -> Vec<ToolDef>;    // sync OK — catalogue statique
}
```

### 3.3 Ce qui manque dans McpPool

```rust
// nika-kernel/src/mcp.rs — trait actuel

pub trait McpPool: Send + Sync {
    // EXISTANT — call_tool retourne Value (pas McpToolResult — pas encore S15-A0)
    fn call_tool<'a>(
        &'a self,
        server: &'a str,
        tool: &'a str,
        args: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, McpError>> + Send + 'a>>;

    // EXISTANT
    fn read_resource<'a>(
        &'a self,
        uri: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, McpError>> + Send + 'a>>;

    // EXISTANT — PAS has_tool(), c'est has_server()
    fn has_server(&self, server: &str) -> bool;

    // NOUVEAU pour Wave C — async car nécessite I/O réseau vers les serveurs MCP
    fn tool_definitions<'a>(
        &'a self,
        servers: &'a [String],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ToolDef>, McpError>> + Send + 'a>>;
}
```

### 3.4 Ce qui manque dans AgentCaps

```rust
// nika-kernel/src/caps.rs — AgentCaps EXISTE DÉJÀ avec 11 champs, pattern &'a dyn

// Seul ajout nécessaire : record_query
pub struct AgentCaps<'a> {
    // 11 champs existants (provider: Arc<dyn Provider>, reste: &'a dyn)
    pub provider: Arc<dyn Provider>,
    pub builtin_router: &'a dyn BuiltinRouter,
    pub mcp_pool: &'a dyn McpPool,
    pub fs_read: &'a dyn FsRead,
    pub fs_write: &'a dyn FsWrite,
    pub http: &'a dyn HttpClient,
    pub blobs: &'a dyn BlobStore,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
    pub cancel: &'a CancellationToken,
    pub workflow_base_dir: &'a Path,

    // NOUVEAU — 12e champ, pour inject_records (RecordQuery existe depuis S9)
    pub record_query: &'a dyn RecordQuery,
}
```

### 3.5 Ce qui manque dans RigProvider

Le bridge dans `nika-engine/src/providers/rig/` ou `kernel_bridge.rs` doit :

1. Traduire `request.tools: Vec<ToolDef>` → format rig (tools OpenAI/Anthropic)
2. Extraire les `ContentBlock::ToolUse { id, name, input }` depuis la réponse rig
3. Construire `InferResponse { content: vec![...ToolUse...], stop_reason: StopReason::ToolUse, ... }`

### 3.6 Résumé des vrais commits S15 (Wave C kernel)

```
W15-CK0  — DÉJÀ FAIT: ToolDef, ToolChoice, InferRequest.tools, StopReason::ToolUse
W15-CK1  — DÉJÀ FAIT: idem (dans InferRequest)
W15-CK2  — DÉJÀ FAIT: StopReason::ToolUse, ContentBlock::ToolUse/ToolResult
W15-CK3  — DÉJÀ FAIT: Message/ContentBlock/Role (pas InferMessage!)
W15-CK4  feat(kernel): BuiltinRouter::tool_definitions() — VRAI TRAVAIL S15
W15-CK5  feat(kernel): McpPool::tool_definitions() — VRAI TRAVAIL S15
           + AgentCaps += record_query
W15-CP0  feat(engine): RigProvider tool translation + ContentBlock::ToolUse parse
```

---

## 4. nika-verb-agent — Architecture de la boucle (types réels)

### 4.1 Structs d'entrée/sortie

```rust
// tools/nika-verb-agent/src/lib.rs

pub struct AgentInput {
    pub prompt: String,
    pub system: Option<String>,
    pub model: String,
    pub max_turns: u32,                         // default: 10, max: 100
    pub tools: Vec<String>,                     // noms: "nika:read", "server::tool"
    pub tool_choice: ToolChoice,
    pub task_id: Arc<str>,

    // Features v2 (S16)
    pub inject_records: Option<InjectRecordsConfig>,
    pub reflection: Option<ReflectionConfig>,
    pub session_id: Option<String>,             // clé lookup (pas CAS hash direct)
    pub resume_session: Option<String>,         // CAS hash session précédente
    pub parallel_tools: bool,                   // default: true
    pub concurrency_limit: usize,               // default: 8 (invariant #27)
}

#[derive(Debug, Clone)]
pub struct InjectRecordsConfig {
    pub task_id: Option<String>,    // None = task courant
    pub limit: usize,               // default: 10
    pub token_budget: usize,        // default: 2000
}

#[derive(Debug, Clone)]
pub struct ReflectionConfig {
    pub evaluator: String,              // prompt de l'évaluateur
    pub stop_threshold: f64,            // arrêt si score >= N (0.0-10.0)
    pub max_iterations: u32,            // default: 3, safeguard anti-loop infini
}

pub struct AgentOutput {
    pub text: String,
    pub turns: u32,
    pub tool_calls_total: u32,
    pub session_hash: Option<String>,   // Some si session_id set
}
```

### 4.2 AgentCaps — utiliser le struct kernel existant

nika-verb-agent importe `AgentCaps` depuis `nika-kernel::caps`. Le crate ne définit PAS son propre AgentCaps.

```rust
// Dans nika-verb-agent/src/lib.rs
use nika_kernel::caps::AgentCaps;

pub async fn run(
    input: &AgentInput,
    caps: &AgentCaps<'_>,          // caps du kernel — PAS un type local
    event_log: &EventLog,
) -> Result<AgentOutput, VerbAgentError> {
    // ...
}
```

**Pattern &'a dyn** : les champs de `AgentCaps` sont `&'a dyn Trait` (sauf `provider: Arc<dyn Provider>`). `dispatch_parallel` doit utiliser ces borrows directement — PAS `Arc::clone`.

### 4.3 La boucle — run()

```rust
pub async fn run(
    input: &AgentInput,
    caps: &AgentCaps<'_>,
    event_log: &EventLog,
) -> Result<AgentOutput, VerbAgentError> {

    // ── 1. Load session history (resume_session) ──────────────────────────
    let mut history: Vec<Message> = load_session(input, caps).await?;

    // ── 2. Build system message + inject_records ──────────────────────────
    if let Some(system) = build_system_with_records(input, caps).await? {
        history.insert(0, Message {
            role: Role::System,
            content: vec![ContentBlock::Text { text: system }],
        });
    }

    // ── 3. Build tool definitions (builtin + MCP) ─────────────────────────
    let tools = build_tool_definitions(input, caps).await?;

    // ── 4. User message initiale ──────────────────────────────────────────
    history.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text { text: input.prompt.clone() }],
    });

    let mut final_text = String::new();
    let mut tool_calls_total: u32 = 0;
    let mut refl_count: u32 = 0;
    let mut turns_completed: u32 = 0;

    'agent_loop: for turn in 0..input.max_turns {
        turns_completed = turn + 1;

        // ── Cancel check ─────────────────────────────────────────────────
        if caps.cancel.is_cancelled() {
            return Err(VerbAgentError::Cancelled {
                task_id: input.task_id.to_string(),
                turn,
            });
        }

        event_log.emit(AgentTurnStarted { task_id: &input.task_id, turn })?;

        // ── LLM call via kernel trait (async_trait — async fn OK) ─────────
        let response = caps.provider.infer(InferRequest {
            model: input.model.clone(),
            messages: history.clone(),
            tools: tools.clone(),
            tool_choice: input.tool_choice.clone(),
            temperature: None,
            max_tokens: None,
            response_format: ResponseFormat::default(),
            stop_sequences: vec![],
            thinking_budget: None,
            extra: ProviderExtras::default(),
        }).await.map_err(VerbAgentError::Provider)?;

        event_log.emit(ProviderResponded {
            task_id: &input.task_id,
            turn,
            input_tokens: response.usage.input_tokens,      // u64 dans TokenUsage
            output_tokens: response.usage.output_tokens,    // u64
            cost_usd: response.cost_usd,
            request_id: response.request_id.clone(),
        })?;

        // ── Extraire les tool calls depuis content ────────────────────────
        let tool_uses: Vec<(String, String, serde_json::Value)> = response.content.iter()
            .filter_map(|cb| match cb {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        // ── Extraire le texte de la réponse ──────────────────────────────
        let response_text: String = response.content.iter()
            .filter_map(|cb| match cb {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        // ── No tool calls = output candidat ──────────────────────────────
        if tool_uses.is_empty() {
            // ── Reflection gate ───────────────────────────────────────────
            if let Some(ref refl) = input.reflection {
                if refl_count < refl.max_iterations {
                    let score = evaluate_output(&response_text, refl, caps).await?;

                    if score < refl.stop_threshold {
                        let critique = format!(
                            "Score: {score:.1}/10. Améliore cet output en adressant les points faibles identifiés."
                        );

                        // Ajouter la réponse assistant dans l'historique
                        history.push(Message {
                            role: Role::Assistant,
                            content: response.content.clone(),
                        });
                        // Critique utilisateur pour le prochain tour
                        history.push(Message {
                            role: Role::User,
                            content: vec![ContentBlock::Text { text: critique }],
                        });
                        refl_count += 1;

                        event_log.emit(AgentReflectionRetry {
                            task_id: &input.task_id,
                            turn,
                            score,
                            iteration: refl_count,
                        })?;
                        continue 'agent_loop;
                    }
                }
            }

            final_text = response_text;
            event_log.emit(AgentTurnCompleted {
                task_id: &input.task_id,
                turn,
                tool_count: 0,
                is_final: true,
            })?;
            break 'agent_loop;
        }

        // ── Tool dispatch ─────────────────────────────────────────────────
        tool_calls_total += tool_uses.len() as u32;

        // Ajouter le message assistant (avec les ToolUse blocks) à l'historique
        history.push(Message {
            role: Role::Assistant,
            content: response.content.clone(),
        });

        let results = if input.parallel_tools {
            dispatch_parallel(&tool_uses, caps, input.concurrency_limit).await?
        } else {
            dispatch_sequential(&tool_uses, caps).await?
        };

        // Ajouter les résultats (un Message par tool result, role: Tool)
        for (tool_use_id, content, is_error) in results {
            history.push(Message {
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult { tool_use_id, content, is_error }],
            });
        }

        event_log.emit(AgentTurnCompleted {
            task_id: &input.task_id,
            turn,
            tool_count: tool_uses.len() as u32,
            is_final: false,
        })?;
    }

    // ── Persist session ───────────────────────────────────────────────────
    let session_hash = persist_session_if_needed(input, &history, caps).await?;

    Ok(AgentOutput {
        text: final_text,
        turns: turns_completed,
        tool_calls_total,
        session_hash,
    })
}
```

### 4.4 Build tool definitions

```rust
/// Construit la liste ToolDef à partir des noms de tools autorisés.
/// - "nika:*" → caps.builtin_router.tool_definitions() filtré
/// - "server::tool" → caps.mcp_pool.tool_definitions() filtré
async fn build_tool_definitions(
    input: &AgentInput,
    caps: &AgentCaps<'_>,
) -> Result<Vec<ToolDef>, VerbAgentError> {
    if input.tools.is_empty() {
        return Ok(vec![]);
    }

    let mut defs: Vec<ToolDef> = vec![];

    // nika:* tools — sync call (catalogue statique)
    let builtin_names: Vec<&str> = input.tools.iter()
        .filter(|t| t.starts_with("nika:"))
        .map(|t| t.strip_prefix("nika:").unwrap())
        .collect();

    if !builtin_names.is_empty() {
        let all_builtin = caps.builtin_router.tool_definitions();
        defs.extend(
            all_builtin.into_iter()
                .filter(|d| builtin_names.contains(&d.name.as_str()))
        );
    }

    // server::tool MCP tools — async call
    let mcp_servers: Vec<String> = input.tools.iter()
        .filter_map(|t| t.split_once("::").map(|(s, _)| s.to_string()))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if !mcp_servers.is_empty() {
        let mcp_defs = caps.mcp_pool.tool_definitions(&mcp_servers).await
            .map_err(VerbAgentError::McpTools)?;
        // Filtrer selon les tools autorisés ("server::tool")
        defs.extend(mcp_defs.into_iter().filter(|d| input.tools.contains(&d.name)));
    }

    Ok(defs)
}
```

### 4.5 Parallel tool dispatch

```rust
/// Invariant #27: bounded concurrency — jamais unbounded join_all sur tool_calls user-contrôlés.
/// Pattern: &'a dyn borrows directs depuis caps — PAS Arc::clone (les champs sont &'a dyn).
/// buffer_unordered fonctionne avec les borrows car les futures restent sur la même tâche.
async fn dispatch_parallel<'a>(
    tool_uses: &'a [(String, String, serde_json::Value)],  // (id, name, input)
    caps: &'a AgentCaps<'a>,
    concurrency_limit: usize,
) -> Result<Vec<(String, String, bool)>, VerbAgentError> {  // (id, result: String, is_error)
    use futures::stream::{self, StreamExt};

    let results: Vec<Result<(String, String, bool), VerbAgentError>> =
        stream::iter(tool_uses)
            .map(|(id, name, input)| async move {
                // Cancel check par tool (caps.cancel = le token du caller)
                if caps.cancel.is_cancelled() {
                    return Err(VerbAgentError::Cancelled {
                        task_id: String::new(),
                        turn: 0,
                    });
                }
                dispatch_single_tool(id, name, input.clone(), caps).await
            })
            .buffer_unordered(concurrency_limit)
            .collect()
            .await;

    results.into_iter().collect()
}

async fn dispatch_single_tool(
    id: &str,
    name: &str,
    input: serde_json::Value,
    caps: &AgentCaps<'_>,
) -> Result<(String, String, bool), VerbAgentError> {
    // nika:* builtins — via BuiltinRouter::dispatch()
    // dispatch() prend le nom SANS le préfixe "nika:" et des args JSON string
    if let Some(tool_name) = name.strip_prefix("nika:") {
        if caps.builtin_router.knows(tool_name) {
            let args = input.to_string();
            return match caps.builtin_router.dispatch(tool_name, args).await {
                Ok(result) => Ok((id.to_string(), result, false)),
                Err(e) => Ok((id.to_string(), serde_json::json!({"error": e.to_string()}).to_string(), true)),
            };
        }
    }

    // MCP tools — format "server::tool"
    if let Some((server, tool)) = name.split_once("::") {
        return match caps.mcp_pool.call_tool(server, tool, input).await {
            // call_tool retourne serde_json::Value (pas McpToolResult — pas encore S15-A0)
            Ok(v) => Ok((id.to_string(), v.to_string(), false)),
            Err(e) => Ok((id.to_string(), serde_json::json!({"error": e.to_string()}).to_string(), true)),
        };
    }

    Err(VerbAgentError::UnknownTool(name.to_string()))
}

async fn dispatch_sequential(
    tool_uses: &[(String, String, serde_json::Value)],
    caps: &AgentCaps<'_>,
) -> Result<Vec<(String, String, bool)>, VerbAgentError> {
    let mut results = Vec::with_capacity(tool_uses.len());
    for (id, name, input) in tool_uses {
        if caps.cancel.is_cancelled() {
            return Err(VerbAgentError::Cancelled { task_id: String::new(), turn: 0 });
        }
        results.push(dispatch_single_tool(id, name, input.clone(), caps).await?);
    }
    Ok(results)
}
```

### 4.6 inject_records (Karpathy pattern)

```rust
/// Query nika:records avant le premier tour et injecte dans le system prompt.
/// RecordQuery::iter_record_views() retourne Vec<RecordView> — .into_iter() requis.
async fn build_system_with_records(
    input: &AgentInput,
    caps: &AgentCaps<'_>,
) -> Result<Option<String>, VerbAgentError> {
    let Some(ref cfg) = input.inject_records else {
        return Ok(input.system.clone());
    };

    // iter_record_views() retourne Vec<RecordView> — PAS un iterator direct
    let records = caps.record_query
        .iter_record_views()
        .into_iter()
        .filter(|r| {
            cfg.task_id.as_deref()
                .map_or(true, |id| r.task_id.as_ref() == id)
        })
        .take(cfg.limit)
        .collect::<Vec<_>>();

    if records.is_empty() {
        return Ok(input.system.clone());
    }

    // Format + token budget (approximation: 4 chars ≈ 1 token)
    let records_text = records.iter()
        .map(|r| format!("## Run: {}\n{}\n---", r.task_id, r.summary))
        .collect::<Vec<_>>()
        .join("\n");

    let max_chars = cfg.token_budget * 4;
    let records_text = if records_text.len() > max_chars {
        records_text[..max_chars].to_string()
    } else {
        records_text
    };

    let system = format!(
        "{}\n\n## Past Run Records\n{}",
        input.system.as_deref().unwrap_or(""),
        records_text
    );

    Ok(Some(system))
}
```

### 4.7 Reflection evaluation (invariant #28)

```rust
/// Invariant #28: reflection MUST use structured output, jamais regex sur free-form text.
/// Provider::infer est async via async_trait — `async fn` OK sur le trait.
async fn evaluate_output(
    output: &str,
    config: &ReflectionConfig,
    caps: &AgentCaps<'_>,
) -> Result<f64, VerbAgentError> {
    use serde_json::json;

    let eval_response = caps.provider.infer(InferRequest {
        model: "claude-haiku-4-5-20251001".to_string(), // modèle léger pour l'éval
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: format!("Output à évaluer:\n{output}\n\n{}", config.evaluator),
            }],
        }],
        response_format: ResponseFormat::JsonSchema(json!({
            "type": "object",
            "properties": {
                "score": { "type": "number", "minimum": 0, "maximum": 10 },
                "issues": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["score", "issues"]
        })),
        tools: vec![],
        tool_choice: ToolChoice::None,
        temperature: Some(0.0),
        max_tokens: Some(200),
        stop_sequences: vec![],
        thinking_budget: None,
        extra: ProviderExtras::default(),
    }).await.map_err(VerbAgentError::Provider)?;

    let text: String = eval_response.content.iter()
        .filter_map(|cb| match cb {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();

    let parsed: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| VerbAgentError::ReflectionParseFailed(e.to_string()))?;

    parsed["score"].as_f64()
        .ok_or_else(|| VerbAgentError::ReflectionParseFailed("no score field".into()))
}
```

### 4.8 Session persistence via BlobStore

```rust
/// History = Vec<Message> (sérialisable via Serialize/Deserialize sur Message/ContentBlock)
async fn load_session(
    input: &AgentInput,
    caps: &AgentCaps<'_>,
) -> Result<Vec<Message>, VerbAgentError> {
    let Some(ref hash) = input.resume_session else {
        return Ok(vec![]);
    };

    let blob = caps.blobs.get(hash).await
        .map_err(VerbAgentError::SessionLoad)?;

    serde_json::from_slice::<Vec<Message>>(&blob)
        .map_err(|e| VerbAgentError::SessionDeserialize(e.to_string()))
}

async fn persist_session_if_needed(
    input: &AgentInput,
    history: &[Message],
    caps: &AgentCaps<'_>,
) -> Result<Option<String>, VerbAgentError> {
    let Some(ref session_id) = input.session_id else {
        return Ok(None);
    };

    // Invariant #29: session_id scopé au projet (project fingerprint + session_id)
    // La clé CAS finale inclut le fingerprint du projet pour éviter la contamination cross-projet.
    let _ = session_id; // utilisé pour construire la clé dans la vraie impl

    let blob = serde_json::to_vec(history)
        .map_err(|e| VerbAgentError::SessionSerialize(e.to_string()))?;

    let hash = caps.blobs.put(blob.into()).await
        .map_err(VerbAgentError::SessionPersist)?;

    Ok(Some(hash.to_string()))
}
```

---

## 5. Nouvelles features YAML (spec complète)

```yaml
# workflow.nika.yaml — Agent v2 feature set complet

tasks:
  - id: research
    agent:
      prompt: "Analyse {{inputs.topic}} en profondeur"
      provider: claude
      model: claude-sonnet-4-6
      max_turns: 20
      tools: [nika:read, nika:write, nika:grep, nika:glob]

      # ── Feature 1: Karpathy pattern ────────────────────────────────────
      # Charge les records des runs précédents avant le premier tour.
      # L'agent voit ses propres expériences passées via nika:records.
      inject_records:
        task_id: research       # None = task courant
        limit: 10               # max N records
        token_budget: 2000      # cap tokens injectés dans system

      # ── Feature 2: Reflexion pattern ────────────────────────────────────
      # Gate sur l'output final: score < threshold = critique + re-essai.
      # Invariant #28: évaluateur utilise structured output, jamais regex.
      reflection:
        evaluator: |
          Évalue l'output sur :
          1. Couverture du sujet (0-10)
          2. Précision factuelle (0-10)
          Donne un score global et liste 3 améliorations spécifiques.
        stop_threshold: 8.0     # arrêt si score >= 8
        max_iterations: 3       # safeguard — jamais > max_iterations retries

      # ── Feature 3: Session cross-run ────────────────────────────────────
      # Sauvegarder l'historique Vec<Message> dans le CAS blake3.
      # session_id = clé lookup scopée au projet (invariant #29).
      session_id: "research-topic-v1"
      # resume_session: "blake3:abc123…"  # reprendre une session précédente

      # ── Feature 4: Parallel tool dispatch ───────────────────────────────
      # Par défaut true — tool calls indépendants dans un même tour
      # s'exécutent via futures::stream::buffer_unordered.
      parallel_tools: true
      concurrency_limit: 8      # DoS protection (invariant #27)

  # ── Feature 5: Planning mode (ReWOO) ────────────────────────────────────
  # 1 LLM call pour planifier → exécution parallèle des tools → 1-2 calls synth.
  # -67% LLM calls sur tâches structurées vs ReAct.
  - id: research_planned
    agent:
      prompt: "Plan et exécute une analyse de {{inputs.topic}}"
      planning:
        enabled: true
        strategy: rewoo         # rewoo | react (default)
        max_plan_tokens: 1000

  # ── Feature 6: Context compression ─────────────────────────────────────
  # Résumé mid-loop quand history dépasse le seuil de tokens.
  # Critique pour les agents 20+ tours.
  - id: long_research
    agent:
      prompt: "..."
      max_turns: 50
      context_compression:
        enabled: true
        threshold_tokens: 50000    # compresser quand history > 50k tokens
        keep_last_n_turns: 5       # garder les N derniers tours verbatim
        compression_model: claude-haiku-4-5-20251001

  # ── Feature 7: Tool error policy ────────────────────────────────────────
  # Distinguer erreurs transitoires (retry) des erreurs logiques (fail).
  - id: resilient_agent
    agent:
      prompt: "..."
      tool_error_policy:
        on_transient: retry       # retry | skip | fail
        on_logical: skip          # erreur logique = skip + continue
        max_tool_retries: 3

  # ── Pattern: Sleep consolidation (Letta) ────────────────────────────────
  # Après le run principal, un agent léger consolide les lessons.
  - id: consolidate
    depends_on: [research]
    agent:
      prompt: |
        Lis les records du run 'research' et extrais des lessons structurées.
      provider: claude
      model: claude-haiku-4-5-20251001
      tools: [nika:records, nika:write]
      max_turns: 5
      inject_records:
        task_id: research
        limit: 1
        token_budget: 4000
```

---

## 6. RigProvider — traduction tool definitions

Ce travail se fait dans le bridge existant (`nika-engine/src/providers/rig/` ou `kernel_bridge.rs`) :

```rust
// RigProvider::infer() — ajout de la traduction tools
// Le trait Provider utilise #[async_trait::async_trait] — async fn OK.

impl Provider for RigProvider {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {

        if !request.tools.is_empty() {
            // Traduire Vec<ToolDef> → format rig per provider
            let rig_tools: Vec<rig::ToolDefinition> = request.tools.iter()
                .map(|t| rig::ToolDefinition {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                })
                .collect();

            let response = self.client
                .completion(CompletionRequest {
                    messages: translate_messages(&request.messages),
                    tools: rig_tools,
                    tool_choice: translate_tool_choice(&request.tool_choice),
                    // ...
                })
                .await?;

            // Parser les ContentBlock::ToolUse depuis la réponse rig
            // (rig expose les tool calls per provider dans CompletionModel)
            let content = parse_rig_response_to_content_blocks(&response)?;
            let has_tool_use = content.iter().any(|cb| matches!(cb, ContentBlock::ToolUse { .. }));

            return Ok(InferResponse::new(
                content,
                translate_usage(&response),
                if has_tool_use { StopReason::ToolUse } else { StopReason::EndTurn },
            ));
        }

        // Chemin existant sans tools (nika-verb-infer)
        // (code actuel inchangé)
        todo!()
    }
}
```

**Note** : `ContentBlock::ToolResult.content` est `String` (pas `Value`) — le marshalling JSON se fait en `serde_json::to_string(value)` quand on construit le ToolResult dans `dispatch_single_tool`.

---

## 7. Séquençage — sessions et commits

### S15 — Kernel + minimum extraction

```
# Wave C kernel — VRAI TRAVAIL (CK0/1/2/3 étaient déjà là !)
W15-CK4  feat(kernel): BuiltinRouter::tool_definitions() — sync, Vec<ToolDef>
W15-CK5  feat(kernel): McpPool::tool_definitions() — async Pin<Box<...>>,
                        + AgentCaps += record_query: &'a dyn RecordQuery

# Wave C engine
W15-CP0  feat(engine): RigProvider tool translation + ContentBlock::ToolUse parse

# Wave C agent
W15-CA0  feat(verb-agent): nika-verb-agent crate — loop minimum, sequential tools
           (utilise les types réels: Vec<Message>, ContentBlock, ToolDef, ToolChoice)
W15-CA1  test(verb-agent): 5 tests minimum:
           - cancel_before_first_turn → VerbAgentError::Cancelled
           - no_tools_returns_text → AgentOutput.text non-vide
           - tool_call_and_result → ToolUse + ToolResult dans history
           - max_turns_reached → graceful stop
           - stop_reason_tool_use → StopReason::ToolUse détecté correctement

# Wave C runtime
W15-CR0  feat(runtime): verb_agent adapter + AgentCaps constructeur en VerbCapabilities
```

### S16 — Features + bridge surgery

```
W16-CA2  feat(verb-agent): parallel tool dispatch (buffer_unordered)
W16-CA3  feat(verb-agent): inject_records via RecordQuery (iter_record_views = Vec!)
W16-CA4  feat(verb-agent): reflection gate (structured output, score, retry)
W16-CA5  feat(verb-agent): resume_session / session_id via BlobStore
W16-CA6  feat(engine): agent.rs bridge surgery (~26KB → ~300 LOC bridge)
W16-CS0  feat(verb-agent): planning: mode (ReWOO strategy)
W16-CS1  feat(verb-agent): context_compression: (mid-loop summarization)
W16-CS2  feat(verb-agent): tool_error_policy: (transient vs logical errors)
W16-ARC  docs(architecture): ARCHITECTURE.md update — invariants #26-#31
```

---

## 8. Invariants #26-#31 (à merger dans architecture.md post-S15/S16)

**Invariant #26** : **nika-verb-agent n'importe jamais de types rig-core directement.**
`Provider` trait est la seule interface LLM. Si du code dans nika-verb-agent référence
`AgentBuilder`, `MultiTurnChat`, `ToolDyn`, ou `CompletionModel`, c'est une violation de
layering. rig-core reste interne à RigProvider (L2 engine bridge).

**Invariant #27** : **Parallel tool dispatch est bounded à un `concurrency_limit` configurable
(default: 8).** `futures::future::join_all()` non borné sur une liste de tool calls
user-contrôlée est un vecteur DoS. Utiliser `futures::stream::buffer_unordered(limit)`.
Note: buffer_unordered sur borrows `&'a dyn` fonctionne car les futures restent sur la même tâche.

**Invariant #28** : **La reflection DOIT utiliser structured output via `Provider::infer`.**
Schema minimum `{score: number, issues: array}` via `ResponseFormat::JsonSchema(...)`.
Jamais de regex sur du free-form text pour extraire un score — c'est un bug de parsing masqué.

**Invariant #29** : **`session_id` est scopé au projet nika (racine `nika.toml`).**
La clé CAS inclut le project fingerprint, pas seulement le session_id user-fourni.
Sessions cross-projet = contamination de contexte.

**Invariant #30** : **Les events `AgentTurnStarted` / `AgentTurnCompleted` sont émis uniquement
dans `nika-verb-agent::run()`.** Pas dans les helpers dispatch_parallel/dispatch_single_tool.
Un point d'émission unique par event kind (invariant #24 généralisé).

**Invariant #31** : **`agent:` est genuinement itératif.** Si la tâche est completable en un
seul LLM call, utiliser `infer:`. `agent:` sans tool calls et sans reflection est un `infer:`
dégradé — linter rule L-AGT-001 doit le signaler. (Finding: Agentless paper FSE 2025.)

---

## 9. Dependances entre debts (graphe)

```
W15-CK4  BuiltinRouter::tool_definitions()     ← indépendant, sync
W15-CK5  McpPool::tool_definitions()           ← s'ajoute à S15-A0/A1 McpPool expansion
          AgentCaps += record_query             ← indépendant
W15-CP0  RigProvider tool translation           ← DÉPEND de W15-CK4/5 (ToolDef déjà existant)
W15-CA0  nika-verb-agent min                   ← DÉPEND de W15-CK4/5 + W15-CP0
W15-CA1  tests verb-agent                      ← DÉPEND de W15-CA0
W15-CR0  runtime adapter                       ← DÉPEND de W15-CA0

W16-CA2  parallel dispatch                     ← DÉPEND de W15-CA0 + futures crate dep
W16-CA3  inject_records                        ← DÉPEND de W15-CA0 + W15-CK5 (record_query)
W16-CA4  reflection                            ← DÉPEND de W15-CA0 + ResponseFormat::JsonSchema
W16-CA5  session CAS                           ← DÉPEND de W15-CA0 (blobs déjà dans AgentCaps)
W16-CA6  bridge surgery engine                 ← DÉPEND de W15-CR0 + TOUS W16-CA*
W16-CS0  planning: mode                        ← DÉPEND de W16-CA2 (parallel dispatch)
W16-CS1  context_compression:                  ← DÉPEND de W15-CA0 + Provider::infer
W16-CS2  tool_error_policy:                    ← DÉPEND de W15-CA0

McpPool trait expansion S15-A0/A1             ← prerequis McpToolResult (parallel à Wave C)
```

```
       W15-CK4/5  ← S15-A0/A1 McpPool
            │
       W15-CP0
            │
       W15-CA0 ─────────────────┐
       W15-CA1                  │
       W15-CR0                  │
            │         W16-CA2/3/4/5/CS0/1/2
            └──────────────────→W16-CA6
```

---

## 10. Anti-patterns à éviter

```
❌  Créer ToolDefinition, ToolCall, ToolResult         (types DÉJÀ existants sous d'autres noms)
❌  Créer InferMessage enum                             (utiliser Message + ContentBlock)
❌  Ajouter InferResponse.tool_calls: Vec<ToolCall>    (tool calls sont dans .content)
❌  ToolChoice::Tool(String)                            (c'est Specific(String) !)
❌  router.has_tool()                                   (c'est router.knows() !)
❌  Arc::clone(&caps.builtin_router)                    (c'est &'a dyn, pas Arc)
❌  CancellationToken::new() dans dispatch              (passer caps.cancel !)
❌  iter_record_views().take(N)                         (retourne Vec, appeler .into_iter() d'abord)
❌  response.input_tokens                               (c'est response.usage.input_tokens — u64)
❌  ContentBlock::Text(string)                          (struct variant: Text { text: string })
❌  ContentBlock::ToolResult { content: Value }         (content: String, pas Value)
❌  infer_with_tools() trait method                     (invariant #17 — tout dans InferRequest)
❌  fork rig pour posséder le loop                      (mauvaise raison de forker)
❌  join_all() non borné sur tools                      (invariant #27)
❌  regex parse sur score reflection                    (invariant #28)
❌  agent: sans tools ni reflection                     (utiliser infer:, invariant #31)
❌  AgentBuilder/MultiTurnChat dans nika-verb-agent     (invariant #26)
❌  Émettre AgentTurnStarted dans dispatch_parallel()   (invariant #30)
```
