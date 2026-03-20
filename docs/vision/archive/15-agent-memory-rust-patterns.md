# Research: Rust Patterns for AI Agent Memory/Record System

**Date**: 2026-03-15
**Author**: Claude (research agent)
**Scope**: Crates, patterns, and architectural decisions for building an append-only agent memory system in Nika

---

## Summary

This report covers six interconnected areas for building an agent memory/record system in Rust: NDJSON handling, append-only log patterns, rig-core conversation history, token counting, LLM compression patterns, and serde schema evolution. The existing Nika codebase already has strong foundations in `event::TraceWriter` (NDJSON) and `io::atomic` (safe writes) that can be extended rather than replaced.

---

## 1. NDJSON Handling in Rust

### Recommendation: serde_json line-by-line (no additional crate needed)

Nika already implements the optimal pattern in `tools/nika/src/event/trace.rs`. The ecosystem does not have a dominant NDJSON crate -- the existing ones are marginal:

| Crate | Version | Downloads | Notes |
|-------|---------|-----------|-------|
| `ndjson` | 0.2.0 | 2,887 | CLI formatter/colorizer only, not a library |
| `ndjson-stream` | 0.1.0 | 2,142 | Streaming reader, single release, stale |

**The standard Rust approach is to use `serde_json` directly**, which is what Nika already does.

### Pattern: Synchronous Write (Current Nika)

```rust
// From tools/nika/src/event/trace.rs (lines 67-75)
pub fn write_event(&self, event: &Event) -> Result<()> {
    let json = serde_json::to_string(event)?;
    let mut writer = self.writer.lock();
    writeln!(writer, "{}", json)?;
    writer.flush()?;
    Ok(())
}
```

**Key properties**:
- `BufWriter<File>` for buffered I/O
- `parking_lot::Mutex` for lock performance (2-3x faster than std)
- `flush()` after each event for durability
- `writeln!` ensures newline delimiter

### Pattern: Async NDJSON Writer (for new memory system)

```rust
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};

pub struct AsyncNdjsonWriter {
    writer: tokio::sync::Mutex<BufWriter<tokio::fs::File>>,
    path: PathBuf,
}

impl AsyncNdjsonWriter {
    pub async fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)  // O_APPEND for kernel-level atomicity
            .open(path)
            .await?;
        Ok(Self {
            writer: tokio::sync::Mutex::new(BufWriter::new(file)),
            path: path.to_path_buf(),
        })
    }

    pub async fn append<T: Serialize>(&self, record: &T) -> io::Result<()> {
        let mut json = serde_json::to_vec(record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        json.push(b'\n');

        let mut writer = self.writer.lock().await;
        writer.write_all(&json).await?;
        writer.flush().await?;
        Ok(())
    }
}
```

### Pattern: NDJSON Reader (async streaming)

```rust
use tokio::io::{AsyncBufReadExt, BufReader};

pub async fn read_ndjson<T: DeserializeOwned>(
    path: &Path,
) -> io::Result<Vec<T>> {
    let file = tokio::fs::File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut records = Vec::new();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() { continue; }
        match serde_json::from_str(&line) {
            Ok(record) => records.push(record),
            Err(e) => {
                tracing::warn!(
                    line = %line.chars().take(80).collect::<String>(),
                    error = %e,
                    "Skipping malformed NDJSON line"
                );
            }
        }
    }
    Ok(records)
}
```

### Pattern: Streaming Iterator (memory-efficient, sync)

```rust
pub fn iter_ndjson<T: DeserializeOwned>(
    reader: impl BufRead,
) -> impl Iterator<Item = Result<T, serde_json::Error>> {
    reader.lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(&line))
}
```

---

## 2. Append-Only Log Patterns in Rust

### Existing Crates

| Crate | Version | Downloads | Architecture |
|-------|---------|-----------|-------------|
| `aol` | 0.3.2 | 11,951 | Append-only log with segments, CRC32 checksums |
| `waly` | 0.1.4 | 493 | Simple WAL with `Arc<Mutex<File>>`, JSON entries |
| `walrus-rust` | 0.2.0 | 816 | WAL with page-based storage |

