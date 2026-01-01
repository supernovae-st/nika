# Nika - Specification

> **Version:** 0.1.0
> **Status:** MVP - Minimal Viable Product
> **Goal:** Un DAG runner qui marche, qu'on maîtrise, super rapide.

---

## What is Nika?

**Un orchestrateur de workflows AI.** Tu décris tes tasks en YAML, Nika les exécute en parallèle et passe les outputs entre elles.

```
workflow.nika.yaml → Parser → DAG → Runner → Output
```

---

## YAML Format

### Minimal Example

```yaml
# hello.nika.yaml

schema: "nika/workflow@0.1"

tasks:
  - id: greet
    infer:
      prompt: "Say hello in French"
```

### Diamond Pattern (Fan-out + Fan-in)

```yaml
# review.nika.yaml

schema: "nika/workflow@0.1"

tasks:
  - id: read_code
    exec:
      command: "cat src/main.rs"

  - id: security
    infer:
      prompt: "Failles sécu: {{ read_code.output }}"

  - id: perf
    infer:
      prompt: "Problèmes perf: {{ read_code.output }}"

  - id: report
    infer:
      prompt: |
        Rapport:
        Sécu: {{ security.output }}
        Perf: {{ perf.output }}

flows:
  - source: read_code
    target: [security, perf]

  - source: [security, perf]
    target: report
    merge:
      strategy: all
```

---

## Schema

### Root

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `schema` | string | ✅ | Must be `"nika/workflow@0.1"` |
| `provider` | string | ❌ | Default provider: `claude` \| `openai` (default: `claude`) |
| `model` | string | ❌ | Default model (default: `claude-sonnet-4-5` or `gpt-4o`) |
| `tasks` | array | ✅ | List of tasks (min 1) |
| `flows` | array | ❌ | DAG edges (optional if single task) |

### Task

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | ✅ | Unique identifier (snake_case) |
| + **ONE verb** | object | ✅ | One of: `infer`, `exec`, `fetch` |

### Verbs (MVP: 3 verbs)

#### `infer:` - One-shot LLM

```yaml
- id: classify
  infer:
    prompt: "Classify this: {{ input.output }}"

- id: second_opinion
  infer:
    prompt: "Review this: {{ classify.output }}"
    provider: openai     # Override provider
    model: gpt-4o        # Override model
```

| Field | Type | Required | Default |
|-------|------|----------|---------|
| `prompt` | string | ✅ | - |
| `provider` | string | ❌ | Workflow default |
| `model` | string | ❌ | Workflow default |

#### `exec:` - Shell command

```yaml
- id: test
  exec:
    command: "npm test"
```

| Field | Type | Required | Default |
|-------|------|----------|---------|
| `command` | string | ✅ | - |

#### `fetch:` - HTTP request

```yaml
- id: get_data
  fetch:
    url: "https://api.example.com/data"
    method: GET
```

| Field | Type | Required | Default |
|-------|------|----------|---------|
| `url` | string | ✅ | - |
| `method` | string | ❌ | `GET` |
| `headers` | object | ❌ | `{}` |
| `body` | string | ❌ | - |

### Flow

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `source` | string \| array | ✅ | Source task(s) |
| `target` | string \| array | ✅ | Target task(s) |
| `merge` | object | ❌ | Merge strategy (for fan-in) |

### Merge

| Field | Type | Required | Default |
|-------|------|----------|---------|
| `strategy` | enum | ❌ | `all` |

**Strategies:**
- `all` - Wait for ALL sources to complete
- `any` - Proceed when ANY source completes
- `first` - Use first completed, ignore rest

---

## Templates

Access task outputs with `{{ task_id.output }}`:

```yaml
- id: analyze
  infer:
    prompt: "Analyze: {{ read_code.output }}"
```

---

## Project Structure

```
nika/
├── Cargo.toml
├── SPEC.md              # This file
├── src/
│   ├── main.rs          # CLI entry point
│   ├── lib.rs           # Public exports
│   ├── error.rs         # Error types with FixSuggestion
│   ├── workflow.rs      # Workflow, Task, Flow structs
│   ├── task.rs          # Verb enums (Infer, Exec, Fetch)
│   ├── dag.rs           # DagAnalyzer (adjacency + BFS)
│   ├── runner.rs        # DAG execution with tokio
│   ├── datastore.rs     # TaskData + storage
│   ├── template.rs      # {{ }} resolution + security
│   └── provider/
│       ├── mod.rs       # Provider trait + factory
│       ├── claude.rs    # Claude API (Anthropic)
│       └── openai.rs    # OpenAI API (GPT)
└── examples/
    ├── minimal.nika.yaml
    ├── linear.nika.yaml
    ├── fanout.nika.yaml
    ├── diamond.nika.yaml
    └── multi-provider.nika.yaml
```

