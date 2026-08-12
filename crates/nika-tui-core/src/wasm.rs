// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The WASM surface — the session law served to the browser, at the
//! binary's version (the `nika-check-wasm` precedent · ADR-107).
//!
//! THE BOUNDARY SHAPE IS THE PRECEDENT'S: strings in, strings out, and a
//! refusal is a JSON VALUE (`{"error": "door · reason"}`), never a
//! thrown `JsValue` — a panic at the FFI arrives as `unreachable`, and a
//! refused door must stay readable by the caller AND testable natively
//! (the same function runs in `cargo test` and in the browser).
//!
//! Four doors cover the studio's whole read of the law — the derivation
//! of a (workflow, run) pair, the journal fold, and the two seating
//! moments of the plan board (the board round-trips through the caller
//! between revisions; the law never keeps state).

use wasm_bindgen::prelude::*;

use crate::derive;
use crate::ingress::{self, GraphDoc};
use crate::model::{Run, Workflow};
use crate::plan::{self, Board};

/// The one refusal shape — a JSON value naming its door, never a throw.
fn refuse(door: &str, why: impl std::fmt::Display) -> String {
    serde_json::json!({ "error": format!("{door} · {why}") }).to_string()
}

/// The derivation of one (workflow, run) pair — the same block the parity
/// fixtures pin, so the browser shows exactly what the studio computed.
#[wasm_bindgen]
#[must_use]
pub fn derive_run(workflow_json: &str, run_json: &str) -> String {
    let wf: Workflow = match serde_json::from_str(workflow_json) {
        Ok(wf) => wf,
        Err(e) => return refuse("derive_run · workflow", e),
    };
    let run: Run = match serde_json::from_str(run_json) {
        Ok(run) => run,
        Err(e) => return refuse("derive_run · run", e),
    };
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
    out.to_string()
}

/// The journal fold — NDJSON bytes to the session's [`Run`].
#[wasm_bindgen]
#[must_use]
pub fn fold_journal(ndjson: &str) -> String {
    match ingress::run_from_journal(ndjson) {
        Ok(run) => serde_json::to_string(&run).unwrap_or_else(|e| refuse("fold_journal · emit", e)),
        Err(e) => refuse("fold_journal", e),
    }
}

/// The first seating — every slot born.
#[wasm_bindgen]
#[must_use]
pub fn seat_first(graph_json: &str) -> String {
    let g: GraphDoc = match serde_json::from_str(graph_json) {
        Ok(g) => g,
        Err(e) => return refuse("seat_first · graph", e),
    };
    serde_json::to_string(&plan::seat_first(&g)).unwrap_or_else(|e| refuse("seat_first · emit", e))
}

/// The next seating — the caller hands the previous board back (the law
/// keeps no state between revisions; the board IS the state).
#[wasm_bindgen]
#[must_use]
pub fn seat_next(board_json: &str, graph_json: &str) -> String {
    let prev: Board = match serde_json::from_str(board_json) {
        Ok(b) => b,
        Err(e) => return refuse("seat_next · board", e),
    };
    let g: GraphDoc = match serde_json::from_str(graph_json) {
        Ok(g) => g,
        Err(e) => return refuse("seat_next · graph", e),
    };
    serde_json::to_string(&plan::seat_next(&prev, &g))
        .unwrap_or_else(|e| refuse("seat_next · emit", e))
}
