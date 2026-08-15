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
//! Five doors cover the studio's whole read of the law — the version pin,
//! the derivation of a (workflow, run) pair, the journal fold, and the two
//! seating moments of the plan board (the board round-trips through the
//! caller between revisions; the law never keeps state).
//!
//! ⚠️ NAMING — the doors are `board_first`/`board_next`, NOT the bare
//! `seat_*`: a door named exactly like its `plan::*` import returned an
//! empty string on native targets (measured twice · wasm-bindgen's native
//! shim resolves the imported name — a macro-hygiene class). Distinctive
//! door names are the workaround, and this note is why.

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::claims;
use crate::derive;
use crate::ingress::{self, GraphDoc};
use crate::model::{Run, Workflow};
use crate::plan::{self, Board};

/// The engine version this law speaks for — a consumer pins the artifact's
/// build the way the sibling's `engine_version()` enables (ADR-107).
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[must_use]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// A door's input ceiling — 64 MiB of JSON is already a workbook, not a
/// workflow. The pre-flight keeps a hostile multi-hundred-MB string from
/// ever reaching the parser (the swarm's F4 · hardening, named).
const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

/// Defense in depth for JSON text that echoes caller bytes (the sibling's
/// gate-11 F3, same five): serde escapes every C0 control, but `<` `>` `&`
/// U+2028 U+2029 are legal in JSON strings and hostile where a page inlines
/// JSON (a `</script>` ends the element · U+2028/9 break a JS parse). These
/// five only ever occur INSIDE string literals here, so the whole-text
/// `\uXXXX` rewrite is exactly equivalent JSON — escaping stays every
/// consumer's duty; this removes the class at the source.
fn escape_for_embedding(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(c),
        }
    }
    out
}

/// The one refusal shape — a JSON value naming its door, never a throw.
fn refuse(door: &str, why: impl std::fmt::Display) -> String {
    escape_for_embedding(&serde_json::json!({ "error": format!("{door} · {why}") }).to_string())
}

/// The pre-flight every door runs first.
fn admit(door: &str, inputs: &[&str]) -> Result<(), String> {
    for input in inputs {
        if input.len() > MAX_INPUT_BYTES {
            return Err(refuse(
                door,
                "input over the 64 MiB ceiling — a workflow is not a workbook",
            ));
        }
    }
    Ok(())
}

/// The derivation of one (workflow, run) pair — the same block the parity
/// fixtures pin, so the browser shows exactly what the studio computed.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[must_use]
pub fn derive_run(workflow_json: &str, run_json: &str) -> String {
    if let Err(e) = admit("derive_run", &[workflow_json, run_json]) {
        return e;
    }
    let wf: Workflow = match serde_json::from_str(workflow_json) {
        Ok(wf) => wf,
        Err(e) => return refuse("derive_run · workflow", e),
    };
    let run: Run = match serde_json::from_str(run_json) {
        Ok(run) => run,
        Err(e) => return refuse("derive_run · run", e),
    };
    let ws = derive::waves(&wf);
    let neck = derive::bottleneck(&ws, &run);
    // ⚠️ This used to end in `.unwrap_or_default()`, which turns ONE group
    // failing to serialize into an EMPTY array — the surface would render
    // "no fan-out anywhere" and read it as a fact about the workflow. A
    // total silent loss dressed as an answer. The door has one refusal
    // shape; a failure here takes it, like every other.
    let groups = match derive::groups_of(&wf)
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(g) => g,
        Err(e) => return refuse("derive_run · groups", e),
    };
    let out = serde_json::json!({
        "waves": ws.groups().iter().map(|g| g.iter().map(|t| &t.id).collect::<Vec<_>>()).collect::<Vec<_>>(),
        // the BATCH forms · asking the singular ones n times was O(n²) twice
        // over (a wave scan per wave, a step scan plus a wave scan per step).
        // Same law, asked once for everything.
        "wave_end": derive::wave_ends(&ws, &run),
        "idle": derive::idles(&ws, &run).into_iter().map(|(k, v)| (k, serde_json::Value::from(v))).collect::<serde_json::Map<String, serde_json::Value>>(),
        "bottleneck": neck,
        "total_cost": derive::total_cost(&run),
        "total_time": derive::total_time(&run),
        "verbs_used": derive::verbs_used(&wf),
        "has_failed": derive::has_failed(&run),
        "cost_by_verb": derive::cost_by_verb(&wf, &run).iter().map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v))).collect::<serde_json::Map<String, serde_json::Value>>(),
        "undeclared": derive::undeclared(&wf),
        "blast_radius": derive::blast_radius(&wf),
        // the fan-out projection rides the door too — otherwise the browser
        // re-derives the grouping in TS, and the divergence disease survives
        // for exactly that feature (the API lens, gate 11)
        "groups": groups,
        // the CLAIMS ride the door for exactly the reason `groups` does.
        // Until this line the `claims` module had zero consumers in `src/`:
        // a law nothing applied, while the browser decided the same
        // questions inline with a DIFFERENT rule (`run.trace === 'synthetic'`
        // against this crate's `when`). One verdict, computed once, printed
        // by whoever renders.
        "claims": {
            "chain_intact": claims::may_claim_chain_intact(&run),
            "bottleneck": claims::may_claim_bottleneck(neck.as_ref()),
        },
    });
    escape_for_embedding(&out.to_string())
}

/// The journal fold — NDJSON bytes to the session's [`Run`].
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[must_use]
pub fn fold_journal(ndjson: &str) -> String {
    if let Err(e) = admit("fold_journal", &[ndjson]) {
        return e;
    }
    match ingress::run_from_journal(ndjson) {
        Ok(run) => serde_json::to_string(&run).map_or_else(
            |e| refuse("fold_journal · emit", e),
            |s| escape_for_embedding(&s),
        ),
        Err(e) => refuse("fold_journal", e),
    }
}

/// The first seating — every slot born.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[must_use]
pub fn board_first(graph_json: &str) -> String {
    if let Err(e) = admit("board_first", &[graph_json]) {
        return e;
    }
    let g: GraphDoc = match serde_json::from_str(graph_json) {
        Ok(g) => g,
        Err(e) => return refuse("board_first · graph", e),
    };
    serde_json::to_string(&plan::seat_first(&g)).map_or_else(
        |e| refuse("board_first · emit", e),
        |s| escape_for_embedding(&s),
    )
}

/// The next seating — the caller hands the previous board back (the law
/// keeps no state between revisions; the board IS the state).
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[must_use]
pub fn board_next(board_json: &str, graph_json: &str) -> String {
    if let Err(e) = admit("board_next", &[board_json, graph_json]) {
        return e;
    }
    let prev: Board = match serde_json::from_str(board_json) {
        Ok(b) => b,
        Err(e) => return refuse("board_next · board", e),
    };
    let g: GraphDoc = match serde_json::from_str(graph_json) {
        Ok(g) => g,
        Err(e) => return refuse("board_next · graph", e),
    };
    serde_json::to_string(&plan::seat_next(&prev, &g)).map_or_else(
        |e| refuse("board_next · emit", e),
        |s| escape_for_embedding(&s),
    )
}
