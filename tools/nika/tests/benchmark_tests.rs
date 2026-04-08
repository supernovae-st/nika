// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Benchmark test harness for Nika
//!
//! Performance benchmarks for parsing, DAG construction, and serialization.
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test benchmark_tests --release -- --nocapture
//! ```

#[path = "benchmarks/micro_benchmarks.rs"]
mod micro_benchmarks;
