# Research Report: mistral.rs Vision Model Patterns in Rust (v0.7.0)

## Summary

mistral.rs 0.7.0 provides a mature, ergonomic Rust SDK for loading and running vision models locally. The API centers on three key types: `ModelBuilder`/`VisionModelBuilder` for model loading, `VisionMessages` for composing image+text requests, and `Model` for both blocking and streaming inference. ISQ quantization, PagedAttention, and prefix caching are all supported for vision models on CUDA, Metal, and CPU.

## Key Findings

### 1. Model Loading: Two Builder Approaches

**Preferred: `ModelBuilder` (auto-detecting)**

As of v0.7.0, `ModelBuilder` auto-detects whether a HuggingFace model is text, vision, or embedding based on `config.json`. This is the recommended approach:

```rust
use mistralrs::{IsqBits, ModelBuilder};

let model = ModelBuilder::new("google/gemma-3-4b-it")
    .with_auto_isq(IsqBits::Four)  // platform-optimal: AFQ4 on Metal, Q4K on CUDA/CPU
    .with_logging()
    .build()
    .await?;
```

**Explicit: `VisionModelBuilder`**

For explicit control over vision-specific settings like `max_edge`:

```rust
use mistralrs::{IsqType, VisionModelBuilder};

let model = VisionModelBuilder::new("Qwen/Qwen2.5-VL-3B-Instruct")
    .with_isq(IsqType::Q4K)           // explicit quantization type
    .with_max_edge(1280)               // resize images (Qwen2-VL, Idefics2 only)
    .with_logging()
    .build()
    .await?;
```

**Tested Vision Model IDs:**

| Model | HuggingFace ID |
|-------|----------------|
| Gemma 3 (4B) | `google/gemma-3-4b-it` |
| Gemma 3n | `google/gemma-3n-E4B-it` |
| Qwen3-VL | `Qwen/Qwen3-VL-4B-Instruct` |
| Qwen2.5-VL | `Qwen/Qwen2.5-VL-3B-Instruct` |
| Phi-3.5 Vision | `microsoft/Phi-3.5-vision-instruct` |
| Phi-4 Multimodal | `microsoft/Phi-4-multimodal-instruct` |
| Llama 3.2 Vision | `lamm-mit/Cephalo-Llama-3.2-11B-Vision-Instruct-128k` |
| Mistral Small 3.1 | `mistralai/Mistral-Small-3.1-24B-Instruct-2503` |
| Llama 4 Scout | `meta-llama/Llama-4-Scout-17B-16E-Instruct` |
| MiniCPM-o 2.6 | `openbmb/MiniCPM-o-2_6` |
| SmolVLM | `HuggingFaceTB/SmolVLM-Instruct` |

- Source: `mistralrs/examples/models/vision_models/main.rs`

### 2. Sending Images via `DynamicImage`

Images are passed as `image::DynamicImage` directly -- no base64 encoding, no tensor conversion. The engine handles all preprocessing internally.

```rust
use image::DynamicImage;
use mistralrs::{TextMessageRole, VisionMessages};

// From bytes (URL, file, CAS blob, etc.)
let image: DynamicImage = image::load_from_memory(&bytes)?;

// Single image message
let messages = VisionMessages::new().add_image_message(
    TextMessageRole::User,
    "What is in this image?",
    vec![image],           // Vec<DynamicImage> -- supports multiple images
);
```

**Multi-image support:**

```rust
let messages = VisionMessages::new().add_image_message(
    TextMessageRole::User,
    "Compare these two images.",
    vec![image_a, image_b],  // multiple images in one message
);
```

**Multi-turn conversation with images across turns:**

```rust
let mut messages = VisionMessages::new()
    .add_message(TextMessageRole::User, "Hello!");

// First turn: text only
let resp = model.send_chat_request(messages.clone()).await?;
messages = messages.add_message(TextMessageRole::Assistant, resp_text);

// Second turn: add an image
messages = messages.add_image_message(
    TextMessageRole::User,
    "What is depicted here?",
    vec![image],
);
let resp = model.send_chat_request(messages.clone()).await?;
```

**Key detail:** Model-specific image prefix tokens (e.g., `<image>`) are applied automatically at send-time via `resolve_pending_prefixes()`. The user never needs to insert special tokens manually.

- Source: `mistralrs/src/messages.rs` (lines 210-290)

### 3. Streaming Responses

The `Model::stream_chat_request` method returns a `Stream` that implements `futures::Stream`:

