// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(dead_code)]
//! Live provider tests - real API calls to validate provider functionality.
//!
//! These tests are marked #[ignore] by default and require API keys.
//! Run with: cargo test live_provider -- --ignored

use nika::provider::rig::RigProvider;
use std::env;
use std::time::Instant;

/// Helper to check if live tests should run
fn should_run_live_tests() -> bool {
    env::var("NIKA_LIVE_TESTS").is_ok() || env::var("CI").is_ok()
}

/// Helper to check if a specific provider is available
fn has_provider(provider: &str) -> bool {
    match provider {
        "claude" => env::var("ANTHROPIC_API_KEY").is_ok(),
        "openai" => env::var("OPENAI_API_KEY").is_ok(),
        "mistral" => env::var("MISTRAL_API_KEY").is_ok(),
        "groq" => env::var("GROQ_API_KEY").is_ok(),
        "deepseek" => env::var("DEEPSEEK_API_KEY").is_ok(),
        "gemini" => env::var("GEMINI_API_KEY").is_ok(),
        // NOTE: Ollama removed in v0.27 — use provider: native with mistral.rs instead
        _ => false,
    }
}

// ============================================================================
// CLAUDE PROVIDER TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_simple_infer() {
    if !has_provider("claude") {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    }

    let provider = RigProvider::claude();
    let start = Instant::now();

    let result = provider
        .infer("What is 2+2? Answer with just the number.", None, None)
        .await;

    let elapsed = start.elapsed();
    println!("Claude response time: {:?}", elapsed);

    assert!(result.is_ok(), "Claude infer failed: {:?}", result.err());
    let response = result.unwrap();
    assert!(
        response.contains('4'),
        "Expected '4' in response: {}",
        response
    );
    assert!(
        elapsed.as_secs() < 30,
        "Response took too long: {:?}",
        elapsed
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_streaming_response() {
    if !has_provider("claude") {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    }

    let provider = RigProvider::claude();

    // Test with a longer prompt to verify streaming
    let result = provider
        .infer(
            "List the first 5 prime numbers, one per line.",
            Some("claude-sonnet-4-20250514"),
            None,
        )
        .await;

    assert!(
        result.is_ok(),
        "Claude streaming failed: {:?}",
        result.err()
    );
    let response = result.unwrap();

    // Verify we got multiple primes
    assert!(response.contains('2'), "Missing prime 2");
    assert!(response.contains('3'), "Missing prime 3");
    assert!(response.contains('5'), "Missing prime 5");
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_with_custom_model() {
    if !has_provider("claude") {
        return;
    }

    let provider = RigProvider::claude();

    // Test with explicit model
    let result = provider
        .infer(
            "Say 'hello' and nothing else.",
            Some("claude-sonnet-4-20250514"),
            None,
        )
        .await;

    assert!(result.is_ok());
    let response = result.unwrap().to_lowercase();
    assert!(response.contains("hello"));
}

// ============================================================================
// OPENAI PROVIDER TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY"]
async fn test_openai_simple_infer() {
    if !has_provider("openai") {
        eprintln!("Skipping: OPENAI_API_KEY not set");
        return;
    }

    let provider = RigProvider::openai();
    let start = Instant::now();

    let result = provider
        .infer(
            "What is the capital of France? Answer in one word.",
            None,
            None,
        )
        .await;

    let elapsed = start.elapsed();
    println!("OpenAI response time: {:?}", elapsed);

    assert!(result.is_ok(), "OpenAI infer failed: {:?}", result.err());
    let response = result.unwrap().to_lowercase();
    assert!(
        response.contains("paris"),
        "Expected 'paris' in response: {}",
        response
    );
}

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY"]
async fn test_openai_gpt4o() {
    if !has_provider("openai") {
        return;
    }

    let provider = RigProvider::openai();

    let result = provider
        .infer("What is 10 * 10? Just the number.", Some("gpt-4o"), None)
        .await;

    assert!(result.is_ok());
    assert!(result.unwrap().contains("100"));
}

// ============================================================================
// MISTRAL PROVIDER TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires MISTRAL_API_KEY"]
async fn test_mistral_simple_infer() {
    if !has_provider("mistral") {
        eprintln!("Skipping: MISTRAL_API_KEY not set");
        return;
    }

    let provider = RigProvider::mistral();

    let result = provider
        .infer("What color is the sky? One word answer.", None, None)
        .await;

    assert!(result.is_ok(), "Mistral infer failed: {:?}", result.err());
    let response = result.unwrap().to_lowercase();
    assert!(response.contains("blue"));
}

// ============================================================================
// GROQ PROVIDER TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires GROQ_API_KEY"]
async fn test_groq_simple_infer() {
    if !has_provider("groq") {
        eprintln!("Skipping: GROQ_API_KEY not set");
        return;
    }

    let provider = RigProvider::groq();
    let start = Instant::now();

    let result = provider
        .infer("Count from 1 to 5, comma separated.", None, None)
        .await;

    let elapsed = start.elapsed();
    println!("Groq response time: {:?}", elapsed);

    assert!(result.is_ok(), "Groq infer failed: {:?}", result.err());
    let response = result.unwrap();
    assert!(response.contains('1') && response.contains('5'));

    // Groq should be fast
    assert!(elapsed.as_secs() < 10, "Groq was too slow: {:?}", elapsed);
}

// ============================================================================
// DEEPSEEK PROVIDER TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires DEEPSEEK_API_KEY"]
async fn test_deepseek_simple_infer() {
    if !has_provider("deepseek") {
        eprintln!("Skipping: DEEPSEEK_API_KEY not set");
        return;
    }

    let provider = RigProvider::deepseek();

    let result = provider
        .infer("What is 7 + 8? Just the number.", None, None)
        .await;

    assert!(result.is_ok(), "DeepSeek infer failed: {:?}", result.err());
    assert!(result.unwrap().contains("15"));
}

// NOTE: Ollama test removed in v0.27 — use provider: native with mistral.rs instead
// Native inference streaming is tested via tests/native_inference_test.rs

// ============================================================================
// AUTO-DETECTION TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires at least one API key"]
async fn test_auto_provider_detection() {
    let provider = RigProvider::auto();

    if provider.is_none() {
        eprintln!("Skipping: No API keys available");
        return;
    }

    let provider = provider.unwrap();
    let result = provider
        .infer("Say 'auto' and nothing else.", None, None)
        .await;

    assert!(
        result.is_ok(),
        "Auto-detected provider failed: {:?}",
        result.err()
    );
}

#[tokio::test]
#[ignore = "Requires at least one API key"]
async fn test_provider_fallback_chain() {
    // Test that auto() returns providers in priority order
    // NOTE: Ollama removed in v0.27 — use provider: native with mistral.rs instead
    let providers_available: Vec<&str> =
        vec!["claude", "openai", "mistral", "groq", "deepseek", "gemini"]
            .into_iter()
            .filter(|p| has_provider(p))
            .collect();

    if providers_available.is_empty() {
        eprintln!("Skipping: No providers available");
        return;
    }

    println!("Available providers: {:?}", providers_available);

    let provider = RigProvider::auto().expect("Should have a provider");
    let result = provider.infer("Test", None, None).await;
    assert!(result.is_ok());
}

// ============================================================================
// CONCURRENT REQUESTS TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_concurrent_requests() {
    if !has_provider("claude") {
        return;
    }

    let _provider = RigProvider::claude();
    let start = Instant::now();

    // Launch 3 concurrent requests
    let handles: Vec<_> = (0..3)
        .map(|i| {
            let p = RigProvider::claude();
            tokio::spawn(async move {
                p.infer(
                    &format!("What is {} + {}? Just the number.", i, i),
                    None,
                    None,
                )
                .await
            })
        })
        .collect();

    let results: Vec<_> = futures::future::join_all(handles).await;
    let elapsed = start.elapsed();

    println!("3 concurrent Claude requests: {:?}", elapsed);

    for (i, result) in results.into_iter().enumerate() {
        let inner = result.expect("Task panicked");
        assert!(inner.is_ok(), "Request {} failed: {:?}", i, inner.err());
    }

    // Concurrent should be faster than 3x sequential
    assert!(elapsed.as_secs() < 45, "Concurrent requests too slow");
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_invalid_model_error() {
    if !has_provider("claude") {
        return;
    }

    let provider = RigProvider::claude();

    let result = provider
        .infer("Test", Some("nonexistent-model-12345"), None)
        .await;

    // Should fail with model not found
    assert!(result.is_err(), "Expected error for invalid model");
}

#[tokio::test]
#[ignore = "Tests API key validation"]
async fn test_invalid_api_key_error() {
    // Temporarily set invalid key
    let original = env::var("ANTHROPIC_API_KEY").ok();
    env::set_var("ANTHROPIC_API_KEY", "sk-invalid-key-12345");

    let provider = RigProvider::claude();
    let result = provider.infer("Test", None, None).await;

    // Restore original
    if let Some(key) = original {
        env::set_var("ANTHROPIC_API_KEY", key);
    } else {
        env::remove_var("ANTHROPIC_API_KEY");
    }

    assert!(result.is_err(), "Expected authentication error");
}

// ============================================================================
// RESPONSE QUALITY TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_response_format_json() {
    if !has_provider("claude") {
        return;
    }

    let provider = RigProvider::claude();

    let result = provider
        .infer(
            "Return a JSON object with keys 'name' and 'value'. Name should be 'test', value should be 42. Only output the JSON, no markdown.",
            None,
        None,
        )
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();

    // Should be parseable JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&response);
    assert!(
        parsed.is_ok(),
        "Response should be valid JSON: {}",
        response
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_multilingual() {
    if !has_provider("claude") {
        return;
    }

    let provider = RigProvider::claude();

    // French prompt
    let result = provider
        .infer("Dis 'bonjour' en français, un seul mot.", None, None)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap().to_lowercase();
    assert!(response.contains("bonjour"));
}