**Recommendation**: None of these are production-ready enough. Build on Nika's existing `io::atomic` module.

### Nika's Existing Foundation

Nika already has the building blocks in `tools/nika/src/io/atomic.rs`:

```rust
// Atomic write: temp file -> flush -> sync -> rename
pub async fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()>

// Append to file
pub async fn write_append(path: &Path, content: &[u8]) -> io::Result<()>

// Unique filename generation
pub async fn write_unique(path: &Path, content: &[u8]) -> io::Result<PathBuf>

// Fail-if-exists (atomic check)
pub async fn write_fail(path: &Path, content: &[u8]) -> io::Result<()>
```

### Pattern: Append-Only NDJSON Log with Corruption Prevention

```rust
use std::io::Write;

pub struct AppendLog {
    file: std::fs::File,
    path: PathBuf,
    /// Byte offset of last successful write (for recovery)
    last_good_offset: u64,
}

impl AppendLog {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        let last_good_offset = file.metadata()?.len();

        Ok(Self {
            file,
            path: path.to_path_buf(),
            last_good_offset,
        })
    }

    pub fn append<T: Serialize>(&mut self, record: &T) -> io::Result<()> {
        let mut json = serde_json::to_vec(record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        json.push(b'\n');

        // Write + fsync for durability
        self.file.write_all(&json)?;
        self.file.sync_data()?;  // fdatasync - faster than sync_all

        self.last_good_offset += json.len() as u64;
        Ok(())
    }

    /// Truncate any partial write after crash recovery
    pub fn recover(&mut self) -> io::Result<()> {
        let actual_len = self.file.metadata()?.len();
        if actual_len > self.last_good_offset {
            self.file.set_len(self.last_good_offset)?;
            tracing::warn!(
                truncated_bytes = actual_len - self.last_good_offset,
                "Recovered from partial write"
            );
        }
        Ok(())
    }
}
```

### fsync Strategy (Performance vs. Durability)

| Strategy | Method | Durability | Performance |
|----------|--------|-----------|-------------|
| Per-record | `sync_data()` after each write | Highest | ~200-500 records/sec |
| Batched | `sync_data()` every N records or T ms | Good | ~5,000-10,000 records/sec |
| OS-managed | No explicit sync, rely on OS | Lowest | ~50,000+ records/sec |

**Recommendation for agent memory**: Batched sync (every 100ms or 10 records) is the sweet spot. Agent turns are infrequent enough that per-record sync is also acceptable.

### Pattern: CRC32 Integrity Checking (Optional)

```rust
use crc32fast::Hasher;

#[derive(Serialize, Deserialize)]
pub struct ChecksummedRecord<T> {
    pub data: T,
    pub crc32: u32,
}

impl<T: Serialize> ChecksummedRecord<T> {
    pub fn new(data: T) -> Result<Self, serde_json::Error> {
        let json = serde_json::to_vec(&data)?;
        let mut hasher = Hasher::new();
        hasher.update(&json);
        Ok(Self { data, crc32: hasher.finalize() })
    }
}
```

---

## 3. rig-core Conversation History and Agent Memory

### Current Version: rig-core v0.32.0

**Repository**: https://github.com/0xPlaygrounds/rig (under `rig/rig-core/`)
**Cargo.toml**: `rig-core = "0.32.0"` (published 2026-03-05)
**Key deps**: serde, tokio, reqwest, schemars, rmcp 0.16 (optional)

### Key Architecture Finding

rig-core has **no built-in persistence for conversation history**. History is passed as `Vec<Message>` at call sites. This is by design -- rig-core is stateless, and the caller (Nika) owns the history.

### Message Types (`rig/rig-core/src/completion/message.rs`)

```rust
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User { content: OneOrMany<UserContent> },
    Assistant {
        id: Option<String>,
        content: OneOrMany<AssistantContent>,
    },
}

// User can send text, tool results, images, audio, video, documents
pub enum UserContent {
    Text(Text),
    ToolResult(ToolResult),
    Image(Image),
    Audio(Audio),
    Video(Video),
    Document(Document),
}

// Assistant responds with text, tool calls, reasoning, or images
pub enum AssistantContent {
    Text(Text),
    ToolCall(ToolCall),
    Reasoning(Reasoning),  // Extended thinking support
    Image(Image),
}
```

