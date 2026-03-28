# Research: Testing OpenAI-Compatible API Endpoints in Rust

**Date**: 2026-03-28
**Context**: Nika custom endpoints (`provider: h100`, `base_url:`) need integration tests

---

## Summary

Five approaches exist for testing OpenAI-compatible endpoints in Rust, ranging from zero-dependency trait mocking (what rig-core does) to full fake API servers (llmposter). The recommended strategy for Nika is a layered approach: keep `provider: mock` for unit tests, add `llmposter` for HTTP-level integration tests, and optionally use Ollama in CI for smoke tests against a real model.

---

## 1. wiremock-rs / httpmock (Hand-Rolled Mock Server)

**Already used in Nika** for fetch verb tests (`tools/nika-engine/src/runtime/executor/tests_wiremock.rs`).

### Setup

```toml
[dev-dependencies]
wiremock = "0.6"
```

### Rust Code Example

```rust
#[tokio::test]
async fn test_openai_chat_completion() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-test", "object": "chat.completion",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "Hello!"},
                         "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        })))
        .mount(&server).await;
    // Point rig openai::Client at server.uri()
}
```

### CI/CD

Already in the dependency tree. No external process. Random port, parallel-safe.

### Pros

- Zero new dependencies (already using wiremock 0.6)
- Full control over response shape, headers, status codes
- Can test error paths (429, 500, malformed JSON)
- Parallel test execution (random ports)

### Cons

- Must hand-craft every OpenAI response JSON (verbose, error-prone)
- No streaming SSE support out of the box (would need custom Tower service)
- Response shapes can drift from real API without contract validation
- Each new endpoint/field requires manual fixture updates

---

## 2. llmposter (Purpose-Built LLM Mock Server)

**The most promising new option.** Rust-native, fixture-driven, supports OpenAI + Anthropic + Gemini + Responses API. Published 2026-03-23, AGPL-3.0.

### Setup

```toml
[dev-dependencies]
llmposter = { version = "0.4", default-features = false }  # disable oauth feature to minimize deps
```

### Rust Code Example

```rust
use llmposter::{ServerBuilder, Fixture};

#[tokio::test]
async fn test_openai_via_llmposter() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new()
            .match_user_message("hello")
            .respond_with_content("Hi from mock!"))
        .build().await.unwrap();
    let client = openai::Client::builder()
        .api_key("test").base_url(&format!("{}/v1", server.url()))
        .build().unwrap();
    // Use client normally -- responses are deterministic
}
```

### Failure Simulation

```rust
// Test retry logic with 429
let f = Fixture::new().match_model("fail").with_error(429, "Rate limit exceeded");
// Test stream truncation
let f = Fixture::new().with_failure(FailureConfig { truncate_after_frames: Some(3), .. });
```

### CI/CD

In-process axum server (like wiremock). No external binary. Random port. Drop to stop.

### Pros

