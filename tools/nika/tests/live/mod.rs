//! Live integration tests with real API providers.
//!
//! These tests require actual API keys to run:
//! - ANTHROPIC_API_KEY for Claude tests
//! - OPENAI_API_KEY for OpenAI tests
//! - MISTRAL_API_KEY for Mistral tests
//! - GROQ_API_KEY for Groq tests
//! - DEEPSEEK_API_KEY for DeepSeek tests
//! - OLLAMA_API_BASE_URL for Ollama tests
//!
//! Run with: cargo test --test live_provider_tests -- --ignored
//! Or set NIKA_LIVE_TESTS=1 to run automatically

mod provider_tests;
mod verb_integration;
mod workflow_execution;
