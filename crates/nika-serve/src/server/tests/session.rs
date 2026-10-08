// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native Session behind this server's door, over real HTTP: off unless the operator seats
//! it, every route behind the bearer token, the health word and the public contract only when it
//! is served, and one Session from intent to saved bytes to its run as ONE job of this resident.
//! Save starts no job; a run admits exactly the saved bytes once, runs them under the run's own
//! ceiling (restricted by the server's, never raised, a restart included) and the Session
//! observes that job's end, its journal when the resident's runtime ran it; a workflow outside
//! the served registry admits nothing.
//! A run admits only the bytes the Session checked for it: rewritten since, or unnamed, nothing.
//! A run this server reviews first waits in the Session's door: the approval admits that reviewed
//! job once, a decline none. The Session is a scripted one over the test project (its reasoner is
//! never asked; the intent reaches the deterministic compiler), so no HOME, census or provider of
//! this machine is touched.

use std::sync::atomic::AtomicUsize;

use nika_session::change::Witness;
use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use nika_session::{ScriptedReasoner, SessionRuntime};
use nika_session_host::http::Opener;
use nika_session_host::run::{Admitted, Jobs as _, RunRequest};

use super::super::session::Resident;
use super::*;
use crate::server::production::{JournalSeal, ResidentExecutionBackend};

const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
/// A brief over several lines, with accents and guillemets: its bytes reach the run unchanged.
const BRIEF: &str = "# Brief\n\nLe lancement passe en octobre — « vite ».\nÉtape 2 : relire.\n";

/// The Session over `root`, as the Session's own fixtures open it.
fn scripted(root: &std::path::Path) -> SessionRuntime {
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".to_owned());
    let preference = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        None,
    );
    let mut runtime = SessionRuntime::open(
        root,
        ResolvedSessionIntelligence::resolve(&preference, &census),
        Box::new(ScriptedReasoner::new(Vec::new())),
    );
    runtime.enable_continuous_preparation();
    runtime
}

/// A journal seal with no key: the resident's journal is kept, unsealed.
struct Unsealed;

impl JournalSeal for Unsealed {
    fn seal(
        &self,
        _: &mut nika_dap::journal::TraceFileSink,
        _: Option<&str>,
        _: Option<&nika_dap::seal::SealTeardown>,
    ) -> bool {
        false
    }
}

/// What each admitted job ran: its exact source and the ceiling it ran under. Producing, the
/// resident's own backend runs it for real (its files, its journal); otherwise it settles at once.
#[derive(Default)]
struct Witnessed {
    produce: Option<ResidentExecutionBackend>,
    runs: std::sync::Mutex<Vec<(String, Option<f64>)>>,
}

impl Witnessed {
    fn producing(root: &std::path::Path) -> Self {
        let backend = ResidentExecutionBackend::new(root).with_journal_seal(Arc::new(Unsealed));
        Self {
            produce: Some(backend),
            ..Self::default()
        }
    }

    fn runs(&self) -> Vec<(String, Option<f64>)> {
        self.runs.lock().expect("runs").clone()
    }

    fn ceilings(&self) -> Vec<Option<f64>> {
        self.runs()
            .into_iter()
            .map(|(_, ceiling)| ceiling)
            .collect()
    }
}

impl ExecutionBackend for Witnessed {
    fn execute<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        match &self.produce {
            Some(backend) => backend.execute(context),
            None => Box::pin(async { ExecutionDisposition::Succeeded.into() }),
        }
    }

    fn execute_with_inputs<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
        access_pin: Option<&str>,
        inputs: &BTreeMap<String, Value>,
        cancel: nika_types::cancel::CancelCtx,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        let snapshot = context.snapshot();
        let source = snapshot
            .text(snapshot.root())
            .unwrap_or_default()
            .to_owned();
        self.runs.lock().expect("runs").push((source, max_cost_usd));
        match &self.produce {
            Some(backend) => {
                backend.execute_with_inputs(context, max_cost_usd, access_pin, inputs, cancel)
            }
            None => Box::pin(async { ExecutionDisposition::Succeeded.into() }),
        }
    }

    fn trace_journal_dir(&self) -> Option<PathBuf> {
        self.produce
            .as_ref()
            .and_then(ExecutionBackend::trace_journal_dir)
    }
}

