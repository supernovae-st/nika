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

use nika_cli::Theme;
use nika_cli::verbs::{check, exit};

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

#[test]
fn every_cataloged_provider_either_seats_or_is_refused_by_name() {
    let export = nika_catalog::export::catalog_export();
    let wired: std::collections::BTreeSet<&str> =
        nika_providers::CANONICAL_IDS.into_iter().collect();

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