- Speaks real OpenAI/Anthropic/Gemini wire protocol (correct response shapes)
- Built-in SSE streaming support with configurable chunk size/latency
- Failure simulation: 429, corrupt body, stream truncation, disconnect
- Tool call responses out of the box
- Multi-provider: same fixture serves OpenAI + Anthropic formats
- AGPL-3.0 (compatible with Nika's license)
- Fixture YAML files for reusable test scenarios

### Cons

- Brand new crate (v0.4.0, ~292 downloads, 1 GitHub star)
- Limited bus factor (single org: SkillDoAI)
- Pulls in axum + serde_yaml_ng + tokio as dependencies
- No OpenAPI spec validation (responses match llmposter's format, not necessarily OpenAI's exact schema)
- Only substring matching on fixtures (regex requires YAML format)

---

## 3. rig-core Trait-Level Mocking (CompletionModel)

**This is what rig-core itself uses** and what Nika's `provider: mock` already does.

### How It Works

rig-core's `CompletionModel` trait is generic. You implement it with fixed responses:

```rust
#[derive(Clone)]
struct MockModel;

impl CompletionModel for MockModel {
    type Response = ();
    type StreamingResponse = ();
    type Client = ();
    fn make(_: &(), _: impl Into<String>) -> Self { Self }
    async fn completion(&self, _: CompletionRequest)
        -> Result<CompletionResponse<()>, CompletionError> {
        Ok(CompletionResponse {
            choice: OneOrMany::one(AssistantContent::text("mock response")),
            usage: Usage { input_tokens: 10, output_tokens: 5, total_tokens: 15, cached_input_tokens: 0 },
            raw_response: (), message_id: Some("mock-1".to_string()),
        })
    }
    async fn stream(&self, _: CompletionRequest)
        -> Result<StreamingCompletionResponse<()>, CompletionError> {
        Ok(StreamingCompletionResponse::stream(Box::pin(futures::stream::empty())))
    }
}
```

### What rig-core Tests This Way

From `rig-core-0.33.0/tests/prompt_response_messages.rs`:
- `SimpleTextModel` -- fixed text responses
- `ToolThenTextModel` -- multi-turn with tool calls (AtomicUsize turn counter)
- `AlwaysToolCallModel` -- tests MaxTurnsError path

### CI/CD

Zero dependencies. Pure Rust. Instant. Already how Nika works.

### Pros

- Zero network I/O (fastest possible)
- Tests the agent loop, not the HTTP layer
- Full control over CompletionResponse shape (tool calls, usage, errors)
- Already proven in rig-core's own test suite
- Can simulate multi-turn conversations with state

### Cons

- Does NOT test HTTP serialization/deserialization
- Does NOT test base_url routing, headers, auth
- Does NOT test streaming SSE parsing
- Cannot verify OpenAI wire-protocol compatibility
- Nika's `RigProvider` enum dispatch makes it hard to inject custom CompletionModel impls at the integration level (would need refactoring)

---

## 4. Ollama for CI Testing

### Smallest Models

| Model | Size | Speed |
|-------|------|-------|
| `qwen2.5:0.5b` | 397 MB | ~100 tok/s on CPU |
| `tinyllama` | 637 MB | ~80 tok/s on CPU |
| `phi3:mini` | 2.3 GB | ~40 tok/s on CPU |
| `all-minilm` | 46 MB | Embedding only |

### GitHub Actions Setup

```yaml
jobs:
  integration-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Ollama
        run: curl -fsSL https://ollama.com/install.sh | sh
      - name: Pull smallest model
        run: ollama pull qwen2.5:0.5b
      - name: Start Ollama
        run: ollama serve &
      - name: Wait for Ollama
        run: until curl -s http://localhost:11434/api/tags; do sleep 1; done
      - name: Run integration tests
        env:
          NIKA_TEST_OLLAMA: "1"
          OLLAMA_BASE_URL: "http://localhost:11434/v1"
        run: cargo test --lib -- integration_ollama
```

### Rust Code Example

```rust
#[tokio::test]
#[ignore]  // Only run when NIKA_TEST_OLLAMA=1
async fn test_ollama_openai_compat() {
    if std::env::var("NIKA_TEST_OLLAMA").is_err() { return; }
    let base = std::env::var("OLLAMA_BASE_URL").unwrap_or("http://localhost:11434/v1".into());
    let client = openai::Client::builder().api_key("ollama").base_url(&base).build().unwrap();
    let model = client.completion_model("qwen2.5:0.5b");
    let resp = model.completion(CompletionRequest::default()).await.unwrap();
    assert!(!resp.choice.first().to_string().is_empty());
}
```

### CI/CD

- Ubuntu runners: works out of the box (CPU inference)
- macOS runners: works but slower to install
- Windows: Ollama has Windows support but GitHub runners are limited
- Cache the model pull (`~/.ollama/models/`) to save CI time

### Pros

- Tests against a REAL OpenAI-compatible server
- Validates streaming, tool calling, error handling end-to-end
- Catches real compatibility issues (field names, streaming format)
- Free, no API keys needed
- Good for smoke tests before deploying to production endpoints

### Cons

- Slow: 397 MB model download + CPU inference (30-60s per test)
- Non-deterministic responses (flaky assertions unless testing structure only)
- Adds 2-5 minutes to CI pipeline
- Ollama's OpenAI compat is not 100% (missing some fields, different error shapes)
- Model responses are low quality (0.5B) -- can only test structure, not content
- Cannot simulate failure modes (429, 500, stream truncation)

---

## 5. Contract Testing (OpenAPI Spec Validation)

### OpenAI Spec

OpenAI publishes their spec at:
`https://app.stainless.com/api/spec/documented/openai/openapi.documented.yml`

### Approach A: Response Schema Validation

Test that your mock/recorded responses match the OpenAPI schema:

```rust
// Validate response JSON against OpenAI schema
#[test]
fn test_response_matches_openai_schema() {
    let response = serde_json::json!({
        "id": "chatcmpl-test", "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "Hi"},
                     "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    });
    // Roundtrip through rig-core's types
    let parsed: rig::providers::openai::CompletionResponse =
        serde_json::from_value(response.clone()).expect("should parse");
    let reserialized = serde_json::to_value(&parsed).expect("should serialize");
    // Validate required fields exist
    assert!(reserialized["choices"][0]["message"]["content"].is_string());
}
```

### Approach B: Serde Roundtrip Tests

What async-openai does -- pure serde tests without any HTTP:

```rust
#[test]
fn chat_completion_roundtrip() {
    let request = CreateChatCompletionRequest { /* ... */ };
    let json = serde_json::to_string(&request).unwrap();
    let back: CreateChatCompletionRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, back);
}
```

### Approach C: Snapshot Testing with Real Responses

Record real API responses, save as fixtures, replay in tests:

```rust
#[test]
fn test_real_openai_response_parses() {
    let fixture = include_str!("fixtures/openai_chat_completion_response.json");
    let parsed: CompletionResponse<openai::Response> =
        serde_json::from_str(fixture).expect("real response should parse");
    assert_eq!(parsed.usage.total_tokens, 42);
}
```

### CI/CD

Pure serde tests: instant, no network, no dependencies. Snapshot tests need initial recording.

### Pros

- Catches schema drift (new fields, renamed fields, type changes)
- Zero network I/O
- Tests the exact types your code uses
- async-openai uses this pattern exclusively
- Can validate against official OpenAPI YAML spec

### Cons

- Does not test HTTP layer (status codes, headers, auth)
- Does not test streaming
- Snapshots go stale unless periodically refreshed
- Different providers have different response shapes (vLLM vs Ollama vs OpenAI)

---

## Recommendation for Nika

### Layered Testing Strategy

```
Layer 0 (existing): provider: mock           -- trait-level, no HTTP (8400+ tests)
Layer 1 (add):       llmposter fixtures       -- HTTP protocol, streaming, errors
Layer 2 (optional):  Ollama in CI             -- real model smoke tests
Layer 3 (cheap):     serde roundtrip snapshots -- schema contract validation
```

### Implementation Plan

**Step 1: llmposter for custom endpoint tests**

Add to `tools/nika-engine/Cargo.toml`:
```toml
[dev-dependencies]
llmposter = { version = "0.4", default-features = false }
```

Create `tests/openai_compat_test.rs`:
- Test `RigProvider::openai_compat()` against llmposter
- Test streaming with SSE
- Test 429 retry logic
- Test malformed response handling
- Test tool call responses for agent verb

**Step 2: Response fixture snapshots**

Record responses from each target (vLLM, Ollama, OpenAI) and store in `tests/fixtures/`:
```
tests/fixtures/
  openai_chat_completion.json
  vllm_chat_completion.json
  ollama_chat_completion.json
```
Serde roundtrip tests validate rig-core parses them all correctly.

**Step 3: Optional Ollama CI job**

Add a separate CI job (not blocking) that:
- Pulls `qwen2.5:0.5b`
- Runs `cargo test --lib -- integration_ollama`
- Validates streaming, tool calling, error paths against a real server

### What NOT to Do

- **Do not use openai-mock** -- 2 stars, unmaintained, only 0.1.0
- **Do not use llmsim** -- it's a load testing simulator, not a test mock
- **Do not use Prism/OpenAPI mock servers** -- Node.js dependencies, complex setup
- **Do not write a custom axum mock server** -- llmposter already does this

---

## Comparison Table

| Approach | HTTP Layer | Streaming | Failure Sim | Speed | New Deps | Deterministic |
|----------|-----------|-----------|-------------|-------|----------|---------------|
| wiremock (hand-rolled) | Yes | Manual | Yes | Fast | 0 (existing) | Yes |
| llmposter | Yes | Yes | Yes | Fast | 1 crate | Yes |
| rig-core trait mock | No | No | Partial | Instant | 0 | Yes |
| Ollama CI | Yes | Yes | No | Slow | External | No |
| Serde snapshots | No | No | No | Instant | 0 | Yes |

---

## Sources

1. [llmposter](https://crates.io/crates/llmposter) -- v0.4.0, AGPL-3.0, fixture-driven LLM mock server
2. [llmsim](https://crates.io/crates/llmsim) -- v0.2.3, LLM traffic simulator (load testing, not mocking)
3. [openai-mock](https://crates.io/crates/openai-mock) -- v0.1.0, unmaintained
4. [rig-core tests](https://github.com/0xPlaygrounds/rig) -- CompletionModel trait mocking pattern
5. [async-openai tests](https://github.com/64bit/async-openai) -- serde roundtrip pattern
6. [OpenAI OpenAPI spec](https://app.stainless.com/api/spec/documented/openai/openapi.documented.yml) -- official schema
7. [Ollama](https://ollama.com) -- local model runner with OpenAI-compatible API
8. [wiremock-rs](https://crates.io/crates/wiremock) -- HTTP mock server (already in Nika's deps)