struct Served {
    address: SocketAddr,
    opened: Arc<AtomicUsize>,
    backend: Arc<Witnessed>,
    state: Arc<AppState>,
    shutdown: oneshot::Sender<()>,
    join: tokio::task::JoinHandle<Result<(), ServerError>>,
}

/// Where a test server's registry is: the whole project, or a directory the Session never
/// saves into.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Registry {
    Project,
    Elsewhere,
}

/// A server over `world`, its Session door seated when `sessions`, the Session scripted, its runs
/// this resident's jobs on `backend` under the server's per-run `ceiling`.
async fn serve(
    world: &TestWorld,
    (sessions, registry): (bool, Registry),
    ceiling: Option<f64>,
    backend: Arc<Witnessed>,
) -> Served {
    let limits = ServerLimits::default().with_default_max_cost_usd(ceiling);
    let authority = ResidentAuthority::open(
        ResidentConfig::new(&world.state)
            .with_workflow_root(world.root.path())
            .with_limits(limits),
        Arc::clone(&backend) as Arc<dyn ExecutionBackend>,
    )
    .await
    .expect("authority");
    let served = match registry {
        Registry::Project => world.root.path().to_path_buf(),
        Registry::Elsewhere => world.root.path().join("elsewhere"),
    };
    std::fs::create_dir_all(&served).expect("registry");
    let opened = Arc::new(AtomicUsize::new(0));
    let (root, count) = (world.root.path().to_path_buf(), Arc::clone(&opened));
    let session_opener: Opener = Arc::new(move |_| {
        count.fetch_add(1, Ordering::SeqCst);
        Ok((scripted(&root), Vec::new()))
    });
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &served,
        &world.token,
    )
    .with_sessions(sessions);
    (crate::server::session::TEST_OPENERS
        .lock()
        .expect("openers"))
    .insert(world.root.path().to_path_buf(), session_opener);
    let bound = BoundServer::attach(config, &authority).await.expect("bind");
    assert_eq!(
        bound.state.sessions.is_some(),
        sessions,
        "the operator's switch"
    );
    let address = bound.local_addr().expect("address");
    let state = Arc::clone(&bound.state);
    let (shutdown, receiver) = oneshot::channel();
    let join = tokio::spawn(authority.serve_with_http(bound, async move {
        let _ = receiver.await;
    }));
    Served {
        address,
        opened,
        backend,
        state,
        shutdown,
        join,
    }
}

/// The run request a Session makes for `workflow` after checking `checked` (none: no check).
fn checked_run(workflow: &str, checked: Option<&str>) -> RunRequest {
    RunRequest {
        workflow: PathBuf::from(workflow),
        vars: Vec::new(),
        max_cost_usd: 0.25,
        access_pin: None,
        bytes: checked.map(|source| Box::new(Witness::of(source.as_bytes()))),
    }
}

/// A test project holding the brief the Session's intent reads.
fn briefed() -> TestWorld {
    let world = TestWorld::new();
    std::fs::create_dir_all(world.root.path().join("notes")).expect("notes");
    std::fs::write(world.root.path().join("notes/brief.md"), BRIEF).expect("brief");
    world
}

fn post(path: &str, body: &str, token: bool) -> String {
    let auth = if token { auth_header() } else { String::new() };
    format!(
        "POST {path} HTTP/1.1\r\nHost: test\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn get(path: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: test\r\n{}Connection: close\r\n\r\n",
        auth_header()
    )
}

fn submit(command: &str, snapshot: &Value, line: &str) -> String {
    serde_json::json!({
        "contract": "nika/session-host@1", "op": "submit", "command": command,
        "snapshot": snapshot, "line": line,
    })
    .to_string()
}

fn kinds(result: &Value) -> Vec<String> {
    (result["outcomes"].as_array().expect("outcomes").iter())
        .filter_map(|outcome| outcome["kind"].as_str().map(str::to_owned))
        .collect()
}

fn capabilities(health: &Value) -> Vec<String> {
    (health["supportedCapabilities"]
        .as_array()
        .expect("capabilities")
        .iter())
    .filter_map(|word| word.as_str().map(str::to_owned))
    .collect()
}

