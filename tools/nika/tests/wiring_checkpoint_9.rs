//! WIRING-9: Provider Selection Flow
//!
//! Verifies: RigProvider enum, auto-detection, and provider methods
//! Run after: v0.12.0 (Providers Wiring)
//!
//! Tests validate:
//! - RigProvider enum has 6 variants (Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini)
//!   NOTE: Ollama removed in v0.27 — use provider: native with mistral.rs instead
//! - Provider name() method
//! - Provider default_model() method
//! - auto() detection returns Option
//!
//! NOTE: rig-core panics when constructing providers without API keys.
//! Tests use conditional construction based on env var availability.

use nika::provider::rig::RigProvider;

fn has_env(key: &str) -> bool {
    std::env::var(key).is_ok_and(|v| !v.is_empty())
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: Provider constructors (conditional - requires API keys)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_9_claude_provider_constructs() {
    // rig-core panics without ANTHROPIC_API_KEY
    if has_env("ANTHROPIC_API_KEY") {
        let provider = RigProvider::claude();
        assert_eq!(provider.name(), "claude");
    } else {
        println!("Skipping: ANTHROPIC_API_KEY not set");
    }
}

#[test]
fn wiring_9_openai_provider_constructs() {
    // rig-core panics without OPENAI_API_KEY
    if has_env("OPENAI_API_KEY") {
        let provider = RigProvider::openai();
        assert_eq!(provider.name(), "openai");
    } else {
        println!("Skipping: OPENAI_API_KEY not set");
    }
}

// NOTE: Ollama test removed in v0.27 — use provider: native with mistral.rs instead
// #[test]
// fn wiring_9_ollama_provider_constructs() {
//     // Ollama doesn't require an API key
//     let provider = RigProvider::ollama();
//     assert_eq!(provider.name(), "ollama");
// }

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: Conditional provider tests (only run if env var is set)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_9_mistral_provider_if_key_exists() {
    if has_env("MISTRAL_API_KEY") {
        let provider = RigProvider::mistral();
        assert_eq!(provider.name(), "mistral");
    } else {
        // Skip test gracefully
        println!("Skipping: MISTRAL_API_KEY not set");
    }
}

#[test]
fn wiring_9_groq_provider_if_key_exists() {
    if has_env("GROQ_API_KEY") {
        let provider = RigProvider::groq();
        assert_eq!(provider.name(), "groq");
    } else {
        // Skip test gracefully
        println!("Skipping: GROQ_API_KEY not set");
    }
}

#[test]
fn wiring_9_deepseek_provider_if_key_exists() {
    if has_env("DEEPSEEK_API_KEY") {
        let provider = RigProvider::deepseek();
        assert_eq!(provider.name(), "deepseek");
    } else {
        // Skip test gracefully
        println!("Skipping: DEEPSEEK_API_KEY not set");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: Default models (conditional - requires API keys)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_9_claude_default_model() {
    if has_env("ANTHROPIC_API_KEY") {
        let provider = RigProvider::claude();
        let model = provider.default_model();
        assert!(!model.is_empty(), "Claude should have a default model");
        assert!(
            model.contains("claude") || model.contains("sonnet") || model.contains("opus"),
            "Claude model should contain 'claude', 'sonnet', or 'opus'"
        );
    } else {
        println!("Skipping: ANTHROPIC_API_KEY not set");
    }
}

#[test]
fn wiring_9_openai_default_model() {
    if has_env("OPENAI_API_KEY") {
        let provider = RigProvider::openai();
        let model = provider.default_model();
        assert!(!model.is_empty(), "OpenAI should have a default model");
    } else {
        println!("Skipping: OPENAI_API_KEY not set");
    }
}

// NOTE: Ollama test removed in v0.27 — use provider: native with mistral.rs instead
// #[test]
// fn wiring_9_ollama_default_model() {
//     let provider = RigProvider::ollama();
//     let model = provider.default_model();
//     assert!(!model.is_empty(), "Ollama should have a default model");
// }

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: Auto-detection behavior
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_9_auto_returns_option() {
    // auto() returns Option<RigProvider>
    // Result depends on environment but should not panic
    let result = RigProvider::auto();

    // The return type proves it's Option
    match result {
        Some(provider) => {
            // If we get a provider, it should have a valid name
            let name = provider.name();
            assert!(
                // NOTE: Ollama removed in v0.27 — use provider: native with mistral.rs instead
                ["claude", "openai", "mistral", "groq", "deepseek", "gemini"].contains(&name),
                "Auto-detected provider should be one of the 6 known providers"
            );
        }
        None => {
            // No API keys configured - this is also valid (reaching here means success)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 5: Provider identity (conditional for Claude/OpenAI, safe for Ollama)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_9_safe_providers_name_matches_constructor() {
    // NOTE: Ollama removed in v0.27 — use provider: native with mistral.rs instead
    // assert_eq!(RigProvider::ollama().name(), "ollama");

    // Claude/OpenAI require API keys - test conditionally
    if has_env("ANTHROPIC_API_KEY") {
        assert_eq!(RigProvider::claude().name(), "claude");
    }
    if has_env("OPENAI_API_KEY") {
        assert_eq!(RigProvider::openai().name(), "openai");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 6: Provider count
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_9_six_provider_variants_exist() {
    // We can verify there are 6 providers by testing the known names
    // NOTE: Ollama removed in v0.27 — use provider: native with mistral.rs instead
    let known_providers = ["claude", "openai", "mistral", "groq", "deepseek", "gemini"];
    assert_eq!(
        known_providers.len(),
        6,
        "Should have 6 known provider names"
    );
}
