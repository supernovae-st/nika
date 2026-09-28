// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

#[test]
fn http_attach_failure_closes_authority_without_activating_arm() {
    let dir = project("attach-failure", HOURLY_A);
    write_workflow(dir.path(), "doctor.nika");
    let state_root = dir.path().join("serve-state");
    let result = nika_serve::serve_resident_process(
        dir.path(),
        state_root.clone(),
        Some(nika_serve::ServerConfig::new(
            "127.0.0.1:0".parse().expect("address"),
            dir.path().join("workflows"),
            dir.path().join("missing.token"),
        )),
    );
    assert!(
        result.is_err_and(|error| error.contains("token file unreadable")),
        "credential refusal must win"
    );
    assert!(
        !dir.path().join(".nika/arm/doctor/history.ndjson").exists(),
        "ARM remains dormant when listener attachment fails"
    );
    nika_serve::JobStore::open(&state_root).expect("authority lock released after attach failure");
}

/// C6 · the door and the ceiling are explicit operator options on the
/// listener: `--cost-review` seats the door and never changes a ceiling;
/// `--run-cost-ceiling` takes a non-negative amount (0 is a binding veto,
/// never a disarm) or exactly `none`; both need `--bind`.
#[test]
fn cost_review_and_run_cost_ceiling_are_explicit_listener_options() {
    let command = || <ServeArgs as clap::Args>::augment_args(clap::Command::new("serve"));
    for alone in [
        &["serve", "--cost-review"][..],
        &["serve", "--run-cost-ceiling", "none"][..],
    ] {
        assert!(
            command().try_get_matches_from(alone).is_err(),
            "{alone:?} needs --bind"
        );
    }
    let parse = |extra: &[&str]| {
        let mut argv = vec!["serve", "--bind", "127.0.0.1:0"];
        argv.extend_from_slice(extra);
        let matches = command().try_get_matches_from(argv).expect("flags");
        <ServeArgs as clap::FromArgMatches>::from_arg_matches(&matches).expect("args")
    };
    let args = parse(&["--cost-review", "--run-cost-ceiling", "none"]);
    assert!(args.cost_review);
    assert_eq!(args.run_cost_ceiling.as_deref(), Some("none"));
    let listener = || {
        Some(nika_serve::ServerConfig::new(
            "127.0.0.1:0".parse().expect("address"),
            "workflows",
            "serve.token",
        ))
    };
    let seated = nika_serve::server::seat_cost_review(listener(), true, None);
    assert!(seated.is_ok_and(|config| config.is_some()));
    for refused in ["-1", "-0.5", "NaN", "inf", "one", "", "None"] {
        let outcome = nika_serve::server::seat_cost_review(listener(), true, Some(refused));
        assert_eq!(
            outcome.err(),
            Some(nika_serve::ServerLaunchRefuse::InvalidRunCostCeiling),
            "{refused}"
        );
    }
    for accepted in ["none", "0", "0.5", "2"] {
        assert!(nika_serve::server::seat_cost_review(listener(), false, Some(accepted)).is_ok());
    }
    assert_eq!(
        nika_serve::server::seat_cost_review(None, true, None).err(),
        Some(nika_serve::ServerLaunchRefuse::MissingBindOrWorkflows)
    );
    assert!(matches!(
        nika_serve::server::seat_cost_review(None, false, None),
        Ok(None)
    ));
}
