// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Loopback protocol mechanics, not live qualification (R4 B16 · C11). A clean child owns its
//! environment: `NIKA_AUTHORING_REASONING` reaches the Session's authoring context through the
//! shared parser, and an explicit level on a route whose catalog does not qualify it (a local
//! engine behind a base-URL override) is refused before any byte reaches the seat; the control
//! naming no level sends its request as before.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]

mod common;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::LoopbackSeat;
use nika_onboard::compile::{AuthoringReasoning, CompileRequest};
use nika_session::authoring::{AuthoringContext, AuthoringSeat, compile_in};

const REQUEST: &str = "Read ./orders.csv, keep the rows whose status is paid, write them to ./paid.csv with the same header and write their total amount as a number to ./paid-total.txt.";
const ORDERS: &str = "id,date,customer,status,amount,currency,ref\n1,2026-09-01,Acme,paid,120.50,EUR,A-1\n2,2026-09-02,Bolt,due,80,EUR,A-2\n";

/// Run the child under `effort` (none: the variable unset) and return every body the seat got.
fn scenario(effort: Option<&str>) -> Vec<serde_json::Value> {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().join("project");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&root).expect("project");
    std::fs::create_dir_all(&home).expect("home");
    let log = home.join("child.log");
    let seat = LoopbackSeat::start(vec!["{}".to_owned()]);
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command
        .args([
            "--exact",
            "child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .env("REASONING_ROOT", &root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(&log).expect("log")))
        .stderr(Stdio::from(
            std::fs::OpenOptions::new()
                .append(true)
                .open(&log)
                .expect("log"),
        ));
    if let Some(word) = effort {
        command.env("NIKA_AUTHORING_REASONING", word);
    }
    let mut child = command.spawn().expect("child");
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("bounded child timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    seat.shutdown();
    assert!(
        status.success(),
        "{}",
        std::fs::read_to_string(log).expect("child evidence")
    );
    seat.bodies()
}

#[test]
fn an_environment_level_on_an_unqualified_route_sends_nothing() {
    let bodies = scenario(Some("max"));
    assert!(bodies.is_empty(), "actual HTTP attempts: {bodies:?}");
}

#[test]
fn naming_no_level_sends_the_request_as_before() {
    let bodies = scenario(None);
    assert!(!bodies.is_empty(), "the seat was asked");
    for body in &bodies {
        assert!(body.get("reasoning_effort").is_none(), "{body}");
        assert!(body.get("thinking").is_none(), "{body}");
    }
}

#[test]
#[ignore = "bounded child invoked by loopback parents"]
fn child() {
    let root = std::env::var("REASONING_ROOT").expect("root");
    let root = Path::new(&root);
    std::fs::write(root.join("orders.csv"), ORDERS).expect("fixture");
    let context = AuthoringContext::from_env().with_project_root(root);
    let named = std::env::var("NIKA_AUTHORING_REASONING").ok();
    let expected = named.as_deref().and_then(AuthoringReasoning::parse);
    assert_eq!(
        context.reasoning(),
        expected,
        "resolved once, from the environment"
    );
    assert_eq!(
        context.source(),
        if named.is_some() {
            "environment"
        } else {
            "default"
        }
    );
    let seat = AuthoringSeat::Provider {
        model: "vllm/reasoning-wire".to_owned(),
    };
    let out =
        compile_in(&seat, &context, &CompileRequest::create(REQUEST), REQUEST).expect("an outcome");
    let calls = out
        .provenance
        .authoring
        .as_ref()
        .map(|receipt| receipt.context.clone())
        .unwrap_or_default();
    assert!(!calls.is_empty(), "the seat was asked: {out:?}");
    // B19 F3: a call the route refused before sending is never said to be sent.
    let words = nika_onboard::compile::reading::receipt_words(
        out.provenance.authoring.as_ref().expect("the receipt"),
        "",
    );
    assert_eq!(
        words.contains("\n  nothing was sent to: vllm · host 127.0.0.1"),
        expected.is_some(),
        "{words}"
    );
    for call in &calls {
        let reasoning = &call["reasoning"];
        match expected {
            Some(level) => {
                assert_eq!(reasoning["configured"], level.word(), "{call}");
                assert_eq!(reasoning["transmitted"], "unobserved", "{call}");
            }
            None => assert!(reasoning["configured"].is_null(), "{call}"),
        }
    }
}
