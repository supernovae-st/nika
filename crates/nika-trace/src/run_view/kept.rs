// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One observed run as a host keeps it between sessions: what the
//! observation named (the workflow, its exit, the trace, the execution, the
//! source hash its start named, the receipt's chain head and length), nothing
//! re-derived. A served run also keeps the job its resident ran it as, the
//! receipt and declared outputs that resident serves for the job, and the
//! seal facts the ONE verifier attests for its journal. A part the
//! observation did not carry stays absent. Pure: no I/O, no history, no
//! consent; a kept run grants nothing and replays nothing.
//!
//! Its value is closed and versioned (`version` 1): a value of another
//! version, with an unknown key or a mistyped one is refused with its reason,
//! never repaired; its owner keeps those bytes unchanged. The served parts
//! (`job` · `receipt` · `outputs` · `seal_verified` · `signed`) are additive
//! under `version` 1, written only when present: an engine that predates them
//! refuses a kept run carrying one and keeps its bytes unchanged. No key here
//! is called `sealed`: that word names a run whose receipt named its chain
//! head (`Run::sealed`), never a verifier's judgment.

use std::path::Path;

use serde_json::{Map, Value};

use super::ComponentRun;

/// The value version this engine writes and reads.
const VERSION: u64 = 1;

/// The most of a served run's declared outputs a kept run keeps, encoded: every later turn
/// appends the kept run to HOME history again, so larger outputs stay with their job alone.
const KEPT_OUTPUTS_BYTES: usize = 16 * 1024;

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
    /// `trace` is the receipt's opaque trace identity, resolved through the
    /// door that ran the run: never a journal path a host may read.
    pub trace_opaque: bool,
    /// The execution its frames and settlement carried.
    pub execution: Option<String>,
    /// The source hash its start named.
    pub workflow_sha256: Option<String>,
    /// The journal head its receipt named.
    pub chain_head: Option<String>,
    /// The journal length its receipt named.
    pub chain_len: Option<u64>,
    /// The job its resident ran it as: present when the run was served.
    pub job: Option<String>,
    /// The receipt its resident's job route serves for that job: present when the run was
    /// served and its observation ended with one. Boxed: an observed step carries its kept run
    /// by value, and the served parts pushed that step past clippy's `large_enum_variant` wall.
    pub receipt: Option<Box<Map<String, Value>>>,
    /// The declared outputs its resident's job route serves for that job, whole, when they fit
    /// 16 KiB encoded (an empty object when the workflow declared none). Absent means not
    /// carried (a run not served, outputs its route did not serve, or larger ones), never « no
    /// outputs »: the job keeps them either way.
    pub outputs: Option<Map<String, Value>>,
    /// `nika trace verify` accepts the `run_sealed` record of its journal ([`Self::verified`]):
    /// present when its door verified that journal.
    pub seal_verified: Option<bool>,
    /// That record's signature verifies against a custody key ([`Self::verified`]): present
    /// when its door verified that journal.
    pub signed: Option<bool>,
}

impl KeptRun {
    /// Nothing observed yet (INV-019).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This observation as its host saw the run end: the workflow it asked,
    /// the exit it observed and the trace it was told (the host's own facts).
    /// A trace the observed leg already names (a receipt's opaque identity) is
    /// kept: the host's path then stays the host's, read for the observation
    /// and never kept in its place.
    #[must_use]
    pub fn ended(mut self, workflow: Option<&Path>, exit: u8, trace: Option<&Path>) -> Self {
        self.workflow = workflow.map(|w| w.display().to_string());
        self.exit = Some(exit);
        if self.trace.is_none() {
            self.trace = trace.map(|t| t.display().to_string());
            self.trace_opaque = false;
        }
        self
    }