/// A Session opened on `served`, then its intent proposed and saved: the session, the saved
/// workflow's path and bytes, and the snapshot that answers next.
async fn saved(served: &Served, world: &TestWorld) -> (String, String, Vec<u8>, Value) {
    let opened = wire_request(served.address, &post("/v1/sessions", "", true)).await;
    assert_eq!(opened.status, 201, "{}", opened.body);
    let opened = opened.json();
    let session = opened["session"].as_str().expect("session").to_owned();
    let commands = format!("/v1/sessions/{session}/commands");
    let first = &opened["snapshot"]["snapshot"];
    let request = post(&commands, &submit("c-1", first, COPY), true);
    let proposed = wire_request(served.address, &request).await.json();
    assert_eq!(proposed["outcomes"][0]["kind"], "proposal", "{proposed}");
    let file = &proposed["snapshot"]["work"]["candidate"]["files"][0];
    let path = file["path"].as_str().expect("path").to_owned();
    assert!(
        !world.root.path().join(&path).exists(),
        "nothing lands before the yes"
    );
    let current = &proposed["snapshot"]["snapshot"];
    let request = post(&commands, &submit("c-2", current, "yes"), true);
    let saved = wire_request(served.address, &request).await.json();
    assert_eq!(
        saved["snapshot"]["work"]["saved"]["workflow"],
        path.as_str(),
        "{saved}"
    );
    let bytes = std::fs::read(world.root.path().join(&path)).expect("saved in the served project");
    assert_eq!(
        Value::String(nika_session::Witness::of(&bytes).0),
        file["bytes"]
    );
    assert!(served.backend.runs().is_empty(), "Save starts no job");
    (session, path, bytes, saved["snapshot"]["snapshot"].clone())
}

/// The frames of an event stream that ended, its chunked body decoded.
fn frames(chunked: &str) -> Vec<Value> {
    let mut body = String::new();
    let mut rest = chunked;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let size = usize::from_str_radix(size.trim(), 16).expect("chunk size");
        if size == 0 {
            break;
        }
        body.push_str(&after[..size]);
        rest = &after[size + 2..];
    }
    (body.lines())
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|data| serde_json::from_str(data).expect("frame"))
        .collect()
}

/// The Session's run, asked against `snapshot`: its result.
async fn run_it(served: &Served, session: &str, snapshot: &Value) -> Value {
    run_line(served, session, snapshot, "run it").await
}

/// One `line` typed against `snapshot`: its result.
async fn run_line(served: &Served, session: &str, snapshot: &Value, line: &str) -> Value {
    let commands = format!("/v1/sessions/{session}/commands");
    let request = post(&commands, &submit("c-3", snapshot, line), true);
    wire_request(served.address, &request).await.json()
}

async fn close(served: Served, session: &str) {
    close_session(&served, session).await;
    stop(served).await;
}

/// Close the Session through its own door: its `closed` reply.
async fn close_session(served: &Served, session: &str) {
    let closed = format!(
        "DELETE /v1/sessions/{session} HTTP/1.1\r\nHost: test\r\n{}Connection: close\r\n\r\n",
        auth_header()
    );
    let closed = wire_request(served.address, &closed).await;
    assert_eq!(
        (closed.status, closed.json()["frame"].clone()),
        (200, "closed".into())
    );
}

