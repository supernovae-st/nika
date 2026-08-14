// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::float_cmp)] // the parity proof compares f64 BITS — strict equality is its whole point

//! The parity proof (T28 · D-2026-08-11-N6): the studio's derivation
//! fixtures, replayed against the crate. Every number must land on the
//! same BITS — a divergence is a bug in this crate, never a "platform
//! difference" (that is why the law moved).
//!
//! Fixtures are copied from the studio's `gen-parity` output at authoring;
//! the sync mechanism (who re-copies, when) is an admission-time question
//! (the crate-spec §4).

use nika_tui_core::derive;
use nika_tui_core::model::{Group, Run, Touch, Workflow};

/// One pinned fixture: the two truths + every derived value expected.
struct Fixture {
    name: String,
    workflow: Workflow,
    run: Run,
    derived: serde_json::Value,
}

fn load(file: &str) -> Fixture {
    let raw = std::fs::read_to_string(format!("tests/fixtures/{file}"))
        .unwrap_or_else(|e| panic!("fixture {file}: {e}"));
    let v: serde_json::Value = serde_json::from_str(&raw).expect("fixture parses");
    Fixture {
        name: v["name"].as_str().expect("name").to_owned(),
        workflow: serde_json::from_value(v["workflow"].clone()).expect("workflow"),
        run: serde_json::from_value(v["run"].clone()).expect("run"),
        derived: v["derived"].clone(),
    }
}

/// Every fixture, one sweep — the failure names the case AND the field.
#[test]
fn the_derivation_lands_on_the_same_bits() {
    for file in [
        "demo-ok.json",
        "demo-fail.json",
        "stress.json",
        "sonde-single.json",
        "sonde-neverborn.json",
        "sonde-diamant.json",
    ] {
        let f = load(file);
        check_case(&f);
        check_vocabulary(&f);
    }
}

fn check_case(f: &Fixture) {
    let (wf, run, want) = (&f.workflow, &f.run, &f.derived);
    let case = f.name.as_str();

    // waves — the ids, grouped, in declared order.
    let got_waves_struct = derive::waves(wf);
    let got_waves_vec = got_waves_struct.groups();
    let got_waves: Vec<Vec<&str>> = got_waves_vec
        .iter()
        .map(|g| g.iter().map(|t| t.id.as_str()).collect())
        .collect();
    let want_waves: Vec<Vec<&str>> = want["waves"]
        .as_array()
        .expect("waves")
        .iter()
        .map(|g| {
            g.as_array()
                .expect("group")
                .iter()
                .map(|t| t.as_str().expect("id"))
                .collect()
        })
        .collect();
    assert_eq!(got_waves, want_waves, "{case} · waves");

    // wave_end — one number per wave.
    let got_ends: Vec<f64> = (0..got_waves.len())
        .map(|w| derive::wave_end(&got_waves_struct, run, w))
        .collect();
    let want_ends: Vec<f64> = want["wave_end"]
        .as_array()
        .expect("wave_end")
        .iter()
        .map(|x| x.as_f64().expect("f64"))
        .collect();
    assert_eq!(got_ends, want_ends, "{case} · wave_end");

    // idle — per step id.
    let want_idle = want["idle"].as_object().expect("idle");
    for s in &run.steps {
        let want_i = want_idle[&s.id].as_f64().expect("idle f64");
        assert_eq!(
            derive::idle_of(&got_waves_struct, run, &s.id),
            want_i,
            "{case} · idle · {}",
            s.id
        );
    }

    // bottleneck — the holder, its summed idle, its blocked count.
    match (
        &want["bottleneck"],
        derive::bottleneck(&got_waves_struct, run),
    ) {
        (serde_json::Value::Null, None) => {}
        (w, Some(got)) => {
            assert_eq!(
                got.id,
                w["id"].as_str().expect("neck id"),
                "{case} · bottleneck id"
            );
            assert_eq!(
                got.idle_total,
                w["idleTotal"].as_f64().expect("idleTotal"),
                "{case} · bottleneck idleTotal"
            );
            assert_eq!(
                got.blocked,
                usize::try_from(w["blocked"].as_u64().expect("blocked")).expect("a count fits"),
                "{case} · bottleneck blocked"
            );
        }
        (w, got) => panic!("{case} · bottleneck · want {w:?} · got {got:?}"),
    }

    // totals.
    assert_eq!(
        derive::total_cost(run),
        want["total_cost"].as_f64().expect("total_cost"),
        "{case} · total_cost"
    );
    assert_eq!(
        derive::total_time(run),
        want["total_time"].as_f64().expect("total_time"),
        "{case} · total_time"
    );
    assert_eq!(
        derive::has_failed(run),
        want["has_failed"].as_bool().expect("has_failed"),
        "{case} · has_failed"
    );
}

