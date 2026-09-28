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
        let path = nika_dap::store::locate_trace(&self.dir, &self.execution, &self.trace)?;
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
}
