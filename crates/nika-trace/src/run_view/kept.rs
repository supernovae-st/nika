// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One observed run as a host keeps it between sessions: what the
//! observation named (the workflow, its exit, the trace, the execution, the
//! source hash its start named, the receipt's chain head and length), nothing
//! re-derived. A part the observation did not carry stays absent. Pure: no
//! I/O, no history, no consent; a kept run grants nothing and replays nothing.
//!
//! Its value is closed and versioned (`version` 1): a value of another
//! version, with an unknown key or a mistyped one is refused with its reason,
//! never repaired; its owner keeps those bytes unchanged.

use std::path::Path;

use serde_json::{Map, Value};

/// The value version this engine writes and reads.
const VERSION: u64 = 1;

/// One observed run, as kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeptRun {
    /// The workflow the run was asked of, relative to the project's root.
    pub workflow: Option<String>,
    /// The exit the host observed.
    pub exit: Option<u8>,
    /// The trace its settlement named.
    pub trace: Option<String>,
    /// The execution its frames and settlement carried.
    pub execution: Option<String>,
    /// The source hash its start named.
    pub workflow_sha256: Option<String>,
    /// The journal head its receipt named.
    pub chain_head: Option<String>,
    /// The journal length its receipt named.
    pub chain_len: Option<u64>,
}

impl KeptRun {
    /// Nothing observed yet (INV-019).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This observation as its host saw the run end: the workflow it asked,
    /// the exit it observed and the trace it was told (the host's own facts).
    #[must_use]
    pub fn ended(mut self, workflow: Option<&Path>, exit: u8, trace: Option<&Path>) -> Self {
        self.workflow = workflow.map(|w| w.display().to_string());
        self.exit = Some(exit);
        self.trace = trace.map(|t| t.display().to_string());
        self
    }

    /// The versioned value: `version` 1 and every part observed, the absent
    /// ones omitted.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("version".to_owned(), VERSION.into());
        let texts = [
            ("workflow", &self.workflow),
            ("trace", &self.trace),
            ("execution", &self.execution),
            ("workflow_sha256", &self.workflow_sha256),
            ("chain_head", &self.chain_head),
        ];
        for (key, value) in texts {
            if let Some(value) = value {
                map.insert(key.to_owned(), value.clone().into());
            }
        }
        if let Some(exit) = self.exit {
            map.insert("exit".to_owned(), exit.into());
        }
        if let Some(len) = self.chain_len {
            map.insert("chain_len".to_owned(), len.into());
        }
        Value::Object(map)
    }

    /// Read a kept value back.
    ///
    /// # Errors
    ///
    /// The value is not an object, is another version, holds an unknown key,
    /// or a part of another type: its reason, never a repaired run.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let map = value.as_object().ok_or("a kept run is not an object")?;
        match map.get("version").and_then(Value::as_u64) {
            Some(VERSION) => {}
            Some(other) => {
                return Err(format!(
                    "kept run version {other} is not one this engine reads"
                ));
            }
            None => return Err("a kept run names no version".to_owned()),
        }
        let mut kept = Self::new();
        for (key, part) in map {
            let text = || {
                part.as_str()
                    .map(str::to_owned)
                    .ok_or(format!("kept run `{key}` is not text"))
            };
            match key.as_str() {
                "version" => {}
                "workflow" => kept.workflow = Some(text()?),
                "trace" => kept.trace = Some(text()?),
                "execution" => kept.execution = Some(text()?),
                "workflow_sha256" => kept.workflow_sha256 = Some(text()?),
                "chain_head" => kept.chain_head = Some(text()?),
                "exit" => {
                    let exit = part.as_u64().and_then(|n| u8::try_from(n).ok());
                    kept.exit = Some(exit.ok_or("kept run `exit` is not an exit code")?);
                }
                "chain_len" => {
                    kept.chain_len =
                        Some(part.as_u64().ok_or("kept run `chain_len` is not a count")?);
                }
                other => return Err(format!("kept run holds an unknown key `{other}`")),
            }
        }
        Ok(kept)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_run_round_trips_and_keeps_absent_parts_absent() {
        let mut run = KeptRun::new();
        run.workflow = Some("two.nika".to_owned());
        run.exit = Some(0);
        run.execution = Some("01a0ef11-0212-70de-a8b3-99de9427fccc".to_owned());
        run.chain_len = Some(9);
        let value = run.to_value();
        assert_eq!(value["version"], 1);
        assert!(value.get("trace").is_none() && value.get("chain_head").is_none());
        assert_eq!(KeptRun::from_value(&value).expect("read back"), run);
        let legacy = serde_json::json!({"version": 1, "workflow": "two.nika", "exit": 4});
        let read = KeptRun::from_value(&legacy).expect("a partial observation");
        assert_eq!(
            (read.execution, read.workflow_sha256, read.exit),
            (None, None, Some(4))
        );
    }

    #[test]
    fn another_version_an_unknown_key_or_a_mistyped_part_is_refused() {
        for (value, why) in [
            (serde_json::json!({"version": 2}), "version 2"),
            (serde_json::json!({"workflow": "w"}), "no version"),
            (
                serde_json::json!({"version": 1, "next": "run it"}),
                "unknown key `next`",
            ),
            (
                serde_json::json!({"version": 1, "exit": 300}),
                "not an exit code",
            ),
            (
                serde_json::json!({"version": 1, "trace": 3}),
                "`trace` is not text",
            ),
            (serde_json::json!("run"), "not an object"),
        ] {
            let refused = KeptRun::from_value(&value).expect_err("refused");
            assert!(refused.contains(why), "{why}: {refused}");
        }
    }
}
