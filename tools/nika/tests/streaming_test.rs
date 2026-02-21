//! Streaming integration test
//!
//! Tests the infer_stream() method with real API calls.
//! Requires API keys for respective providers:
//! - ANTHROPIC_API_KEY for Claude
//! - OPENAI_API_KEY for OpenAI
//! - MISTRAL_API_KEY for Mistral
//! - GROQ_API_KEY for Groq
//! - DEEPSEEK_API_KEY for DeepSeek
//! - OLLAMA_API_BASE_URL for Ollama (local)

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

// =============================================================================
// v0.7.0 Provider Streaming Tests (Mistral, Groq, DeepSeek, Ollama)
// =============================================================================

/// Helper to run a streaming test for any provider
async fn run_streaming_test(
    provider: RigProvider,
    provider_name: &'static str,
) -> Result<(Vec<String>, bool, bool), String> {
    let (tx, mut rx) = mpsc::channel::<StreamChunk>(100);
    let prompt = "Say 'hello' and nothing else.";

    let collector = tokio::spawn(async move {
        let mut tokens = Vec::new();
        let mut got_done = false;
        let mut got_metrics = false;

        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Token(t) => tokens.push(t),
                StreamChunk::Done(_) => got_done = true,
                StreamChunk::Metrics { .. } => got_metrics = true,
                StreamChunk::Error(e) => eprintln!("{} ERROR: {}", provider_name, e),
                _ => {}
            }
        }
        (tokens, got_done, got_metrics)
    });

    let result = provider.infer_stream(prompt, tx, None).await;
    if let Err(e) = &result {
        return Err(format!("{} infer_stream failed: {:?}", provider_name, e));
    }

    let stream_result = result.unwrap();
    eprintln!(
        "{}: tokens={}, text='{}'",
        provider_name, stream_result.total_tokens, stream_result.text
    );

    let (tokens, got_done, got_metrics) = collector.await.expect("Collector failed");
    Ok((tokens, got_done, got_metrics))
}

#[tokio::test]
#[ignore] // Run with: cargo test --test streaming_test test_mistral -- --ignored
async fn test_mistral_streaming() {
    if std::env::var("MISTRAL_API_KEY").is_err() {
        eprintln!("Skipping: MISTRAL_API_KEY not set");
        return;
    }

    let provider = RigProvider::mistral();
    match run_streaming_test(provider, "Mistral").await {
        Ok((tokens, got_done, got_metrics)) => {
            assert!(!tokens.is_empty(), "Should receive tokens");
            assert!(got_done, "Should receive Done chunk");
            assert!(got_metrics, "Should receive Metrics chunk");
            eprintln!("✅ Mistral streaming test passed! {} tokens", tokens.len());
        }
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore] // Run with: cargo test --test streaming_test test_groq -- --ignored
async fn test_groq_streaming() {
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!("Skipping: GROQ_API_KEY not set");
        return;
    }

    let provider = RigProvider::groq();
    match run_streaming_test(provider, "Groq").await {
        Ok((tokens, got_done, got_metrics)) => {
            assert!(!tokens.is_empty(), "Should receive tokens");
            assert!(got_done, "Should receive Done chunk");
            assert!(got_metrics, "Should receive Metrics chunk");
            eprintln!("✅ Groq streaming test passed! {} tokens", tokens.len());
        }
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore] // Run with: cargo test --test streaming_test test_deepseek -- --ignored
async fn test_deepseek_streaming() {
    if std::env::var("DEEPSEEK_API_KEY").is_err() {
        eprintln!("Skipping: DEEPSEEK_API_KEY not set");
        return;
    }

    let provider = RigProvider::deepseek();
    match run_streaming_test(provider, "DeepSeek").await {
        Ok((tokens, got_done, got_metrics)) => {
            assert!(!tokens.is_empty(), "Should receive tokens");
            assert!(got_done, "Should receive Done chunk");
            assert!(got_metrics, "Should receive Metrics chunk");
            eprintln!("✅ DeepSeek streaming test passed! {} tokens", tokens.len());
        }
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore] // Run with: cargo test --test streaming_test test_ollama -- --ignored
async fn test_ollama_streaming() {
    if std::env::var("OLLAMA_API_BASE_URL").is_err() {
        eprintln!("Skipping: OLLAMA_API_BASE_URL not set");
        return;
    }

    let provider = RigProvider::ollama();
    match run_streaming_test(provider, "Ollama").await {
        Ok((tokens, got_done, got_metrics)) => {
            assert!(!tokens.is_empty(), "Should receive tokens");
            assert!(got_done, "Should receive Done chunk");
            // Ollama may not always return metrics
            eprintln!(
                "✅ Ollama streaming test passed! {} tokens (metrics: {})",
                tokens.len(),
                got_metrics
            );
        }
        Err(e) => panic!("{}", e),
    }
}
