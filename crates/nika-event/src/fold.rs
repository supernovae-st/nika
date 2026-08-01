// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Journal folding — a recorded run's facts from its head + tail lines.
//!
//! The flight recorder writes one NDJSON event per line; this module
//! reads a journal's FIRST and LAST lines only (never a full parse) and
//! folds them into [`RunFact`] — the shape `nika welcome --deep` aggregates
//! and future consumers (MCP · cockpit) reuse. It lives HERE because
//! the fold is a projection of this crate's event format: the
//! `workflow_*` terminal kinds and the `fields` key/value envelope.
//!
//! Honesty invariants:
//! - A truncated tail (a crashed run) folds to `verdict: None` —
//!   honestly unknown, never guessed.
//! - Spend fields surface only when the closing event carried them
//!   (`cost_usd` absent ≠ `$0`).

use std::path::{Path, PathBuf};

/// One recorded run's facts, folded from a journal's first + last lines.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunFact {
    /// Root-relative journal path.
    pub trace: String,
    /// The workflow id the journal opened with (`None` = unreadable head).
    pub workflow: Option<String>,
    /// The closing event kind (`workflow_completed` · `workflow_failed` ·
    /// `workflow_paused` · `None` = truncated tail, run may have crashed).
    pub verdict: Option<String>,
    /// Recorded spend when the closing event carried it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Calls with no catalog price (local models — unpriced, never free).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unpriced_calls: Option<u64>,
}

/// Fold the newest journals in `dir` (first + last line each — never a
/// full read), newest first, at most `max`. Returns the facts, whether
/// the cap cut the inventory, and how many journals were FOUND (a cap
/// without a total reads as « all »).
#[must_use]
pub fn fold_traces(dir: &Path, max: usize) -> (Vec<RunFact>, bool, usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (Vec::new(), false, 0);
    };
    let mut journals: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            path.extension().is_some_and(|x| x == "ndjson").then(|| {
                let mtime = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                (mtime, path)
            })
        })
        .collect();
    journals.sort_by(|a, b| b.0.cmp(&a.0));
    let found = journals.len();
    let capped = found > max;
    journals.truncate(max);
    let runs = journals
        .into_iter()
        .map(|(_, path)| fold_one(&path))
        .collect();
    (runs, capped, found)
}

/// One journal → facts from its head + tail lines (a truncated tail —
/// a crashed run — folds to `verdict: None`, honestly unknown).
#[must_use]
pub fn fold_one(path: &Path) -> RunFact {
    let trace = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| format!(".nika/traces/{}", n.to_string_lossy()),
    );
    let Ok(body) = std::fs::read_to_string(path) else {
        return RunFact {
            trace,
            workflow: None,
            verdict: None,
            cost_usd: None,
            unpriced_calls: None,
        };
    };
    let mut lines = body.lines().filter(|l| !l.trim().is_empty());
    let head: Option<serde_json::Value> = lines.next().and_then(|l| serde_json::from_str(l).ok());
    let tail: Option<serde_json::Value> = body
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| serde_json::from_str(l).ok());
    let field = |v: &serde_json::Value, key: &str| -> Option<serde_json::Value> {
        v.get("fields")?
            .as_array()?
            .iter()
            .find_map(|f| (f.get("key")?.as_str()? == key).then(|| f.get("value").cloned())?)
    };
    let workflow = head
        .as_ref()
        .and_then(|h| field(h, "workflow"))
        .and_then(|v| v.as_str().map(str::to_owned));
    let verdict = tail
        .as_ref()
        .and_then(|t| t.get("kind"))
        .and_then(|k| k.as_str())
        // TERMINAL kinds only — a journal truncated after its opening
        // line must fold to `None` (honestly unknown), never surface
        // `workflow_started` as if a crashed run had reached a state
        // (the V-arc e2e caught exactly that).
        .filter(|k| {
            matches!(
                *k,
                "workflow_completed" | "workflow_failed" | "workflow_paused"
            )
        })
        .map(str::to_owned);
    let cost_usd = tail
        .as_ref()
        .and_then(|t| field(t, "total_cost_usd"))
        .and_then(|v| v.as_f64());
    let unpriced_calls = tail
        .as_ref()
        .and_then(|t| field(t, "unpriced_calls"))
        .and_then(|v| v.as_u64());
    RunFact {
        trace,
        workflow,
        verdict,
        cost_usd,
        unpriced_calls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_journal_folds_to_unknown_verdict() {
        let dir = std::env::temp_dir().join(format!(
            "nika-fold-test-{}-{}",
            std::process::id(),
            std::thread::current().name().map_or(0, str::len)
        ));
        std::fs::create_dir_all(&dir).unwrap_or(());
        let path = dir.join("truncated.ndjson");
        // A journal cut after its OPENING event — the exact V-arc repro:
        // the head kind starts with `workflow_` but is NOT terminal.
        std::fs::write(
            &path,
            "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"demo\"}]}\n",
        )
        .expect("fixture write");
        let fact = fold_one(&path);
        assert_eq!(fact.workflow.as_deref(), Some("demo"));
        assert_eq!(
            fact.verdict, None,
            "opening event must never read as a verdict"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn completed_tail_folds_to_terminal_verdict() {
        let dir = std::env::temp_dir().join(format!(
            "nika-fold-ok-{}-{}",
            std::process::id(),
            std::thread::current().name().map_or(0, str::len)
        ));
        std::fs::create_dir_all(&dir).unwrap_or(());
        let path = dir.join("done.ndjson");
        std::fs::write(
            &path,
            concat!(
                "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"demo\"}]}\n",
                "{\"kind\":\"workflow_completed\",\"fields\":[{\"key\":\"total_cost_usd\",\"value\":0.5}]}\n",
            ),
        )
        .expect("fixture write");
        let fact = fold_one(&path);
        assert_eq!(fact.verdict.as_deref(), Some("workflow_completed"));
        assert_eq!(fact.cost_usd, Some(0.5));
        let _ = std::fs::remove_file(&path);
    }
}
