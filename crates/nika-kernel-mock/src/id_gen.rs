// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Deterministic ID generators for testing.

use std::sync::atomic::{AtomicU64, Ordering};

use nika_error::id::{EventId, RunId, SpanId, TraceId};
use nika_kernel::id_gen::IdGenerator;
use uuid::Uuid;

/// Sequential ID generator for deterministic tests.
///
/// Generates IDs based on an incrementing counter for reproducibility.
#[derive(Debug, Default)]
pub struct SequentialIdGenerator {
    counter: AtomicU64,
}

impl SequentialIdGenerator {
    /// Create a new sequential ID generator starting at 1.
    #[must_use]
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(1),
        }
    }

    fn next(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }
}

impl IdGenerator for SequentialIdGenerator {
    fn new_run_id(&self) -> RunId {
        let n = self.next();
        let mut bytes = [0u8; 16];
        bytes[8..].copy_from_slice(&n.to_be_bytes());
        RunId::new(Uuid::from_bytes(bytes))
    }

    fn new_event_id(&self) -> EventId {
        let n = self.next();
        let mut bytes = [0u8; 16];
        bytes[8..].copy_from_slice(&n.to_be_bytes());
        EventId::new(Uuid::from_bytes(bytes))
    }

    fn new_trace_id(&self) -> TraceId {
        let n = self.next();
        let mut bytes = [0u8; 16];
        bytes[8..].copy_from_slice(&n.to_be_bytes());
        TraceId::new(bytes)
    }

    fn new_span_id(&self) -> SpanId {
        let n = self.next();
        SpanId::new(n.to_be_bytes())
    }
}

/// Alias for `SequentialIdGenerator` (common mock name).
pub type MockIdGenerator = SequentialIdGenerator;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_run_ids_are_unique() {
        let generator = SequentialIdGenerator::new();
        let a = generator.new_run_id();
        let b = generator.new_run_id();
        assert_ne!(a, b);
    }

    #[test]
    fn sequential_event_ids() {
        let generator = SequentialIdGenerator::new();
        let a = generator.new_event_id();
        let b = generator.new_event_id();
        assert_ne!(a, b);
    }

    #[test]
    fn sequential_trace_ids() {
        let generator = SequentialIdGenerator::new();
        let a = generator.new_trace_id();
        let b = generator.new_trace_id();
        assert_ne!(a, b);
    }

    #[test]
    fn sequential_span_ids() {
        let generator = SequentialIdGenerator::new();
        let a = generator.new_span_id();
        let b = generator.new_span_id();
        assert_ne!(a, b);
    }

    #[test]
    fn satisfies_id_generator() {
        fn _accepts(_: &dyn IdGenerator) {}
        _accepts(&SequentialIdGenerator::new());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn sequential_is_send_sync() {
        _assert_send_sync::<SequentialIdGenerator>();
    }
}
