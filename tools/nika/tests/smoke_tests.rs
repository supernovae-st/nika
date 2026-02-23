//! Smoke test harness for Nika
//!
//! Tests binary compilation, CLI commands, and basic functionality.
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test smoke_tests
//! cargo nextest run --test smoke_tests
//! ```

#[path = "smoke/cli_tests.rs"]
mod cli_tests;

#[path = "smoke/binary_tests.rs"]
mod binary_tests;
