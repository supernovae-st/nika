// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Generation-based document cache for the LSP.
//!
//! [`WorldDatabase`] is the central store for all open files. It is
//! thread-safe (lock-free reads via [`DashMap`]) and designed for O(1)
//! snapshot cloning.

use std::hash::BuildHasherDefault;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use rustc_hash::FxHasher;

use crate::position::{LineIndex, PositionIndex};

/// Maximum file size for LSP processing (64 MB).
/// Files larger than this are rejected to prevent u32 overflow
/// in byte-offset arithmetic (LineIndex, PositionIndex, TextRange).
pub const MAX_FILE_SIZE: usize = 64 * 1024 * 1024;

type FxDashMap<K, V> = DashMap<K, V, BuildHasherDefault<FxHasher>>;

// ---------------------------------------------------------------------------
// FileKey
// ---------------------------------------------------------------------------

/// Opaque file identifier (hash-based, no lifetime issues).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileKey(u64);

impl FileKey {
    /// Derive a key from a URI string.
    pub fn from_uri(uri: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = FxHasher::default();
        uri.hash(&mut hasher);
        Self(hasher.finish())
    }
}

// ---------------------------------------------------------------------------
// Versioned
// ---------------------------------------------------------------------------

/// Version-stamped value.
#[derive(Debug, Clone)]
pub struct Versioned<T> {
    /// The wrapped value.
    pub value: Arc<T>,
    /// The revision at which this value was produced.
    pub revision: u64,
}

// ---------------------------------------------------------------------------
// FileSnapshot
// ---------------------------------------------------------------------------

/// Snapshot of a single file's state. `Arc`-wrapped, O(1) to clone.
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    /// The full text of the file.
    pub text: Arc<str>,
    /// Byte-offset to line/col mapping.
    pub line_index: Arc<LineIndex>,
    /// Byte-offset to AST-context mapping.
    pub position_index: Arc<PositionIndex>,
    /// Global revision at which this snapshot was taken.
    pub revision: u64,
}

// ---------------------------------------------------------------------------
// WorldDatabase
// ---------------------------------------------------------------------------

/// The central database for all open files.
///
/// Thread-safe via [`DashMap`] (sharded concurrent map) and atomics.
/// All values are `Arc`-wrapped for cheap snapshots.
pub struct WorldDatabase {
    revision: AtomicU64,
    texts: FxDashMap<FileKey, (Arc<str>, u64)>,
    line_indices: FxDashMap<FileKey, Versioned<LineIndex>>,
    position_indices: FxDashMap<FileKey, Versioned<PositionIndex>>,
    uri_to_key: FxDashMap<String, FileKey>,
}

impl std::fmt::Debug for WorldDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldDatabase")
            .field("revision", &self.revision.load(Ordering::SeqCst))
            .field("file_count", &self.texts.len())
            .finish()
    }
}

impl WorldDatabase {
    /// Create a new, empty database.
    pub fn new() -> Self {
        Self {
            revision: AtomicU64::new(0),
            texts: DashMap::with_hasher(BuildHasherDefault::default()),
            line_indices: DashMap::with_hasher(BuildHasherDefault::default()),
            position_indices: DashMap::with_hasher(BuildHasherDefault::default()),
            uri_to_key: DashMap::with_hasher(BuildHasherDefault::default()),
        }
    }

    /// Insert or update a file. Returns the new revision.
    ///
    /// Files larger than [`MAX_FILE_SIZE`] are silently skipped to
    /// prevent u32 overflow in byte-offset arithmetic.
    pub fn set_text(&self, uri: &str, text: String) -> u64 {
        let rev = self.revision.fetch_add(1, Ordering::SeqCst) + 1;

        if text.len() > MAX_FILE_SIZE {
            tracing::warn!(uri, len = text.len(), "file too large, skipping");
            return rev;
        }

        let key = FileKey::from_uri(uri);
        let text: Arc<str> = text.into();

        // Build line index immediately (cheap, O(n) scan).
        let line_index = Arc::new(LineIndex::new(&text));

        self.uri_to_key.insert(uri.to_string(), key);
        self.texts.insert(key, (Arc::clone(&text), rev));
        self.line_indices.insert(
            key,
            Versioned {
                value: line_index,
                revision: rev,
            },
        );

        // Clear stale position index -- caller must rebuild after re-analysis.
        self.position_indices.remove(&key);

        rev
    }

