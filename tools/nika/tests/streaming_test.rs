//! Streaming integration test
//!
//! Tests the infer_stream() method with real API calls.
//! Requires ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable.

use nika::provider::rig::{RigInferError, RigProvider, StreamChunk, StreamResult};
use tokio::sync::mpsc;

#[tokio::test]
#[ignore] // Run with: cargo test --test streaming_test -- --ignored
async fn test_claude_streaming() {
    // Skip if no API key
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    }

    let provider = RigProvider::claude();
    let (tx, mut rx) = mpsc::channel::<StreamChunk>(100);

    // Simple prompt
    let prompt = "Say 'hello' and nothing else.";

    // Spawn task to collect chunks
    let collector = tokio::spawn(async move {
        let mut tokens = Vec::new();
        let mut done = false;
        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Token(t) => {
                    eprintln!("TOKEN: '{}'", t);
                    tokens.push(t);
                }
                StreamChunk::Thinking(t) => {
                    eprintln!("THINKING: '{}'", t);
                }
                StreamChunk::Done(t) => {
                    eprintln!("DONE: '{}'", t);
                    done = true;
                }
                StreamChunk::Error(e) => {
                    eprintln!("ERROR: '{}'", e);
                }
                StreamChunk::Metrics {
                    input_tokens,
                    output_tokens,
                } => {
                    eprintln!("METRICS: input={}, output={}", input_tokens, output_tokens);
                }
                StreamChunk::McpConnected(server) => {
                    eprintln!("MCP CONNECTED: '{}'", server);
                }
                StreamChunk::McpError { server_name, error } => {
                    eprintln!("MCP ERROR: '{}' - {}", server_name, error);
                }
            }
        }
        (tokens, done)
    });

    // Run streaming inference
    let result: Result<StreamResult, RigInferError> = provider.infer_stream(prompt, tx, None).await;
    assert!(result.is_ok(), "infer_stream failed: {:?}", result.err());

    let stream_result = result.unwrap();
    eprintln!("Complete response: '{}'", stream_result.text);
    eprintln!(
        "Token usage: input={}, output={}, total={}",
        stream_result.input_tokens, stream_result.output_tokens, stream_result.total_tokens
    );
    assert!(
        stream_result.text.to_lowercase().contains("hello"),
        "Response should contain 'hello': {}",
        stream_result.text
    );
    // Token counts should be populated (non-zero for real API calls)
    assert!(
        stream_result.total_tokens > 0,
        "Token counts should be populated"
    );

    // Wait for collector
    let (tokens, done) = collector.await.expect("Collector task failed");
    assert!(
        !tokens.is_empty(),
        "Should have received at least one token"
    );
    assert!(done, "Should have received Done chunk");

    eprintln!("✅ Streaming test passed! Received {} tokens", tokens.len());
}

#[tokio::test]
#[ignore] // Run with: cargo test --test streaming_test -- --ignored
async fn test_openai_streaming() {
    // Skip if no API key
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("Skipping: OPENAI_API_KEY not set");
        return;
    }

    let provider = RigProvider::openai();
    let (tx, mut rx) = mpsc::channel::<StreamChunk>(100);

    let prompt = "Say 'hello' and nothing else.";

    let collector = tokio::spawn(async move {
        let mut tokens = Vec::new();
        while let Some(chunk) = rx.recv().await {
            if let StreamChunk::Token(t) = chunk {
                tokens.push(t);
            }
        }
        tokens
    });

    let result: Result<StreamResult, RigInferError> = provider.infer_stream(prompt, tx, None).await;
    assert!(result.is_ok(), "infer_stream failed: {:?}", result.err());

    let stream_result = result.unwrap();
    eprintln!(
        "Token usage: input={}, output={}, total={}",
        stream_result.input_tokens, stream_result.output_tokens, stream_result.total_tokens
    );

    let tokens = collector.await.expect("Collector task failed");
    assert!(
        !tokens.is_empty(),
        "Should have received at least one token"
    );
    // Token counts should be populated for OpenAI too
    assert!(
        stream_result.total_tokens > 0,
        "Token counts should be populated"
    );

    eprintln!(
        "✅ OpenAI streaming test passed! Received {} tokens",
        tokens.len()
    );
}
