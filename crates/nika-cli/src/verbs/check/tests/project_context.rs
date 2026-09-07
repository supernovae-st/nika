// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_cli_host::output::exit;

const WORKFLOW: &str = "nika: value\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  value:\n    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }\n";
const INVALID_PROJECT: &str = "nika: project\nceiling: -1\n";

fn theme() -> Theme {
    Theme::new(false, true, false)
}

fn assert_ambient_refusal(out: &VerbOutput, json: bool, err: &nika_vocab::project::ProjectError) {
    assert_eq!(out.code, exit::ENV, "{}", out.text);
    let path = err.path().expect("discovery binds the project path");
    if json {
        let payload: serde_json::Value = serde_json::from_str(&out.text).expect("project json");
        assert_eq!(payload["report_version"], 1, "{payload}");
        assert_eq!(payload["kind"], "project", "{payload}");
        assert_eq!(payload["file"], path.display().to_string(), "{payload}");
        assert_eq!(payload["clean"], false, "{payload}");
        let findings = payload["findings"].as_array().expect("findings");
        assert_eq!(findings.len(), 1, "{payload}");
        assert_eq!(findings[0]["code"], err.kind().spec_code(), "{payload}");
        assert_eq!(findings[0]["message"], err.to_string(), "{payload}");
        assert!(payload.get("run_budget").is_none(), "{payload}");
        assert!(payload.get("execution_snapshot").is_none(), "{payload}");
    } else {
        assert!(
            out.text.contains(&path.display().to_string()),
            "{}",
            out.text
        );
        assert!(out.text.contains(err.kind().spec_code()), "{}", out.text);
        assert!(out.text.contains(err.detail()), "{}", out.text);
        assert!(out.text.contains(err.remedy()), "{}", out.text);
        if let Some(line) = err.line() {
            assert!(
                out.text.contains(&format!("{}:{line}", path.display())),
                "{}",
                out.text
            );
        }
        assert!(!out.text.contains("BUDGET"), "{}", out.text);
    }
}

#[test]
fn ambient_refusals_survive_json_strict_and_operational_routes() {
    let room = tempfile::tempdir().expect("room");
    std::fs::write(room.path().join("value.nika.yaml"), WORKFLOW).expect("workflow");
    let _cwd = crate::cwd::enter(room.path()).expect("cwd lease");
    for yaml in [
        "nika: project\nceiling: 0\n",
        INVALID_PROJECT,
        "nika: project\nceiling: \"0.01\"\n",
        "nika: project\nceiling: [\n",
    ] {
        std::fs::write("nika.yaml", yaml).expect("invalid project");
        let err = nika_vocab::project::discover_from_cwd().expect_err("project refusal");
        for json in [false, true] {
            for strict in [false, true] {
                for profile in [Profile::Advisory, Profile::Operational] {
                    let out = run_with_profile(
                        "value.nika.yaml",
                        json,
                        strict,
                        profile,
                        (None, None),
                        theme(),
                    );
                    assert_ambient_refusal(&out, json, &err);
                }
            }
        }
    }
}

#[test]
fn an_unreadable_ambient_project_names_the_project_in_both_routes() {
    let room = tempfile::tempdir().expect("room");
    std::fs::write(room.path().join("value.nika.yaml"), WORKFLOW).expect("workflow");
    std::fs::create_dir(room.path().join("nika.yaml")).expect("unreadable project");
    let _cwd = crate::cwd::enter(room.path()).expect("cwd lease");
    let err = nika_vocab::project::discover_from_cwd().expect_err("project refusal");
    assert_eq!(err.kind().spec_code(), "project.unreadable");
    for json in [false, true] {
        assert_ambient_refusal(
            &run("value.nika.yaml", json, false, None, theme()),
            json,
            &err,
        );
    }
}