    /// What `journal` proves of a component of saved bytes through this run's execution: that
    /// execution's one start must name `source_sha256`, the hash of the exact saved bytes as a
    /// start names it, and each of `tasks` is settled by that execution's own frames alone. The
    /// source hash this value keeps, the exit and the run's terminal word are never the key;
    /// what the frames cannot attribute stays unknown ([`ComponentRun`]).
    #[must_use]
    pub fn component_run(
        &self,
        journal: &str,
        source_sha256: &str,
        tasks: &[&str],
    ) -> ComponentRun {
        super::join::component_run(journal, self.execution.as_deref(), source_sha256, tasks)
    }

    /// The same run as its resident served it: the job it ran as, and the receipt and the
    /// declared outputs its job route serves for that job. Outputs over 16 KiB encoded are not
    /// kept (every later turn appends the kept run to HOME history again): the job keeps them.
    #[must_use]
    pub fn served(
        mut self,
        job: &str,
        receipt: Option<Map<String, Value>>,
        outputs: Option<Map<String, Value>>,
    ) -> Self {
        self.job = Some(job.to_owned());
        self.receipt = receipt.map(Box::new);
        self.outputs = outputs.filter(|outputs| {
            serde_json::to_vec(outputs).is_ok_and(|bytes| bytes.len() <= KEPT_OUTPUTS_BYTES)
        });
        self
    }

