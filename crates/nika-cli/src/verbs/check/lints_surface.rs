// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! #763 — one-obvious-way lints on `check --json` `hints[]`.
//! Sibling of `tests.rs` (that file sits on the 1500-LOC wall).

use super::*;

/// One-obvious-way lints ride `check --json` `hints[]` (warnings, never
/// errors). A skip-for-dependents file names the rule; infer-only does
/// not grow the kind.
#[test]
fn json_exposes_one_obvious_way_hints_as_warnings() {
    let theme = Theme::new(false, true, false);
    let dir = std::env::temp_dir().join(format!("nika-cli-oow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp");
    let run_json = |name: &str, yaml: &str| {
        let path = dir.join(name);
        std::fs::write(&path, yaml).expect("write");
        run(path.to_str().expect("utf8"), true, false, None, theme)
    };

    let dirty = run_json(
        "d.nika.yaml",
        "nika: skip\npermits: { exec: [\"true\"] }\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n    on_error: { skip: true }\n  b:\n    with: { data: \"${{ tasks.a.output }}\" }\n    exec: { command: [\"true\", \"${{ with.data }}\"] }\n",
    );
    assert_eq!(dirty.code, 0, "{}", dirty.text);
    let payload: serde_json::Value = serde_json::from_str(&dirty.text).expect("json");
    assert_eq!(payload["clean"], true, "{payload:#}");
    let hit = payload["hints"]
        .as_array()
        .expect("hints")
        .iter()
        .find(|h| h["kind"] == "one-obvious-way")
        .expect("one-obvious-way hint");
    assert_eq!(hit["code"], "one-obvious-way/002", "{payload:#}");
    assert_eq!(hit["task"], "a", "{payload:#}");
    assert!(
        hit["advice"]
            .as_str()
            .expect("advice")
            .starts_with("one-obvious-way/002"),
        "{payload:#}"
    );

    let clean = run_json(
        "c.nika.yaml",
        "nika: ok\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
    );
    assert_eq!(clean.code, 0, "{}", clean.text);
    let clean_payload: serde_json::Value = serde_json::from_str(&clean.text).expect("json");
    assert!(
        clean_payload["hints"]
            .as_array()
            .expect("hints")
            .iter()
            .all(|h| h["kind"] != "one-obvious-way"),
        "{clean_payload:#}"
    );
}