/// Stop the server and join it.
async fn stop(served: Served) {
    served.shutdown.send(()).ok();
    served.join.await.expect("join").expect("clean stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_session_door_is_off_unless_the_operator_seats_it() {
    let world = TestWorld::new();
    let off = (false, Registry::Elsewhere);
    let served = serve(&world, off, Some(1.0), Arc::default()).await;
    let health = wire_request(served.address, &get("/health")).await.json();
    assert!(
        !capabilities(&health)
            .iter()
            .any(|word| word == "sessionHost" || word == "sessionIntelligence")
    );
    let refused = wire_request(served.address, &post("/v1/sessions", "", true)).await;
    assert_eq!(refused.status, 404);
    assert!(
        refused.json()["error"].is_object(),
        "the server's own envelope"
    );
    let contract = wire_request(served.address, &get("/v1/openapi.json"))
        .await
        .json();
    assert!(contract["paths"].get("/v1/sessions").is_none());
    assert_eq!(served.opened.load(Ordering::SeqCst), 0);
    served.shutdown.send(()).ok();
    served.join.await.expect("join").expect("clean stop");
}

/// The default journey on a default server (its 1 USD per-run ceiling): the Session saves its
/// workflow, then its run is one job of this resident carrying the saved bytes exactly, under the
/// Session's own 0.25 USD ceiling, and the Session observes that job's end.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_seated_session_saves_then_runs_exactly_the_saved_bytes_once_as_a_job() {
    let world = briefed();
    let backend = Arc::new(Witnessed::default());
    let seated = (true, Registry::Project);
    let served = serve(&world, seated, Some(DEFAULT_MAX_COST_USD), backend).await;
    let health = wire_request(served.address, &get("/health")).await.json();
    let words = capabilities(&health);
    assert!(words.iter().any(|word| word == "sessionHost"), "{words:?}");
    assert!(
        words.iter().any(|word| word == "sessionIntelligence"),
        "{words:?}"
    );
    let contract = wire_request(served.address, &get("/v1/openapi.json"))
        .await
        .json();
    assert!(contract["paths"]["/v1/sessions/{session}/commands"]["post"].is_object());
    assert!(contract["components"]["schemas"]["SessionFrame"].is_object());
    // No token: the server's 401, and the Session is never opened.
    let anonymous = wire_request(served.address, &post("/v1/sessions", "", false)).await;
    assert_eq!(anonymous.status, 401);
    assert!(anonymous.json()["error"].is_object());
    assert_eq!(served.opened.load(Ordering::SeqCst), 0);
    let (session, path, bytes, current) = saved(&served, &world).await;
    assert_eq!(served.opened.load(Ordering::SeqCst), 1);
    assert!(
        !world.root.path().join("out/copy.md").exists(),
        "Save wrote no output"
    );
    // The whole log, read from its first event until the Session closes.
    let (address, stream) = (
        served.address,
        get(&format!("/v1/sessions/{session}/events")),
    );
    let events = tokio::spawn(async move { wire_request(address, &stream).await });
    let ran = run_it(&served, &session, &current).await;
    assert_eq!(kinds(&ran), ["run_requested", "facts"], "{ran}");
    let observed = ran["outcomes"][1]["text"].as_str().expect("observation");
    assert!(observed.contains("run observed · exit 0"), "{observed}");
    let source = String::from_utf8(bytes).expect("UTF-8 workflow");
    assert_eq!(
        served.backend.runs(),
        [(source, Some(0.25))],
        "one job, the saved bytes exactly, under the Session's own ceiling"
    );
    let run = &ran["snapshot"]["work"]["run"];
    assert_eq!(run["workflow"], path.as_str(), "{run}");
    assert_eq!(run["end"], serde_json::json!({"end": "succeeded"}), "{run}");
    // The log ends with the Session's own `closed`: read whole before the server stops.
    close_session(&served, &session).await;
    let log = events.await.expect("events");
    stop(served).await;
    assert_eq!(log.status, 200);
    let frames = frames(&log.body);
    let notes: Vec<&str> = (frames.iter())
        .filter(|frame| frame["frame"] == "activity" && frame["phase"] == "run")
        .filter_map(|frame| frame["note"].as_str())
        .collect();
    let job = (notes.first())
        .and_then(|note| note.strip_prefix("run admitted as job "))
        .expect("the admitted job")
        .to_owned();
    assert_eq!(
        notes,
        [
            format!("run admitted as job {job}"),
            format!("job {job} ended with exit 0")
        ]
    );
    assert_eq!(
        frames.last().map(|frame| &frame["frame"]),
        Some(&"closed".into())
    );
}

/// The acceptance the Session's saved copy workflow owes, run for real by the resident's runtime:
/// the copied file holds the brief's exact bytes. Not yet qualified: the resident's file builtins
/// resolve `./notes/brief.md` against the server process cwd, not the served project, so the run
/// fails NIKA-BUILTIN-READ-001 unless the server was started in the project.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "resident defect: relative file paths resolve against the server cwd, not the served project (NIKA-BUILTIN-READ-001); un-ignore with the runtime root-path fix"]
async fn a_session_runs_its_saved_copy_workflow_in_the_residents_runtime() {
    let world = briefed();
    let backend = Arc::new(Witnessed::producing(world.root.path()));
    let served = serve(&world, (true, Registry::Project), Some(1.0), backend).await;
    let (session, _path, _bytes, current) = saved(&served, &world).await;
    let ran = run_it(&served, &session, &current).await;
    assert_eq!(kinds(&ran), ["run_requested", "facts"], "{ran}");
    let observed = ran["outcomes"][1]["text"].as_str().expect("observation");
    assert!(observed.contains("run observed · exit 0"), "{observed}");
    let copied = std::fs::read(world.root.path().join("out/copy.md")).expect("the run's output");
    assert_eq!(copied, BRIEF.as_bytes(), "the brief's exact bytes");
    close(served, &session).await;
}