    /// The same run with the seal facts `nika trace verify` attests for its journal, read from
    /// the verifier's machine document (its `--json`, or a door's projection of it) through the
    /// one reading the proof view makes. Both hold only for an intact chain whose `run_sealed`
    /// signature verifies against a custody key: at seal format 1 that record is its
    /// signature, so `seal_verified` and `signed` agree, and `signed` stays its own fact so a
    /// seal never reads as signed without one. An unsealed journal, a seal naming a key this
    /// machine cannot check (its signature not judged), a forged or buried seal, a torn or
    /// incomplete chain, or a document the verifier did not write is neither.
    #[must_use]
    pub fn verified(mut self, verify: &Value) -> Self {
        let signed = super::Attestation::of(verify).signed();
        self.seal_verified = Some(signed);
        self.signed = Some(signed);
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
            ("job", &self.job),
        ];
        for (key, value) in texts {
            if let Some(value) = value {
                map.insert(key.to_owned(), value.clone().into());
            }
        }
        for (key, value) in [
            ("receipt", self.receipt.as_deref()),
            ("outputs", self.outputs.as_ref()),
        ] {
            if let Some(value) = value {
                map.insert(key.to_owned(), Value::Object(value.clone()));
            }
        }
        for (key, value) in [
            ("seal_verified", self.seal_verified),
            ("signed", self.signed),
        ] {
            if let Some(value) = value {
                map.insert(key.to_owned(), value.into());
            }
        }
        if let Some(exit) = self.exit {
            map.insert("exit".to_owned(), exit.into());
        }
        if let Some(len) = self.chain_len {
            map.insert("chain_len".to_owned(), len.into());
        }
        if self.trace_opaque {
            map.insert("trace_opaque".to_owned(), true.into());
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
            let object =
                || (part.as_object().cloned()).ok_or(format!("kept run `{key}` is not an object"));
            let flag = || (part.as_bool()).ok_or(format!("kept run `{key}` is not a flag"));
            match key.as_str() {
                "version" => {}
                "workflow" => kept.workflow = Some(text()?),
                "trace" => kept.trace = Some(text()?),
                "execution" => kept.execution = Some(text()?),
                "workflow_sha256" => kept.workflow_sha256 = Some(text()?),
                "chain_head" => kept.chain_head = Some(text()?),
                "job" => kept.job = Some(text()?),
                "receipt" => kept.receipt = Some(Box::new(object()?)),
                "outputs" => kept.outputs = Some(object()?),
                "seal_verified" => kept.seal_verified = Some(flag()?),
                "signed" => kept.signed = Some(flag()?),
                "exit" => {
                    let exit = part.as_u64().and_then(|n| u8::try_from(n).ok());
                    kept.exit = Some(exit.ok_or("kept run `exit` is not an exit code")?);
                }
                "chain_len" => {
                    kept.chain_len =
                        Some(part.as_u64().ok_or("kept run `chain_len` is not a count")?);
                }
                "trace_opaque" => {
                    kept.trace_opaque =
                        (part.as_bool()).ok_or("kept run `trace_opaque` is not a flag")?;
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

    /// A leg that names its trace keeps it over the host's path; a leg that
    /// names none takes the path the host was told.
    #[test]
    fn a_trace_the_leg_names_is_kept_and_the_path_fills_only_an_unnamed_one() {
        let path = Path::new("/srv/project/.nika/traces/run.ndjson");
        let mut named = KeptRun::new();
        named.trace = Some("01a0ef11-0212-70de-a8b3-99de9427f000".to_owned());
        let ended = named.ended(Some(Path::new("copy.nika")), 0, Some(path));
        assert_eq!(
            ended.trace.as_deref(),
            Some("01a0ef11-0212-70de-a8b3-99de9427f000")
        );
        assert_eq!(
            (ended.workflow.as_deref(), ended.exit),
            (Some("copy.nika"), Some(0))
        );
        let unnamed = KeptRun::new().ended(None, 1, Some(path));
        assert_eq!(
            unnamed.trace.as_deref(),
            Some("/srv/project/.nika/traces/run.ndjson")
        );
        assert_eq!(KeptRun::new().ended(None, 1, None).trace, None);
    }

    /// An opaque trace keeps its mark through the kept value, so no host reads
    /// it back as a journal path; a path the host was told is never marked, and
    /// a mark of another type is refused.
    #[test]
    fn an_opaque_trace_keeps_its_mark_and_a_path_never_carries_one() {
        let mut leg = KeptRun::new();
        leg.trace = Some("01a11bc6d9d775e3b6f53a184c031914".to_owned());
        leg.trace_opaque = true;
        let ended = leg.ended(None, 0, Some(Path::new(".nika/traces/t.ndjson")));
        let value = ended.to_value();
        assert_eq!(value["trace_opaque"], true);
        assert_eq!(KeptRun::from_value(&value).expect("read back"), ended);
        let mut unnamed = KeptRun::new();
        unnamed.trace_opaque = true;
        let told = unnamed.ended(None, 0, Some(Path::new(".nika/traces/t.ndjson")));
        assert!(
            !told.trace_opaque,
            "a path the host was told is no opaque identity"
        );
        assert!(told.to_value().get("trace_opaque").is_none());
        let mistyped = serde_json::json!({"version": 1, "trace_opaque": "yes"});
        let refused = KeptRun::from_value(&mistyped).expect_err("refused");
        assert!(
            refused.contains("`trace_opaque` is not a flag"),
            "{refused}"
        );
    }

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
            (
                serde_json::json!({"version": 1, "receipt": "r"}),
                "`receipt` is not an object",
            ),
            (
                serde_json::json!({"version": 1, "outputs": [1]}),
                "`outputs` is not an object",
            ),
            (
                serde_json::json!({"version": 1, "seal_verified": "yes"}),
                "`seal_verified` is not a flag",
            ),
            (
                // A `sealed` claim is never read as a verifier's judgment: no key carries it.
                serde_json::json!({"version": 1, "sealed": true}),
                "unknown key `sealed`",
            ),
            (
                // `version` 1 stays closed: no writer's name makes an unknown key tolerated.
                serde_json::json!({"version": 1, "written_by": "nika"}),
                "unknown key `written_by`",
            ),
            (
                serde_json::json!({"version": 1, "signed": 1}),
                "`signed` is not a flag",
            ),
            (
                serde_json::json!({"version": 1, "job": 7}),
                "`job` is not text",
            ),
        ] {
            let refused = KeptRun::from_value(&value).expect_err("refused");
            assert!(refused.contains(why), "{why}: {refused}");
        }
    }

    fn object(value: &Value) -> Map<String, Value> {
        value.as_object().cloned().expect("an object literal")
    }

    /// A served run keeps the job it ran as, and the receipt and declared outputs its resident
    /// serves for that job, as additive parts of `version` 1. Outputs over the bound stay with
    /// the job alone; a run never served writes none of these keys, so its kept value is the one
    /// an earlier engine reads.
    #[test]
    fn a_served_run_keeps_its_job_receipt_and_bounded_outputs() {
        let receipt = object(&serde_json::json!({
            "job_id": "01a0ef11-0212-70de-a8b3-99de9427f000",
            "execution_id": "exe-01a0ef11-0212-70de-a8b3-99de9427fccc",
            "trace_id": "01a11bc6d9d775e3b6f53a184c031914",
            "snapshot_digest": "aa".repeat(32),
            "origin": {"kind": "manual"},
            "chain_head": "cd",
        }));
        let outputs = object(&serde_json::json!({"summary": "three lines", "count": 3}));
        let run = KeptRun::new()
            .served("job-1", Some(receipt.clone()), Some(outputs.clone()))
            .ended(Some(Path::new("copy.nika")), 0, None);
        assert_eq!(run.job.as_deref(), Some("job-1"));
        assert_eq!(run.receipt.as_deref(), Some(&receipt));
        assert_eq!(run.outputs.as_ref(), Some(&outputs));
        let value = run.to_value();
        assert_eq!(
            value["version"], 1,
            "no version bump: the parts are additive"
        );
        assert_eq!(value["job"], "job-1");
        assert_eq!(value["receipt"]["chain_head"], "cd");
        assert_eq!(value["outputs"]["count"], 3);
        assert_eq!(KeptRun::from_value(&value).expect("read back"), run);

        let mut large = Map::new();
        large.insert("text".to_owned(), "x".repeat(KEPT_OUTPUTS_BYTES).into());
        let over = KeptRun::new().served("job-2", Some(receipt), Some(large));
        assert_eq!(over.outputs, None, "over the bound: the job keeps them");
        assert_eq!(over.job.as_deref(), Some("job-2"));
        assert!(over.receipt.is_some(), "the receipt is kept beside them");
        let none = KeptRun::new().served("job-3", None, None).to_value();
        assert!(none.get("receipt").is_none() && none.get("outputs").is_none());

        let mut native = KeptRun::new();
        native.execution = Some("01a0ef11-0212-70de-a8b3-99de9427fccc".to_owned());
        let native = native
            .ended(Some(Path::new("copy.nika")), 0, None)
            .to_value();
        let mut keys: Vec<&str> = native
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["execution", "exit", "version", "workflow"]);
    }