### Chat Interface

```rust
// High-level chat with history
pub trait Chat {
    async fn chat(
        &self,
        prompt: impl Into<Message>,
        chat_history: Vec<Message>,  // Caller owns history
    ) -> Result<String, PromptError>;
}

// Low-level completion with full control
pub trait Completion<M: CompletionModel> {
    async fn completion(
        &self,
        prompt: impl Into<Message>,
        chat_history: Vec<Message>,
    ) -> Result<CompletionRequestBuilder<M>, CompletionError>;
}
```

### Agent Loop (Multi-Turn Tool Calling)

The agent loop in `rig/rig-core/src/agent/prompt_request/mod.rs` implements:

1. **Prompt** -- Build `CompletionRequest` with history
2. **Call model** -- Get response (text or tool calls)
3. **If tool calls**: Execute tools, append results to history, loop back
4. **If text**: Return response
5. **Max turns** protection via `max_turns` setting

Key code from the source:

```rust
// From PromptRequest::send() (prompt_request/mod.rs)
let chat_history = if let Some(history) = self.chat_history.as_mut() {
    history.push(self.prompt.to_owned());
    history
} else {
    &mut vec![self.prompt.to_owned()]
};

// ... agent loop runs ...
// On max turns exceeded:
Err(PromptError::MaxTurnsError {
    max_turns,
    chat_history: Box::new(chat_history),
    prompt: Box::new(prompt),
})
```

The loop appends assistant responses (including tool calls) and tool results to history, then sends the full history back to the model on each turn.

### PromptHook System (for observability)

rig-core v0.32.0 provides `PromptHook` trait for intercepting the agent loop (`agent/prompt_request/hooks.rs`):

```rust
pub trait PromptHook<M: CompletionModel>: Clone + Send + Sync {
    async fn on_completion_call(
        &self, prompt: &Message, history: &[Message]
    ) -> HookAction;

    async fn on_completion_response(
        &self, prompt: &Message, response: &CompletionResponse<M::Response>
    ) -> HookAction;

    async fn on_tool_call(
        &self, tool_name: &str, call_id: Option<String>,
        internal_id: &str, args: &str
    ) -> ToolCallHookAction;

    async fn on_tool_result(
        &self, tool_name: &str, call_id: Option<String>,
        internal_id: &str, args: &str, result: &str
    ) -> HookAction;

    async fn on_text_delta(
        &self, text_delta: &str, aggregated_text: &str
    ) -> HookAction;

    async fn on_tool_call_delta(
        &self, tool_call_id: &str, internal_call_id: &str,
        tool_name: Option<&str>, tool_call_delta: &str
    ) -> HookAction;

    async fn on_stream_completion_response_finish(
        &self, prompt: &Message, response: &M::StreamingResponse
    ) -> HookAction;
}

pub enum HookAction {
    Continue,
    Terminate { reason: String },
}

pub enum ToolCallHookAction {
    Continue,
    Skip { reason: String },
    Terminate { reason: String },
}
```

**This is the integration point for Nika's memory system** -- a `PromptHook` implementation can intercept every turn, tool call, and response to persist to the append-only log.

### Reasoning Capture

rig-core supports extended thinking via `Reasoning` and `ReasoningContent`:

```rust
pub struct Reasoning {
    pub id: Option<String>,
    pub content: Vec<ReasoningContent>,
}

pub enum ReasoningContent {
    Text { text: String, signature: Option<String> },
    Encrypted(String),
    Redacted { data: String },
    Summary(String),
}
```

### Extended Response Details

```rust
// Use .extended_details() for token usage and full message history
let response = agent.prompt("Hello")
    .extended_details()
    .await?;

// response.output: String
// response.usage: Usage { input_tokens, output_tokens }
// response.messages: Option<Vec<Message>>  -- full history from the loop
```

