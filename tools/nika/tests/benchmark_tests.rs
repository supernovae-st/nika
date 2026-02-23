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
