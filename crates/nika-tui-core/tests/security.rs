// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::float_cmp)] // the parity proof compares f64 BITS — strict equality is its whole point

//! The gate-11 regression proofs — each one exists because the review
//! swarm named a class. If the iterative wave walk ever becomes recursion
//! again, the long-chain test does not fail: it dies. That is the point.

use nika_tui_core::model::{Run, Workflow};
use nika_tui_core::wasm;

/// A 5 000-task chain — the recursion that preceded the iterative walk
/// would trap the 1 MiB wasm stack here (the sibling crate's jq-bomb
/// class). The walk is iterative now: this must simply answer.
#[test]
fn a_five_thousand_task_chain_folds_without_a_trap() {
    let mut tasks = Vec::new();
    for i in 0..5000 {
        tasks.push(serde_json::json!({
            "id": format!("t{i}"),
            "verb": "infer",
            "glyph": "◇",
            "needs": if i == 0 { vec![] } else { vec![format!("t{}", i - 1)] },
        }));
    }
    let wf = serde_json::json!({
        "file": "chain.nika.yaml", "engine": "test", "prompt": "",
        "permits": [], "missing": "", "tasks": tasks,
    });
    let run: Run = serde_json::from_value(serde_json::json!({
        "trace": "t", "when": "recorded", "output": "", "steps": [],
    }))
    .expect("run");
    let wf_typed: Workflow = serde_json::from_value(wf.clone()).expect("wf");
    let ws = nika_tui_core::derive::waves(&wf_typed);
    assert_eq!(ws.len(), 5000, "each link its own wave");
    assert_eq!(ws.of("t4999"), 4999);

    // and through the door, the same answer
    let out = wasm::derive_run(
        &wf.to_string(),
        &serde_json::to_string(&run).expect("run json"),
    );
    let got: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(got["waves"].as_array().expect("waves").len(), 5000);
}

/// The input ceiling is named, not crashed: a door refuses an over-size
/// string with its own name in the message.
#[test]
fn the_input_ceiling_is_a_named_refusal() {
    let big = " ".repeat(65 * 1024 * 1024);
    let out = wasm::fold_journal(&big);
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert!(
        v["error"]
            .as_str()
            .is_some_and(|m| m.contains("fold_journal")),
        "the refusal names its door"
    );
}

/// The embedding escape: a hostile `</script>` in a task id comes out
/// `\u003c` — the class is removed at the source.
#[test]
fn hostile_markup_is_escaped_at_the_source() {
    let wf = serde_json::json!({
        "file": "x.nika.yaml", "engine": "test", "prompt": "", "permits": [], "missing": "",
        "tasks": [{ "id": "a</script><script>b", "verb": "infer", "glyph": "◇", "needs": [] }],
    });
    let run = serde_json::json!({"trace": "t", "when": "recorded", "output": "", "steps": []});
    let out = wasm::derive_run(&wf.to_string(), &run.to_string());
    assert!(
        !out.contains("</script>"),
        "raw markup never leaves a door: {out}"
    );
    assert!(out.contains("\\u003c"), "the five-char rewrite rode");
}