```rust
use futures::StreamExt;
use mistralrs::{Response, TextMessageRole, VisionMessages};

let messages = VisionMessages::new().add_image_message(
    TextMessageRole::User,
    "Describe this image in detail.",
    vec![image],
);

let mut stream = model.stream_chat_request(messages).await?;

while let Some(chunk) = stream.next().await {
    match chunk {
        Response::Chunk(c) => {
            if let Some(text) = c.choices.first().and_then(|ch| ch.delta.content.as_ref()) {
                print!("{text}");
            }
        }
        Response::Done(response) => {
            // Final response with full usage stats
            dbg!(response.usage.avg_prompt_tok_per_sec);
            dbg!(response.usage.avg_compl_tok_per_sec);
        }
        Response::ModelError(msg, _partial) => {
            eprintln!("Model error: {msg}");
        }
        Response::InternalError(e) => {
            eprintln!("Internal error: {e}");
        }
        Response::ValidationError(e) => {
            eprintln!("Validation error: {e}");
        }
        Response::CompletionDone(_) | Response::CompletionModelError(_, _)
        | Response::Raw { .. } | Response::ImageGeneration(_) => {
            // Not applicable for chat streaming
        }
    }
}
```

**Non-streaming (simpler):**

```rust
let response = model.send_chat_request(messages).await?;
let text = response.choices[0].message.content.as_ref().unwrap();
let usage = &response.usage;
println!("Response: {text}");
println!("Prompt: {:.1} tok/s, Completion: {:.1} tok/s",
    usage.avg_prompt_tok_per_sec,
    usage.avg_compl_tok_per_sec,
);
```

**`RequestBuilder` for fine-grained control:**

```rust
use mistralrs::RequestBuilder;

let request = RequestBuilder::from(messages)
    .set_sampler_max_len(100)           // max tokens
    .set_sampler_temperature(0.7);      // temperature

let response = model.send_chat_request(request).await?;
```

- Source: `mistralrs/src/model.rs` (stream_chat_request), `mistralrs/src/lib.rs` (doc examples)

### 4. ISQ Quantization Patterns

Two approaches:

**Auto ISQ (recommended for portability):**

```rust
use mistralrs::IsqBits;

// Selects platform-optimal type at build time:
// - Metal: AFQ4
// - CUDA/CPU: Q4K
let model = ModelBuilder::new("google/gemma-3-4b-it")
    .with_auto_isq(IsqBits::Four)   // 4-bit
    .build().await?;
```

**Explicit ISQ type:**

```rust
use mistralrs::IsqType;

let model = VisionModelBuilder::new("microsoft/Phi-3.5-vision-instruct")
    .with_isq(IsqType::Q4K)    // Q4_0, Q4_1, Q4K, Q5_0, Q5_1, Q5K, Q6K, Q8_0, Q8_1, HQQ4, HQQ8
    .build().await?;
```

**Pre-quantized UQFF models:**

```rust
use mistralrs::UqffVisionModelBuilder;

let model = UqffVisionModelBuilder::new(
    "EricB/Phi-3.5-vision-instruct-UQFF",
    vec!["phi3.5-vision-instruct-q8_0.uqff".into()],
)
.into_inner()
.with_auto_isq(IsqBits::Four)  // can further quantize on top of UQFF
.build().await?;
```

Available `IsqType` variants: `Q4_0`, `Q4_1`, `Q4K`, `Q5_0`, `Q5_1`, `Q5K`, `Q6K`, `Q8_0`, `Q8_1`, `HQQ4`, `HQQ8`.

Available `IsqBits` variants: `Four`, `Eight` (maps to platform-optimal type).

- Source: `mistralrs/src/builder_macros.rs` (with_isq, with_auto_isq)

### 5. PagedAttention and Memory Management

PagedAttention is supported for vision models on CUDA and Metal. On CUDA it is enabled by default; on Metal it must be explicitly enabled.

```rust
use mistralrs::{MemoryGpuConfig, PagedAttentionMetaBuilder, PagedCacheType};

let model = ModelBuilder::new("google/gemma-3-4b-it")
    .with_auto_isq(IsqBits::Four)
    .with_paged_attn(
        PagedAttentionMetaBuilder::default()
            .with_block_size(32)                             // tokens per block (default: 32)
            .with_gpu_memory(MemoryGpuConfig::ContextSize(4096)) // or MbAmount(8192)
            .with_cache_type(PagedCacheType::F8E4M3)         // FP8 KV cache (~50% memory savings)
            .build()?,
    )
    .build().await?;
```

**MemoryGpuConfig options:**

| Variant | Description |
|---------|-------------|
| `ContextSize(n)` | Allocate KV cache for `n` tokens of context |
| `MbAmount(mb)` | Allocate exactly `mb` megabytes for KV cache |
| `Utilization(0.0..1.0)` | Use this fraction of available GPU memory |

