// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The WASM surface — the session law served to the browser, at the
//! binary's version (the `nika-check-wasm` precedent · ADR-107).
//!
//! JSON in, JSON out, zero unwrap: every refusal crosses the FFI as a
//! NAMED error, never as an opaque `unreachable`. Four doors cover the
//! studio's whole read of the law — the derivation of a (workflow, run)
//! pair, the journal fold, and the two seating moments of the plan board
//! (the board round-trips through the caller between revisions; the law
//! never keeps state).

use wasm_bindgen::prelude::*;

use crate::derive;
use crate::ingress::{self, GraphDoc};
use crate::model::{Run, Workflow};
use crate::plan::{self, Board};

/// The boundary's one refusal shape — the door's name + the reason.
fn refuse(door: &str, why: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("{door}: {why}"))
}

/// The derivation of one (workflow, run) pair — the same block the parity
/// fixtures pin, so the browser shows exactly what the studio computed.
#[wasm_bindgen]
pub fn derive_run(workflow_json: &str, run_json: &str) -> Result<String, JsValue> {
    let wf: Workflow =
        serde_json::from_str(workflow_json).map_err(|e| refuse("derive_run · workflow", e))?;
    let run: Run = serde_json::from_str(run_json).map_err(|e| refuse("derive_run · run", e))?;
    let ws = derive::waves(&wf);
    let neck = derive::bottleneck(&wf, &run);
    let out = serde_json::json!({
        "waves": ws.iter().map(|g| g.iter().map(|t| &t.id).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "wave_end": (0..ws.len()).map(|w| derive::wave_end(&wf, &run, w)).collect::<Vec<_>>(),
        "idle": run.steps.iter().map(|s| (s.id.clone(), serde_json::Value::from(derive::idle_of(&wf, &run, &s.id)))).collect::<serde_json::Map<String, serde_json::Value>>(),
        "bottleneck": neck,
        "total_cost": derive::total_cost(&run),
        "total_time": derive::total_time(&run),
        "verbs_used": derive::verbs_used(&wf),
        "has_failed": derive::has_failed(&run),
        "cost_by_verb": derive::cost_by_verb(&wf, &run).iter().map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v))).collect::<serde_json::Map<String, serde_json::Value>>(),
        "undeclared": derive::undeclared(&wf),
        "blast_radius": derive::blast_radius(&wf),
    });
    serde_json::to_string(&out).map_err(|e| refuse("derive_run · emit", e))
}

/// The journal fold — NDJSON bytes to the session's [`Run`].
#[wasm_bindgen]
pub fn fold_journal(ndjson: &str) -> Result<String, JsValue> {
    let run = ingress::run_from_journal(ndjson).map_err(|e| refuse("fold_journal", e))?;
    serde_json::to_string(&run).map_err(|e| refuse("fold_journal · emit", e))
}

/// The first seating — every slot born.
#[wasm_bindgen]
pub fn seat_first_json(graph_json: &str) -> Result<String, JsValue> {
    let g: GraphDoc =
        serde_json::from_str(graph_json).map_err(|e| refuse("seat_first · graph", e))?;
    serde_json::to_string(&plan::seat_first(&g)).map_err(|e| refuse("seat_first · emit", e))
}

/// The next seating — the caller hands the previous board back (the law
/// keeps no state between revisions; the board IS the state).
#[wasm_bindgen]
pub fn seat_next_json(board_json: &str, graph_json: &str) -> Result<String, JsValue> {
    let prev: Board =
        serde_json::from_str(board_json).map_err(|e| refuse("seat_next · board", e))?;
    let g: GraphDoc =
        serde_json::from_str(graph_json).map_err(|e| refuse("seat_next · graph", e))?;
    serde_json::to_string(&plan::seat_next(&prev, &g)).map_err(|e| refuse("seat_next · emit", e))
}
