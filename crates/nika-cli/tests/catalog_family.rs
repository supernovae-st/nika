// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The provider FAMILY traversal — every catalog row, not a spot check.
//!
//! G3 (`wiring-g3-unprobed`) counts 38 provider declarations with no probe
//! behind them. Thirty-eight ledger rows would be thirty-eight places to
//! drift; one traversal that walks the whole family is the probe.
//!
//! What it pins · the catalog is DATA (38 vendors, their models, prices and
//! capabilities) and the wire is CODE (`CANONICAL_IDS`). The two are
//! deliberately different sizes. The defect this catches is the pair moving
//! apart in silence: a vendor row added with no adapter, or an adapter
//! removed under a row that still teaches it.
//!
//! Every seat is judged the way a user meets it — through `check`'s MODELS
//! rung, on a real workflow, before a token is spent.
//!
//! #1184 widened the traversal from « the wire agrees with the catalog » to
//! « and every surface a chooser reads SAYS SO ». The refusal rung was
//! already honest; what was missing is that the three projections rendered a
//! reachable and an unreachable vendor identically, so the fact only arrived
//! after the choice was made. The loop below now judges, per row and in one
//! pass · the shipped `--json` mark · the shipped human row · the MODELS
//! verdict. A row can no longer be marked runnable and refused, or refused
//! and rendered inviting.

use std::io::{BufRead as _, Write as _};

use nika_cli::Theme;
use nika_cli::verbs::{catalog, check, exit};

const PLAIN: Theme = Theme::new(false, false, false);

/// A one-task workflow with the seat written into the envelope.
///
/// Not `--model`: that flag is a PRICING lens (« price as if this model
/// replaced the envelope default ») and leaves the MODELS rung judging the
/// authored seat. A first cut of this test used it and every row came back
/// resolvable — a probe that was measuring `mock/mock` 38 times.
fn fixture(seat: &str) -> String {
    format!(
        "nika: catalog-family-probe

permits: {{}}

tasks:
  t:
    infer:
      model: {seat}
      prompt: hi
      max_tokens: 16

outputs:
  a: ${{{{ tasks.t.output }}}}
"
    )
}

/// Cataloged vendors this binary carries NO wire for.
///
/// This list is the point of the test. A cataloged vendor that cannot be
/// seated is a legitimate state — the catalog also serves pricing and
/// capability lookups for models reached through a gateway. What is NOT
/// legitimate is the set changing without anyone deciding.
///
/// When this fails, exactly one of two things happened, and the fix differs:
///
/// - a row was ADDED to `llm-providers.toml` with no adapter → either wire
///   it in `nika-providers::profile`, or add it here on purpose;
/// - a provider was WIRED → delete its line here.
///
/// Never edit this list to make a red go away without naming which of the
/// two it was.
const UNWIRED: &[&str] = &[
    "ai21",
    "azure",
    "bedrock",
    "cerebras",
    "cloudflare",
    "cohere",
    "databricks",
    "deepinfra",
    "fireworks",
    "hyperbolic",
    "minimax",
    "moonshot",
    "native",
    "perplexity",
    "qwen",
    "replicate",
    "sambanova",
    "together",
    "vertex",
    "voyage",
    "writer",
    "zhipu",
];

/// One file per seat — the id rides the filename so a failure names the row.
fn plant(seat: &str, slug: &str) -> String {
    let dir = std::env::temp_dir().join("nika-cli-catalog-family");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(format!("seat-{slug}.nika.yaml"));
    std::fs::write(&path, fixture(seat)).expect("write fixture");
    path.to_str().expect("utf8 path").to_owned()
}

/// Does a seat resolve in THIS binary? Asked through the audit a user runs,
/// never through the resolver asking itself.
fn seat_resolves(seat: &str, slug: &str) -> bool {
    let path = plant(seat, slug);
    let out = check::run(&path, true, false, None, PLAIN);
    let payload: serde_json::Value = serde_json::from_str(&out.text)
        .unwrap_or_else(|e| panic!("json for {seat}: {e}\n{}", out.text));
    payload["models_resolve"]
        .as_bool()
        .unwrap_or_else(|| panic!("models_resolve is a bool for {seat}: {payload:#}"))
}