#[test]
fn scaffold_slot_admission_cannot_promote_an_ambient_refusal() {
    let room = tempfile::tempdir().expect("room");
    std::fs::write(
        room.path().join("draft.nika.yaml"),
        "nika: draft\nmodel: mock/echo\ntasks:\n  think:\n    infer:\n      prompt: \"<SLOT: the one model job>\"\n      max_tokens: 10\n",
    )
    .expect("unfilled scaffold");
    std::fs::write(room.path().join("nika.yaml"), "nika: project\n").expect("valid boundary");
    let _cwd = crate::cwd::enter(room.path()).expect("cwd lease");
    let ordinary = run("draft.nika.yaml", false, false, None, theme());
    assert_eq!(ordinary.code, exit::FILE, "{}", ordinary.text);
    let scaffold = run_scaffold("draft.nika.yaml", theme());
    assert_eq!(
        scaffold.code,
        exit::OK,
        "slot-only control: {}",
        scaffold.text
    );
    std::fs::write("nika.yaml", INVALID_PROJECT).expect("invalid project");
    let err = nika_vocab::project::discover_from_cwd().expect_err("project refusal");
    assert_ambient_refusal(&run_scaffold("draft.nika.yaml", theme()), false, &err);
}

#[test]
fn admitted_pair_preserves_the_ambient_refusal_for_its_callers() {
    let room = tempfile::tempdir().expect("room");
    std::fs::write(room.path().join("nika.yaml"), INVALID_PROJECT).expect("invalid project");
    let _cwd = crate::cwd::enter(room.path()).expect("cwd lease");
    let wf = parse_wf(WORKFLOW);
    let report = nika_check::check(&wf);
    let skills = crate::verbs::resolve_workflow_skills(&wf, room.path());
    let target = CheckTarget::workspace("value.nika.yaml");
    let err = nika_vocab::project::discover_from_cwd().expect_err("project refusal");
    for json in [false, true] {
        let out = run_admitted_pair(
            WORKFLOW,
            &target.path,
            target.repair_target,
            &wf,
            &report,
            &skills,
            json,
            theme(),
        );
        assert_ambient_refusal(&out, json, &err);
    }
}

#[test]
fn snapshot_export_returns_the_ambient_refusal_without_snapshot_bytes() {
    let room = tempfile::tempdir().expect("room");
    std::fs::write(room.path().join("value.nika.yaml"), WORKFLOW).expect("workflow");
    std::fs::write(room.path().join("nika.yaml"), "nika: project\n").expect("valid boundary");
    let _cwd = crate::cwd::enter(room.path()).expect("cwd lease");
    let control = run_snapshot_export("value.nika.yaml", theme());
    assert_eq!(control.code, exit::OK, "{}", control.text);
    let payload: serde_json::Value = serde_json::from_str(&control.text).expect("snapshot json");
    assert!(payload["execution_snapshot"].is_string(), "{payload}");
    std::fs::write("nika.yaml", INVALID_PROJECT).expect("invalid project");
    let err = nika_vocab::project::discover_from_cwd().expect_err("project refusal");
    assert_ambient_refusal(&run_snapshot_export("value.nika.yaml", theme()), true, &err);
}

#[test]
fn multifile_check_keeps_the_ambient_environment_exit() {
    let room = tempfile::tempdir().expect("room");
    for name in ["a.nika.yaml", "b.nika.yaml"] {
        std::fs::write(room.path().join(name), WORKFLOW).expect("workflow");
    }
    std::fs::write(room.path().join("nika.yaml"), INVALID_PROJECT).expect("invalid project");
    let _cwd = crate::cwd::enter(room.path()).expect("cwd lease");
    let paths = ["a.nika.yaml".to_owned(), "b.nika.yaml".to_owned()];
    let out = run_many(&paths, true, Profile::Operational, None, theme());
    assert_eq!(out.code, exit::ENV, "{}", out.text);
    assert_eq!(
        out.text.matches("project.bad-value").count(),
        2,
        "{}",
        out.text
    );
}

