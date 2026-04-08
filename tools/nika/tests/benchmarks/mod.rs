// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Performance benchmarks for Nika.
//!
//! These benchmarks measure critical path performance:
//! - Workflow parsing
//! - DAG validation
//! - Binding resolution
//! - Task execution overhead
//!
//! Run with: cargo bench
//! Or: cargo bench --bench <name>
//!
//! See also: benches/ directory for Criterion benchmarks

mod micro_benchmarks;
