// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A paid request still in flight when its process leaves (S98 F10): the
//! record already says it may have been sent, a restarted Session replays
//! nothing and names the supported way on (F8), and the history carries the
//! record's uncertainty (F3). A priced request made without any budget keeps
//! the same boundary but no restriction (F11). Loopback mechanics only: a held
//! peer is the controlled start gate, the files copied while it holds are the
//! crash image.
use super::*;
use crate::runtime::inference::{DISPATCH_PREFIX, OBSERVED_PREFIX, RECONFIRM};
use nika_runtime::cost_choice::CostHostEvidence;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

const UNPRICED: &str = "deepseek/s81-unpriced-fixture";
const UNPRICED_WIRE: &str = "s81-unpriced-fixture";
const WAIT: Duration = Duration::from_secs(60);
const RECORD: &str = ".nika/session-state.json";
const CANARY: &str = "private-route-canary";

/// A loopback provider that holds its first request open: `wait_received`
/// is the start gate (a request is in transport), then `answer` or `hang_up`
/// ends it. Any later request is counted and never answered.
struct HeldPeer {
    url: String,
    opened: mpsc::Receiver<()>,
    verdict: mpsc::Sender<Option<Value>>,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl HeldPeer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
        let url = format!(
            "http://{}/chat/completions",
            listener.local_addr().expect("addr")
        );
        let (open_signal, opened) = mpsc::channel();
        let (verdict, release_rx) = mpsc::channel::<Option<Value>>();
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (count, halt) = (requests.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            for incoming in listener.incoming() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = incoming else { continue };
                if !read_request(&mut stream) || count.fetch_add(1, Ordering::SeqCst) > 0 {
                    continue;
                }
                let _ = open_signal.send(());
                if let Ok(Some(body)) = release_rx.recv_timeout(WAIT) {
                    let data = body.to_string();
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",
                        data.len()
                    );
                }
            }
        });
        Self {
            url,
            opened,
            verdict,
            requests,
            stop,
            thread: Some(thread),
        }
    }
    fn wait_received(&self) {
        self.opened
            .recv_timeout(WAIT)
            .expect("a request entered transport");
    }
    fn answer(&self, body: Value) {
        self.verdict.send(Some(body)).expect("the peer holds");
    }
    fn hang_up(&self) {
        self.verdict.send(None).expect("the peer holds");
    }
    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}
impl Drop for HeldPeer {
    fn drop(&mut self) {
        // A test that failed before its verdict must not wait out the hold.
        let _ = self.verdict.send(None);
        self.stop.store(true, Ordering::SeqCst);
        let addr = self.url.trim_start_matches("http://");
        let _ = TcpStream::connect(addr.split('/').next().unwrap_or_default());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Headers and the declared body; `false` for a probe that sent nothing.
fn read_request(stream: &mut TcpStream) -> bool {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut data = Vec::new();
    let mut chunk = [0; 8192];
    let head = loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return false,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
        }
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let length: usize = String::from_utf8_lossy(&data[..head])
        .to_lowercase()
        .lines()
        .find_map(|l| {
            l.strip_prefix("content-length:")
                .map(str::trim)
                .map(str::to_owned)
        })
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    while data.len() < head + length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return false,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
        }
    }
    true
}

/// The same selected unpriced route as the unknown-cost fixtures.
fn open_unpriced(root: &Path) -> SessionRuntime {
    let mut s = open(root);
    s.intelligence.model = Some(UNPRICED.into());
    s.reasoner = Box::new(ProviderReasoner {
        model: UNPRICED.into(),
        label: "selected unpriced route".into(),
    });
    s.factory = Some(Box::new(|_| {
        Box::new(ProviderReasoner {
            model: UNPRICED.into(),
            label: "selected unpriced route".into(),
        })
    }));
    s.refresh_seat();
    s.set_cost_host_evidence(CostHostEvidence::unmanaged_interactive_local());
    s
}

fn asked_cost(out: &TurnOutcome) {
    assert!(
        matches!(out, TurnOutcome::Question { key, .. } if key == "unknown_cost"),
        "{out:?}"
    );
}

fn in_flight(decisions: &[String]) -> Vec<&String> {
    decisions
        .iter()
        .filter(|d| d.starts_with(DISPATCH_PREFIX))
        .collect()
}

fn observed_in_flight(decisions: &[String]) -> Vec<&String> {
    decisions
        .iter()
        .filter(|d| d.starts_with(OBSERVED_PREFIX))
        .collect()
}