---

## Stack (Cargo.toml)

```toml
[package]
name = "nika"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "nika"
path = "src/main.rs"

[dependencies]
# CLI
clap = { version = "4.5", features = ["derive"] }

# Async (même versions que nika principal)
tokio = { version = "1.48", features = ["rt-multi-thread", "macros", "process", "sync", "time"] }
futures = "0.3"
async-trait = "0.1"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde-saphyr = "0.0.11"    # YAML sécurisé (pas de vulnérabilités)
serde_json = "1.0"

# Errors
anyhow = "1.0"
thiserror = "1.0"

# HTTP (pour fetch:)
reqwest = { version = "0.12", features = ["json"] }

# Utilities
uuid = { version = "1.11", features = ["v4"] }
regex = "1.11"
colored = "2.1"
chrono = { version = "0.4", features = ["serde"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3.14"
```

**Note:** Pas de `petgraph` - on utilise notre propre `DagAnalyzer` avec HashMap (plus simple, suffisant pour le MVP).

---

## Architecture Technique

### 1. Error Handling (error.rs)

Pattern récupéré du code principal - chaque erreur propose une solution:

```rust
use thiserror::Error;

/// Trait for errors that provide fix suggestions
pub trait FixSuggestion {
    fn fix_suggestion(&self) -> Option<&str>;
}

#[derive(Error, Debug)]
pub enum NikaError {
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_saphyr::Error),

    #[error("Task '{task_id}' not found")]
    TaskNotFound { task_id: String },

    #[error("Template error: {0}")]
    Template(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl FixSuggestion for NikaError {
    fn fix_suggestion(&self) -> Option<&str> {
        match self {
            NikaError::YamlParse(_) => Some("Check YAML syntax: indentation and quoting"),
            NikaError::TaskNotFound { .. } => Some("Verify task ID exists in tasks array"),
            NikaError::Template(_) => Some("Use {{ task_id.output }} format"),
            NikaError::Provider(_) => Some("Check ANTHROPIC_API_KEY is set"),
            NikaError::Execution(_) => Some("Check command/URL is valid"),
            NikaError::Io(_) => Some("Check file path and permissions"),
        }
    }
}
```

### 2. Workflow Parsing (workflow.rs)

Serde patterns pour parser le YAML proprement:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Workflow {
    pub schema: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub flows: Vec<Flow>,
}

fn default_provider() -> String {
    "claude".to_string()
}

#[derive(Debug, Deserialize)]
pub struct Task {
    pub id: String,

    #[serde(flatten)]  // Le verbe est "aplati" dans la task
    pub action: TaskAction,
}

/// Les 3 verbes MVP - serde détecte automatiquement lequel
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TaskAction {
    Infer { infer: InferDef },
    Exec { exec: ExecDef },
    Fetch { fetch: FetchDef },
}

#[derive(Debug, Deserialize)]
pub struct Flow {
    pub source: FlowEndpoint,
    pub target: FlowEndpoint,
    #[serde(default)]
    pub merge: Option<Merge>,
}

/// Gère string OU array pour source/target
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FlowEndpoint {
    Single(String),
    Multiple(Vec<String>),
}