### Dynamic Context (RAG)

```rust
pub struct Agent<M, P> {
    // ...
    pub static_context: Vec<Document>,
    pub dynamic_context: DynamicContextStore,  // Vector store indexes
    // ...
}

type DynamicContextStore = Arc<
    TokioRwLock<Vec<(usize, Box<dyn VectorStoreIndexDyn + Send + Sync>)>>
>;
```

### Implications for Nika Memory System

1. **History is ephemeral**: rig-core does not persist history -- Nika must own this
2. **PromptHook is the observability API**: Use it to capture turns for the memory log
3. **Message is serde-compatible**: `Message` derives `Serialize`/`Deserialize` -- can be stored directly in NDJSON
4. **Reasoning is captured**: Extended thinking blocks can be persisted alongside responses
5. **Token usage is available**: Via `PromptResponse` with `extended_details()`
6. **MaxTurnsError returns history**: Even on failure, the full chat history is returned for recovery

---

## 4. Token Counting in Rust

### Crate Comparison

| Crate | Version | Downloads | Encodings | Pure Rust? | Maintained? |
|-------|---------|-----------|-----------|-----------|-------------|
| **tiktoken-rs** | 0.9.1 | 4,919,158 | o200k_harmony, o200k, cl100k, p50k, r50k | Yes | Active (Nov 2025) |
| **tokenizers** | 0.22.2 | 12,012,257 | HuggingFace tokenizers (BPE, WordPiece, etc.) | Yes | Active (HuggingFace) |
| **tiktoken** | 3.1.2 | 5,048 | Same as tiktoken-rs | Yes | Active (Mar 2026) |

### Recommendation: tiktoken-rs v0.9.1

**By far the most popular** with 4.9M downloads. Supports all OpenAI encoding schemes including the latest `o200k_harmony` for GPT-5/o4 models.

```toml
[dependencies]
tiktoken-rs = "0.9"
```

### Usage: Basic Token Counting

```rust
use tiktoken_rs::o200k_base;

fn count_tokens(text: &str) -> usize {
    let bpe = o200k_base().unwrap();
    bpe.encode_with_special_tokens(text).len()
}
```

### Usage: Model-Aware Token Counting

```rust
use tiktoken_rs::{get_chat_completion_max_tokens, ChatCompletionRequestMessage};

let messages = vec![
    ChatCompletionRequestMessage {
        content: Some("You are a helpful assistant.".to_string()),
        role: "system".to_string(),
        name: None,
        function_call: None,
    },
    ChatCompletionRequestMessage {
        content: Some("Hello!".to_string()),
        role: "user".to_string(),
        name: None,
        function_call: None,
    },
];
let max_tokens = get_chat_completion_max_tokens("o1-mini", &messages).unwrap();
```

### Supported Encodings

| Encoding | Models | Tokens/Char (approx) |
|----------|--------|--------------------|
| `o200k_harmony` | GPT-5, gpt-oss-20b/120b | ~0.25 |
| `o200k_base` | GPT-4.1, GPT-4o, o4, o3, o1 | ~0.25 |
| `cl100k_base` | ChatGPT, text-embedding-ada-002 | ~0.25 |
| `p50k_base` | Code models, text-davinci-002/003 | ~0.30 |

### Alternative: HuggingFace tokenizers v0.22.2

Use this if you need to support non-OpenAI models (Llama, Mistral local models):

```toml
[dependencies]
tokenizers = { version = "0.22", features = ["onig"] }
```

```rust
use tokenizers::Tokenizer;

let tokenizer = Tokenizer::from_pretrained("mistralai/Mistral-7B-v0.1", None)?;
let encoding = tokenizer.encode("Hello world", false)?;
println!("Tokens: {}", encoding.get_ids().len());
```

### Fast Estimation (No Encoding)

For approximate token counts without loading a full tokenizer:

```rust
/// Approximate token count using character-based heuristic.
/// Accurate to ~10-15% for English text.
pub fn estimate_tokens(text: &str) -> usize {
    // Average ratio: ~4 characters per token for English
    (text.len() as f64 / 4.0).ceil() as usize
}

/// More accurate estimation using word + punctuation counting.
pub fn estimate_tokens_better(text: &str) -> usize {
    let words = text.split_whitespace().count();
    let punct = text.chars().filter(|c| c.is_ascii_punctuation()).count();
    // ~1.3 tokens per word on average, punctuation typically 1 token each
    ((words as f64 * 1.3) + punct as f64).ceil() as usize
}
```

### Integration Pattern for Memory System

```rust
use tiktoken_rs::o200k_base;
use std::sync::OnceLock;

static BPE: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();

fn get_tokenizer() -> &'static tiktoken_rs::CoreBPE {
    BPE.get_or_init(|| o200k_base().expect("Failed to load tokenizer"))
}

/// Count tokens across a list of rig-core Messages
pub fn count_message_tokens(messages: &[rig::completion::Message]) -> usize {
    let tokenizer = get_tokenizer();
    messages.iter().map(|msg| {
        let text = serde_json::to_string(msg).unwrap_or_default();
        tokenizer.encode_with_special_tokens(&text).len() + 4  // +4 for message overhead
    }).sum()
}
```

---

## 5. LLM Compression Patterns (Summarization via rig-core)

### Pattern: Summarization Agent

Using rig-core's Agent to compress/summarize conversation history:

```rust
use rig::completion::Prompt;

pub struct MemoryCompressor<M: CompletionModel> {
    agent: rig::agent::Agent<M>,
}

impl<M: CompletionModel> MemoryCompressor<M> {
    pub fn new(model: M) -> Self {
        let agent = model
            .agent("claude-sonnet-4-20250514")
            .preamble(
                "You are a conversation memory compressor. \
                 Given a conversation history, produce a concise summary that \
                 preserves all key facts, decisions, tool results, and context. \
                 Output only the summary, no preamble."
            )
            .temperature(0.0)
            .build();
        Self { agent }
    }

    pub async fn compress(
        &self,
        messages: &[Message],
        token_budget: usize,
    ) -> Result<String, PromptError> {
        let history_json = serde_json::to_string_pretty(messages)
            .unwrap_or_default();
        let prompt = format!(
            "Compress this conversation to fit within ~{} tokens. \
             Preserve: tool call results, decisions, key facts, user preferences.\n\n\
             Conversation:\n{}",
            token_budget, history_json
        );
        self.agent.prompt(&prompt).await
    }
}
```

### Pattern: Hierarchical Compression (Rolling Summary)

```rust
pub struct RollingMemory<M: CompletionModel> {
    /// Compressed summary of older messages
    summary: String,
    /// Recent messages kept in full
    recent: Vec<Message>,
    /// Max tokens for recent window
    recent_token_budget: usize,
    /// Compressor agent
    compressor: MemoryCompressor<M>,
}

impl<M: CompletionModel> RollingMemory<M> {
    pub async fn add_turn(
        &mut self,
        user: Message,
        assistant: Message,
    ) -> Result<(), PromptError> {
        self.recent.push(user);
        self.recent.push(assistant);

        let current_tokens = count_message_tokens(&self.recent);
        if current_tokens > self.recent_token_budget {
            // Move oldest messages to summary
            let to_compress: Vec<_> =
                self.recent.drain(..self.recent.len() / 2).collect();
            let additional_summary = self.compressor
                .compress(&to_compress, self.recent_token_budget / 4)
                .await?;
            self.summary = format!(
                "{}\n\n{}",
                self.summary, additional_summary
            );
        }
        Ok(())
    }

    pub fn build_context(&self) -> Vec<Message> {
        let mut context = vec![];
        if !self.summary.is_empty() {
            context.push(Message::User {
                content: OneOrMany::one(UserContent::Text(Text {
                    text: format!(
                        "[Previous conversation summary]\n{}",
                        self.summary
                    ),
                })),
            });
        }
        context.extend(self.recent.clone());
        context
    }
}
```

### Pattern: Structured Extraction (Memory Records)