**Metal-specific behavior:**
- On Metal (Apple Silicon), GPU and CPU share unified memory
- mistral.rs auto-caps KV cache to `max_seq_len * max_batch_size` to avoid system memory pressure
- Override with explicit `MbAmount` if needed

**Prefix caching (enabled by default with PagedAttention):**

```rust
let model = ModelBuilder::new("model")
    .with_prefix_cache_n(Some(16))   // cache 16 sequences (default)
    .with_paged_attn(PagedAttentionMetaBuilder::default().build()?)
    .build().await?;
```

Block-level prefix caching reuses computed KV blocks across requests sharing common prefixes. Particularly useful for multi-turn vision conversations where the system prompt is constant.

- Source: `docs/PAGED_ATTENTION.md`, `mistralrs/src/builder_macros.rs`

### 6. Error Handling

The SDK uses a structured `mistralrs::error::Error` enum (thiserror-based, `#[non_exhaustive]`):

```rust
use mistralrs::error::Error;

match model.send_chat_request(messages).await {
    Ok(response) => { /* success */ }
    Err(Error::ModelLoad(e)) => {
        // Download, parsing, device mapping failures
        eprintln!("Failed to load model: {e}");
    }
    Err(Error::Inference(e)) => {
        // Runtime inference errors
        eprintln!("Inference failed: {e}");
        // Downcast to concrete type if needed:
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            eprintln!("I/O error: {io_err}");
        }
    }
    Err(Error::RequestValidation(msg)) => {
        // Empty messages, bad constraints, etc.
        eprintln!("Invalid request: {msg}");
    }
    Err(Error::ModelError { message, partial_response }) => {
        // Model-level error (e.g., context length exceeded)
        eprintln!("Model error: {message}");
        if let Some(partial) = partial_response {
            eprintln!("Partial response was: {:?}", partial);
        }
    }
    Err(Error::Channel(msg)) => {
        // Internal channel dropped
        eprintln!("Channel error: {msg}");
    }
    Err(e) => {
        // Json, Management, UnexpectedResponse, future variants
        eprintln!("Other error: {e}");
    }
}
```

The error type implements `std::error::Error` and converts seamlessly from `anyhow::Error`.

- Source: `mistralrs/src/error.rs`

### 7. Complete Integration Example (Nika-Relevant)

For Nika's use case (CAS image hash -> local vision inference), the pattern would be:

```rust
use anyhow::Result;
use futures::StreamExt;
use image::DynamicImage;
use mistralrs::{
    IsqBits, ModelBuilder, Model, Response, TextMessageRole, VisionMessages,
    PagedAttentionMetaBuilder, MemoryGpuConfig,
};

/// Load a vision model once at startup
async fn load_vision_model(model_id: &str) -> Result<Model> {
    let model = ModelBuilder::new(model_id)
        .with_auto_isq(IsqBits::Four)
        .with_paged_attn(
            PagedAttentionMetaBuilder::default()
                .with_block_size(32)
                .with_gpu_memory(MemoryGpuConfig::ContextSize(4096))
                .build()?,
        )
        .with_logging()
        .build()
        .await?;
    Ok(model)
}

/// Run vision inference on a DynamicImage (from CAS blob)
async fn describe_image(
    model: &Model,
    image: DynamicImage,
    prompt: &str,
) -> Result<String> {
    let messages = VisionMessages::new().add_image_message(
        TextMessageRole::User,
        prompt,
        vec![image],
    );

    let response = model.send_chat_request(messages).await?;
    let text = response.choices[0]
        .message
        .content
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No content in response"))?;
    Ok(text.clone())
}

/// Stream vision inference with cancellation
async fn stream_describe_image(
    model: &Model,
    image: DynamicImage,
    prompt: &str,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<String> {
    let messages = VisionMessages::new().add_image_message(
        TextMessageRole::User,
        prompt,
        vec![image],
    );

    let mut stream = model.stream_chat_request(messages).await?;
    let mut result = String::new();

    while let Some(chunk) = stream.next().await {
        if *cancel.borrow() {
            break;
        }
        match chunk {
            Response::Chunk(c) => {
                if let Some(text) = c.choices.first()
                    .and_then(|ch| ch.delta.content.as_ref())
                {
                    result.push_str(text);
                }
            }
            Response::Done(_) => break,
            Response::ModelError(msg, _) => anyhow::bail!("Model error: {msg}"),
            Response::InternalError(e) => anyhow::bail!("Internal error: {e}"),
            Response::ValidationError(e) => anyhow::bail!("Validation error: {e}"),
            _ => {}
        }
    }
    Ok(result)
}
```

## Response Type Reference

