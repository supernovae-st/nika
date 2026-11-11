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

/// `--sessions` is an explicit listener option: it needs `--bind`, and only it seats the
/// Session routes.
#[test]
fn sessions_is_an_explicit_listener_option() {
    let command = || <ServeArgs as clap::Args>::augment_args(clap::Command::new("serve"));
    assert!(
        command()
            .try_get_matches_from(["serve", "--sessions"])
            .is_err(),
        "--sessions needs --bind"
    );
    let parse = |argv: &[&str]| {
        let matches = command().try_get_matches_from(argv).expect("flags");
        <ServeArgs as clap::FromArgMatches>::from_arg_matches(&matches).expect("args")
    };
    assert!(parse(&["serve", "--bind", "127.0.0.1:0", "--sessions"]).sessions);
    assert!(!parse(&["serve", "--bind", "127.0.0.1:0"]).sessions);
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

/// `--cost-policy` is an explicit, repeatable listener option: it needs
/// `--bind`, each occurrence is one rule kept as written, and it never
/// changes the ceiling or seats the review door. The launch reads every rule
/// and refuses one it cannot hold, so a server that starts holds the policy
/// its operator wrote.
#[test]
fn cost_policy_is_an_explicit_repeatable_listener_option() {
    let command = || <ServeArgs as clap::Args>::augment_args(clap::Command::new("serve"));
    let rule = "deepseek/c6-unpriced-fixture:requests=4,output-tokens=64,timeout=30,runs=3/hour";
    assert!(
        command()
            .try_get_matches_from(["serve", "--cost-policy", rule])
            .is_err(),
        "--cost-policy needs --bind"
    );
    let parse = |extra: &[&str]| {
        let mut argv = vec!["serve", "--bind", "127.0.0.1:0"];
        argv.extend_from_slice(extra);
        let matches = command().try_get_matches_from(argv).expect("flags");
        <ServeArgs as clap::FromArgMatches>::from_arg_matches(&matches).expect("args")
    };
    let none = parse(&[]);
    assert!(none.cost_policy.is_empty(), "off unless asked");
    let other = rule.replace("c6-unpriced-fixture", "another-unpriced-fixture");
    let args = parse(&["--cost-policy", rule, "--cost-policy", &other]);
    assert_eq!(args.cost_policy, [rule.to_owned(), other.clone()]);
    assert!(!args.cost_review && args.run_cost_ceiling.is_none());
    let listener = || {
        Some(nika_serve::ServerConfig::new(
            "127.0.0.1:0".parse().expect("address"),
            "workflows",
            "serve.token",
        ))
    };
    let seated = nika_serve::server::seat_cost_policy(listener(), &args.cost_policy);
    assert!(seated.is_ok_and(|config| config.is_some()));
    assert!(matches!(
        nika_serve::server::seat_cost_policy(None, &none.cost_policy),
        Ok(None)
    ));
    let unbounded = ["deepseek/c6-unpriced-fixture:requests=4".to_owned()];
    let Err(refused) = nika_serve::server::seat_cost_policy(listener(), &unbounded) else {
        panic!("a rule that bounds too little was seated");
    };
    let refused = refused.to_string();
    assert!(
        refused.starts_with("standing cost rule ") && refused.contains("is required"),
        "{refused}"
    );
    // The verb refuses the same rule before it reads a project, a credential
    // or a socket, in the words every other launch refusal uses.
    let launch = parse(&[
        "--workflows",
        "workflows",
        "--token-file",
        "serve.token",
        "--cost-policy",
        &unbounded[0],
    ]);
    let out = run(&launch);
    assert_eq!(out.code, exit::WORKFLOW);
    assert_eq!(out.text, format!("serve · {refused}"));
}

/// #1649 · a project that arms nothing says so: `--once` and `--dry` are
/// rehearsals of nothing and exit 0 at once, naming the empty registry and
/// the HTTP door that serves workflows without a beat. Measured on 0.123.2
/// after `nika init --yes`: `--once` exited 0 in silence and `--dry` never
/// returned.
#[test]
fn an_unarmed_project_says_so_and_rehearsals_exit() {
    let dir = project("unarmed", "nika: proj\n");
    let _lease = crate::cwd::enter(dir.path()).expect("enter the unarmed project");
    for (once, dry) in [(true, false), (false, true)] {
        let (sent, received) = std::sync::mpsc::channel();
        // A thread with a deadline: on the broken path `--dry` never returns.
        std::thread::Builder::new()
            .name("unarmed-rehearsal".to_owned())
            .spawn(move || {
                let mut args = serve_args();
                args.once = once;
                args.dry = dry;
                let _ = sent.send(run(&args));
            })
            .expect("spawn the rehearsal");
        let out = received
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("once={once} dry={dry} · a rehearsal of nothing returns"));
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("nothing armed"), "{}", out.text);
        assert!(out.text.contains("--bind"), "{}", out.text);
        // A fresh project has no `.nika/` yet: the recipe creates it before
        // it writes the token there, or the pasted line fails.
        let mkdir = out.text.find("mkdir -p .nika &&");
        let write = out.text.find("> .nika/serve.token");
        assert!(
            matches!((mkdir, write), (Some(m), Some(w)) if m < w),
            "the recipe creates `.nika/` before writing into it · {}",
            out.text
        );
        assert!(
            !dir.path().join(".nika").exists(),
            "the rehearsal itself creates nothing"
        );
    }
}