/// The last history event of the one project recorded under `home`.
fn last_history_event(home: &Path) -> Value {
    let sessions = home.join(".nika/sessions");
    let project = std::fs::read_dir(sessions)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let log = std::fs::read_to_string(project.join("events.ndjson")).unwrap();
    let last: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    last["event"].clone()
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn a_request_in_flight_is_recorded_before_transport_and_its_restart_replays_nothing() {
    let peer = HeldPeer::start();
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut first = open_unpriced(dir.path());
    first.enable_history(home.path()).unwrap();
    assert!(matches!(
        first.turn("Read ./entree.txt and write it to ./sortie.txt"),
        TurnOutcome::Proposal { .. }
    ));
    assert!(matches!(first.consent("yes"), TurnOutcome::Facts(_)));
    asked_cost(&first.turn("hello"));
    assert_eq!(peer.requests(), 0, "nothing before the one-time yes");
    let url = peer.url.clone();
    let worker = std::thread::spawn(move || {
        let _transport = test_transport::install(&url);
        let out = first.turn("yes");
        (first, out)
    });
    peer.wait_received();
    // The crash image: exactly what a process leaving now keeps on disk.
    let image = std::fs::read(dir.path().join(RECORD)).unwrap();
    let history = tempfile::tempdir().unwrap();
    copy_tree(home.path(), history.path());
    let during: crate::SessionState = serde_json::from_slice(&image).unwrap();
    let line = in_flight(&during.decisions);
    assert_eq!(line.len(), 1, "{:?}", during.decisions);
    assert!(
        line[0].contains(UNPRICED) && line[0].contains("at most 7 request(s)"),
        "{}",
        line[0]
    );
    // The abandoned call never answers; its settlement is conservative.
    peer.hang_up();
    let (first, out) = worker.join().unwrap();
    assert!(!matches!(out, TurnOutcome::Reply(_)), "{out:?}");
    let settled = crate::SessionState::load(dir.path()).unwrap().unwrap();
    assert!(in_flight(&settled.decisions).is_empty(), "settled");
    let last = settled.inference_observations.last().unwrap();
    assert_eq!(last["state"], "Uncertain", "abort is not completion");
    assert_eq!(last["unknown_calls"], 1);
    assert_eq!(last["unknown_attempts"][0]["usage"], Value::Null);
    assert_eq!(last["billed_nano_usd"], Value::Null);
    assert_eq!(last_history_event(home.path())["effect"], "unknown");
    drop(first);
    std::fs::write(dir.path().join(RECORD), &image).unwrap();
    restart_from_the_crash_image(dir.path(), history.path(), &peer);
}

/// A second process opens the crash image: both stores report the same
/// possible exposure, nothing is replayed, only the existing ceremonies remain.
fn restart_from_the_crash_image(root: &Path, home: &Path, peer: &HeldPeer) {
    let mut resumed = open_unpriced(root);
    let conversation = resumed.enable_history(home).unwrap().unwrap();
    assert!(
        conversation.contains("interrupted operation: its result may be unknown"),
        "{conversation}"
    );
    let record = resumed.restore_state().unwrap();
    assert!(
        record.contains(DISPATCH_PREFIX) && record.contains("nothing was replayed"),
        "{record}"
    );
    let status = resumed.status();
    assert!(
        status.contains("1 paid dispatch(es) left without a recorded settlement"),
        "{status}"
    );
    assert!(
        resumed.inference_receipt().unwrap().is_none(),
        "no authority"
    );
    let replay = resumed.turn("yes");
    assert!(
        matches!(&replay, TurnOutcome::Refusal(r) if r.class == RefusalClass::WrongState),
        "a pending approval never survives a restart: {replay:?}"
    );
    for line in ["hello", "run compiled-workflow.nika"] {
        let out = resumed.turn(line);
        assert!(
            matches!(&out, TurnOutcome::Refusal(r)
                if r.text.contains("restored inference exposure is unknown")
                && r.text.contains("« run <workflow>.nika budget 0.50 USD »")),
            "{line}: {out:?}"
        );
    }
    assert!(matches!(
        resumed.turn("run compiled-workflow.nika budget 0.50 USD"),
        TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.5_f64.to_bits()
    ));
    resumed.observe_run(0, None); // synthetic host observation; no engine ran
    let kept = crate::SessionState::load(root).unwrap().unwrap();
    assert_eq!(
        in_flight(&kept.decisions).len(),
        1,
        "never silently cleared"
    );
    assert_eq!(peer.requests(), 1, "nothing was replayed");
}

#[test]
fn history_and_record_agree_after_a_settled_or_contradicted_call() {
    for served in [UNPRICED_WIRE, "s102-another-served-model"] {
        let mut body = response("Hello");
        body["model"] = json!(served);
        let peer = Peer::start(vec![(200, body)]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut s = open_unpriced(dir.path());
        s.enable_history(home.path()).unwrap();
        asked_cost(&s.turn("hello"));
        let _ = s.turn("yes");
        assert_eq!(peer.bodies().len(), 1);
        let settled = served == UNPRICED_WIRE;
        let record = crate::SessionState::load(dir.path()).unwrap().unwrap();
        assert!(in_flight(&record.decisions).is_empty(), "{served}");
        let last = record.inference_observations.last().unwrap();
        assert_eq!(last["unknown_calls"], 1, "{served}");
        let (state, effect) = if settled {
            ("Closed", "no_uncertainty_reported")
        } else {
            ("Uncertain", "unknown")
        };
        assert_eq!(last["state"], state, "{served}");
        assert_eq!(
            last_history_event(home.path())["effect"],
            effect,
            "{served}"
        );
        drop(s);
        // A restart without an interrupted dispatch names the same way on.
        let mut resumed = open_unpriced(dir.path());
        assert!(resumed.restore_state().is_some());
        let out = resumed.turn("run compiled-workflow.nika");
        assert!(
            matches!(&out, TurnOutcome::Refusal(r)
                if r.text.contains("budget 0.50 USD") && !r.text.contains("left without a recorded settlement")),
            "{out:?}"
        );
        assert_eq!(peer.bodies().len(), 1, "{served}");
    }
}

#[test]
fn a_catalog_request_is_recorded_before_transport_and_settles_either_way() {
    for answered in [true, false] {
        let peer = HeldPeer::start();
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        s.admit_money("budget 2 USD", false, false).expect("admit");
        assert!(
            crate::SessionState::load(dir.path()).unwrap().is_none(),
            "nothing is written before a dispatch"
        );
        let url = peer.url.clone();
        let worker = std::thread::spawn(move || {
            let _transport = test_transport::install(&url);
            let reply = s.reason_with_money("hello", false).map(|r| r.text);
            (s, reply)
        });
        peer.wait_received();
        let during = crate::SessionState::load(dir.path()).unwrap().unwrap();
        let line = in_flight(&during.decisions);
        assert_eq!(line.len(), 1, "{:?}", during.decisions);
        assert!(
            line[0].contains(MODEL) && line[0].contains("catalog allowance"),
            "{}",
            line[0]
        );
        if answered {
            peer.answer(response("hello"));
        } else {
            peer.hang_up();
        }
        let (s, reply) = worker.join().unwrap();
        assert_eq!(reply.is_ok(), answered, "{reply:?}");
        let settled = crate::SessionState::load(dir.path()).unwrap().unwrap();
        assert!(in_flight(&settled.decisions).is_empty());
        let last = settled.inference_observations.last().unwrap();
        let receipt = s.inference_receipt().unwrap().unwrap();
        assert_eq!(receipt.attempts.len(), 1);
        if answered {
            assert_eq!(last["state"], "Open");
            assert!(last["attempts"][0]["estimated_nano_usd"].is_string());
        } else {
            assert_eq!(last["state"], "Uncertain", "abort is not completion");
            assert_eq!(last["attempts"][0]["estimated_nano_usd"], Value::Null);
            assert!(receipt.held_unknown.nano_usd > 0);
        }
        assert_eq!(peer.requests(), 1);
    }
}

#[test]
fn a_no_budget_request_in_flight_is_recorded_and_its_restart_keeps_the_model() {
    let peer = HeldPeer::start();
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut first = open(dir.path());
    first.enable_history(home.path()).unwrap();
    let url = peer.url.clone();
    let worker = std::thread::spawn(move || {
        let _transport = test_transport::install(&url);
        let out = first.turn("hello");
        (first, out)
    });
    peer.wait_received();
    // The crash image: exactly what a process leaving now keeps on disk.
    let image = std::fs::read(dir.path().join(RECORD)).unwrap();
    let history = tempfile::tempdir().unwrap();
    copy_tree(home.path(), history.path());
    let during: crate::SessionState = serde_json::from_slice(&image).unwrap();
    let line = observed_in_flight(&during.decisions);
    assert_eq!(line.len(), 1, "{:?}", during.decisions);
    assert!(
        line[0].contains(MODEL) && line[0].contains("no Session budget"),
        "{}",
        line[0]
    );
    assert!(
        in_flight(&during.decisions).is_empty() && !during.decisions.iter().any(|d| d == RECONFIRM),
        "no allowance, so nothing to reconfirm: {:?}",
        during.decisions
    );
    peer.hang_up();
    let (first, out) = worker.join().unwrap();
    assert!(!matches!(out, TurnOutcome::Reply(_)), "{out:?}");
    let settled = crate::SessionState::load(dir.path()).unwrap().unwrap();
    assert!(observed_in_flight(&settled.decisions).is_empty(), "settled");
    let last = settled.inference_observations.last().unwrap();
    assert_eq!(last["unbudgeted"], true);
    assert_eq!(last["state"], "Uncertain", "abort is not completion");
    assert_eq!(last["attempts"][0]["usage"], Value::Null);
    assert_eq!(last_history_event(home.path())["effect"], "unknown");
    drop(first);
    std::fs::write(dir.path().join(RECORD), &image).unwrap();
    restart_without_a_budget(dir.path(), history.path(), &peer);
}

/// A second process opens the crash image: the exposure is named, nothing is
/// replayed, and the chosen model still answers: no budget was ever set.
fn restart_without_a_budget(root: &Path, home: &Path, peer: &HeldPeer) {
    let again = Peer::start(vec![(200, response("again"))]);
    let _transport = test_transport::install(&again.url);
    let mut resumed = open(root);
    let conversation = resumed.enable_history(home).unwrap().unwrap();
    assert!(
        conversation.contains("interrupted operation: its result may be unknown"),
        "{conversation}"
    );
    let record = resumed.restore_state().unwrap();
    assert!(
        record.contains(OBSERVED_PREFIX) && record.contains("nothing was replayed"),
        "{record}"
    );
    let status = resumed.status();
    assert!(
        status.contains("1 no-budget dispatch(es) left without a recorded settlement"),
        "{status}"
    );
    let out = resumed.turn("hello");
    assert!(
        matches!(&out, TurnOutcome::Reply(text) if text == "again"),
        "{out:?}"
    );
    assert_eq!(peer.requests(), 1, "nothing was replayed");
    assert_eq!(again.bodies().len(), 1);
    let kept = crate::SessionState::load(root).unwrap().unwrap();
    assert_eq!(
        observed_in_flight(&kept.decisions).len(),
        1,
        "never silently cleared"
    );
    assert!(
        kept.inference_observations
            .iter()
            .all(|o| o["unbudgeted"] == true)
    );
}

#[test]
fn a_contradicted_or_failed_no_budget_call_is_never_retried_or_priced() {
    let mut served = response("Hello");
    served["model"] = json!("s108-another-served-model");
    let failed = json!({"error": {"message": "overloaded"}});
    for (status, body) in [(200, served), (503, failed)] {
        let peer = Peer::start(vec![(status, body), (200, response("Hello"))]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        s.enable_history(home.path()).unwrap();
        let out = s.turn("hello");
        assert!(!matches!(out, TurnOutcome::Reply(_)), "{status}: {out:?}");
        assert_eq!(peer.bodies().len(), 1, "{status}: no automatic retry");
        let record = crate::SessionState::load(dir.path()).unwrap().unwrap();
        let last = record.inference_observations.last().unwrap();
        assert_eq!(last["unbudgeted"], true, "{status}");
        assert_eq!(last["state"], "Uncertain", "{status}");
        assert_eq!(last["unknown_calls"], 1, "{status}");
        assert_eq!(last["schema"], DURABLE, "{status}: the durable form (E35)");
        let estimated = &last["attempts"][0]["estimated_nano_usd"];
        assert_eq!(estimated, &Value::Null, "{status}: never an invented price");
        assert_eq!(last_history_event(home.path())["effect"], "unknown");
        // No ceremony follows: the next operation observes afresh.
        let next = s.turn("hello");
        assert!(
            matches!(&next, TurnOutcome::Reply(text) if text == "Hello"),
            "{status}: {next:?}"
        );
        assert_eq!(peer.bodies().len(), 2, "{status}");
        let effect = last_history_event(home.path())["effect"].clone();
        assert_eq!(effect, "no_uncertainty_reported", "{status}");
        let kept = crate::SessionState::load(dir.path()).unwrap().unwrap();
        assert_eq!(kept.inference_observations.len(), 2, "{status}");
        let durable = |o: &Value| o["schema"] == DURABLE;
        assert!(kept.inference_observations.iter().all(durable), "{status}");
    }
}

#[test]
fn an_unknown_priced_route_keeps_its_review_and_is_never_observed_instead() {
    let mut body = response("Hello");
    body["model"] = json!(UNPRICED_WIRE);
    let peer = Peer::start(vec![(200, body)]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unpriced(dir.path());
    asked_cost(&s.turn("hello"));
    assert!(peer.bodies().is_empty(), "nothing before the one-time yes");
    let out = s.turn("yes");
    assert!(matches!(out, TurnOutcome::Reply(_)), "{out:?}");
    assert_eq!(peer.bodies().len(), 1);
    let observed = s.money.observed.snapshot().unwrap();
    assert!(
        observed.attempts.is_empty(),
        "the reviewed scope carried it"
    );
    let observations = s.cost_observations();
    assert!(observations.iter().all(|o| o["unbudgeted"] != true));
}

const DURABLE: &str = "nika/inference-cost-observation@2";

/// An unknown-cost account over a canonical route whose path carries a private segment.
fn private_account() -> nika_providers::InferenceAdmission {
    let base = format!("https://example.test/{CANARY}/v1");
    let config = nika_providers::ProvidersConfig::new().with_base_url("deepseek", &base);
    let route = nika_runtime::cost_choice::CostRoute::observe(MODEL, config).unwrap();
    nika_runtime::cost_choice::CostReview::new(
        "candidate".into(),
        "session".into(),
        route.clone(),
        CostHostEvidence::unmanaged_interactive_local(),
        None,
        None,
    )
    .unwrap()
    .for_session()
    .confirm("candidate", &route)
    .unwrap()
}

/// E35: at the dispatch boundary the record names a private route by its origin (`@2`), never
/// its path; the live account keeps its exact endpoint, a restart reads the record with the same
/// meaning, and an entry recorded earlier is carried as written (no migration here).
#[test]
fn a_private_route_is_recorded_by_its_origin_at_the_dispatch_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let earlier = json!({"schema": "nika/inference-cost-observation@1", "state": "Closed",
        "note": format!("recorded earlier at https://example.test/{CANARY}-earlier/v1")});
    let mut record = crate::SessionState::new("2026-09-28T00:00:00Z".to_owned());
    record.inference_observations.push(earlier.clone());
    record.save(dir.path()).unwrap();
    let mut s = open(dir.path());
    assert!(s.restore_state().is_some());
    let account = private_account();
    s.money.account = Some(account.clone());
    s.unknown_cost.active = true;
    s.save_dispatch_boundary().unwrap();
    let record = crate::SessionState::load(dir.path()).unwrap().unwrap();
    let line = in_flight(&record.decisions);
    assert_eq!(line.len(), 1, "{:?}", record.decisions);
    assert!(!line[0].contains(CANARY), "{}", line[0]);
    let origin = format!("{MODEL} at https://example.test:443");
    assert!(line[0].contains(&origin), "{}", line[0]);
    assert_eq!(
        record.inference_observations[0], earlier,
        "carried as written"
    );
    let observation = &record.inference_observations[1];
    assert!(!observation.to_string().contains(CANARY), "{observation}");
    assert_eq!(observation["schema"], DURABLE);
    assert_eq!(
        observation["unknown_cost"]["origin"],
        "https://example.test:443"
    );
    let exact = account.snapshot().unwrap().observation().to_string();
    assert!(
        exact.contains(CANARY),
        "the live account keeps its exact route"
    );
    drop(s);
    let mut resumed = open(dir.path());
    let notice = resumed.restore_state().unwrap();
    assert!(notice.contains(DISPATCH_PREFIX), "{notice}");
    assert!(notice.contains("nothing was replayed"), "{notice}");
    assert!(!notice.contains(CANARY), "{notice}");
    assert!(
        resumed.money.reconfirm,
        "a paid observation still restricts"
    );
    let status = resumed.status();
    assert!(
        status.contains("1 paid dispatch(es) left without a recorded settlement"),
        "status must retain the unsettled paid dispatch"
    );
}

/// E35: a private-route account that a fresh one-time review supersedes is kept by its origin,
/// while the account itself keeps its exact endpoint.
#[test]
fn a_superseded_private_account_is_kept_by_its_origin() {
    let mut body = response("Hello");
    body["model"] = json!(UNPRICED_WIRE);
    let peer = Peer::start(vec![(200, body)]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unpriced(dir.path());
    let superseded = private_account();
    s.money.account = Some(superseded.clone());
    asked_cost(&s.turn("hello"));
    let out = s.turn("yes");
    assert!(matches!(out, TurnOutcome::Reply(_)), "{out:?}");
    assert_eq!(peer.bodies().len(), 1);
    let record = crate::SessionState::load(dir.path()).unwrap().unwrap();
    let text = serde_json::to_string(&record.inference_observations).unwrap();
    assert!(!text.contains(CANARY), "{text}");
    let kept = &record.inference_observations[0];
    assert_eq!(kept["schema"], DURABLE);
    assert_eq!(kept["unknown_cost"]["origin"], "https://example.test:443");
    let exact = superseded.snapshot().unwrap().observation().to_string();
    assert!(exact.contains(CANARY), "the account keeps its exact route");
}

/// C11 F2 · at a live round whose question asks for words, a chosen seat whose unknown cost
/// awaits its one-time review asks that review for the line instead of refusing it as unreadable;
/// nothing is sent before the yes, and declining keeps the question waiting unchanged.
#[test]
fn a_live_round_reaches_the_one_time_review_for_words_only_the_seat_reads() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    let csv = "customer,amount,status\nacme,10,late\nbeta,5,ok\nacme,7,ok\n";
    std::fs::write(dir.path().join("data/orders.csv"), csv).unwrap();
    let mut s = under_an_unpriced_seat(dir.path());
    let request = "Read ./data/orders.csv, compute the total amount per customer and post the result to a webhook";
    let TurnOutcome::Question { key, .. } = s.turn(request) else {
        panic!("the deterministic round asks first");
    };
    assert!(
        peer.bodies().is_empty(),
        "the deterministic reading sends nothing"
    );
    asked_cost(&s.turn("the total of the amount column for each customer"));
    assert!(peer.bodies().is_empty(), "nothing before the one-time yes");
    let out = s.turn("no");
    assert!(
        matches!(&out, TurnOutcome::Question { key: waiting, .. } if *waiting == key),
        "declining keeps the question: {out:?}"
    );
    assert!(peer.bodies().is_empty(), "declining sends nothing");
}

/// The unpriced route's Session with the default authoring strategy (escalate).
fn under_an_unpriced_seat(root: &Path) -> SessionRuntime {
    let mut s = open_unpriced(root);
    s.set_authoring_context(crate::authoring::AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none(),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    ));
    s
}

/// A brief round under the unpriced seat, answered to its endpoint question (a value question):
/// the model, when asked, is said as a value alone and bound with no reading.
fn at_the_endpoint(root: &Path) -> (SessionRuntime, String) {
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("notes/brief.md"), "# Brief\n\nOctober launch.\n").unwrap();
    let mut s = under_an_unpriced_seat(root);
    let brief =
        "Read ./notes/brief.md, draft a 3-bullet summary of it and post the summary to a webhook";
    let mut asked = s.turn(brief);
    if matches!(&asked, TurnOutcome::Question { key, .. } if key == "model") {
        asked = s.turn("mock/echo");
    }
    let TurnOutcome::Question { key, .. } = asked else {
        panic!("the endpoint question: {asked:?}");
    };
    (s, key)
}

/// C11 F2 · at a value question a value said in words, which only the chosen seat can read,
/// reaches the one-time review, while a value alone binds with no reading and a question about
/// the question keeps its free local answer; declining sends nothing and keeps the question.
#[test]
fn a_value_in_words_reaches_the_review_while_an_exact_value_or_a_question_does_not() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let (mut s, key) = at_the_endpoint(dir.path());
    let aside = s.turn("which endpoint will it post to?");
    assert!(
        matches!(aside, TurnOutcome::Aside(_)),
        "a question stays local: {aside:?}"
    );
    asked_cost(&s.turn("post it to https://hooks.example.com/orders please"));
    let out = s.turn("no");
    assert!(
        matches!(&out, TurnOutcome::Question { key: waiting, .. } if *waiting == key),
        "declining keeps the question: {out:?}"
    );
    let out = s.turn("https://hooks.example.com/orders");
    assert!(
        matches!(out, TurnOutcome::Proposal { .. }),
        "a value alone binds: {out:?}"
    );
    assert!(peer.bodies().is_empty(), "nothing was sent");
}

