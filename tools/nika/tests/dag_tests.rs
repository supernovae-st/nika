// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG test harness for Nika
//!
//! Tests complex dependency patterns, cycle detection, and execution ordering.
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test dag_tests
//! cargo nextest run --test dag_tests
//! ```

#[path = "dag/complex_deps.rs"]
mod complex_deps;

#[path = "dag/cycle_detection.rs"]
mod cycle_detection;

#[path = "dag/execution_order.rs"]
mod execution_order;