/// The project's own workflow, run by the resident's own runtime through the same Session: the
/// job's journal is the one the Session reads back for its observation, and the run names what
/// it observed of itself (the source hash its journal started with, its job's execution and the
/// job's opaque trace identity, which the job's own trace door resolves), never the resident's
/// path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_run_executes_in_the_residents_runtime_and_reads_its_journal() {
    let world = TestWorld::new();
    std::fs::write(world.root.path().join("root.nika"), WORKFLOW).expect("workflow");
    let backend = Arc::new(Witnessed::producing(world.root.path()));
    let served = serve(&world, (true, Registry::Project), Some(1.0), backend).await;
    let opened = wire_request(served.address, &post("/v1/sessions", "", true)).await;
    let opened = opened.json();
    let session = opened["session"].as_str().expect("session").to_owned();
    let (address, stream) = (
        served.address,
        get(&format!("/v1/sessions/{session}/events")),
    );
    let events = tokio::spawn(async move { wire_request(address, &stream).await });
    let ran = run_line(
        &served,
        &session,
        &opened["snapshot"]["snapshot"],
        "run root.nika",
    )
    .await;
    assert_eq!(kinds(&ran), ["run_requested", "facts"], "{ran}");
    let observed = ran["outcomes"][1]["text"].as_str().expect("observation");
    assert!(observed.contains("run observed · exit 0"), "{observed}");
    let source = WORKFLOW.to_owned();
    assert_eq!(served.backend.runs(), [(source, Some(0.25))]);
    let run = &ran["snapshot"]["work"]["run"];
    assert_eq!(run["end"], serde_json::json!({"end": "succeeded"}), "{run}");
    let saved = format!("{:x}", Sha256::digest(WORKFLOW.as_bytes()));
    assert_eq!(run["workflow_sha256"], saved.as_str(), "{run}");
    close_session(&served, &session).await;
    let log = events.await.expect("events");
    let notes = frames(&log.body);
    let job = (notes.iter())
        .filter_map(|frame| frame["note"].as_str())
        .find_map(|note| note.strip_prefix("run admitted as job "))
        .expect("the admitted job")
        .to_owned();
    let view = wire_request(served.address, &get(&format!("/v1/jobs/{job}"))).await;
    let view = view.json();
    let execution = run["execution"].as_str().expect("the run's execution");
    assert_eq!(
        view["execution_id"],
        format!("exe-{execution}"),
        "{run} {view}"
    );
    let trace = run["trace"].as_str().expect("the job's trace identity");
    assert_eq!(view["trace_id"], trace, "{run} {view}");
    assert!(!trace.contains('/'), "never the resident's path: {trace}");
    let verify = get(&format!("/v1/jobs/{job}/trace/verify"));
    let verdict = wire_request(served.address, &verify).await;
    assert_eq!(verdict.status, 200);
    assert_eq!(verdict.json()["trace_id"], trace);
    stop(served).await;
}