```rust
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MemoryExtraction {
    /// Key facts learned during conversation
    pub facts: Vec<String>,
    /// Decisions made
    pub decisions: Vec<String>,
    /// User preferences observed
    pub preferences: Vec<String>,
    /// Tool results worth preserving
    pub tool_results: Vec<ToolResultSummary>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ToolResultSummary {
    pub tool: String,
    pub key_data: String,
}
```

This could be extracted using rig-core's typed prompt (structured output):

```rust
let extraction: MemoryExtraction = agent
    .prompt(&format!("Extract key memories from:\n{}", history_json))
    .prompt_typed()
    .await?;
```

---

## 6. Serde Patterns for Evolving Schemas

### The Problem

When storing NDJSON records over time, the schema will evolve (new fields, renamed fields, removed fields). Older records must remain readable.

### Crate Comparison

| Crate | Version | Downloads | Approach |
|-------|---------|-----------|----------|
| **magic_migrate** | 2.0.0 | 8,528 | Chain of `TryFrom` conversions, derive macro |
| **serde_flow** | 1.1.1 | 59,818 | Binary versioning with migration functions |
| **pro-serde-versioned** | 1.0.2 | 8,197 | Version byte prepended to serialized data |
| **serde-evolve** | 0.1.0 | 1,293 | Compile-time verified migrations |

### Recommendation: Built-in serde patterns (no additional crate)

For NDJSON specifically, the built-in serde attributes handle 90% of schema evolution needs without external dependencies.

### Pattern 1: Forward-Compatible Records (Additive Changes)

This is the simplest and most robust approach for NDJSON:

```rust
#[derive(Serialize, Deserialize)]
pub struct MemoryRecord {
    /// Schema version (always written, defaults to 1 for old records)
    #[serde(default = "default_v1")]
    pub v: u32,

    /// Record type tag
    #[serde(rename = "type")]
    pub record_type: String,

    /// ISO 8601 timestamp
    pub ts: String,

    /// Added in v2 -- Option + default handles old records
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u32>,

    /// Added in v3
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,

    /// Catch-all for unknown future fields (forward compatibility)
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_v1() -> u32 { 1 }
```

**Key serde attributes for evolution**:

| Attribute | Purpose | Example |
|-----------|---------|---------|
| `#[serde(default)]` | Missing field gets Default | New optional fields |
| `#[serde(default = "fn")]` | Custom default function | Version field |
| `#[serde(flatten)]` | Catch unknown fields in a map | Future fields |
| `#[serde(skip_serializing_if = "...")]` | Omit None/empty fields | Clean output |
| `#[serde(alias = "old_name")]` | Accept renamed fields | Migrations |
| `#[serde(deny_unknown_fields)]` | Strict mode (use sparingly) | Validation |

### Pattern 2: Tagged Version Dispatch

For breaking changes that cannot be handled with defaults:

```rust
use serde_json::Value;

#[derive(Serialize, Deserialize)]
#[serde(tag = "v")]
pub enum VersionedRecord {
    #[serde(rename = "1")]
    V1(RecordV1),
    #[serde(rename = "2")]
    V2(RecordV2),
    #[serde(rename = "3")]
    V3(RecordV3),
}

#[derive(Serialize, Deserialize)]
pub struct RecordV1 {
    pub timestamp: String,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
pub struct RecordV2 {
    pub timestamp: String,
    pub content: String,
    pub token_count: u32,
}

#[derive(Serialize, Deserialize)]
pub struct RecordV3 {
    pub timestamp: String,
    pub content: String,
    pub token_count: u32,
    pub model_id: String,
    pub compressed: bool,
}

// Migration chain
impl From<RecordV1> for RecordV3 {
    fn from(v1: RecordV1) -> Self {
        RecordV3 {
            timestamp: v1.timestamp,
            content: v1.content,
            token_count: 0,
            model_id: "unknown".to_string(),
            compressed: false,
        }
    }
}

impl From<RecordV2> for RecordV3 {
    fn from(v2: RecordV2) -> Self {
        RecordV3 {
            timestamp: v2.timestamp,
            content: v2.content,
            token_count: v2.token_count,
            model_id: "unknown".to_string(),
            compressed: false,
        }
    }
}

/// Read any version, migrate to latest
pub fn read_record(line: &str) -> Result<RecordV3, serde_json::Error> {
    let versioned: VersionedRecord = serde_json::from_str(line)?;
    Ok(match versioned {
        VersionedRecord::V1(v1) => v1.into(),
        VersionedRecord::V2(v2) => v2.into(),
        VersionedRecord::V3(v3) => v3,
    })
}
```