    /// The verify document for one journal, as `nika trace verify --json` writes it.
    fn verify(tier: &str, headline: &str, seal: &str) -> Value {
        serde_json::json!({
            "verify_version": 1,
            "tier": tier,
            "exit": 0,
            "chain": {"events": 4, "head": "cd", "headline": headline, "liveness": null},
            "seal": {"tier": seal},
            "anchor": {"tier": "not-present"},
            "replay": {"tier": "not-asked"},
            "lines": [],
        })
    }

    /// `seal_verified` and `signed` are what the ONE verifier attests, never what a receipt
    /// names: both only for an intact chain whose `run_sealed` signature verifies. An unsigned
    /// journal, a key this machine cannot check, a forged or buried seal, a cut chain and a
    /// document the verifier did not write are neither; a door's projection reads the same.
    #[test]
    fn the_seal_facts_are_the_verifiers_never_a_receipts() {
        let facts = |doc: &Value| {
            let run = KeptRun::new().verified(doc);
            (run.seal_verified, run.signed)
        };
        let both = (Some(true), Some(true));
        let neither = (Some(false), Some(false));
        assert_eq!(facts(&verify("sealed", "intact", "sealed")), both);
        assert_eq!(facts(&verify("anchored", "intact", "sealed")), both);
        assert_eq!(
            facts(&verify("ok", "intact", "unsealed")),
            neither,
            "an unsigned run is never sealed, whatever its receipt names"
        );
        assert_eq!(
            facts(&verify("ok", "intact", "unattributable")),
            neither,
            "a seal whose key this machine cannot check is not judged"
        );
        assert_eq!(facts(&verify("ok", "intact", "forged")), neither);
        assert_eq!(facts(&verify("ok", "intact", "buried")), neither);
        assert_eq!(
            facts(&verify("sealed", "incomplete", "sealed")),
            neither,
            "an incomplete chain attests nothing"
        );
        assert_eq!(facts(&verify("ok", "torn", "sealed")), neither);
        assert_eq!(
            facts(&serde_json::json!({"tier": "unknown", "exit": 2})),
            neither
        );
        let mut door = verify("sealed", "intact", "sealed");
        let projection = door.as_object_mut().expect("an object");
        projection.insert("verdict".to_owned(), "sealed".into());
        projection.insert("reason".to_owned(), "sealed".into());
        projection.insert("trace_id".to_owned(), "t1".into());
        assert_eq!(facts(&door), both, "a door's projection keeps the reading");

        let mut unread = KeptRun::new();
        unread.chain_head = Some("cd".to_owned());
        let value = unread.to_value();
        assert!(
            value.get("seal_verified").is_none() && value.get("signed").is_none(),
            "a chain head alone verifies nothing: no seal fact is written"
        );
        let run = KeptRun::new().verified(&verify("ok", "intact", "unsealed"));
        let value = run.to_value();
        assert_eq!(
            (&value["seal_verified"], &value["signed"]),
            (&false.into(), &false.into())
        );
        assert_eq!(KeptRun::from_value(&value).expect("read back"), run);
    }