/// The job runs under the run's own ceiling, restricted by the server's per-run ceiling and
/// never raised: a wider server ceiling keeps the run's, a disarmed one keeps the run's, a
/// narrower one wins, a zero one binds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_run_keeps_its_ceiling_restricted_by_the_servers_never_raised() {
    for (server, ran_under) in [
        (Some(1.0), 0.25),
        (None, 0.25),
        (Some(0.1), 0.1),
        (Some(0.0), 0.0),
    ] {
        let world = briefed();
        let backend = Arc::new(Witnessed::default());
        let served = serve(&world, (true, Registry::Project), server, backend).await;
        let (session, _path, _bytes, current) = saved(&served, &world).await;
        let ran = run_it(&served, &session, &current).await;
        assert_eq!(kinds(&ran), ["run_requested", "facts"], "{ran}");
        assert_eq!(
            served.backend.ceilings(),
            [Some(ran_under)],
            "server {server:?}"
        );
        close(served, &session).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_saved_workflow_outside_the_served_registry_is_never_admitted() {
    let world = briefed();
    let elsewhere = (true, Registry::Elsewhere);
    let served = serve(&world, elsewhere, Some(1.0), Arc::default()).await;
    let (session, _path, _bytes, current) = saved(&served, &world).await;
    let ran = run_it(&served, &session, &current).await;
    assert_eq!(kinds(&ran), ["run_requested", "run_not_started"], "{ran}");
    let why = ran["outcomes"][1]["text"].as_str().expect("why");
    assert!(
        why.starts_with("no workflow by that name under the served registry"),
        "the resident's own words: {why}"
    );
    assert!(served.backend.runs().is_empty(), "no job was admitted");
    assert!(ran["snapshot"]["work"]["run"].is_null(), "no run observed");
    close(served, &session).await;
}

/// The admission seam between the Session's check and the job: the workflow rewritten after the
/// Session checked it (a valid workflow still), or a request naming no checked bytes, admits
/// nothing; the checked bytes admit their one job, which runs exactly them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_admits_only_the_bytes_the_session_checked() {
    let world = TestWorld::new();
    let path = world.root.path().join("root.nika");
    let rewritten = WORKFLOW.replace(r#"expression: ".""#, r#"expression: ". + 1""#);
    assert_ne!(rewritten, WORKFLOW, "a different valid workflow");
    std::fs::write(&path, &rewritten).expect("rewritten after the check");
    let served = serve(&world, (true, Registry::Project), Some(1.0), Arc::default()).await;
    let resident = Resident::lent(&served.state);
    for run in [
        checked_run("root.nika", Some(WORKFLOW)),
        checked_run("root.nika", None),
    ] {
        let refused = resident.admit(&run).await;
        assert!(
            matches!(&refused, Err(words) if words.starts_with("the workflow on disk is not the bytes the Session checked")),
            "{refused:?}"
        );
    }
    assert!(served.backend.runs().is_empty(), "nothing was admitted");
    std::fs::write(&path, WORKFLOW).expect("the checked bytes");
    let admitted = resident
        .admit(&checked_run("root.nika", Some(WORKFLOW)))
        .await;
    assert!(matches!(admitted, Ok(Admitted::Job(_))), "{admitted:?}");
    let Ok(Admitted::Job(job)) = admitted else {
        return;
    };
    let exit = resident.settled(&job).await.expect("its end").exit;
    assert_eq!(exit, 0);
    assert_eq!(
        served.backend.runs(),
        [(WORKFLOW.to_owned(), Some(0.25))],
        "exactly the checked bytes, once"
    );
    stop(served).await;
}

/// A Session job still queued when the resident stops keeps its requested ceiling: the record
/// binds it into its admission event (a widened or removed ceiling no longer opens), and the
/// recovered run runs under it even when the restarted server disarmed its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_jobs_requested_ceiling_is_hash_bound_and_survives_a_restart() {
    let world = TestWorld::new();
    let owned = nika_fs::OwnedDir::open(&world.workflows).expect("owned root");
    let admitted = (nika_execution::ExecutionService::default())
        .admit(&owned, std::path::Path::new("root.nika"))
        .expect("admit");
    let store = crate::JobStore::open(&world.state).expect("store");
    let admission = store
        .create_or_replay_captured_inputs(
            crate::IdempotencyKey::new("session-run-restart").expect("key"),
            crate::RequestDigest::from_bytes([7; 32]),
            64,
            "root.nika".to_owned(),
            &admitted.snapshot().encode().expect("world"),
            None,
            (BTreeMap::new(), Some(0.25)),
        )
        .expect("queued");
    let id = admission.record().id().as_str().to_owned();
    assert_eq!(admission.record().max_cost_usd(), Some(0.25));
    drop(store);
    let state_path = world.state.join("jobs/state.json");
    let original = std::fs::read(&state_path).expect("state");
    for widened in [true, false] {
        let mut state: Value = serde_json::from_slice(&original).expect("state JSON");
        let record = state["jobs"][0]["record"].as_object_mut().expect("record");
        if widened {
            record.insert("max_cost_usd".to_owned(), serde_json::json!(5.0));
        } else {
            record.remove("max_cost_usd");
        }
        std::fs::write(&state_path, serde_json::to_vec(&state).expect("encode"))
            .expect("tamper own fixture");
        assert!(
            crate::JobStore::open(&world.state).is_err(),
            "the ceiling is bound to its admission (widened: {widened})"
        );
    }
    std::fs::write(&state_path, &original).expect("restore own fixture");
    let backend = Arc::new(Witnessed::default());
    let disarmed = limits().with_default_max_cost_usd(None);
    let server = world
        .start(Arc::clone(&backend) as Arc<dyn ExecutionBackend>, disarmed)
        .await;
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("the recovered run");
    assert_eq!(
        backend.ceilings(),
        [Some(0.25)],
        "never unbounded after a restart"
    );
    server.stop().await.expect("stop");
}