impl FlowEndpoint {
    pub fn as_vec(&self) -> Vec<&str> {
        match self {
            FlowEndpoint::Single(s) => vec![s.as_str()],
            FlowEndpoint::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Merge {
    #[serde(default)]
    pub strategy: MergeStrategy,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    #[default]
    All,
    Any,
    First,
}
```

### 3. Task Definitions (task.rs)

```rust
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct InferDef {
    pub prompt: String,
    /// Override provider for this task
    #[serde(default)]
    pub provider: Option<String>,
    /// Override model for this task
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecDef {
    pub command: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FetchDef {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}
```

### 4. DAG Analyzer (dag.rs)

Pattern simplifié du code principal - pas besoin de petgraph:

```rust
use std::collections::{HashMap, HashSet, VecDeque};
use crate::workflow::{Workflow, FlowEndpoint};

/// DAG analyzer - adjacency lists + BFS
pub struct DagAnalyzer {
    /// task_id -> list of successor task_ids
    adjacency: HashMap<String, Vec<String>>,
    /// task_id -> list of predecessor task_ids (dependencies)
    predecessors: HashMap<String, Vec<String>>,
    /// All task IDs
    task_ids: HashSet<String>,
}

impl DagAnalyzer {
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        let mut predecessors: HashMap<String, Vec<String>> = HashMap::new();
        let mut task_ids: HashSet<String> = HashSet::new();

        // Initialize all tasks
        for task in &workflow.tasks {
            task_ids.insert(task.id.clone());
            adjacency.entry(task.id.clone()).or_default();
            predecessors.entry(task.id.clone()).or_default();
        }

        // Build from flows
        for flow in &workflow.flows {
            let sources = flow.source.as_vec();
            let targets = flow.target.as_vec();

            for source in &sources {
                for target in &targets {
                    adjacency
                        .entry(source.to_string())
                        .or_default()
                        .push(target.to_string());
                    predecessors
                        .entry(target.to_string())
                        .or_default()
                        .push(source.to_string());
                }
            }
        }

        Self { adjacency, predecessors, task_ids }
    }

    /// Get dependencies of a task
    pub fn get_dependencies(&self, task_id: &str) -> Vec<String> {
        self.predecessors
            .get(task_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get successors of a task
    pub fn get_successors(&self, task_id: &str) -> Vec<String> {
        self.adjacency
            .get(task_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Find tasks with no successors (final tasks)
    pub fn get_final_tasks(&self) -> Vec<String> {
        self.task_ids
            .iter()
            .filter(|id| self.get_successors(id).is_empty())
            .cloned()
            .collect()
    }

    /// Check if there's a path from `from` to `to` (BFS)
    pub fn has_path(&self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }

        let mut visited: HashSet<&str> = HashSet::new();
        let mut queue: VecDeque<&str> = VecDeque::new();

        queue.push_back(from);
        visited.insert(from);

        while let Some(current) = queue.pop_front() {
            if let Some(neighbors) = self.adjacency.get(current) {
                for neighbor in neighbors {
                    if neighbor == to {
                        return true;
                    }
                    if !visited.contains(neighbor.as_str()) {
                        visited.insert(neighbor.as_str());
                        queue.push_back(neighbor.as_str());
                    }
                }
            }
        }

        false
    }
}
```

### 5. DataStore (datastore.rs)

Pattern récupéré - stocke les résultats avec métadonnées:

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Output data from a completed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskData {
    pub task_id: String,
    pub output: String,
    pub duration: Duration,
    pub success: bool,
    pub error: Option<String>,
}

impl TaskData {
    pub fn success(task_id: impl Into<String>, output: impl Into<String>, duration: Duration) -> Self {
        Self {
            task_id: task_id.into(),
            output: output.into(),
            duration,
            success: true,
            error: None,
        }
    }

    pub fn failure(task_id: impl Into<String>, error: impl Into<String>, duration: Duration) -> Self {
        Self {
            task_id: task_id.into(),
            output: String::new(),
            duration,
            success: false,
            error: Some(error.into()),
        }
    }
}

/// Thread-safe storage for task outputs
#[derive(Clone)]
pub struct DataStore {
    data: Arc<RwLock<HashMap<String, TaskData>>>,
}

impl DataStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert(&self, task_data: TaskData) {
        let mut data = self.data.write().unwrap();
        data.insert(task_data.task_id.clone(), task_data);
    }

    pub fn get(&self, task_id: &str) -> Option<TaskData> {
        let data = self.data.read().unwrap();
        data.get(task_id).cloned()
    }

    pub fn get_output(&self, task_id: &str) -> Option<String> {
        self.get(task_id).map(|d| d.output)
    }

    pub fn contains(&self, task_id: &str) -> bool {
        let data = self.data.read().unwrap();
        data.contains_key(task_id)
    }

    pub fn is_success(&self, task_id: &str) -> bool {
        self.get(task_id).map(|d| d.success).unwrap_or(false)
    }
}
```

### 6. Template Resolution (template.rs)

Pattern récupéré avec sécurité (sanitization + JSON escape):

```rust
use regex::Regex;
use crate::datastore::DataStore;
use crate::error::NikaError;

/// Characters not allowed in task IDs (security)
const DANGEROUS_CHARS: &[char] = &[
    '\0', '\n', '\r',           // Control chars
    '\u{202E}', '\u{202D}',     // RTL/LTR overrides
];

/// Sanitize a task ID
fn sanitize_task_id(task_id: &str) -> String {
    task_id
        .chars()
        .filter(|c| !DANGEROUS_CHARS.contains(c) && !c.is_control())
        .collect::<String>()
        .replace("..", "")
}

/// Escape for JSON string context
fn escape_for_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Resolve all {{ task_id.output }} templates
pub fn resolve(template: &str, datastore: &DataStore) -> Result<String, NikaError> {
    let re = Regex::new(r"\{\{\s*(\w+)\.output\s*\}\}").unwrap();
    let mut result = template.to_string();
    let mut errors = Vec::new();

    for cap in re.captures_iter(template) {
        let full_match = &cap[0];
        let task_id = sanitize_task_id(&cap[1]);

        match datastore.get_output(&task_id) {
            Some(output) => {
                // Escape if we're in a JSON context
                let replacement = if is_in_json_context(template, cap.get(0).unwrap().start()) {
                    escape_for_json(&output)
                } else {
                    output
                };
                result = result.replace(full_match, &replacement);
            }
            None => {
                errors.push(task_id.clone());
            }
        }
    }

    if !errors.is_empty() {
        return Err(NikaError::Template(format!(
            "Task(s) not found: {}",
            errors.join(", ")
        )));
    }

    Ok(result)
}

/// Check if position is inside a JSON string
fn is_in_json_context(template: &str, pos: usize) -> bool {
    let before = &template[..pos];
    // Simple heuristic: count unescaped quotes
    let mut in_string = false;
    let mut escaped = false;

    for ch in before.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => in_string = !in_string,
            _ => {}
        }
    }

    in_string
}
```

### 7. Provider Trait (provider/mod.rs)

```rust
use async_trait::async_trait;
use anyhow::Result;

pub mod claude;
pub mod openai;

pub use claude::ClaudeProvider;
pub use openai::OpenAIProvider;

/// Default models per provider
pub const CLAUDE_DEFAULT_MODEL: &str = "claude-sonnet-4-5";
pub const OPENAI_DEFAULT_MODEL: &str = "gpt-4o";

#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Execute a prompt and return the response
    async fn infer(&self, prompt: &str, model: &str) -> Result<String>;

    /// Check if provider is available
    fn is_available(&self) -> bool;

    /// Default model for this provider
    fn default_model(&self) -> &str;
}

/// Create provider by name
pub fn create_provider(name: &str) -> Result<Box<dyn Provider>> {
    match name.to_lowercase().as_str() {
        "claude" => Ok(Box::new(ClaudeProvider::new()?)),
        "openai" => Ok(Box::new(OpenAIProvider::new()?)),
        "mock" => Ok(Box::new(MockProvider::default())),
        _ => anyhow::bail!(
            "Unknown provider: '{}'. Available: claude, openai, mock",
            name
        ),
    }
}

/// Mock provider for testing
#[derive(Default)]
pub struct MockProvider {
    pub response: String,
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    async fn infer(&self, _prompt: &str, _model: &str) -> Result<String> {
        Ok(self.response.clone())
    }

    fn is_available(&self) -> bool {
        true
    }

    fn default_model(&self) -> &str {
        "mock-v1"
    }
}
```

### 8. Claude Provider (provider/claude.rs)

```rust
use super::{Provider, CLAUDE_DEFAULT_MODEL};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

pub struct ClaudeProvider {
    api_key: String,
    client: Client,
}

impl ClaudeProvider {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY not set")?;

        Ok(Self {
            api_key,
            client: Client::new(),
        })
    }

    /// Resolve model aliases to full Anthropic model IDs
    fn resolve_model(&self, model: &str) -> String {
        match model.to_lowercase().as_str() {
            // Sonnet variants
            "claude-sonnet-4-5" | "claude-sonnet" | "sonnet" => {
                "claude-sonnet-4-20250514".to_string()
            }
            // Opus variants
            "claude-opus-4" | "claude-opus" | "opus" => {
                "claude-opus-4-20250514".to_string()
            }
            // Haiku variants
            "claude-haiku" | "haiku" => {
                "claude-3-5-haiku-20241022".to_string()
            }
            // Pass through if already a full model ID
            _ if model.starts_with("claude-") => model.to_string(),
            // Default
            _ => "claude-sonnet-4-20250514".to_string(),
        }
    }
}

#[async_trait]
impl Provider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn default_model(&self) -> &str {
        CLAUDE_DEFAULT_MODEL
    }

    async fn infer(&self, prompt: &str, model: &str) -> Result<String> {
        let resolved_model = self.resolve_model(model);

        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&json!({
                "model": resolved_model,
                "max_tokens": 4096,
                "messages": [
                    { "role": "user", "content": prompt }
                ]
            }))
            .send()
            .await
            .context("Failed to send request to Claude API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Claude API error {}: {}", status, body);
        }

        let json: serde_json::Value = response.json().await?;
        let text = json["content"][0]["text"]
            .as_str()
            .context("Invalid response format from Claude API")?;

        Ok(text.to_string())
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}
```

### 9. OpenAI Provider (provider/openai.rs)

```rust
use super::{Provider, OPENAI_DEFAULT_MODEL};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

pub struct OpenAIProvider {
    api_key: String,
    client: Client,
}

impl OpenAIProvider {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY not set")?;

        Ok(Self {
            api_key,
            client: Client::new(),
        })
    }

    /// Map model names to valid OpenAI models
    fn resolve_model(&self, model: &str) -> String {
        let model = model.to_lowercase();

        // Direct OpenAI models - passthrough
        match model.as_str() {
            "gpt-4o" | "gpt-4o-mini" | "gpt-4-turbo" | "gpt-3.5-turbo"
            | "o1" | "o1-mini" | "o1-preview" => return model,
            _ => {}
        }

        // Claude Haiku → GPT-4o-mini (fast/cheap)
        if model.contains("haiku") {
            return "gpt-4o-mini".to_string();
        }

        // Claude Sonnet/Opus → GPT-4o
        if model.contains("sonnet") || model.contains("opus") || model.contains("claude") {
            return "gpt-4o".to_string();
        }

        // Default to gpt-4o
        "gpt-4o".to_string()
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn default_model(&self) -> &str {
        OPENAI_DEFAULT_MODEL
    }

    async fn infer(&self, prompt: &str, model: &str) -> Result<String> {
        let resolved_model = self.resolve_model(model);

        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&json!({
                "model": resolved_model,
                "messages": [
                    { "role": "user", "content": prompt }
                ]
            }))
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, body);
        }

        let json: serde_json::Value = response.json().await?;
        let text = json["choices"][0]["message"]["content"]
            .as_str()
            .context("Invalid response format from OpenAI API")?;

        Ok(text.to_string())
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}
```

### 10. Runner (runner.rs)

Le coeur - exécution DAG avec tokio + multi-provider:

```rust
use std::time::Instant;
use tokio::task::JoinSet;
use colored::Colorize;

use crate::dag::DagAnalyzer;
use crate::datastore::{DataStore, TaskData};
use crate::error::NikaError;
use crate::provider::{create_provider, Provider};
use crate::task::{ExecDef, FetchDef, InferDef};
use crate::template;
use crate::workflow::{MergeStrategy, Task, TaskAction, Workflow};

pub struct Runner {
    workflow: Workflow,
    dag: DagAnalyzer,
    datastore: DataStore,
    /// Default provider name (from workflow or CLI)
    default_provider: String,
    /// Default model (from workflow, optional)
    default_model: Option<String>,
}

impl Runner {
    pub fn new(workflow: Workflow) -> Self {
        let dag = DagAnalyzer::from_workflow(&workflow);
        let datastore = DataStore::new();
        let default_provider = workflow.provider.clone();
        let default_model = workflow.model.clone();

        Self {
            workflow,
            dag,
            datastore,
            default_provider,
            default_model,
        }
    }

    /// Get tasks that are ready to run (all dependencies satisfied)
    fn get_ready_tasks(&self) -> Vec<&Task> {
        self.workflow
            .tasks
            .iter()
            .filter(|task| {
                // Skip if already done
                if self.datastore.contains(&task.id) {
                    return false;
                }

                // Check all dependencies are done AND successful
                let deps = self.dag.get_dependencies(&task.id);
                deps.iter().all(|dep| self.datastore.is_success(dep))
            })
            .collect()
    }

    /// Check if all tasks are done
    fn all_done(&self) -> bool {
        self.workflow
            .tasks
            .iter()
            .all(|t| self.datastore.contains(&t.id))
    }

    /// Get the final output (from tasks with no successors)
    fn get_final_output(&self) -> Option<String> {
        let final_tasks = self.dag.get_final_tasks();

        // Return first successful final task output
        for task_id in final_tasks {
            if let Some(data) = self.datastore.get(&task_id) {
                if data.success {
                    return Some(data.output);
                }
            }
        }
        None
    }

    /// Main execution loop
    pub async fn run(&self) -> Result<String, NikaError> {
        let total_tasks = self.workflow.tasks.len();
        let mut completed = 0;

        println!(
            "{} Running workflow with {} tasks...\n",
            "→".cyan(),
            total_tasks
        );

        loop {
            let ready = self.get_ready_tasks();

            // Check for completion or deadlock
            if ready.is_empty() {
                if self.all_done() {
                    break;
                }
                return Err(NikaError::Execution(
                    "Deadlock: no tasks ready but workflow not complete".to_string(),
                ));
            }

            // Spawn all ready tasks in parallel
            let mut join_set = JoinSet::new();

            for task in ready {
                let task_id = task.id.clone();
                let action = task.action.clone();
                let datastore = self.datastore.clone();
                let default_provider = self.default_provider.clone();
                let default_model = self.default_model.clone();

                // Clone what we need for the async block
                let ds_for_resolve = datastore.clone();

                join_set.spawn(async move {
                    let start = Instant::now();
                    let result = execute_task(
                        &action,
                        &ds_for_resolve,
                        &default_provider,
                        default_model.as_deref(),
                    ).await;
                    let duration = start.elapsed();

                    match result {
                        Ok(output) => TaskData::success(&task_id, output, duration),
                        Err(e) => TaskData::failure(&task_id, e.to_string(), duration),
                    }
                });

                println!("  {} {} {}", "[⟳]".yellow(), task_id, "running...".dimmed());
            }

            // Wait for all spawned tasks to complete
            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok(task_data) => {
                        completed += 1;
                        let status = if task_data.success {
                            format!("[{}/{}]", completed, total_tasks).green()
                        } else {
                            format!("[{}/{}]", completed, total_tasks).red()
                        };

                        let symbol = if task_data.success { "✓" } else { "✗" };
                        let duration = format!("({:.1}s)", task_data.duration.as_secs_f32()).dimmed();

                        println!(
                            "  {} {} {} {}",
                            status,
                            task_data.task_id,
                            symbol.green(),
                            duration
                        );

                        self.datastore.insert(task_data);
                    }
                    Err(e) => {
                        return Err(NikaError::Execution(format!("Task panicked: {}", e)));
                    }
                }
            }
        }

        // Get final output
        let output = self.get_final_output().unwrap_or_default();

        println!("\n{} Done!\n", "✓".green());
        println!("{}", "Output:".cyan().bold());
        println!("{}", output);

        Ok(output)
    }
}

/// Execute a single task
async fn execute_task(
    action: &TaskAction,
    datastore: &DataStore,
    default_provider: &str,
    default_model: Option<&str>,
) -> Result<String, NikaError> {
    match action {
        TaskAction::Infer { infer } => {
            execute_infer(infer, datastore, default_provider, default_model).await
        }
        TaskAction::Exec { exec } => execute_exec(exec, datastore).await,
        TaskAction::Fetch { fetch } => execute_fetch(fetch, datastore).await,
    }
}

async fn execute_infer(
    infer: &InferDef,
    datastore: &DataStore,
    default_provider: &str,
    default_model: Option<&str>,
) -> Result<String, NikaError> {
    // Resolve templates in prompt
    let prompt = template::resolve(&infer.prompt, datastore)?;

    // Use task-level override or workflow default
    let provider_name = infer.provider.as_deref().unwrap_or(default_provider);

    // Create provider
    let provider = create_provider(provider_name)
        .map_err(|e| NikaError::Provider(e.to_string()))?;

    // Resolve model: task override → workflow default → provider default
    let model = infer
        .model
        .as_deref()
        .or(default_model)
        .unwrap_or_else(|| provider.default_model());

    provider
        .infer(&prompt, model)
        .await
        .map_err(|e| NikaError::Provider(e.to_string()))
}

async fn execute_exec(exec: &ExecDef, datastore: &DataStore) -> Result<String, NikaError> {
    // Resolve templates in command
    let command = template::resolve(&exec.command, datastore)?;

    // Execute
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| NikaError::Execution(format!("Failed to execute command: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NikaError::Execution(format!(
            "Command failed: {}",
            stderr
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn execute_fetch(fetch: &FetchDef, datastore: &DataStore) -> Result<String, NikaError> {
    // Resolve templates in URL
    let url = template::resolve(&fetch.url, datastore)?;

    let client = reqwest::Client::new();
    let mut request = match fetch.method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        _ => client.get(&url),
    };

    // Add headers
    for (key, value) in &fetch.headers {
        let resolved_value = template::resolve(value, datastore)?;
        request = request.header(key, resolved_value);
    }

    // Add body if present
    if let Some(body) = &fetch.body {
        let resolved_body = template::resolve(body, datastore)?;
        request = request.body(resolved_body);
    }

    let response = request
        .send()
        .await
        .map_err(|e| NikaError::Execution(format!("HTTP request failed: {}", e)))?;

    response
        .text()
        .await
        .map_err(|e| NikaError::Execution(format!("Failed to read response: {}", e)))
}
```

### 11. Main CLI (main.rs)

```rust
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::fs;

mod dag;
mod datastore;
mod error;
mod provider;
mod runner;
mod task;
mod template;
mod workflow;

use error::{FixSuggestion, NikaError};
use runner::Runner;
use workflow::Workflow;

#[derive(Parser)]
#[command(name = "nika")]
#[command(about = "Nika - DAG workflow runner")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a workflow file
    Run {
        /// Path to .nika.yaml file
        file: String,

        /// Override default provider (claude, openai, mock)
        #[arg(short, long)]
        provider: Option<String>,

        /// Override default model
        #[arg(short, long)]
        model: Option<String>,
    },

    /// Validate a workflow file (parse only)
    Validate {
        /// Path to .nika.yaml file
        file: String,
    },
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Run { file, provider, model } => {
            run_workflow(&file, provider, model).await
        }
        Commands::Validate { file } => validate_workflow(&file),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        if let Some(suggestion) = e.fix_suggestion() {
            eprintln!("  {} {}", "Fix:".yellow(), suggestion);
        }
        std::process::exit(1);
    }
}

async fn run_workflow(
    file: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
) -> Result<(), NikaError> {
    // Read and parse
    let yaml = fs::read_to_string(file)?;
    let mut workflow: Workflow = serde_saphyr::from_str(&yaml)?;

    // Validate schema
    if workflow.schema != "nika/workflow@0.1" {
        return Err(NikaError::Template(format!(
            "Invalid schema: expected 'nika/workflow@0.1', got '{}'",
            workflow.schema
        )));
    }

    // Apply CLI overrides
    if let Some(p) = provider_override {
        workflow.provider = p;
    }
    if let Some(m) = model_override {
        workflow.model = Some(m);
    }

    println!(
        "{} Using provider: {} | model: {}",
        "→".cyan(),
        workflow.provider.cyan().bold(),
        workflow.model.as_deref().unwrap_or("(default)").cyan()
    );

    // Run
    let runner = Runner::new(workflow);
    runner.run().await?;

    Ok(())
}

fn validate_workflow(file: &str) -> Result<(), NikaError> {
    let yaml = fs::read_to_string(file)?;
    let workflow: Workflow = serde_saphyr::from_str(&yaml)?;

    println!("{} Workflow '{}' is valid", "✓".green(), file);
    println!("  Provider: {}", workflow.provider);
    println!("  Model: {}", workflow.model.as_deref().unwrap_or("(default)"));
    println!("  Tasks: {}", workflow.tasks.len());
    println!("  Flows: {}", workflow.flows.len());

    Ok(())
}
```

---

## CLI Usage

```bash
# Run with default provider (from workflow or claude)
nika run workflow.nika.yaml

# Override provider
nika run workflow.nika.yaml --provider openai

# Override model
nika run workflow.nika.yaml --model gpt-4o-mini

# Override both
nika run workflow.nika.yaml --provider openai --model gpt-4o

# Use mock provider for testing
nika run workflow.nika.yaml --provider mock

# Validate only
nika validate workflow.nika.yaml
```

**Output:**

```
→ Using provider: claude | model: claude-sonnet-4-5
→ Running workflow with 4 tasks...

  [⟳] read_code running...
  [1/4] read_code ✓ (0.1s)
  [⟳] security running...
  [⟳] perf running...
  [2/4] perf ✓ (1.8s)
  [3/4] security ✓ (2.3s)
  [⟳] report running...
  [4/4] report ✓ (1.5s)

✓ Done!

Output:
# Review Report
...
```

---

## Providers & Models

### Supported Providers

| Provider | Env Variable | Default Model |
|----------|--------------|---------------|
| `claude` | `ANTHROPIC_API_KEY` | `claude-sonnet-4-5` |
| `openai` | `OPENAI_API_KEY` | `gpt-4o` |
| `mock` | (none) | `mock-v1` |

### Model Aliases

**Claude:**
| Alias | Resolves To |
|-------|-------------|
| `sonnet`, `claude-sonnet-4-5`, `claude-sonnet` | `claude-sonnet-4-20250514` |
| `opus`, `claude-opus-4`, `claude-opus` | `claude-opus-4-20250514` |
| `haiku`, `claude-haiku` | `claude-3-5-haiku-20241022` |

**OpenAI:**
| Alias | Resolves To |
|-------|-------------|
| `gpt-4o` | `gpt-4o` |
| `gpt-4o-mini` | `gpt-4o-mini` |
| `o1`, `o1-mini`, `o1-preview` | (passthrough) |
| Claude models → | `gpt-4o` (haiku → `gpt-4o-mini`) |

### Model Resolution Priority

```
1. Task-level model (infer: model: haiku)
2. Workflow-level model (model: claude-sonnet-4-5)
3. Provider default (claude → claude-sonnet-4-5)
```

### Example: Multi-Provider Workflow

```yaml
schema: "nika/workflow@0.1"

# Default: use Claude Sonnet
provider: claude
model: claude-sonnet-4-5

tasks:
  # Uses Claude Sonnet (default)
  - id: analyze
    infer:
      prompt: "Analyze this code..."

  # Uses Claude Haiku (fast/cheap)
  - id: classify
    infer:
      prompt: "Classify..."
      model: haiku

  # Uses OpenAI GPT-4o (second opinion)
  - id: second_opinion
    infer:
      prompt: "Review: {{ analyze.output }}"
      provider: openai
      model: gpt-4o

flows:
  - source: analyze
    target: [classify, second_opinion]
```

---

## Success Criteria

MVP is done when:

1. ✅ Parse `workflow.nika.yaml` files
2. ✅ Execute `infer:`, `exec:`, `fetch:` verbs
3. ✅ Build and execute DAG with tokio (parallel)
4. ✅ Resolve `{{ task.output }}` templates (with security)
5. ✅ Handle fan-out (parallel execution)
6. ✅ Handle fan-in (merge strategies: all, any, first)
7. ✅ Multi-provider support (Claude + OpenAI)
8. ✅ Model selection (workflow + task level)
9. ✅ Error messages with fix suggestions
10. ✅ Output final result to stdout

---

## What's NOT in MVP

| Feature | Why excluded |
|---------|--------------|
| `agent:` verb | Multi-turn is complex, v1.1 |
| `invoke:` verb | MCP/functions are complex, v1.1 |
| Manifest file | Single file is simpler |
| `powers:` | Everything allowed |
| `inputs:` | No declared inputs |
| `outputs:` | Just stdout |
| `budgets:` | No limits |
| `shaka:` | No advisory system |
| `scope_preset:` | No context isolation |
| TUI | CLI output only |
| Checkpoints | No resume |
| 7-layer validation | Basic parsing only |

---

## Files Count

```
src/
├── main.rs       (~110 lines)
├── lib.rs        (~20 lines)
├── error.rs      (~60 lines)
├── workflow.rs   (~90 lines)
├── task.rs       (~50 lines)
├── dag.rs        (~100 lines)
├── runner.rs     (~220 lines)
├── datastore.rs  (~80 lines)
├── template.rs   (~80 lines)
└── provider/
    ├── mod.rs    (~70 lines)
    ├── claude.rs (~90 lines)
    └── openai.rs (~90 lines)

Total: ~1060 lines
```

**Target: ~1000 lignes de Rust pour un MVP fonctionnel avec multi-provider.**