    /// Take a consistent snapshot. O(1) `Arc` clones.
    pub fn snapshot(&self, uri: &str) -> Option<FileSnapshot> {
        let key = *self.uri_to_key.get(uri)?;
        let (text, rev) = {
            let entry = self.texts.get(&key)?;
            (Arc::clone(&entry.0), entry.1)
        };
        let line_index = self
            .line_indices
            .get(&key)
            .map(|v| Arc::clone(&v.value))
            .unwrap_or_else(|| Arc::new(LineIndex::new(&text)));
        let position_index = self
            .position_indices
            .get(&key)
            .map(|v| Arc::clone(&v.value))
            .unwrap_or_else(|| Arc::new(PositionIndex::empty()));

        Some(FileSnapshot {
            text,
            line_index,
            position_index,
            revision: rev,
        })
    }

    /// Remove a file from the database.
    pub fn remove(&self, uri: &str) {
        if let Some((_, key)) = self.uri_to_key.remove(uri) {
            self.texts.remove(&key);
            self.line_indices.remove(&key);
            self.position_indices.remove(&key);
        }
    }

    /// Update the position index for a file.
    pub fn set_position_index(&self, uri: &str, index: PositionIndex) {
        if let Some(key) = self.uri_to_key.get(uri) {
            let rev = self.revision.load(Ordering::SeqCst);
            self.position_indices.insert(
                *key,
                Versioned {
                    value: Arc::new(index),
                    revision: rev,
                },
            );
        }
    }

    /// Current global revision.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    /// Number of tracked files.
    pub fn file_count(&self) -> usize {
        self.texts.len()
    }
}

impl Default for WorldDatabase {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::{SpanEntry, SpanKind};
    use pretty_assertions::assert_eq;

    // -----------------------------------------------------------------------
    // FileKey tests
    // -----------------------------------------------------------------------

    #[test]
    fn file_key_deterministic() {
        let k1 = FileKey::from_uri("file:///foo/bar.nika.yaml");
        let k2 = FileKey::from_uri("file:///foo/bar.nika.yaml");
        assert_eq!(k1, k2);
    }

    #[test]
    fn file_key_different_uris_differ() {
        let k1 = FileKey::from_uri("file:///foo.yaml");
        let k2 = FileKey::from_uri("file:///bar.yaml");
        assert_ne!(k1, k2);
    }

    #[test]
    fn file_key_empty_uri() {
        let k = FileKey::from_uri("");
        // Should not panic; just produce a valid key.
        let _ = k;
    }

    // -----------------------------------------------------------------------
    // WorldDatabase basic tests
    // -----------------------------------------------------------------------

    #[test]
    fn new_db_is_empty() {
        let db = WorldDatabase::new();
        assert_eq!(db.file_count(), 0);
        assert_eq!(db.revision(), 0);
    }

    #[test]
    fn default_db_is_empty() {
        let db = WorldDatabase::default();
        assert_eq!(db.file_count(), 0);
    }

    #[test]
    fn set_text_increments_revision() {
        let db = WorldDatabase::new();
        let r1 = db.set_text("file:///a.yaml", "hello".into());
        assert_eq!(r1, 1);
        let r2 = db.set_text("file:///b.yaml", "world".into());
        assert_eq!(r2, 2);
        assert_eq!(db.revision(), 2);
    }

