// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `GET /v1/jobs/{id}/trace/verify` — the door's verdict on the journal the
//! resident wrote for the job, through the ONE verifier `nika trace verify`
//! runs (`nika_trace::trace_verify::verify_with`, its `--json` document).
//! The door locates the file by the job's identity and PROJECTS the CLI's
//! document; it never judges a chain itself. The wire stays additive over
//! the honest refusal (`verdict` · `reason` · `trace_id`) and never carries
//! a filesystem path.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::JobRecord;

/// The tiers the CLI's ladder attains over an intact chain.
const LADDER_TIERS: [&str; 4] = ["ok", "sealed", "anchored", "replayed"];

/// The journal a job's record names: the backend's journal directory and
/// the job's execution + trace identity.
pub(super) struct JournalKey {
    dir: PathBuf,
    execution: String,
    trace: String,
}

impl JournalKey {
    /// `None` while the record names no execution (a queued job).
    pub(super) fn of(dir: &Path, record: &JobRecord) -> Option<Self> {
        Some(Self {
            dir: dir.to_path_buf(),
            execution: record.execution_id()?.to_owned(),
            trace: record.trace_id()?.to_owned(),
        })
    }

    /// Locate the journal and verify it (blocking · the fs). `None` when no
    /// journal exists for this job — the route's `unavailable`.
    pub(super) fn verify(&self) -> Option<Value> {
        let path = locate_journal(&self.dir, &self.execution, &self.trace)?;
        let opts = nika_trace::trace_verify::VerifyOptions {
            json: true,
            ..Default::default()
        };
        let path = path.to_string_lossy().into_owned();
        let out = nika_trace::trace_verify::verify_with(&path, &opts);
        let doc = serde_json::from_str(out.text.trim_end()).unwrap_or_else(|_| {
            serde_json::json!({
                "tier": "unknown",
                "exit": out.code,
                "lines": [out.text.trim_end()],
            })
        });
        Some(project(doc, &path, &self.trace))
    }
}

/// The journal file for `trace`, by the sink's own naming law:
/// `<ts>-<last 4 hex>.ndjson`, or `<ts>-<32 hex>.ndjson` on a same-second
/// collision. Two runs in different seconds can share a short id, so the
/// first line's `execution.uuid` (the stamp the sink writes on every line)
/// settles which file is the job's.
fn locate_journal(dir: &Path, execution: &str, trace: &str) -> Option<PathBuf> {
    let short = trace.get(trace.len().saturating_sub(4)..)?;
    let tails = [format!("-{short}.ndjson"), format!("-{trace}.ndjson")];
    let wanted = uuid_digits(execution);
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|name| tails.iter().any(|tail| name.ends_with(tail)))
        })
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .find(|path| first_line_execution(path).is_some_and(|found| found == wanted))
}

