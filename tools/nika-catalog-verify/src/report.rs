// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verification report — accumulates per-entry probe results and formats
//! them as either human-readable text (for terminal) or JSON (for CI).

use serde::Serialize;

use crate::probe::ProbeError;

/// A single probe outcome tagged with the catalog entry it came from.
#[derive(Debug, Serialize)]
pub(crate) struct Finding {
    /// Catalog entry id (e.g. `"filesystem"`, `"anthropic"`).
    pub entry: String,
    /// Kind of thing probed: `"npm"`, `"pypi"`, `"oci"`, `"remote"`.
    pub kind: &'static str,
    /// Identifier the probe sent (package name, URL, image ref).
    pub target: String,
    /// `Ok`: upstream present. `Err`: one of the [`ProbeError`] variants.
    pub result: FindingResult,
}

/// Serializable flattening of [`ProbeError`] — avoids exposing reqwest
/// types in the report schema.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum FindingResult {
    /// Entry resolved upstream.
    Ok,
    /// 404 from authoritative source — entry is gone.
    NotFound { url: String },
    /// Non-404 non-OK status.
    BadStatus { url: String, status: u16 },
    /// Transport-level failure.
    Network { url: String, detail: String },
}

impl From<Result<(), ProbeError>> for FindingResult {
    fn from(value: Result<(), ProbeError>) -> Self {
        match value {
            Ok(()) => Self::Ok,
            Err(ProbeError::NotFound(url)) => Self::NotFound { url },
            Err(ProbeError::BadStatus { url, status }) => Self::BadStatus {
                url,
                status: status.as_u16(),
            },
            Err(ProbeError::Network { url, source }) => Self::Network {
                url,
                detail: source.to_string(),
            },
        }
    }
}

/// Aggregate of every probe in a single verify run.
#[derive(Debug, Default, Serialize)]
pub(crate) struct Report {
    /// Total entries probed (OK + failed).
    pub total: usize,
    /// Entries that probed successfully.
    pub ok: usize,
    /// Entries that probed as `NotFound`.
    pub not_found: usize,
    /// Entries that failed for any other reason (bad status, network).
    pub errored: usize,
    /// Individual findings. Stable order: `NotFound` first, then errored,
    /// then OK — failures go to the top of the report for quick triage.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Build a report from the raw probe results, sorted failures-first.
    #[must_use]
    pub(crate) fn build(mut findings: Vec<Finding>) -> Self {
        findings.sort_by_key(|f| match f.result {
            FindingResult::NotFound { .. } => 0,
            FindingResult::Network { .. } | FindingResult::BadStatus { .. } => 1,
            FindingResult::Ok => 2,
        });
        let total = findings.len();
        let mut not_found = 0;
        let mut errored = 0;
        let mut ok = 0;
        for f in &findings {
            match f.result {
                FindingResult::Ok => ok += 1,
                FindingResult::NotFound { .. } => not_found += 1,
                FindingResult::Network { .. } | FindingResult::BadStatus { .. } => errored += 1,
            }
        }
        Self { total, ok, not_found, errored, findings }
    }

    /// Human-readable summary — one line per failure, totals at the end.
    pub(crate) fn print_human(&self) {
        for f in &self.findings {
            match &f.result {
                FindingResult::Ok => {}
                FindingResult::NotFound { url } => {
                    println!("NOT FOUND  {:<10} {:<30} {url}", f.kind, f.entry);
                }
                FindingResult::BadStatus { url, status } => {
                    println!("STATUS {status:<3}  {:<10} {:<30} {url}", f.kind, f.entry);
                }
                FindingResult::Network { url, detail } => {
                    println!("NETWORK    {:<10} {:<30} {url} ({detail})", f.kind, f.entry);
                }
            }
        }
        println!(
            "\nsummary: total={} ok={} not_found={} errored={}",
            self.total, self.ok, self.not_found, self.errored
        );
    }

    /// Exit with a non-zero code if any entry was `NotFound`. Network and
    /// bad-status errors do NOT fail the run by default — registry outages
    /// are not the catalog's problem.
    #[must_use]
    pub(crate) fn should_fail(&self) -> bool {
        self.not_found > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_findings() -> Vec<Finding> {
        vec![
            Finding {
                entry: "a".into(),
                kind: "npm",
                target: "a".into(),
                result: FindingResult::Ok,
            },
            Finding {
                entry: "b".into(),
                kind: "npm",
                target: "b".into(),
                result: FindingResult::NotFound {
                    url: "https://x".into(),
                },
            },
            Finding {
                entry: "c".into(),
                kind: "npm",
                target: "c".into(),
                result: FindingResult::BadStatus {
                    url: "https://y".into(),
                    status: 500,
                },
            },
        ]
    }

    #[test]
    fn build_sorts_failures_first() {
        let r = Report::build(sample_findings());
        assert_eq!(r.findings[0].entry, "b"); // NotFound first
        assert_eq!(r.findings[1].entry, "c"); // errored second
        assert_eq!(r.findings[2].entry, "a"); // OK last
    }

    #[test]
    fn build_counts_correctly() {
        let r = Report::build(sample_findings());
        assert_eq!(r.total, 3);
        assert_eq!(r.ok, 1);
        assert_eq!(r.not_found, 1);
        assert_eq!(r.errored, 1);
    }

    #[test]
    fn should_fail_on_not_found_only() {
        let r = Report::build(sample_findings());
        assert!(r.should_fail());

        let r = Report::build(vec![Finding {
            entry: "only-net-err".into(),
            kind: "npm",
            target: "x".into(),
            result: FindingResult::Network {
                url: "https://x".into(),
                detail: "timeout".into(),
            },
        }]);
        assert!(!r.should_fail());
    }

    #[test]
    fn report_serialises_to_json() {
        let r = Report::build(sample_findings());
        let json = serde_json::to_string(&r).expect("report is always serde-serialisable");
        assert!(json.contains("\"total\":3"));
        assert!(json.contains("not-found"));
    }
}
