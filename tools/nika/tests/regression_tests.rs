//! Regression test harness for Nika
//!
//! Tests output stability using snapshots and output capture.
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test regression_tests
//! cargo insta test --review  # Review new snapshots
//! ```

#[path = "regression/workflow_snapshots.rs"]
mod workflow_snapshots;

#[path = "regression/output_capture.rs"]
mod output_capture;
