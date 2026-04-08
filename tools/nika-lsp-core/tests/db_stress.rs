// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Stress tests for [`WorldDatabase`].
//!
//! Run with: `cargo test --test db_stress`

use std::sync::Arc;

use nika_lsp_core::db::{FileKey, WorldDatabase, MAX_FILE_SIZE};
use nika_lsp_core::position::{LineIndex, PositionIndex, SpanEntry, SpanKind};

// ---------------------------------------------------------------------------
// 1. Open 100 files, verify all snapshots correct
// ---------------------------------------------------------------------------

#[test]
fn open_100_files_all_snapshots_correct() {
    let db = WorldDatabase::new();

    for i in 0..100 {
        let uri = format!("file:///project/file_{i}.nika.yaml");
        let text = format!("schema: '@0.12'\ntasks:\n  task_{i}:\n    infer: openai/gpt-4\n");
        db.set_text(&uri, text);
    }

    assert_eq!(db.file_count(), 100);

    for i in 0..100 {
        let uri = format!("file:///project/file_{i}.nika.yaml");
        let expected = format!("schema: '@0.12'\ntasks:\n  task_{i}:\n    infer: openai/gpt-4\n");

        let snap = db
            .snapshot(&uri)
            .unwrap_or_else(|| panic!("snapshot missing for file_{i}"));
        assert_eq!(&*snap.text, &expected, "text mismatch for file_{i}");
        assert_eq!(
            snap.line_index.line_count(),
            5,
            "line count wrong for file_{i}"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Rapid set_text on same URI (100x) -- no corruption
// ---------------------------------------------------------------------------

#[test]
fn rapid_set_text_same_uri_100x_no_corruption() {
    let db = WorldDatabase::new();
    let uri = "file:///stress/rapid.nika.yaml";

    for i in 0..100 {
        let text = format!("version: {i}\nschema: '@0.12'\n");
        db.set_text(uri, text);
    }

    // Only 1 file in the database, with the last written value.
    assert_eq!(db.file_count(), 1);

    let snap = db.snapshot(uri).unwrap();
    assert_eq!(&*snap.text, "version: 99\nschema: '@0.12'\n");
    assert_eq!(snap.line_index.line_count(), 3);
    assert_eq!(snap.revision, 100);
}

// ---------------------------------------------------------------------------
// 3. Concurrent snapshot reads while writing -- no deadlock
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_reads_while_writing_no_deadlock() {
    let db = Arc::new(WorldDatabase::new());
    let uri = "file:///stress/concurrent.nika.yaml";

    // Seed with initial content.
    db.set_text(uri, "initial\n".to_string());

    let writer_db = Arc::clone(&db);
    let writer = tokio::spawn(async move {
        for i in 0..200 {
            let text = format!("revision: {i}\nschema: '@0.12'\n");
            writer_db.set_text("file:///stress/concurrent.nika.yaml", text);
            // Yield to let readers interleave.
            tokio::task::yield_now().await;
        }
    });

    let mut readers = Vec::new();
    for _ in 0..4 {
        let reader_db = Arc::clone(&db);
        readers.push(tokio::spawn(async move {
            for _ in 0..200 {
                // Snapshot must never panic or return inconsistent data.
                if let Some(snap) = reader_db.snapshot("file:///stress/concurrent.nika.yaml") {
                    // text and line_index must be coherent: line_count matches the text.
                    let actual_newlines = snap.text.bytes().filter(|&b| b == b'\n').count() as u32;
                    assert_eq!(
                        snap.line_index.line_count(),
                        actual_newlines + 1,
                        "line_index inconsistent with text"
                    );
                }
                tokio::task::yield_now().await;
            }
        }));
    }

    // Must complete within a reasonable time (no deadlock).
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        writer.await.unwrap();
        for r in readers {
            r.await.unwrap();
        }
    });

    timeout.await.expect("deadlock: timed out after 10 seconds");
}

// ---------------------------------------------------------------------------
// 4. Open file, edit, remove, re-open -- clean state
// ---------------------------------------------------------------------------

#[test]
fn open_edit_remove_reopen_clean_state() {
    let db = WorldDatabase::new();
    let uri = "file:///lifecycle/test.nika.yaml";

    // Open
    db.set_text(uri, "schema: '@0.12'\n".into());
    assert_eq!(db.file_count(), 1);

    // Edit (set position index to simulate analysis)
    let idx = PositionIndex::from_entries(vec![SpanEntry {
        start: 0,
        end: 15,
        kind: SpanKind::Schema,
    }]);
    db.set_position_index(uri, idx);

    let snap = db.snapshot(uri).unwrap();
    assert_eq!(snap.position_index.len(), 1);

    // Remove
    db.remove(uri);
    assert_eq!(db.file_count(), 0);
    assert!(db.snapshot(uri).is_none());

    // Re-open with different content
    db.set_text(
        uri,
        "schema: '@0.13'\ntasks:\n  hello:\n    exec: echo hi\n".into(),
    );
    assert_eq!(db.file_count(), 1);

    let snap = db.snapshot(uri).unwrap();
    // Must have clean state: no stale position index from before removal.
    assert!(
        snap.position_index.is_empty(),
        "position_index must be empty after re-open, got {} entries",
        snap.position_index.len()
    );
    assert_eq!(
        &*snap.text,
        "schema: '@0.13'\ntasks:\n  hello:\n    exec: echo hi\n"
    );
    assert_eq!(snap.line_index.line_count(), 5);
}

// ---------------------------------------------------------------------------
// 5. MAX_FILE_SIZE enforcement -- 65 MB file rejected
// ---------------------------------------------------------------------------

#[test]
fn max_file_size_65mb_rejected() {
    let db = WorldDatabase::new();
    let uri = "file:///stress/huge.nika.yaml";

    // 65 MB > 64 MB limit
    let size_65mb = 65 * 1024 * 1024;
    let big_text = "x".repeat(size_65mb);
    let rev = db.set_text(uri, big_text);

    // Revision still advances (no panic), but file is NOT stored.
    assert!(rev > 0);
    assert_eq!(db.file_count(), 0);
    assert!(db.snapshot(uri).is_none());
}

#[test]
fn max_file_size_boundary_exactly_64mb_accepted() {
    let db = WorldDatabase::new();
    let uri = "file:///stress/exact64mb.nika.yaml";

    let text = "y".repeat(MAX_FILE_SIZE);
    db.set_text(uri, text);

    assert_eq!(db.file_count(), 1);
    let snap = db.snapshot(uri).unwrap();
    assert_eq!(snap.text.len(), MAX_FILE_SIZE);
}

#[test]
fn max_file_size_boundary_one_byte_over_rejected() {
    let db = WorldDatabase::new();
    let uri = "file:///stress/over64mb.nika.yaml";

    let text = "z".repeat(MAX_FILE_SIZE + 1);
    db.set_text(uri, text);

    assert_eq!(db.file_count(), 0);
    assert!(db.snapshot(uri).is_none());
}

// ---------------------------------------------------------------------------
// 6. FileKey collision test -- two URIs with similar hashes still work
// ---------------------------------------------------------------------------

#[test]
fn file_key_distinct_uris_produce_distinct_keys() {
    // Even URIs that differ by a single character must produce different FileKeys.
    let pairs = [
        ("file:///a.yaml", "file:///b.yaml"),
        ("file:///foo/bar.yaml", "file:///foo/baz.yaml"),
        ("file:///task_0.yaml", "file:///task_1.yaml"),
        ("file:///path/to/file.yaml", "file:///path/to/File.yaml"),
        ("file:///a", "file:///aa"),
    ];

    for (a, b) in &pairs {
        let ka = FileKey::from_uri(a);
        let kb = FileKey::from_uri(b);
        assert_ne!(ka, kb, "FileKey collision: {a:?} vs {b:?}");
    }
}

#[test]
fn two_similar_uris_stored_independently() {
    let db = WorldDatabase::new();

    // URIs that are very similar
    let uri_a = "file:///project/workflow_a.nika.yaml";
    let uri_b = "file:///project/workflow_b.nika.yaml";

    db.set_text(uri_a, "content_a\n".into());
    db.set_text(uri_b, "content_b\n".into());

    assert_eq!(db.file_count(), 2);

    let snap_a = db.snapshot(uri_a).unwrap();
    let snap_b = db.snapshot(uri_b).unwrap();

    assert_eq!(&*snap_a.text, "content_a\n");
    assert_eq!(&*snap_b.text, "content_b\n");

    // Removing one does not affect the other.
    db.remove(uri_a);
    assert_eq!(db.file_count(), 1);
    assert!(db.snapshot(uri_a).is_none());
    assert_eq!(&*db.snapshot(uri_b).unwrap().text, "content_b\n");
}

// ---------------------------------------------------------------------------
// 7. Position index lifecycle -- set, verify, clear on edit, rebuild
// ---------------------------------------------------------------------------

#[test]
fn position_index_lifecycle_set_verify_clear_rebuild() {
    let db = WorldDatabase::new();
    let uri = "file:///lifecycle/positions.nika.yaml";
    let text = "schema: '@0.12'\ntasks:\n  greet:\n    exec: echo hello\n";

    // Step 1: Open file
    db.set_text(uri, text.into());

    // Step 2: Build and set a position index
    let idx = PositionIndex::from_entries(vec![
        SpanEntry {
            start: 0,
            end: 15,
            kind: SpanKind::Schema,
        },
        SpanEntry {
            start: 16,
            end: 52,
            kind: SpanKind::Task("greet".into()),
        },
    ]);
    db.set_position_index(uri, idx);

    // Step 3: Verify position index is accessible
    let snap = db.snapshot(uri).unwrap();
    assert_eq!(snap.position_index.len(), 2);
    assert_eq!(
        snap.position_index.context_at(5).unwrap().kind,
        SpanKind::Schema
    );
    assert_eq!(
        snap.position_index.context_at(30).unwrap().kind,
        SpanKind::Task("greet".into())
    );

    // Step 4: Edit the file -> position index must be cleared
    let new_text = "schema: '@0.13'\ntasks:\n  greet:\n    exec: echo world\n";
    db.set_text(uri, new_text.into());

    let snap2 = db.snapshot(uri).unwrap();
    assert!(
        snap2.position_index.is_empty(),
        "position_index must be cleared after edit"
    );

    // Step 5: Rebuild position index after re-analysis
    let idx2 = PositionIndex::from_entries(vec![
        SpanEntry {
            start: 0,
            end: 15,
            kind: SpanKind::Schema,
        },
        SpanEntry {
            start: 16,
            end: 53,
            kind: SpanKind::Task("greet".into()),
        },
        SpanEntry {
            start: 34,
            end: 53,
            kind: SpanKind::Verb {
                task_id: "greet".into(),
                verb: "exec".into(),
            },
        },
    ]);
    db.set_position_index(uri, idx2);

    let snap3 = db.snapshot(uri).unwrap();
    assert_eq!(snap3.position_index.len(), 3);
    // Innermost span at offset 40 should be the verb
    let ctx = snap3.position_index.context_at(40).unwrap();
    assert_eq!(
        ctx.kind,
        SpanKind::Verb {
            task_id: "greet".into(),
            verb: "exec".into(),
        }
    );
}

// ---------------------------------------------------------------------------
// 8. Empty text -> LineIndex works correctly
// ---------------------------------------------------------------------------

#[test]
fn empty_text_line_index_correct() {
    let db = WorldDatabase::new();
    let uri = "file:///edge/empty.nika.yaml";

    db.set_text(uri, String::new());

    let snap = db.snapshot(uri).unwrap();
    assert_eq!(&*snap.text, "");
    assert_eq!(snap.line_index.line_count(), 1); // empty text = 1 line (line 0)
    assert_eq!(snap.line_index.line_col(0), (0, 0)); // offset 0 -> (0, 0)
}

#[test]
fn empty_text_standalone_line_index() {
    let idx = LineIndex::new("");
    assert_eq!(idx.line_count(), 1);
    assert_eq!(idx.line_col(0), (0, 0));
    // Out-of-bounds offset should clamp
    assert_eq!(idx.line_col(100), (0, 0));
    // Roundtrip
    assert_eq!(idx.offset(0, 0), 0);
    assert_eq!(idx.offset(99, 99), 0); // clamped to len=0
}

// ---------------------------------------------------------------------------
// 9. Unicode text (emoji, CJK) -> LineIndex counts correctly
// ---------------------------------------------------------------------------

#[test]
fn unicode_emoji_line_index() {
    // Each emoji is 4 bytes in UTF-8.
    let text = "hello \u{1F600}\nworld \u{1F680}\n";
    let db = WorldDatabase::new();
    let uri = "file:///edge/emoji.nika.yaml";

    db.set_text(uri, text.into());

    let snap = db.snapshot(uri).unwrap();
    assert_eq!(snap.line_index.line_count(), 3); // two lines + trailing empty

    // "hello " = 6 bytes, then emoji = 4 bytes, then \n = 1 byte -> line 0 has 11 bytes
    // Line 1 starts at byte 11
    assert_eq!(snap.line_index.line_col(0), (0, 0)); // 'h'
    assert_eq!(snap.line_index.line_col(6), (0, 6)); // start of emoji
    assert_eq!(snap.line_index.line_col(10), (0, 10)); // \n
    assert_eq!(snap.line_index.line_col(11), (1, 0)); // 'w'
}

#[test]
fn unicode_cjk_line_index() {
    // CJK characters are 3 bytes each in UTF-8.
    let text = "\u{4F60}\u{597D}\n\u{4E16}\u{754C}\n"; // "ni hao\nshi jie\n"
    let idx = LineIndex::new(text);

    assert_eq!(idx.line_count(), 3);
    // Line 0: 2 CJK chars = 6 bytes, then \n
    assert_eq!(idx.line_col(0), (0, 0)); // first CJK char start
    assert_eq!(idx.line_col(3), (0, 3)); // second CJK char start
    assert_eq!(idx.line_col(6), (0, 6)); // \n
    assert_eq!(idx.line_col(7), (1, 0)); // line 1 start
}

#[test]
fn unicode_mixed_content_roundtrip() {
    // Mix ASCII, accented, CJK, emoji.
    let text = "caf\u{00E9}\n\u{4F60}\u{597D}\n\u{1F600}\n";
    let idx = LineIndex::new(text);

    assert_eq!(idx.line_count(), 4);

    // Verify roundtrip for every byte offset
    for offset in 0..text.len() as u32 {
        let (line, col) = idx.line_col(offset);
        let back = idx.offset(line, col);
        assert_eq!(
            back, offset,
            "unicode roundtrip failed at offset {offset} (line={line}, col={col})"
        );
    }
}

// ---------------------------------------------------------------------------
// 10. CRLF line endings -> LineIndex handles \r\n
// ---------------------------------------------------------------------------

#[test]
fn crlf_line_endings_basic() {
    let text = "line1\r\nline2\r\nline3\r\n";
    let db = WorldDatabase::new();
    let uri = "file:///edge/crlf.nika.yaml";

    db.set_text(uri, text.into());

    let snap = db.snapshot(uri).unwrap();
    // \n triggers line breaks. \r is counted as a regular byte in the column.
    // "line1\r\n" = 7 bytes -> line 0
    // "line2\r\n" = 7 bytes -> line 1
    // "line3\r\n" = 7 bytes -> line 2
    // trailing empty line -> line 3
    assert_eq!(snap.line_index.line_count(), 4);

    assert_eq!(snap.line_index.line_col(0), (0, 0)); // 'l' of line1
    assert_eq!(snap.line_index.line_col(5), (0, 5)); // '\r'
    assert_eq!(snap.line_index.line_col(6), (0, 6)); // '\n'
    assert_eq!(snap.line_index.line_col(7), (1, 0)); // 'l' of line2
    assert_eq!(snap.line_index.line_col(14), (2, 0)); // 'l' of line3
}

#[test]
fn crlf_vs_lf_line_count() {
    // Same logical content, different line endings.
    let lf_text = "a\nb\nc\n";
    let crlf_text = "a\r\nb\r\nc\r\n";

    let lf_idx = LineIndex::new(lf_text);
    let crlf_idx = LineIndex::new(crlf_text);

    // Both should have the same number of lines.
    assert_eq!(lf_idx.line_count(), crlf_idx.line_count());
    assert_eq!(lf_idx.line_count(), 4); // 3 lines + trailing empty
}

#[test]
fn crlf_roundtrip() {
    let text = "first\r\nsecond\r\nthird\r\n";
    let idx = LineIndex::new(text);

    for offset in 0..text.len() as u32 {
        let (line, col) = idx.line_col(offset);
        let back = idx.offset(line, col);
        assert_eq!(back, offset, "CRLF roundtrip failed at offset {offset}");
    }
}

#[test]
fn crlf_mixed_with_lf() {
    // Some lines use \r\n, others use \n.
    let text = "win\r\nunix\nmixed\r\n";
    let idx = LineIndex::new(text);

    assert_eq!(idx.line_count(), 4);
    assert_eq!(idx.line_col(0), (0, 0)); // 'w'
    assert_eq!(idx.line_col(5), (1, 0)); // 'u' of "unix"
    assert_eq!(idx.line_col(10), (2, 0)); // 'm' of "mixed"
}

// ---------------------------------------------------------------------------
// Bonus: concurrent writes from multiple tasks
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writes_to_different_files() {
    let db = Arc::new(WorldDatabase::new());
    let mut handles = Vec::new();

    for batch in 0..4 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            for i in 0..25 {
                let uri = format!("file:///concurrent/batch_{batch}/file_{i}.yaml");
                let text = format!("batch: {batch}\nfile: {i}\n");
                db.set_text(&uri, text);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // All 100 files must be present.
    assert_eq!(db.file_count(), 100);

    // Verify each file's content.
    for batch in 0..4 {
        for i in 0..25 {
            let uri = format!("file:///concurrent/batch_{batch}/file_{i}.yaml");
            let snap = db
                .snapshot(&uri)
                .unwrap_or_else(|| panic!("missing batch_{batch}/file_{i}"));
            let expected = format!("batch: {batch}\nfile: {i}\n");
            assert_eq!(&*snap.text, &expected);
        }
    }
}
