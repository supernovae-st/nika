// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A session offer read from a real adapter process (a keyless scripted one): the process
//! receives the handshake, `session/new` and the model selection and nothing else, the session
//! is rooted in a scratch directory that is removed with it, and the adapter's whole process
//! group is ended when the offer returns or its deadline passes.
use super::*;
use crate::{EffortOffer, ModelOffer, ServiceTierOffer};
use std::time::Duration;

/// A scripted adapter. It forks a `sleep` grandchild (an npm wrapper forks the real adapter)
/// and writes that pid to `grandchild.pid`, appends every method it receives and the session
/// root it was given to `seen.log`, answers `initialize` after `delay` seconds, `session/new`
/// and one model selection, then reads its input until EOF.
fn adapter(dir: &std::path::Path, delay: &str) -> SpawnedHarness {
    let script = dir.join("adapter.py");
    std::fs::write(
        &script,
        format!(
            r#"import json, subprocess, sys, time
kid = subprocess.Popen(["sleep", "600"])
open({pid:?}, "w").write(str(kid.pid))
def option(id, category, current, values):
    return {{"id": id, "name": id, "category": category, "type": "select", "currentValue": current,
            "options": [{{"value": v, "name": v}} for v in values]}}
def config(model, efforts):
    return [option("model", "model", model, ["gpt-5.4", "gpt-5.5"]),
            option("reasoning_effort", "thought_level", "medium", efforts)]
def answer(req, result):
    print(json.dumps({{"jsonrpc": "2.0", "id": req["id"], "result": result}}))
    sys.stdout.flush()
for line in sys.stdin:
    req = json.loads(line)
    method = req.get("method")
    with open({seen:?}, "a") as log:
        log.write(method + "\n")
        if method == "session/new":
            log.write("cwd " + req["params"]["cwd"] + "\n")
    if method == "initialize":
        time.sleep({delay})
        answer(req, {{"protocolVersion": 1, "agentInfo": {{"name": "scripted-acp", "version": "0.16.2"}}}})
    elif method == "session/new":
        answer(req, {{"sessionId": "s-spawned", "configOptions": config("gpt-5.4", ["low", "medium"])}})
    elif method == "session/set_config_option":
        answer(req, {{"configOptions": config(req["params"]["value"], ["low", "medium", "high"])}})
"#,
            pid = dir.join("grandchild.pid").to_string_lossy(),
            seen = dir.join("seen.log").to_string_lossy(),
        ),
    )
    .expect("script");
    let row = HarnessAdapter::new("scripted-acp", "python3")
        .expect("id is fine")
        .with_args(vec![script.to_string_lossy().into_owned()])
        .with_handshake_probe()
        .with_version_pin(crate::probe::VersionPin::new((0, 16), 0));
    SpawnedHarness::new(row)
}

fn seen(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("seen.log"))
        .map(|log| log.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// Whether the adapter's grandchild is gone, asked for up to two seconds.
async fn grandchild_ended(dir: &std::path::Path) -> bool {
    let pid: i32 = std::fs::read_to_string(dir.join("grandchild.pid"))
        .expect("the adapter wrote its grandchild's pid")
        .trim()
        .parse()
        .expect("a pid");
    let grandchild = rustix::process::Pid::from_raw(pid).expect("a positive pid");
    for _ in 0..40 {
        if rustix::process::test_kill_process(grandchild).is_err() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// The spawned adapter is asked once (its `initialize` answer is its identity), selects the
/// model, and receives nothing after: no effort, tier or mode setter and no prompt. Its
/// session root is a scratch directory removed with the session, and the wrapper's grandchild
/// is ended with it.
#[tokio::test]
async fn an_offer_is_read_from_a_spawned_adapter_that_receives_no_prompt_and_is_ended() {
    let dir = tempfile::tempdir().expect("dir");
    let offer = adapter(dir.path(), "0")
        .offer(Some("openai/gpt-5.5"), Duration::from_secs(20))
        .await
        .expect("an offer");
    assert_eq!(offer.agent_name.as_deref(), Some("scripted-acp"));
    assert_eq!(offer.agent_version.as_deref(), Some("0.16.2"));
    assert_eq!(
        offer.model,
        ModelOffer::Confirmed {
            native: "gpt-5.5".to_owned()
        }
    );
    assert_eq!(
        offer.effort,
        EffortOffer::Offered {
            option: "reasoning_effort".to_owned(),
            values: vec!["low".to_owned(), "medium".to_owned(), "high".to_owned()],
            omitted: 0,
            current: Some("medium".to_owned()),
        }
    );
    assert!(
        matches!(offer.service_tier, ServiceTierOffer::Unobserved { .. }),
        "no speed control is measured for this adapter: {:?}",
        offer.service_tier
    );
    assert!(
        grandchild_ended(dir.path()).await,
        "the process group outlived the offer"
    );
    let seen = seen(dir.path());
    let (methods, roots): (Vec<&String>, Vec<&String>) =
        seen.iter().partition(|line| !line.starts_with("cwd "));
    assert_eq!(
        methods,
        ["initialize", "session/new", "session/set_config_option"],
        "one process, three requests, no prompt"
    );
    let root = roots[0].trim_start_matches("cwd ");
    assert!(
        !std::path::Path::new(root).exists(),
        "the session's scratch root `{root}` outlived the session"
    );
    assert!(
        !std::path::Path::new(root).starts_with(dir.path()),
        "the session is rooted in its own scratch directory, never in a caller's"
    );
}

/// An adapter that does not answer in time is unavailable, as it is for a run, and its whole
/// process group is ended at the deadline: nothing lingers after a failed reading.
#[tokio::test]
async fn a_silent_adapter_is_unavailable_at_the_deadline_and_its_group_is_ended() {
    let dir = tempfile::tempdir().expect("dir");
    let started = std::time::Instant::now();
    let refused = adapter(dir.path(), "30")
        .offer(None, Duration::from_millis(600))
        .await
        .expect_err("no offer from a silent adapter");
    assert!(
        matches!(refused, HarnessError::Unavailable { .. }),
        "{refused:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the reading is bounded by its own deadline: {:?}",
        started.elapsed()
    );
    assert!(
        grandchild_ended(dir.path()).await,
        "the process group outlived the deadline"
    );
    assert_eq!(
        seen(dir.path()),
        ["initialize"],
        "the handshake and nothing else"
    );
}
