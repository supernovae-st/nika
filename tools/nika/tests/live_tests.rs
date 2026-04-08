// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Live API test harness for Nika
//!
//! Tests real API calls to providers (Claude, OpenAI, Mistral, etc.).
//! Requires API keys to be set in environment.
//!
//! ## Running Tests
//!
//! ```bash
//! # Set API keys first
//! export ANTHROPIC_API_KEY=sk-ant-...
//! export OPENAI_API_KEY=sk-...
//!
//! # Run live tests (ignored by default)
//! cargo test --test live_tests -- --ignored
//! ```

#[path = "live/provider_tests.rs"]
mod provider_tests;

#[path = "live/verb_integration.rs"]
mod verb_integration;

#[path = "live/workflow_execution.rs"]
mod workflow_execution;