```rust
// Non-streaming response
pub struct ChatCompletionResponse {
    pub choices: Vec<Choice>,
    pub usage: Usage,
    // ...
}

pub struct Choice {
    pub message: ResponseMessage,
    pub finish_reason: String,
    // ...
}

pub struct ResponseMessage {
    pub content: Option<String>,
    pub role: String,
    // ...
}

pub struct Usage {
    pub avg_prompt_tok_per_sec: f32,
    pub avg_compl_tok_per_sec: f32,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    // ...
}

// Streaming chunk
pub struct ChatCompletionChunkResponse {
    pub choices: Vec<ChunkChoice>,
    // ...
}

pub struct ChunkChoice {
    pub delta: Delta,
    pub finish_reason: Option<String>,
    // ...
}

pub struct Delta {
    pub content: Option<String>,
    pub role: String,
    // ...
}
```

## Sources

1. [mistralrs/src/lib.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/src/lib.rs) -- SDK documentation, architecture overview, all public re-exports
2. [mistralrs/src/vision_model.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/src/vision_model.rs) -- VisionModelBuilder full source
3. [mistralrs/src/messages.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/src/messages.rs) -- VisionMessages, TextMessages, RequestBuilder implementations
4. [mistralrs/src/model.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/src/model.rs) -- Model struct: send_chat_request, stream_chat_request, chat, generate_structured
5. [mistralrs/src/error.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/src/error.rs) -- Error enum with all variants
6. [mistralrs/src/auto_model.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/src/auto_model.rs) -- ModelBuilder (auto-detecting)
7. [mistralrs/src/builder_macros.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/src/builder_macros.rs) -- common_builder_methods! (ISQ, PagedAttention, etc.)
8. [examples/getting_started/vision/main.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/examples/getting_started/vision/main.rs) -- Minimal vision example
9. [examples/models/vision_models/main.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/examples/models/vision_models/main.rs) -- All supported vision model IDs
10. [examples/models/vision_multiturn/main.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/examples/models/vision_multiturn/main.rs) -- Multi-turn vision conversation
11. [examples/quantization/uqff_vision/main.rs](https://github.com/EricLBuehler/mistral.rs/blob/master/mistralrs/examples/quantization/uqff_vision/main.rs) -- UQFF pre-quantized vision model
12. [docs/PAGED_ATTENTION.md](https://github.com/EricLBuehler/mistral.rs/blob/master/docs/PAGED_ATTENTION.md) -- PagedAttention configuration, prefix caching, Metal behavior
13. [docs.rs/mistralrs](https://docs.rs/mistralrs) -- Published API documentation
14. [crates.io/crates/mistralrs](https://crates.io/crates/mistralrs) -- v0.7.0 (latest as of 2026-01-27)

## Methodology

- Tools used: Perplexity web search (5 queries), raw GitHub source fetching (12 files), GitHub API tree listing
- Files analyzed: 12 Rust source files from the mistral.rs repository
- All code examples verified against actual source code, not inferred from search summaries
- Version: mistral.rs 0.7.0 (matches Nika's Cargo.lock)

## Confidence Level

**High** -- All patterns are sourced directly from the repository's source code and official examples, not from third-party blog posts. The API surface was verified by reading `lib.rs` re-exports, `messages.rs` trait implementations, and `model.rs` method signatures. The vision model IDs list comes from the official `vision_models` example.

## Key Differences from v0.6.x

1. `ModelBuilder` (auto-detecting) is now preferred over explicit `VisionModelBuilder`
2. `with_auto_isq(IsqBits::Four)` replaces manual `with_isq(IsqType::Q4K)` for portability
3. `VisionMessages::add_image_message()` signature changed: now takes `Vec<DynamicImage>` directly (no model reference needed)
4. `Model::chat()` convenience method added for one-shot text queries
5. `Model::generate_structured::<T>()` for JSON schema-constrained output
6. Prefix caching enabled by default with PagedAttention (block-level)
7. `error::Error` is now structured enum instead of `anyhow::Error`
8. MCP client support added via `ModelBuilder::with_mcp_client()`

## Nika Integration Notes

For PR4 (vision-media), the relevant patterns are:

1. **Model loading**: Use `ModelBuilder::new(model_id).with_auto_isq(IsqBits::Four)` for the native provider. The model auto-detects vision capability.
2. **CAS to DynamicImage**: Nika already has `decode_image_safe()` in `media/safety.rs`. The resulting `DynamicImage` can be passed directly to `VisionMessages::add_image_message()`.
3. **Streaming**: Use `model.stream_chat_request(messages).await?` with the existing cancellation token pattern from `tests_vision_e2e.rs`.
4. **Error mapping**: Map `mistralrs::error::Error` variants to `NikaError` codes in the 030-039 (provider) range.
5. **Memory**: On macOS (Metal), PagedAttention auto-caps memory. No special configuration needed for typical single-image inference.