    #[test]
    fn set_text_updates_file_count() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "a".into());
        assert_eq!(db.file_count(), 1);
        db.set_text("file:///b.yaml", "b".into());
        assert_eq!(db.file_count(), 2);
    }

    #[test]
    fn set_text_same_uri_overwrites() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "v1".into());
        db.set_text("file:///a.yaml", "v2".into());
        assert_eq!(db.file_count(), 1);

        let snap = db.snapshot("file:///a.yaml").unwrap();
        assert_eq!(&*snap.text, "v2");
    }

    // -----------------------------------------------------------------------
    // Snapshot tests
    // -----------------------------------------------------------------------

    #[test]
    fn snapshot_returns_none_for_unknown_uri() {
        let db = WorldDatabase::new();
        assert!(db.snapshot("file:///unknown.yaml").is_none());
    }

    #[test]
    fn snapshot_returns_text() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "hello\nworld".into());
        let snap = db.snapshot("file:///a.yaml").unwrap();
        assert_eq!(&*snap.text, "hello\nworld");
    }

    #[test]
    fn snapshot_has_correct_revision() {
        let db = WorldDatabase::new();
        let r = db.set_text("file:///a.yaml", "text".into());
        let snap = db.snapshot("file:///a.yaml").unwrap();
        assert_eq!(snap.revision, r);
    }

    #[test]
    fn snapshot_line_index_is_correct() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "line1\nline2\nline3".into());
        let snap = db.snapshot("file:///a.yaml").unwrap();
        assert_eq!(snap.line_index.line_count(), 3);
        assert_eq!(snap.line_index.line_col(6), (1, 0)); // 'l' of "line2"
    }

    #[test]
    fn snapshot_position_index_empty_by_default() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "text".into());
        let snap = db.snapshot("file:///a.yaml").unwrap();
        assert!(snap.position_index.is_empty());
    }

    #[test]
    fn snapshot_is_cheap_clone() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "x".repeat(10_000));
        let snap1 = db.snapshot("file:///a.yaml").unwrap();
        let snap2 = snap1.clone();
        // Both point to the same Arc -- same pointer.
        assert!(Arc::ptr_eq(&snap1.text, &snap2.text));
        assert!(Arc::ptr_eq(&snap1.line_index, &snap2.line_index));
    }

    // -----------------------------------------------------------------------
    // Remove tests
    // -----------------------------------------------------------------------

    #[test]
    fn remove_existing_file() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "text".into());
        assert_eq!(db.file_count(), 1);
        db.remove("file:///a.yaml");
        assert_eq!(db.file_count(), 0);
        assert!(db.snapshot("file:///a.yaml").is_none());
    }

    #[test]
    fn remove_nonexistent_file_is_noop() {
        let db = WorldDatabase::new();
        db.remove("file:///nope.yaml"); // should not panic
        assert_eq!(db.file_count(), 0);
    }

    #[test]
    fn remove_only_affects_target() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "a".into());
        db.set_text("file:///b.yaml", "b".into());
        db.remove("file:///a.yaml");
        assert_eq!(db.file_count(), 1);
        assert!(db.snapshot("file:///a.yaml").is_none());
        assert!(db.snapshot("file:///b.yaml").is_some());
    }

    // -----------------------------------------------------------------------
    // Position index tests
    // -----------------------------------------------------------------------

    #[test]
    fn set_position_index() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "schema: '@0.12'".into());

        let idx = PositionIndex::from_entries(vec![SpanEntry {
            start: 0,
            end: 15,
            kind: SpanKind::Schema,
        }]);
        db.set_position_index("file:///a.yaml", idx);

        let snap = db.snapshot("file:///a.yaml").unwrap();
        assert_eq!(snap.position_index.len(), 1);
        let ctx = snap.position_index.context_at(5).unwrap();
        assert_eq!(ctx.kind, SpanKind::Schema);
    }

    #[test]
    fn set_position_index_for_unknown_uri_is_noop() {
        let db = WorldDatabase::new();
        let idx = PositionIndex::from_entries(vec![SpanEntry {
            start: 0,
            end: 10,
            kind: SpanKind::Schema,
        }]);
        db.set_position_index("file:///nope.yaml", idx);
        // Should not panic and should not create a file entry.
        assert_eq!(db.file_count(), 0);
    }

    #[test]
    fn position_index_update_replaces_old() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "text".into());

        let idx1 = PositionIndex::from_entries(vec![SpanEntry {
            start: 0,
            end: 4,
            kind: SpanKind::Schema,
        }]);
        db.set_position_index("file:///a.yaml", idx1);
        assert_eq!(
            db.snapshot("file:///a.yaml").unwrap().position_index.len(),
            1
        );

        let idx2 = PositionIndex::from_entries(vec![
            SpanEntry {
                start: 0,
                end: 2,
                kind: SpanKind::Schema,
            },
            SpanEntry {
                start: 2,
                end: 4,
                kind: SpanKind::WorkflowName,
            },
        ]);
        db.set_position_index("file:///a.yaml", idx2);
        assert_eq!(
            db.snapshot("file:///a.yaml").unwrap().position_index.len(),
            2
        );
    }

    // -----------------------------------------------------------------------
    // Multi-file tests
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_files_independent() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "aaa\nbbb".into());
        db.set_text("file:///b.yaml", "ccc\nddd\neee".into());

        let snap_a = db.snapshot("file:///a.yaml").unwrap();
        let snap_b = db.snapshot("file:///b.yaml").unwrap();

        assert_eq!(&*snap_a.text, "aaa\nbbb");
        assert_eq!(snap_a.line_index.line_count(), 2);

        assert_eq!(&*snap_b.text, "ccc\nddd\neee");
        assert_eq!(snap_b.line_index.line_count(), 3);
    }

    #[test]
    fn revision_monotonically_increases() {
        let db = WorldDatabase::new();
        let mut prev = 0;
        for i in 0..100 {
            let r = db.set_text(&format!("file:///{i}.yaml"), format!("content {i}"));
            assert!(r > prev, "revision did not increase at i={i}");
            prev = r;
        }
    }

    // -----------------------------------------------------------------------
    // Workflow-like scenario
    // -----------------------------------------------------------------------

    #[test]
    fn workflow_lifecycle() {
        let db = WorldDatabase::new();
        let uri = "file:///project/workflow.nika.yaml";

        // 1. Open: editor sends didOpen
        let yaml = "\
schema: '@0.12'
tasks:
  summarize:
    infer: openai/gpt-4
";
        db.set_text(uri, yaml.into());
        assert_eq!(db.file_count(), 1);

        // 2. Snapshot for diagnostics
        let snap = db.snapshot(uri).unwrap();
        assert_eq!(snap.line_index.line_count(), 5);

        // 3. Background analysis produces position index
        let idx = PositionIndex::from_entries(vec![
            SpanEntry {
                start: 0,
                end: 15,
                kind: SpanKind::Schema,
            },
            SpanEntry {
                start: 16,
                end: yaml.len() as u32,
                kind: SpanKind::Task("summarize".into()),
            },
        ]);
        db.set_position_index(uri, idx);

        // 4. New snapshot includes position index
        let snap2 = db.snapshot(uri).unwrap();
        assert_eq!(snap2.position_index.len(), 2);

        // 5. Edit: user types
        let yaml_v2 = yaml.replace("gpt-4", "gpt-4o");
        db.set_text(uri, yaml_v2);

        // 6. Position index cleared -- stale index removed by set_text
        let snap3 = db.snapshot(uri).unwrap();
        assert!(snap3.revision > snap2.revision);
        assert!(
            snap3.position_index.is_empty(),
            "stale position_index must be cleared after set_text"
        );

        // 7. Close: editor sends didClose
        db.remove(uri);
        assert_eq!(db.file_count(), 0);
        assert!(db.snapshot(uri).is_none());
    }

    #[test]
    fn set_text_clears_stale_position_index() {
        let db = WorldDatabase::new();
        let uri = "file:///a.yaml";
        db.set_text(uri, "schema: '@0.12'".into());

        // Simulate background analysis producing a position index.
        let idx = PositionIndex::from_entries(vec![SpanEntry {
            start: 0,
            end: 15,
            kind: SpanKind::Schema,
        }]);
        db.set_position_index(uri, idx);
        assert_eq!(db.snapshot(uri).unwrap().position_index.len(), 1);

        // Edit the file -- position index must be cleared.
        db.set_text(uri, "schema: '@0.13'".into());
        let snap = db.snapshot(uri).unwrap();
        assert!(
            snap.position_index.is_empty(),
            "position_index must be empty after set_text (was {})",
            snap.position_index.len()
        );
    }

    #[test]
    fn set_text_large_file_rejected() {
        let db = WorldDatabase::new();
        let uri = "file:///huge.yaml";
        let big = "x".repeat(MAX_FILE_SIZE + 1);
        let rev = db.set_text(uri, big);
        // Revision is still incremented, but no file is stored.
        assert!(rev > 0);
        assert_eq!(db.file_count(), 0);
        assert!(db.snapshot(uri).is_none());
    }

    #[test]
    fn set_text_exactly_max_size_accepted() {
        let db = WorldDatabase::new();
        let uri = "file:///max.yaml";
        let text = "y".repeat(MAX_FILE_SIZE);
        db.set_text(uri, text);
        assert_eq!(db.file_count(), 1);
        let snap = db.snapshot(uri).unwrap();
        assert_eq!(snap.text.len(), MAX_FILE_SIZE);
    }

    #[test]
    fn debug_impl_does_not_panic() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "text".into());
        let debug = format!("{db:?}");
        assert!(debug.contains("WorldDatabase"));
        assert!(debug.contains("file_count"));
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn empty_text() {
        let db = WorldDatabase::new();
        db.set_text("file:///empty.yaml", String::new());
        let snap = db.snapshot("file:///empty.yaml").unwrap();
        assert_eq!(&*snap.text, "");
        assert_eq!(snap.line_index.line_count(), 1);
    }

    #[test]
    fn very_long_uri() {
        let db = WorldDatabase::new();
        let uri = format!("file:///{}", "a".repeat(10_000));
        db.set_text(&uri, "text".into());
        let snap = db.snapshot(&uri).unwrap();
        assert_eq!(&*snap.text, "text");
    }

    #[test]
    fn unicode_uri() {
        let db = WorldDatabase::new();
        let uri = "file:///path/to/\u{00e9}t\u{00e9}.nika.yaml";
        db.set_text(uri, "content".into());
        let snap = db.snapshot(uri).unwrap();
        assert_eq!(&*snap.text, "content");
    }

    #[test]
    fn set_text_then_remove_then_set_again() {
        let db = WorldDatabase::new();
        db.set_text("file:///a.yaml", "v1".into());
        db.remove("file:///a.yaml");
        db.set_text("file:///a.yaml", "v2".into());

        let snap = db.snapshot("file:///a.yaml").unwrap();
        assert_eq!(&*snap.text, "v2");
        assert_eq!(db.file_count(), 1);
    }
}