#[test]
fn valid_ambient_budget_and_bare_boundary_keep_their_existing_projections() {
    let room = tempfile::tempdir().expect("room");
    let child = room.path().join("child");
    std::fs::create_dir(&child).expect("child");
    std::fs::write(room.path().join("nika.yaml"), "nika: root\nceiling: 0.50\n").expect("ancestor");
    std::fs::write(child.join("value.nika.yaml"), WORKFLOW).expect("workflow");
    let _cwd = crate::cwd::enter(&child).expect("cwd lease");
    for (yaml, amount) in [
        ("nika: child\nceiling: 0.01\n", Some(0.01)),
        ("nika: child\n", None),
    ] {
        std::fs::write("nika.yaml", yaml).expect("valid project");
        for profile in [Profile::Advisory, Profile::Operational] {
            let human = run_with_profile(
                "value.nika.yaml",
                false,
                true,
                profile,
                (None, None),
                theme(),
            );
            assert_eq!(human.code, exit::OK, "{}", human.text);
            assert_eq!(
                human.text.contains("BUDGET"),
                amount.is_some(),
                "{}",
                human.text
            );
            let out = run_with_profile(
                "value.nika.yaml",
                true,
                true,
                profile,
                (None, None),
                theme(),
            );
            assert_eq!(out.code, exit::OK, "{}", out.text);
            let payload: serde_json::Value = serde_json::from_str(&out.text).expect("check json");
            assert_eq!(payload["clean"], true, "{payload}");
            if let Some(amount) = amount {
                assert_eq!(payload["run_budget"]["max_cost_usd"], amount, "{payload}");
                assert_eq!(payload["run_budget"]["line"], 2, "{payload}");
                let path = std::env::current_dir().expect("cwd").join("nika.yaml");
                assert_eq!(payload["run_budget"]["source"], path.display().to_string());
                assert_eq!(payload["run_budget"]["via"], "project", "{payload}");
            } else {
                assert!(payload.get("run_budget").is_none(), "{payload}");
            }
        }
    }
}

#[test]
fn direct_project_refusal_keeps_its_file_exit_and_existing_message() {
    let room = tempfile::tempdir().expect("room");
    std::fs::write(room.path().join("nika.yaml"), INVALID_PROJECT).expect("invalid project");
    let _cwd = crate::cwd::enter(room.path()).expect("cwd lease");
    let err = nika_vocab::project::parse(INVALID_PROJECT).expect_err("parser refusal");
    for json in [false, true] {
        let out = run("nika.yaml", json, false, None, theme());
        assert_eq!(out.code, exit::FILE, "{}", out.text);
        if json {
            let payload: serde_json::Value = serde_json::from_str(&out.text).expect("project json");
            assert_eq!(payload["file"], "nika.yaml", "{payload}");
            assert_eq!(payload["kind"], "project", "{payload}");
            assert_eq!(payload["clean"], false, "{payload}");
            assert_eq!(payload["findings"][0]["code"], err.kind().spec_code());
            assert_eq!(payload["findings"][0]["message"], err.detail());
        } else {
            assert_eq!(
                out.text,
                nika_display::project_render::refusal(
                    "nika.yaml",
                    err.line(),
                    err.kind().spec_code(),
                    err.detail(),
                    err.remedy(),
                )
            );
        }
    }
}

#[test]
fn an_ambient_path_with_control_characters_stays_machine_readable() {
    let room = tempfile::tempdir().expect("room");
    let child = room.path().join("project\u{1b}context");
    std::fs::create_dir(&child).expect("project directory");
    std::fs::write(child.join("value.nika.yaml"), WORKFLOW).expect("workflow");
    std::fs::write(child.join("nika.yaml"), INVALID_PROJECT).expect("invalid project");
    let _cwd = crate::cwd::enter(&child).expect("cwd lease");
    let err = nika_vocab::project::discover_from_cwd().expect_err("project refusal");
    assert_ambient_refusal(
        &run("value.nika.yaml", true, false, None, theme()),
        true,
        &err,
    );
    assert_ambient_refusal(&run_snapshot_export("value.nika.yaml", theme()), true, &err);
}