/// C11 F2 · under a restored exposure the review a value in words would need is refused before
/// anything is sent: the restriction still holds every paid continuation.
#[test]
fn a_restored_exposure_refuses_the_review_before_anything_is_sent() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _) = at_the_endpoint(dir.path());
    s.money.reconfirm = true;
    let out = s.turn("post it to https://hooks.example.com/orders please");
    assert!(
        matches!(&out, TurnOutcome::Refusal(r) if r.text.contains("restored inference exposure is unknown")),
        "{out:?}"
    );
    assert!(peer.bodies().is_empty(), "nothing was sent");
}

/// C11 F2 · after the one-time yes the reviewed seat, and only it, reads the value said in words:
/// one request judges the line, one reads the value, both name the reviewed route's model, and
/// the value read binds the endpoint (the round reaches its proposal).
#[test]
fn after_the_one_time_yes_the_reviewed_seat_reads_the_value() {
    let mut judged = response("ANSWER");
    judged["model"] = json!(UNPRICED_WIRE);
    let mut read = response("https://hooks.example.com/orders");
    read["model"] = json!(UNPRICED_WIRE);
    let peer = Peer::start(vec![(200, judged), (200, read)]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _) = at_the_endpoint(dir.path());
    asked_cost(&s.turn("post it to https://hooks.example.com/orders please"));
    assert!(peer.bodies().is_empty(), "nothing before the yes");
    let out = s.turn("yes");
    assert!(
        matches!(&out, TurnOutcome::Proposal { preview, .. }
            if preview.contains("read your answer as « https://hooks.example.com/orders »")),
        "the value read binds the endpoint: {out:?}"
    );
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 2, "one judgement, one reading");
    assert!(
        bodies.iter().all(|b| b["model"] == UNPRICED_WIRE),
        "only the reviewed route: {bodies:?}"
    );
}

