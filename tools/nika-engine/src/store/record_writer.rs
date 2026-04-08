// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NDJSON Record Writer — persists compressed records after workflow completion.
//!
//! Writes one JSON line per record to `.nika/records/{workflow}_{timestamp}.ndjson`

use crate::error::NikaError;
use crate::runtime::record::Record;
use chrono::Utc;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Default directory for record files.
const RECORDS_DIR: &str = ".nika/records";

/// Maximum length for the sanitized workflow name in filenames.
const MAX_NAME_LEN: usize = 64;

/// A single NDJSON line written to the record file.
#[derive(Debug, Serialize)]
struct RecordLine<'a> {
    timestamp: String,
    workflow: &'a str,
    task_id: &'a str,
    summary: &'a str,
    confidence: f64,
    tokens_spent: u64,
}

/// Persists workflow records as NDJSON files.
pub struct RecordWriter;

impl RecordWriter {
    /// Write records to the default `.nika/records/` directory.
    ///
    /// Returns `Ok(None)` when `records` is empty (no file created).
    /// Returns `Ok(Some(path))` on success.
    pub fn write_records(
        workflow_name: &str,
        records: &[(String, Record)],
    ) -> Result<Option<PathBuf>, NikaError> {
        Self::write_records_to(Path::new(RECORDS_DIR), workflow_name, records)
    }

    /// Write records to a specific directory (useful for testing).
    ///
    /// Returns `Ok(None)` when `records` is empty (no file created).
    /// Returns `Ok(Some(path))` on success.
    pub fn write_records_to(
        dir: &Path,
        workflow_name: &str,
        records: &[(String, Record)],
    ) -> Result<Option<PathBuf>, NikaError> {
        if records.is_empty() {
            return Ok(None);
        }

        let sanitized = sanitize_name(workflow_name);
        let timestamp = Utc::now().format("%Y%m%dT%H%M%S");
        let filename = format!("{}_{}.ndjson", sanitized, timestamp);

        fs::create_dir_all(dir)?;

        let path = dir.join(&filename);
        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);

        let now_iso = Utc::now().to_rfc3339();

        for (task_key, record) in records {
            let line = RecordLine {
                timestamp: now_iso.clone(),
                workflow: &sanitized,
                task_id: task_key,
                summary: &record.summary,
                confidence: record.confidence,
                tokens_spent: record.tokens_original,
            };
            let json = serde_json::to_string(&line).map_err(|e| {
                NikaError::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;
            writeln!(writer, "{}", json)?;
        }

        writer.flush()?;

        tracing::debug!(path = %path.display(), count = records.len(), "Wrote NDJSON records");

        Ok(Some(path))
    }
}

/// Sanitize a workflow name for use in filenames.
///
/// Replaces non-alphanumeric characters (except `-` and `_`) with `-`,
/// collapses consecutive dashes, trims leading/trailing dashes,
/// and limits to [`MAX_NAME_LEN`] characters.
fn sanitize_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len().min(MAX_NAME_LEN));

    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            result.push(ch);
        } else {
            result.push('-');
        }
    }

    // Collapse consecutive dashes
    let collapsed: String = result.chars().fold(String::new(), |mut acc, c| {
        if c == '-' && acc.ends_with('-') {
            // skip
        } else {
            acc.push(c);
        }
        acc
    });

    // Trim leading/trailing dashes and limit length
    let trimmed = collapsed.trim_matches('-');
    let end = trimmed.len().min(MAX_NAME_LEN);
    trimmed[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::record::Record;
    use std::sync::Arc;
    use std::time::Duration;

    /// Helper: create a test Record with the given task_id and summary.
    fn make_record(task_id: &str, summary: &str) -> (String, Record) {
        (
            task_id.to_string(),
            Record {
                task_id: Arc::from(task_id),
                summary: summary.to_string(),
                key_findings: vec!["finding1".to_string()],
                raw_output: None,
                confidence: 0.9,
                tokens_original: 500,
                tokens_compressed: 50,
                compression_model: "claude-haiku-4-5".to_string(),
                compression_cost_usd: 0.0001,
                compression_duration: Duration::from_millis(200),
            },
        )
    }

    #[test]
    fn empty_records_produces_no_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = RecordWriter::write_records_to(tmp.path(), "test-flow", &[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Directory should not have been created (no files to write)
        let entries: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn single_record_writes_valid_ndjson() {
        let tmp = tempfile::TempDir::new().unwrap();
        let records = vec![make_record("research", "AI is great")];

        let result = RecordWriter::write_records_to(tmp.path(), "my-flow", &records);
        assert!(result.is_ok());
        let path = result.unwrap().expect("should return a path");
        assert!(path.exists());

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 1);

        // Parse as JSON to verify validity
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["task_id"], "research");
        assert_eq!(parsed["summary"], "AI is great");
        assert_eq!(parsed["confidence"], 0.9);
        assert_eq!(parsed["tokens_spent"], 500);
        assert_eq!(parsed["workflow"], "my-flow");
    }

    #[test]
    fn workflow_name_sanitization() {
        // Special characters become dashes, trailing dashes trimmed
        assert_eq!(sanitize_name("hello world!"), "hello-world");
        assert_eq!(sanitize_name("  spaces  "), "spaces");
        assert_eq!(sanitize_name("foo/bar\\baz"), "foo-bar-baz");
        assert_eq!(sanitize_name("a---b"), "a-b");

        // Long names get truncated
        let long_name = "a".repeat(100);
        assert_eq!(sanitize_name(&long_name).len(), MAX_NAME_LEN);

        // Underscores and hyphens are kept
        assert_eq!(sanitize_name("my_flow-v2"), "my_flow-v2");
    }

    #[test]
    fn multiple_records_write_multiple_lines() {
        let tmp = tempfile::TempDir::new().unwrap();
        let records = vec![
            make_record("step1", "First result"),
            make_record("step2", "Second result"),
            make_record("step3", "Third result"),
        ];

        let path = RecordWriter::write_records_to(tmp.path(), "multi", &records)
            .unwrap()
            .expect("should return a path");

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn each_line_is_valid_json() {
        let tmp = tempfile::TempDir::new().unwrap();
        let records = vec![
            make_record("a", "summary a"),
            make_record("b", "summary b"),
            make_record("c", "summary c"),
        ];

        let path = RecordWriter::write_records_to(tmp.path(), "json-check", &records)
            .unwrap()
            .expect("should return a path");

        let content = fs::read_to_string(&path).unwrap();
        for (i, line) in content.trim().lines().enumerate() {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
            assert!(parsed.is_ok(), "Line {} is not valid JSON: {}", i, line);
            let val = parsed.unwrap();
            assert!(val["timestamp"].is_string(), "Line {} missing timestamp", i);
            assert!(val["workflow"].is_string(), "Line {} missing workflow", i);
            assert!(val["task_id"].is_string(), "Line {} missing task_id", i);
            assert!(val["summary"].is_string(), "Line {} missing summary", i);
            assert!(val["confidence"].is_f64(), "Line {} missing confidence", i);
            assert!(
                val["tokens_spent"].is_u64(),
                "Line {} missing tokens_spent",
                i
            );
        }
    }
}