/// The run this server reviews first (an unpriced route under a disarmed ceiling): the review
/// waits in the Session's door, the approval admits that reviewed job once and its end is
/// observed, a decided review is gone, a decline admits nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reviewed_run_is_admitted_once_on_approval_and_never_on_decline() {
    let world = super::cost_review::world();
    let backend = Arc::new(super::cost_review::ReviewedBackend::default());
    let lent = Arc::clone(&backend) as Arc<dyn ExecutionBackend>;
    let (server, state) =
        super::cost_review::start(&world, lent, super::cost_review::disarmed(), true).await;
    let resident = Resident::lent(&state);
    let source = std::fs::read_to_string(world.workflows.join("review.nika")).expect("source");
    let reviewed = checked_run("review.nika", Some(&source));
    let held = resident.admit(&reviewed).await;
    assert!(
        matches!(held, Ok(Admitted::Review { .. })),
        "a held review: {held:?}"
    );
    let Ok(Admitted::Review {
        review,
        question,
        details,
    }) = held
    else {
        return;
    };
    let document: Value = serde_json::from_str(&details).expect("the public document");
    assert_eq!(document["review_id"], review.as_str());
    assert_eq!(document["question"], question.as_str());
    assert_eq!(document["state"], "pending");
    assert!(
        backend.runs.lock().expect("runs").is_empty(),
        "nothing before the decision"
    );
    let job = resident.decide(&review, true).await;
    let job = job.expect("approved").expect("the reviewed job");
    let exit = resident.settled(&job).await.expect("its end").exit;
    assert_eq!(exit, 0);
    assert_eq!(backend.runs.lock().expect("runs").len(), 1, "admitted once");
    assert!(
        resident.decide(&review, true).await.is_err(),
        "a decided review is no longer held"
    );
    let held = resident.admit(&reviewed).await;
    assert!(
        matches!(held, Ok(Admitted::Review { .. })),
        "a second review: {held:?}"
    );
    let Ok(Admitted::Review { review: second, .. }) = held else {
        return;
    };
    assert_eq!(resident.decide(&second, false).await, Ok(None));
    assert_eq!(
        backend.runs.lock().expect("runs").len(),
        1,
        "a decline admits none"
    );
    let view = server
        .request(&get_request(&format!("/v2/cost-reviews/{second}")))
        .await
        .json();
    assert_eq!(view["state"], "declined", "{view}");
    server.stop().await.expect("stop");
}

/// One real resident run through the Session door, written for the client SDK: the opened
/// frame, the run's reply (its `work.run` named by what the run observed of itself) and the
/// sha256 of the bytes it ran. Under the system temporary directory; prints where.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "writes a fixture for the client SDK; run on demand"]
async fn record_a_resident_run_fixture() {
    use std::io::Write as _;
    let world = TestWorld::new();
    std::fs::write(world.root.path().join("root.nika"), WORKFLOW).expect("workflow");
    let backend = Arc::new(Witnessed::producing(world.root.path()));
    let served = serve(&world, (true, Registry::Project), Some(1.0), backend).await;
    let opened = wire_request(served.address, &post("/v1/sessions", "", true)).await;
    let opened = opened.json();
    let session = opened["session"].as_str().expect("session").to_owned();
    let snapshot = &opened["snapshot"]["snapshot"];
    let ran = run_line(&served, &session, snapshot, "run root.nika").await;
    let fixture = serde_json::json!({
        "workflow": WORKFLOW,
        "workflow_sha256": format!("{:x}", Sha256::digest(WORKFLOW.as_bytes())),
        "opened": opened,
        "ran": ran,
    });
    let out = std::env::temp_dir().join(format!("nika-serve-resident-run-{}", std::process::id()));
    std::fs::create_dir_all(&out).expect("fixture dir");
    let text = serde_json::to_string_pretty(&fixture).expect("json");
    std::fs::write(out.join("http-resident-run.json"), text).expect("fixture file");
    close(served, &session).await;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "fixture written to {}", out.display()).expect("stdout");
}