/// EACH of the five, proven on its own.
///
/// Gate 5, 2026-08-13. The test above passed while FOUR of the five match
/// arms could be deleted: both of its assertions are satisfied by escaping
/// `<` alone. `!contains("</script>")` is true the moment the `<` is gone,
/// and `contains("<")` names only that arm. The other four rode for
/// free, so `>`, `&`, U+2028 and U+2029 had no proof at all.
///
/// This is the security half of the crate, not a coverage statistic: the
/// escape exists because a page that inlines this JSON is one `</script>`
/// away from an element break, and U+2028/9 break a JS parse outright. So
/// each character is now asserted twice, present as its escape and absent
/// raw, and one dropped arm turns exactly one assertion red.
#[test]
fn every_one_of_the_five_embedding_escapes_is_proven_alone() {
    for (raw, escaped, why) in [
        ('<', "\\u003c", "ouvre une balise"),
        (
            '>',
            "\\u003e",
            "ferme une balise · </script> a besoin des deux",
        ),
        ('&', "\\u0026", "ouvre une entité HTML"),
        (
            '\u{2028}',
            "\\u2028",
            "séparateur de ligne · casse un parse JS",
        ),
        ('\u{2029}', "\\u2029", "séparateur de paragraphe · idem"),
    ] {
        // The character rides inside a task id, the caller-controlled byte
        // path this escape exists to cover.
        let wf = serde_json::json!({
            "file": "x.nika.yaml", "engine": "test", "prompt": "", "permits": [], "missing": "",
            "tasks": [{
                "id": format!("a{raw}b"),
                "verb": "infer", "glyph": "◇", "needs": [],
            }],
        });
        let run = serde_json::json!({"trace": "t", "when": "recorded", "output": "", "steps": []});
        let out = wasm::derive_run(&wf.to_string(), &run.to_string());

        assert!(
            out.contains(escaped),
            "{raw:?} ({why}) doit sortir en {escaped} · sortie: {out}"
        );
        assert!(
            !out.contains(raw),
            "{raw:?} ({why}) ne doit JAMAIS sortir brut · sortie: {out}"
        );
    }
}

/// The ceiling is 64 MiB, and the arithmetic that says so is pinned.
///
/// Gate 5, 2026-08-13. `64 * 1024 * 1024` survived a flip to `+`, which
/// drops the ceiling to about 1 MiB. The existing ceiling test uses 65 MiB
/// and is refused under BOTH readings, so it proved the refusal exists and
/// never proved where the line sits. A 2 MiB input is comfortably legal at
/// 64 MiB and refused at 1 MiB, so it separates them.
///
/// It is asserted by ABSENCE: 2 MiB of spaces is not a journal either, so
/// the fold still refuses. What must not appear is the CEILING refusal.
#[test]
fn a_two_mebibyte_input_is_under_the_ceiling() {
    let two_mib = " ".repeat(2 * 1024 * 1024);
    let out = wasm::fold_journal(&two_mib);
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    let msg = v["error"].as_str().unwrap_or_default();
    assert!(
        !msg.contains("ceiling"),
        "2 MiB est SOUS le plafond de 64 MiB · un `+` à la place du `*` le \
         descendrait à ~1 MiB et refuserait ici · message: {msg}"
    );
}

/// A board at `u32::MAX` stays at MAX — a wrap would renumber history.
#[test]
fn the_revision_never_wraps() {
    let board = serde_json::json!({
        "rev": u32::MAX, "slots": ["a"], "marks": ["+"], "prints": {"a": "p"},
    });
    let graph = r#"{"graph_format":3,"workflow":"t","nodes":[],"edges":[]}"#;
    let out = wasm::board_next(&board.to_string(), graph);
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(v["rev"].as_u64(), Some(u64::from(u32::MAX)));
}

/// The version door answers the crate's own version (a consumer pins it).
#[test]
fn the_version_door_answers() {
    assert_eq!(wasm::engine_version(), env!("CARGO_PKG_VERSION"));
}