### Pattern 3: Graceful Degradation Reader

```rust
/// Read NDJSON with mixed schema versions, skipping unparseable lines
pub fn read_mixed_ndjson<T: DeserializeOwned>(
    path: &Path,
) -> io::Result<(Vec<T>, Vec<String>)> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut records = Vec::new();
    let mut errors = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        match serde_json::from_str::<T>(&line) {
            Ok(record) => records.push(record),
            Err(e) => {
                errors.push(format!("Line {}: {}", line_num + 1, e));
                tracing::debug!(
                    line_num, error = %e,
                    "Skipping incompatible record"
                );
            }
        }
    }
    Ok((records, errors))
}
```

### Pattern 4: magic_migrate for Complex Migrations

If migrations involve complex logic (not just additive fields), `magic_migrate` v2.0.0 provides a derive-based chain:

```rust
use magic_migrate::TryMigrate;

#[derive(TryMigrate, Deserialize)]
#[try_migrate(from = None)]
struct RecordV1 { name: String }

#[derive(TryMigrate, Deserialize)]
#[try_migrate(from = RecordV1)]
struct RecordV2 { full_name: String, token_count: u32 }

impl TryFrom<RecordV1> for RecordV2 {
    type Error = std::convert::Infallible;
    fn try_from(v1: RecordV1) -> Result<Self, Self::Error> {
        Ok(RecordV2 {
            full_name: v1.name,
            token_count: 0,
        })
    }
}

// Automatically tries each version in the chain
let record = RecordV2::try_from_str_migrations(json_str);
```

**Note**: magic_migrate defaults to TOML deserialization. For JSON NDJSON, you would need the custom `deserializer` attribute or manual deserialization.

### Recommended Approach for Nika

**Use Pattern 1 (additive, serde defaults) as the primary approach**, escalating to Pattern 2 (tagged version dispatch) only for breaking changes:

```rust
/// Memory record with forward-compatible schema
#[derive(Serialize, Deserialize)]
pub struct MemoryRecord {
    /// Schema version (always written, defaults to 1 for old records)
    #[serde(default = "default_v1")]
    pub v: u32,

    /// Record type tag (discriminator)
    #[serde(rename = "type")]
    pub record_type: String,

    /// ISO 8601 timestamp
    pub ts: String,

    /// Record payload (varies by type)
    #[serde(flatten)]
    pub data: serde_json::Value,
}
```

---

## Architectural Recommendation

### Proposed Memory Module Structure

```
tools/nika/src/memory/
  mod.rs          -- Module exports
  record.rs       -- MemoryRecord types (versioned, forward-compatible)
  store.rs        -- AppendOnlyStore (NDJSON file writer/reader)
  compressor.rs   -- LLM-based summarization using rig-core Agent
  tokens.rs       -- Token counting utilities (tiktoken-rs wrapper)
  reader.rs       -- NDJSON reader with schema migration
```

### Key Dependencies

```toml
[dependencies]
# Already in Nika
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["fs", "io-util"] }
parking_lot = "0.12"
rig-core = "0.32"

# New additions
tiktoken-rs = "0.9"           # Token counting (4.9M downloads, actively maintained)
# crc32fast = "1"             # Optional: integrity checking
```

### Integration Points with Existing Nika Code

| Nika Component | Memory Integration |
|---------------|-------------------|
| `event::TraceWriter` | Extend NDJSON pattern for memory records (same architecture) |
| `io::atomic` | Use `write_append` for durable writes |
| `runtime::rig_agent_loop` | Use rig-core `PromptHook` to capture turns |
| `event::EventLog` | Emit memory events alongside workflow events |
| `event::log::AgentTurnMetadata` | Already captures thinking, tokens, stop_reason |
| `provider/` | Token counting for budget management |