/// The catalog projection as the binary EMITS it (`nika catalog --json`),
/// not as a builder would rebuild it. A test that re-derives the payload
/// cannot see a surface that dropped the wiring chain — which is the whole
/// defect class here.
fn shipped_json() -> serde_json::Value {
    let out = catalog::run(true, PLAIN);
    assert_eq!(out.code, exit::OK, "`nika catalog --json` runs");
    serde_json::from_str(&out.text).expect("the shipped payload is JSON")
}

#[test]
fn every_cataloged_provider_either_seats_or_is_refused_by_name() {
    let export = nika_catalog::export::catalog_export();
    let wired: std::collections::BTreeSet<&str> =
        nika_providers::CANONICAL_IDS.into_iter().collect();

    // The two SHIPPED projections, rendered once and read per row below.
    let json = shipped_json();
    let human = catalog::run(false, PLAIN).text;

    let mut seats = 0usize;
    let mut refused: Vec<&str> = Vec::new();
    for p in &export.providers {
        let seat = format!("{}/{}", p.id, p.default_model);
        let resolves = seat_resolves(&seat, p.id);
        assert_eq!(
            resolves,
            wired.contains(p.id),
            "`{seat}` resolves={resolves} but the wire table says {}. The catalog row \
             and the adapter disagree — one of them is the lie.",
            wired.contains(p.id)
        );

        // #1184 · the machine projection carries the verdict, per row.
        let row = json["providers"]
            .as_array()
            .expect("providers array")
            .iter()
            .find(|r| r["id"] == p.id)
            .unwrap_or_else(|| panic!("`{}` is missing from the shipped payload", p.id));
        assert_eq!(
            row["wired"],
            serde_json::Value::Bool(resolves),
            "`{}` renders wired={} on --json while check says resolves={resolves}. \
             A machine consumer told to pick from this list would pick a seat that \
             cannot run.",
            p.id,
            row["wired"],
        );

        // #1184 · and the human column a user actually scans.
        let line = human
            .lines()
            .find(|l| l.contains(&format!("\u{2502}  {:<14}", p.id)))
            .unwrap_or_else(|| panic!("`{}` is missing from the human listing", p.id));
        assert_eq!(
            !line.contains("catalog only"),
            resolves,
            "`{}` reads runnable={} down the column while check says resolves={resolves}. \
             Row: {line}",
            p.id,
            !line.contains("catalog only"),
        );

        if resolves {
            seats += 1;
        } else {
            refused.push(p.id);
        }
    }

    // Membership is the invariant, never catalog order — the listing is
    // free to re-rank its teaching surface without touching the wire.
    let mut refused_sorted = refused.clone();
    refused_sorted.sort_unstable();
    assert_eq!(
        refused_sorted, UNWIRED,
        "the cataloged-but-unseatable set moved. Read the UNWIRED doc comment \
         before touching it: adding a name and adding an adapter are different fixes."
    );
    assert_eq!(
        seats + refused.len(),
        export.providers.len(),
        "every catalog row got a verdict — a family probe that skips rows proves nothing"
    );
    assert!(
        seats > 0 && !refused.is_empty(),
        "both ends of the partition are non-empty ({seats} seated · {} refused) — \
         a traversal where every row lands on one side is not measuring the split",
        refused.len()
    );
}

