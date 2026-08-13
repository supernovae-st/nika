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

/// A board at `u32::MAX` stays at MAX — a wrap would renumber history.
#[test]
fn the_revision_never_wraps() {
    let board = serde_json::json!({
        "rev": u32::MAX, "slots": ["a"], "marks": ["+"], "prints": {"a": "p"},
    });
    let graph = r#"{"graph_format":2,"workflow":"t","nodes":[],"edges":[]}"#;
    let out = wasm::board_next(&board.to_string(), graph);
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(v["rev"].as_u64(), Some(u64::from(u32::MAX)));
}

/// The version door answers the crate's own version (a consumer pins it).
#[test]
fn the_version_door_answers() {
    assert_eq!(wasm::engine_version(), env!("CARGO_PKG_VERSION"));
}