/// A uuid's hex digits — the record stores `exe-<hyphenated>`, the journal
/// `{"uuid": "<hyphenated>"}`; the digits are the identity.
fn uuid_digits(id: &str) -> String {
    id.strip_prefix("exe-")
        .unwrap_or(id)
        .chars()
        .filter(char::is_ascii_hexdigit)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The first line's execution stamp, read bounded (one line, at most the
/// walk's line bound — the journal is untrusted input).
fn first_line_execution(path: &Path) -> Option<String> {
    use std::io::{BufRead as _, Read as _};
    let file = std::fs::File::open(path).ok()?;
    let bound = u64::try_from(nika_dap::chain::MAX_LINE_BYTES).unwrap_or(u64::MAX);
    let mut reader = std::io::BufReader::new(file.take(bound.saturating_add(1)));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let value: Value = serde_json::from_str(line.trim_end()).ok()?;
    value
        .get("execution")?
        .get("uuid")?
        .as_str()
        .map(uuid_digits)
}

/// The wire body over the CLI's document. `verdict` is the word the CLI
/// prints at the head of its ladder: its attained tier (`ok` · `sealed` ·
/// `anchored` · `replayed`), `incomplete` when its chain headline says so,
/// `tampered` for a buried seal, otherwise its refusal class (`broken` ·
/// `unchained` · `empty` · `unreadable` · `refused` · `line-over-long` ·
/// `unknown`). `reason` is the machine class beside it: the seal tier under
/// a ladder verdict, the writer's liveness under `incomplete`, the refusal
/// class otherwise. `exit` is the CLI's exit class; every other field is the
/// CLI's own, verbatim, except the journal path — replaced by `<journal>`
/// wherever it rides (a door never exposes one).
fn project(mut doc: Value, path: &str, trace_id: &str) -> Value {
    let tier = doc
        .get("tier")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let incomplete = doc.pointer("/chain/headline").and_then(Value::as_str) == Some("incomplete");
    let liveness = doc
        .pointer("/chain/liveness")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let seal = doc
        .pointer("/seal/tier")
        .and_then(Value::as_str)
        .unwrap_or("unsealed")
        .to_owned();
    let (verdict, reason) = if LADDER_TIERS.contains(&tier.as_str()) {
        if incomplete {
            ("incomplete".to_owned(), format!("writer_{liveness}"))
        } else {
            (tier, seal)
        }
    } else if tier == "buried-seal" {
        ("tampered".to_owned(), "buried_seal".to_owned())
    } else {
        (tier.clone(), tier.replace('-', "_"))
    };
    let mut body = serde_json::Map::new();
    body.insert("verdict".to_owned(), Value::from(verdict));
    body.insert("reason".to_owned(), Value::from(reason));
    body.insert("trace_id".to_owned(), Value::from(trace_id));
    if let Some(object) = doc.as_object_mut() {
        object.remove("trace");
        if let Some(lines) = object.get_mut("lines").and_then(Value::as_array_mut) {
            for line in lines.iter_mut() {
                if let Some(text) = line.as_str() {
                    *line = Value::from(text.replace(path, "<journal>"));
                }
            }
        }
        for (key, value) in std::mem::take(object) {
            body.entry(key).or_insert(value);
        }
    }
    Value::Object(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ladder(tier: &str, headline: &str, seal: &str, liveness: Option<&str>) -> Value {
        serde_json::json!({
            "verify_version": 1,
            "trace": "/tmp/p/.nika/traces/x-abcd.ndjson",
            "tier": tier,
            "exit": 0,
            "chain": {"events": 3, "head": "h", "headline": headline, "liveness": liveness},
            "seal": {"tier": seal},
            "anchor": {"tier": "not-present"},
            "replay": {"tier": "not-asked"},
            "lines": ["UNSEALED — /tmp/p/.nika/traces/x-abcd.ndjson carries no run_sealed frame"]
        })
    }

    /// The projection speaks the CLI's words and nothing else: the tier, the
    /// incomplete headline, the buried seal, the refusal classes — and the
    /// path never crosses (the `trace` field dropped, the lines redacted).
    #[test]
    fn the_projection_is_the_cli_document_minus_the_path() {
        let path = "/tmp/p/.nika/traces/x-abcd.ndjson";
        let ok = project(ladder("ok", "intact", "unsealed", None), path, "t1");
        assert_eq!(ok["verdict"], "ok");
        assert_eq!(ok["reason"], "unsealed");
        assert_eq!(ok["trace_id"], "t1");
        assert_eq!(ok["exit"], 0);
        assert_eq!(ok["chain"]["events"], 3);
        assert!(ok.get("trace").is_none(), "{ok}");
        assert_eq!(
            ok["lines"][0],
            "UNSEALED — <journal> carries no run_sealed frame"
        );
        let sealed = project(ladder("sealed", "intact", "sealed", None), path, "t1");
        assert_eq!(
            (&sealed["verdict"], &sealed["reason"]),
            (&"sealed".into(), &"sealed".into())
        );
        let alive = project(
            ladder("ok", "incomplete", "unsealed", Some("alive")),
            path,
            "t1",
        );
        assert_eq!(alive["verdict"], "incomplete");
        assert_eq!(alive["reason"], "writer_alive");
        let buried = project(
            serde_json::json!({"tier": "buried-seal", "exit": 2, "lines": ["TAMPERED — …"]}),
            path,
            "t1",
        );
        assert_eq!(buried["verdict"], "tampered");
        assert_eq!(buried["reason"], "buried_seal");
        assert_eq!(buried["exit"], 2);
        let long = project(
            serde_json::json!({"tier": "line-over-long", "exit": 2, "lines": []}),
            path,
            "t1",
        );
        assert_eq!(long["verdict"], "line-over-long");
        assert_eq!(long["reason"], "line_over_long");
    }

    /// The identity digits: the record's `exe-` form and the journal's
    /// hyphenated uuid name the same run.
    #[test]
    fn uuid_digits_strip_the_prefix_and_the_hyphens() {
        assert_eq!(
            uuid_digits("exe-01a07812-3b0a-7ba0-b27e-a4893cac734f"),
            "01a078123b0a7ba0b27ea4893cac734f"
        );
        assert_eq!(
            uuid_digits("01A07812-3B0A-7BA0-B27E-A4893CAC734F"),
            "01a078123b0a7ba0b27ea4893cac734f"
        );
    }

    /// Two journals sharing a short id (different seconds) are told apart by
    /// the first line's execution stamp; a stranger's file is never the job's.
    #[test]
    fn locate_reads_the_first_lines_execution_to_settle_a_shared_short_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let line = |uuid: &str| {
            format!(
                "{{\"id\":{{\"uuid\":\"{uuid}\"}},\"timestamp\":1,\"kind\":\"workflow_started\",\"execution\":{{\"uuid\":\"{uuid}\"}},\"fields\":[],\"chain\":\"x\"}}\n"
            )
        };
        let mine = "01a07812-3b0a-7ba0-b27e-a4893cac734f";
        let other = "0000aaaa-0000-7000-8000-00000000734f";
        std::fs::write(
            dir.path().join("2026-01-01T00-00-00Z-734f.ndjson"),
            line(other),
        )
        .expect("other");
        std::fs::write(
            dir.path().join("2026-01-01T00-00-01Z-734f.ndjson"),
            line(mine),
        )
        .expect("mine");
        let found = locate_journal(dir.path(), &format!("exe-{mine}"), &uuid_digits(mine))
            .expect("the job's journal");
        assert!(
            found.ends_with("2026-01-01T00-00-01Z-734f.ndjson"),
            "{found:?}"
        );
        assert!(
            locate_journal(
                dir.path(),
                "exe-ffffffff-0000-7000-8000-000000000000",
                "ffffffff00007000800000000000734f"
            )
            .is_none(),
            "a run with no journal is not found in a stranger's file"
        );
    }
}