/// The vocabulary half — verbs · costs · touches · groups (split from the
/// timing half: the 100-line law holds in tests too).
fn check_vocabulary(f: &Fixture) {
    let (wf, run, want) = (&f.workflow, &f.run, &f.derived);
    let case = f.name.as_str();

    // verbs used — declared order, deduplicated.
    let got_verbs: Vec<&str> = derive::verbs_used(wf)
        .iter()
        .map(|v| match v {
            nika_tui_core::model::Verb::Infer => "infer",
            nika_tui_core::model::Verb::Exec => "exec",
            nika_tui_core::model::Verb::Invoke => "invoke",
            nika_tui_core::model::Verb::Agent => "agent",
        })
        .collect();
    let want_verbs: Vec<&str> = want["verbs_used"]
        .as_array()
        .expect("verbs_used")
        .iter()
        .map(|v| v.as_str().expect("verb"))
        .collect();
    assert_eq!(got_verbs, want_verbs, "{case} · verbs_used");

    // cost per verb — the four-key object.
    let got_costs = derive::cost_by_verb(wf, run);
    let want_costs = want["cost_by_verb"].as_object().expect("cost_by_verb");
    for (verb, sum) in got_costs {
        assert_eq!(
            sum,
            want_costs[verb].as_f64().expect("cost f64"),
            "{case} · cost_by_verb · {verb}"
        );
    }

    // undeclared + blast radius — touch spellings ride the enum.
    let spell = |c: &Touch| match c {
        Touch::FsRead => "fs_read",
        Touch::FsWrite => "fs_write",
        Touch::Net => "net",
        Touch::Exec => "exec",
        Touch::Tools => "tools",
        _ => "unknown", // non_exhaustive — a future class spells itself when it lands
    };
    let got_und: Vec<&str> = derive::undeclared(wf).iter().map(spell).collect();
    let want_und: Vec<&str> = want["undeclared"]
        .as_array()
        .expect("undeclared")
        .iter()
        .map(|v| v.as_str().expect("touch"))
        .collect();
    assert_eq!(got_und, want_und, "{case} · undeclared");
    let got_bl: Vec<&str> = derive::blast_radius(wf).iter().map(spell).collect();
    let want_bl: Vec<&str> = want["blast_radius"]
        .as_array()
        .expect("blast_radius")
        .iter()
        .map(|v| v.as_str().expect("touch"))
        .collect();
    assert_eq!(got_bl, want_bl, "{case} · blast_radius");

    // groups — the fan-out projection: {task} or {group, members}.
    let got_groups: Vec<serde_json::Value> = derive::groups_of(wf)
        .iter()
        .map(|g| match g {
            Group::Single(t) => serde_json::json!({ "task": t.id }),
            Group::Fanout { name, members } => serde_json::json!({
                "group": name,
                "members": members.iter().map(|m| &m.id).collect::<Vec<_>>(),
            }),
            _ => serde_json::json!(null), // non_exhaustive — a future variant is visible, not silent
        })
        .collect();
    assert_eq!(
        got_groups,
        *want["groups"].as_array().expect("groups"),
        "{case} · groups"
    );
}