/// The complexity gate — a WALL CLOCK would measure the machine; a RATIO
/// measures the algorithm. Both legs run in the same process on the same
/// core, so the hardware cancels out.
///
/// `waves()` used to rebuild its memo per root, so an n-link chain walked
/// n times. Measured on the pinned 5 000-chain: 14.35 s → 1.62 s release
/// (`cargo test -p nika-tui-core --release --test security a_five_thousand`).
/// The correctness test above would NOT have caught that: it passed the
/// whole time, just slowly. This one is the guard the fix needed.
///
/// Quadruple the input · linear answers ~4× · quadratic answers ~16×.
/// The ceiling sits at 12: far above the honest answer, far below the trap
/// (was 8 · GHA 2026-08-22 measured 8.4× under load).
#[test]
fn the_wave_walk_scales_with_the_chain_not_its_square() {
    fn chain(n: usize) -> Workflow {
        let tasks: Vec<_> = (0..n)
            .map(|i| {
                serde_json::json!({
                    "id": format!("t{i}"), "verb": "infer", "glyph": "◇",
                    "needs": if i == 0 { vec![] } else { vec![format!("t{}", i - 1)] },
                })
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "file": "c.nika.yaml", "engine": "test", "prompt": "",
            "permits": [], "missing": "", "tasks": tasks,
        }))
        .expect("wf")
    }

    // The MINIMUM of several runs, never the mean: a scheduler steals time,
    // it never gives any back, so the floor is the honest measurement.
    fn floor_of(wf: &Workflow) -> std::time::Duration {
        (0..5)
            .map(|_| {
                let t0 = std::time::Instant::now();
                let w = nika_tui_core::derive::waves(wf);
                assert_eq!(w.len(), wf.tasks.len(), "the walk must answer, not skip");
                t0.elapsed()
            })
            .min()
            .expect("five runs")
    }

    let small = chain(2_000);
    let large = chain(8_000); // ×4

    let t_small = floor_of(&small);
    let t_large = floor_of(&large);

    // Guard the guard: too fast to time means the ratio is noise, not signal.
    assert!(
        t_small.as_micros() >= 50,
        "the small leg ran in {t_small:?} — too fast to compare; raise the sizes"
    );

    let ratio = t_large.as_secs_f64() / t_small.as_secs_f64();
    // 8 was "2× linear". GHA rust (tests) 2026-08-22 measured 8.4× on a
    // loaded runner (12.6ms → 105.9ms) — still far from quadratic 16×,
    // but enough to fail the stall-fix PR. 12 keeps the trap and admits
    // scheduler noise.
    assert!(
        ratio < 12.0,
        "×4 the chain cost ×{ratio:.1} the time ({t_small:?} → {t_large:?}) — \
         linear costs ~4×, quadratic ~16×. The per-root memo is back."
    );
}