#[test]
fn the_refusal_names_the_runnable_count_and_where_to_get_the_list() {
    // The MODELS rung tells a refused user how many seats exist and who
    // names them. Both halves are load-bearing: the count is what makes the
    // refusal actionable, and the pointer is the only route to the list.
    let unwired = UNWIRED
        .first()
        .expect("the unwired set is never empty here");
    let path = plant(&format!("{unwired}/whatever"), "refusal");
    let out = check::run(&path, false, false, None, PLAIN);

    assert_eq!(
        out.code,
        exit::FILE,
        "an unseatable model fails the audit: {}",
        out.text
    );
    assert!(out.text.contains("MODELS"), "the rung speaks: {}", out.text);
    assert!(
        out.text.contains(unwired),
        "the refusal names the provider the user typed: {}",
        out.text
    );
    let runnable = nika_providers::CANONICAL_IDS.len();
    assert!(
        out.text.contains(&format!("{runnable} runnable")),
        "the refusal counts the seats this binary actually carries ({runnable}): {}",
        out.text
    );
    assert!(
        out.text.contains("nika doctor"),
        "and points at the surface that NAMES them: {}",
        out.text
    );
}

/// One JSON-RPC round trip against the REAL `nika mcp` stdio server.
///
/// The third projection is the one an AGENT reads, and the only honest way
/// to judge it is to call the tool the way a client does — reading the code
/// that serves it proves the code, not the wire.
fn mcp_roundtrip(request: &serde_json::Value) -> serde_json::Value {
    // Same carve-out as lsp_transport.rs: driving the shipped binary IS the
    // contract under test.
    #[allow(clippy::disallowed_types)]
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_nika-cli"))
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn nika mcp");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        writeln!(stdin, "{request}").expect("write request");
    }
    let stdout = child.stdout.take().expect("stdout");
    let line = std::io::BufReader::new(stdout)
        .lines()
        .next()
        .expect("the server answers")
        .expect("a utf8 reply");
    let _ = child.wait();
    serde_json::from_str(&line).expect("the reply is JSON-RPC")
}

#[test]
fn the_mcp_catalog_tool_marks_every_row_and_teaches_the_filter() {
    // The tool description is the instruction an agent OBEYS. « Pick REAL
    // model ids from here » over an unmarked list is an instruction to
    // author a workflow that cannot run — 22 of 38 rows, at the time this
    // was measured.
    let listed = mcp_roundtrip(&serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/list"
    }));
    let entry = listed["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .find(|t| t["name"] == "nika_catalog")
        .expect("nika_catalog is advertised over the wire");
    let description = entry["description"].as_str().expect("a description");
    assert!(
        description.contains("`wired`") && description.contains("wired` is true"),
        "the advertised description must tell an agent WHICH way to filter: {description}",
    );

    let called = mcp_roundtrip(&serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "nika_catalog", "arguments": {} }
    }));
    let payload = called["result"]["content"][0]["text"]
        .as_str()
        .expect("the tool returns text content");
    let value: serde_json::Value =
        serde_json::from_str(payload).expect("the tool's payload is the catalog JSON");
    let rows = value["providers"].as_array().expect("providers array");

    let wire: std::collections::BTreeSet<&str> =
        nika_providers::CANONICAL_IDS.into_iter().collect();
    let mut marked = 0usize;
    for row in rows {
        let id = row["id"].as_str().expect("every row has an id");
        let mark = row["wired"]
            .as_bool()
            .unwrap_or_else(|| panic!("`{id}` carries no `wired` field over MCP: {row}"));
        assert_eq!(
            mark,
            wire.contains(id),
            "`{id}` is advertised to agents as wired={mark} while the wire layer says {}",
            wire.contains(id),
        );
        if mark {
            marked += 1;
        }
    }
    assert!(
        marked > 0 && marked < rows.len(),
        "both sides of the split reach the agent ({marked} of {} marked) — a payload \
         where every row lands on one side is not measuring",
        rows.len(),
    );
}

#[test]
fn a_wired_seat_audits_clean_through_the_same_door() {
    // The negative half of the family probe. Without this, a resolver that
    // refused EVERYTHING would pass the traversal above with a rewritten
    // UNWIRED list.
    let path = plant("anthropic/claude-sonnet-4-5", "wired");
    let out = check::run(&path, false, false, None, PLAIN);
    assert_eq!(out.code, exit::OK, "a wired seat passes: {}", out.text);
    assert!(
        out.text.contains("MODELS"),
        "the rung still reports: {}",
        out.text
    );
}
