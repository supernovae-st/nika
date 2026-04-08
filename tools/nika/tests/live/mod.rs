// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Live integration tests with real API providers.
//!
//! These tests require actual API keys to run:
//! - ANTHROPIC_API_KEY for Claude tests
//! - OPENAI_API_KEY for OpenAI tests
//! - MISTRAL_API_KEY for Mistral tests
//! - GROQ_API_KEY for Groq tests
//! - DEEPSEEK_API_KEY for DeepSeek tests
//! - GEMINI_API_KEY for Gemini tests
//!
//! NOTE: Ollama removed in v0.27 — use provider: native with mistral.rs instead
//!
//! Run with: cargo test --test live_provider_tests -- --ignored
//! Or set NIKA_LIVE_TESTS=1 to run automatically

mod provider_tests;
mod verb_integration;
mod workflow_execution;