/// The ceiling's EXACT boundary — a mutation sweep found `>` → `>=` alive
/// in `admit`: no test said whether 64 MiB itself is admitted or refused.
/// The message says «over the 64 MiB ceiling», so exactly 64 MiB is UNDER
/// it and passes. Pinning it makes the boundary a decision instead of an
/// accident, and kills the survivor.
#[test]
fn the_ceiling_admits_exactly_its_limit_and_refuses_one_byte_more() {
    const CEILING: usize = 64 * 1024 * 1024;

    // A workflow that is not JSON still gets PAST the size pre-flight —
    // the refusal names the parse, not the ceiling. That distinction is
    // the whole assertion: `admit` ran and let it through.
    let at = "x".repeat(CEILING);
    let out = wasm::derive_run(&at, r#"{"trace":"t","when":"w","output":"","steps":[]}"#);
    assert!(
        !out.contains("ceiling"),
        "exactly 64 MiB is not OVER the ceiling · it must reach the parser"
    );

    let over = "x".repeat(CEILING + 1);
    let out = wasm::derive_run(&over, r#"{"trace":"t","when":"w","output":"","steps":[]}"#);
    assert!(
        out.contains("ceiling"),
        "one byte more must be refused BY THE CEILING, named as such"
    );
}

/// The SISTER complexity gate — `groups_of`, same law, own measurement.
///
/// The first repair hoisted the `format!` out of the inner scan and the
/// comment claimed the quadratic was gone. It was not: only the ALLOCATIONS
/// left. Every task with its own signature still triggered a full n-scan,
/// so an n-chain stayed O(n²) — a review measured 6.6 s at n=10 000 while
/// the correctness test above sat green, because it asserts no bound.
///
/// One gate per function, because one gate covered only `waves()` and that
/// is exactly how the second quadratic survived the first repair.
#[test]
fn the_grouping_scales_with_the_tasks_not_their_square() {
    fn chain(n: usize) -> Workflow {
        let tasks: Vec<_> = (0..n)
            .map(|i| {
                serde_json::json!({
                    "id": format!("t{i}"), "verb": "infer", "glyph": "◇",
                    "needs": if i == 0 { vec![] } else { vec![format!("t{}", i - 1)] },
                })
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "file": "c.nika.yaml", "engine": "test", "prompt": "",
            "permits": [], "missing": "", "tasks": tasks,
        }))
        .expect("wf")
    }

    fn floor_of(wf: &Workflow) -> std::time::Duration {
        (0..5)
            .map(|_| {
                let t0 = std::time::Instant::now();
                let g = nika_tui_core::derive::groups_of(wf);
                assert_eq!(
                    g.len(),
                    wf.tasks.len(),
                    "a chain groups nothing — each alone"
                );
                t0.elapsed()
            })
            .min()
            .expect("five runs")
    }

    let small = chain(2_000);
    let large = chain(8_000); // ×4

    let t_small = floor_of(&small);
    let t_large = floor_of(&large);
    assert!(
        t_small.as_micros() >= 50,
        "the small leg ran in {t_small:?} — too fast to compare; raise the sizes"
    );

    let ratio = t_large.as_secs_f64() / t_small.as_secs_f64();
    assert!(
        ratio < 8.0,
        "×4 the tasks cost ×{ratio:.1} the time ({t_small:?} → {t_large:?}) — \
         linear costs ~4×, quadratic ~16×. The per-task full scan is back."
    );
}

/// The THIRD complexity gate — the door end-to-end, WITH steps.
///
/// The 5 000-chain test above passes `steps: []`, so the per-step half of
/// `derive_run` was never exercised: a Gate-11 lens measured 80 ms in
/// `wave_end` (once per wave, each scanning every step), 99 ms in `idle_of`
/// (once per step, each rebuilding a wave and scanning again) and 18 ms in
/// `cost_by_verb` (a `find` per step) at n=5000 — all invisible.
///
/// An empty `steps` array is a green on a half nobody named. This gate runs
/// the real shape: as many recorded steps as tasks.
#[test]
fn the_door_scales_with_the_run_not_its_square() {
    fn pair(n: usize) -> (String, String) {
        let tasks: Vec<_> = (0..n)
            .map(|i| {
                serde_json::json!({
                    "id": format!("t{i}"), "verb": "infer", "glyph": "◇",
                    "needs": if i == 0 { vec![] } else { vec![format!("t{}", i - 1)] },
                })
            })
            .collect();
        let steps: Vec<_> = (0..n)
            .map(|i| {
                serde_json::json!({
                    "id": format!("t{i}"),
                    // u32 → f64 is exact; the cast lint is right that usize
                    // is not, and the sizes here never leave u32's range
                    "start": f64::from(u32::try_from(i).unwrap_or(u32::MAX)),
                    "dur": 1.0, "cost": 0.001,
                })
            })
            .collect();
        (
            serde_json::json!({
                "file": "d.nika.yaml", "engine": "test", "prompt": "",
                "permits": [], "missing": "", "tasks": tasks,
            })
            .to_string(),
            serde_json::json!({
                "trace": "t", "when": "recorded", "output": "", "steps": steps,
            })
            .to_string(),
        )
    }

    fn floor_of(wf: &str, run: &str) -> std::time::Duration {
        (0..3)
            .map(|_| {
                let t0 = std::time::Instant::now();
                let out = wasm::derive_run(wf, run);
                assert!(
                    !out.contains("\"error\""),
                    "the door must answer, not refuse"
                );
                t0.elapsed()
            })
            .min()
            .expect("three runs")
    }

    let (wf_s, run_s) = pair(1_000);
    let (wf_l, run_l) = pair(4_000); // ×4

    let t_small = floor_of(&wf_s, &run_s);
    let t_large = floor_of(&wf_l, &run_l);
    assert!(
        t_small.as_micros() >= 50,
        "the small leg ran in {t_small:?} — too fast to compare; raise the sizes"
    );

    let ratio = t_large.as_secs_f64() / t_small.as_secs_f64();
    assert!(
        ratio < 8.0,
        "×4 the run cost ×{ratio:.1} the time ({t_small:?} → {t_large:?}) — \
         linear costs ~4×, quadratic ~16×. A per-step scan is back on the door."
    );
}