### How rig-core PromptHook Feeds the Memory Log

```
  ┌─────────────────────────────────────────────────────────────┐
  │  Nika Runtime                                                │
  │                                                              │
  │  RigAgentLoop                                                │
  │    |                                                         │
  │    +-- Agent.prompt("user message")                          │
  │         |                                                    │
  │         +-- .with_hook(NikaMemoryHook)                       │
  │              |                                               │
  │              on_completion_call() ----> MemoryStore.append()  │
  │              on_tool_call()       ----> MemoryStore.append()  │
  │              on_tool_result()     ----> MemoryStore.append()  │
  │              on_completion_response() -> MemoryStore.append() │
  │                                                              │
  │  MemoryStore                                                 │
  │    |                                                         │
  │    +-- .nika/memory/{session_id}.ndjson                      │
  │    |   (append-only, per-session NDJSON)                     │
  │    |                                                         │
  │    +-- Token counting via tiktoken-rs                        │
  │    +-- Rolling compression when budget exceeded              │
  │                                                              │
  └─────────────────────────────────────────────────────────────┘
```

---

## Sources

1. **rig-core v0.32.0** -- https://github.com/0xPlaygrounds/rig (`rig/rig-core/`)
   - `src/completion/message.rs` -- Message types (User/Assistant/Reasoning)
   - `src/agent/completion.rs` -- Agent struct, Chat trait, build_completion_request
   - `src/agent/prompt_request/mod.rs` -- Multi-turn agent loop with tool calling
   - `src/agent/prompt_request/hooks.rs` -- PromptHook observability API (7 hooks)
   - `src/agent/mod.rs` -- Agent module documentation and examples
   - `src/completion/mod.rs` -- Prompt/Chat/Completion traits
   - `Cargo.toml` -- v0.32.0, schemars, rmcp 0.16
2. **tiktoken-rs v0.9.1** -- https://github.com/zurawiki/tiktoken-rs (4.9M downloads)
   - Supports o200k_harmony (GPT-5), o200k_base (GPT-4o/o3/o1), cl100k_base, p50k, r50k
3. **tokenizers v0.22.2** -- https://github.com/huggingface/tokenizers (12M downloads)
   - HuggingFace tokenizer library, needed for local model tokenization
4. **magic_migrate v2.0.0** -- https://github.com/schneems/magic_migrate (8.5K downloads)
   - Derive-based chain of TryFrom migrations
5. **serde_flow v1.1.1** -- crates.io (59.8K downloads) -- Binary versioning
6. **aol v0.3.2** -- crates.io (12K downloads) -- Append-only log with CRC32
7. **waly v0.1.4** -- crates.io (493 downloads) -- Simple WAL
8. **Nika source** -- `tools/nika/src/event/trace.rs` (existing NDJSON TraceWriter)
9. **Nika source** -- `tools/nika/src/io/atomic.rs` (existing atomic write primitives)
10. **Nika source** -- `tools/nika/src/event/log.rs` (EventKind: 34+ variants, AgentTurnMetadata)
11. **Nika source** -- `tools/nika/src/event/emitter.rs` (EventEmitter trait, NoopEmitter)

## Methodology

- **Tools used**: crates.io API, GitHub raw file access, local Nika source reading
- **Pages analyzed**: ~30 source files and READMEs across rig-core, tiktoken-rs, magic_migrate, and Nika
- **Crates evaluated**: 15+ crates across 6 categories
- **Source code verified**: All rig-core patterns read directly from `refs/heads/main` on 2026-03-15

## Confidence Level

**High** -- All recommendations are based on:
- Actual crate source code and documentation (not just descriptions)
- Existing Nika patterns that have been in production (TraceWriter, io::atomic, EventLog)
- rig-core v0.32.0 actual API (read from source on main branch)
- Download counts and maintenance activity as quality signals
- Cross-referenced multiple crates in each category before recommending
