// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Mock `RecordQuery` implementation for testing introspection tools.
//!
//! Stores `RecordView`s in a `DashMap` for concurrent read/write access.
//! Provides `seed()` for test setup and atomic call tracking.

use std::sync::atomic::{AtomicUsize, Ordering};

use dashmap::DashMap;
use nika_kernel::scope::{RecordQuery, RecordView};

/// In-memory `RecordQuery` mock for unit tests.
///
/// Thread-safe via `DashMap`. Call counters track how many times
/// each method was invoked (useful for asserting access patterns).
pub struct MockRecordStore {
    records: DashMap<String, RecordView>,
    has_record_count: AtomicUsize,
    get_count: AtomicUsize,
    iter_count: AtomicUsize,
}

impl MockRecordStore {
    /// Create an empty mock store.
    pub fn new() -> Self {
        Self {
            records: DashMap::new(),
            has_record_count: AtomicUsize::new(0),
            get_count: AtomicUsize::new(0),
            iter_count: AtomicUsize::new(0),
        }
    }

    /// Seed a record into the store (does NOT increment call counters).
    pub fn seed(&self, view: RecordView) {
        self.records.insert(view.task_id.clone(), view);
    }

    /// Number of `has_record()` calls.
    pub fn has_record_call_count(&self) -> usize {
        self.has_record_count.load(Ordering::Acquire)
    }

    /// Number of `get_record_view()` calls.
    pub fn get_call_count(&self) -> usize {
        self.get_count.load(Ordering::Acquire)
    }

    /// Number of `iter_record_views()` calls.
    pub fn iter_call_count(&self) -> usize {
        self.iter_count.load(Ordering::Acquire)
    }
}

impl Default for MockRecordStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordQuery for MockRecordStore {
    fn has_record(&self, task_id: &str) -> bool {
        self.has_record_count.fetch_add(1, Ordering::AcqRel);
        self.records.contains_key(task_id)
    }

    fn get_record_view(&self, task_id: &str) -> Option<RecordView> {
        self.get_count.fetch_add(1, Ordering::AcqRel);
        self.records.get(task_id).map(|r| r.value().clone())
    }

    fn iter_record_views(&self) -> Vec<RecordView> {
        self.iter_count.fetch_add(1, Ordering::AcqRel);
        self.records.iter().map(|r| r.value().clone()).collect()
    }

    fn record_count(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_view(task_id: &str, confidence: f64) -> RecordView {
        RecordView {
            task_id: task_id.into(),
            summary: format!("Summary of {task_id}"),
            key_findings: vec!["finding".into()],
            confidence,
            compression_ratio: 0.05,
        }
    }

    #[test]
    fn empty_store_has_no_records() {
        let store = MockRecordStore::new();
        assert!(!store.has_record("x"));
        assert!(store.get_record_view("x").is_none());
        assert_eq!(store.record_count(), 0);
        assert!(store.iter_record_views().is_empty());
    }

    #[test]
    fn seed_and_retrieve() {
        let store = MockRecordStore::new();
        store.seed(make_view("a", 0.9));
        store.seed(make_view("b", 0.7));

        assert!(store.has_record("a"));
        assert!(store.has_record("b"));
        assert!(!store.has_record("c"));
        assert_eq!(store.record_count(), 2);

        let view = store.get_record_view("a").expect("should exist");
        assert_eq!(view.task_id, "a");
        assert!((view.confidence - 0.9).abs() < f64::EPSILON);
        assert_eq!(view.key_findings, vec!["finding"]);
    }

    #[test]
    fn iter_returns_all() {
        let store = MockRecordStore::new();
        store.seed(make_view("x", 0.5));
        store.seed(make_view("y", 0.8));

        let views = store.iter_record_views();
        assert_eq!(views.len(), 2);
        let ids: Vec<&str> = views.iter().map(|v| v.task_id.as_str()).collect();
        assert!(ids.contains(&"x"));
        assert!(ids.contains(&"y"));
    }

    #[test]
    fn call_counters_track_invocations() {
        let store = MockRecordStore::new();
        store.seed(make_view("a", 0.9));

        assert_eq!(store.has_record_call_count(), 0);
        assert_eq!(store.get_call_count(), 0);
        assert_eq!(store.iter_call_count(), 0);

        store.has_record("a");
        store.has_record("b");
        assert_eq!(store.has_record_call_count(), 2);

        store.get_record_view("a");
        assert_eq!(store.get_call_count(), 1);

        store.iter_record_views();
        store.iter_record_views();
        assert_eq!(store.iter_call_count(), 2);
    }

    #[test]
    fn seed_does_not_increment_counters() {
        let store = MockRecordStore::new();
        store.seed(make_view("a", 0.9));
        store.seed(make_view("b", 0.7));

        assert_eq!(store.has_record_call_count(), 0);
        assert_eq!(store.get_call_count(), 0);
        assert_eq!(store.iter_call_count(), 0);
    }

    #[test]
    fn overwrite_seed() {
        let store = MockRecordStore::new();
        store.seed(make_view("a", 0.5));
        store.seed(RecordView {
            task_id: "a".into(),
            summary: "updated".into(),
            key_findings: vec![],
            confidence: 0.99,
            compression_ratio: 0.01,
        });

        assert_eq!(store.record_count(), 1);
        let view = store.get_record_view("a").unwrap();
        assert_eq!(view.summary, "updated");
        assert!((view.confidence - 0.99).abs() < f64::EPSILON);
    }

    #[test]
    fn send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MockRecordStore>();
    }
}