/// E35 · an unreadable durable observation (a projection the provider could not write, kept as
/// null) stays fail-closed and visible: a restart keeps the restriction, counts it among the
/// charges that may have been billed and names it in the status, and so does a live session.
#[test]
fn an_unreadable_durable_observation_stays_fail_closed_and_visible() {
    let named = "1 cost observation(s) unreadable: never read as settled";
    let dir = tempfile::tempdir().unwrap();
    let mut record = crate::SessionState::new("2026-09-29T00:00:00Z".to_owned());
    record.inference_observations.push(Value::Null);
    record.save(dir.path()).unwrap();
    let mut s = open(dir.path());
    assert!(s.restore_state().is_some());
    assert!(s.money.reconfirm, "an unreadable entry still restricts");
    assert_eq!(
        s.uncertain_charges(),
        1,
        "an unreadable entry may have been billed"
    );
    let status = s.status();
    assert!(
        status.contains(named),
        "restored unreadable cost evidence must remain visible"
    );
    let live = tempfile::tempdir().unwrap();
    let mut s = open(live.path());
    s.unknown_cost.observations.push(Value::Null);
    assert_eq!(
        s.uncertain_charges(),
        1,
        "live, it may have been billed as well"
    );
    let status = s.status();
    assert!(
        status.contains(named),
        "live unreadable cost evidence must remain visible"
    );
}

/// C11 F2 r3 · a live round with no open question still owes the one-time review for words only
/// the chosen seat can read, while a question keeps its free local answer (E's r2 review).
#[test]
fn a_question_less_live_round_owes_the_review_for_words_but_not_for_a_question() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = under_an_unpriced_seat(dir.path());
    s.authoring = Some(crate::authoring::AuthoringRound::new(
        "Read ./a.md and write ./b.md",
    ));
    assert!(
        s.authoring.as_ref().is_some_and(|r| r.current().is_none()),
        "no open question"
    );
    let local = s.turn("what happens now?");
    assert!(
        !matches!(&local, TurnOutcome::Question { key, .. } if key == "unknown_cost"),
        "a question stays local: {local:?}"
    );
    asked_cost(&s.turn("make it shorter"));
    assert!(peer.bodies().is_empty(), "nothing was sent");
}