    /// The outputs a served run keeps are whole or absent: kept when their JSON encoding (keys,
    /// punctuation and UTF-8 bytes included) is at most 16384 bytes, absent one byte past it.
    /// Absent means not carried, never « no outputs »: a run that declared none keeps an empty
    /// object, and its job keeps any it did not carry.
    #[test]
    fn outputs_are_kept_whole_up_to_the_exact_encoded_bound() {
        assert_eq!(KEPT_OUTPUTS_BYTES, 16_384);
        // `{"t":"…"}` encodes as 8 bytes around its text, and « é » is two UTF-8 bytes.
        let sized = |bytes: usize| {
            let mut text = "é".repeat((bytes - 8) / 2);
            if (bytes - 8) % 2 == 1 {
                text.push('x');
            }
            let mut map = Map::new();
            map.insert("t".to_owned(), text.into());
            assert_eq!(serde_json::to_vec(&map).expect("encodes").len(), bytes);
            map
        };
        let at = KeptRun::new().served("job-1", None, Some(sized(KEPT_OUTPUTS_BYTES)));
        assert_eq!(
            at.outputs,
            Some(sized(KEPT_OUTPUTS_BYTES)),
            "exactly at the bound: kept whole"
        );
        let over = KeptRun::new().served("job-1", None, Some(sized(KEPT_OUTPUTS_BYTES + 1)));
        assert_eq!(over.outputs, None, "one byte past it: not carried");
        assert!(
            over.to_value().get("outputs").is_none(),
            "not carried: no key, never an empty object"
        );
        let declared_none = KeptRun::new().served("job-1", None, Some(Map::new()));
        assert_eq!(
            declared_none.to_value()["outputs"],
            serde_json::json!({}),
            "declared none: an empty object, carried"
        );
        let unserved = KeptRun::new().served("job-1", None, None).to_value();
        assert!(unserved.get("outputs").is_none());
    }
}
