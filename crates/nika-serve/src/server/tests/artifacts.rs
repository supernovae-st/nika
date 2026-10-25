// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The artifact door end to end over loopback (NIK-265): a settled job's sealed journal lists
//! what it records, a download serves exactly those bytes, and every refusal is typed and
//! declared by the document the server serves. A scripted backend writes a real journal
//! through the resident's own sink and seals it with a key the test holds; one run goes
//! through the production backend, so the parsed shape is the runtime's own.

use std::path::Path;

use nika_dap::journal::TraceFileSink;
use nika_event::{Event, EventKind};
use nika_runtime::EventSink as _;
use nika_types::id::EventId;
use nika_types::resource::{KeyValue, Value as FieldValue};
use nika_types::timestamp::Timestamp;

use super::declared::{assert_declared_on, served_document};
use super::trace_journal::TestSeal;
use super::*;
use crate::server::production::ResidentExecutionBackend;

const MANIFEST: &str = "/v1/jobs/{id}/artifacts";
const DOWNLOAD: &str = "/v1/jobs/{id}/artifacts/{name}";
const WORKFLOW_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000001";
const CHART_WORKFLOW: &str = "nika: chart\npermits:\n  fs: { write: [\"./out/**\"] }\n  tools: [\"nika:chart\"]\ntasks:\n  draw:\n    invoke:\n      tool: \"nika:chart\"\n      args:\n        data: [{ region: \"north\", total: 10 }, { region: \"south\", total: 4 }]\n        chart: { type: bar, x: region, y: total }\n        out: \"./out/totals.svg\"\n";

/// A run key the test holds: the seal signs with it; the door weighs its public file.
struct Key {
    secret: minisign::SecretKey,
    public: String,
    file: PathBuf,
}

fn test_key(world: &TestWorld) -> Key {
    let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("test key");
    let public = pair.pk.to_box().expect("public box").to_string();
    let file = world.root.path().join("run-signing.pub");
    std::fs::write(&file, &public).expect("public key file");
    Key {
        secret: pair.sk,
        public,
        file,
    }
}

/// A backend whose run leaves `events` as its journal under the project's `.nika/traces`,
/// written through the resident's own sink, sealed with `seal` when it holds a key.
struct JournalingBackend {
    project: PathBuf,
    events: Vec<Event>,
    seal: Option<(minisign::SecretKey, String)>,
    /// A chain head to report instead of the journal's own.
    head: Option<String>,
}

impl ExecutionBackend for JournalingBackend {
    fn execute<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        let dir = self.project.join(nika_dap::store::TRACE_DIR);
        let mut journal =
            TraceFileSink::new(dir).for_execution(context.execution_id(), context.trace_id());
        for event in &self.events {
            journal.emit(event.clone());
        }
        let seal = self.seal.clone();
        let settled = journal.settle_sealed(move |trace| {
            if let Some((secret, public)) = seal {
                nika_dap::journal::seal_journal_with_key(
                    trace,
                    Some(WORKFLOW_HASH),
                    None,
                    move || Some((secret, public)),
                );
            }
        });
        let outcome = ExecutionOutcome::from(ExecutionDisposition::Succeeded);
        let outcome = match (settled, self.head.clone()) {
            (Ok(Some(_)), Some(forged)) => outcome.with_chain_head(forged),
            (Ok(Some(head)), None) => outcome.with_chain_head(head),
            _ => outcome,
        };
        Box::pin(async move { outcome })
    }

    fn trace_journal_dir(&self) -> Option<PathBuf> {
        Some(self.project.join(nika_dap::store::TRACE_DIR))
    }
}

/// A resident over `world`'s project whose artifact door weighs `key` beside the custody
/// defaults (the harness's own start, plus that one key).
async fn start(
    world: &TestWorld,
    backend: Arc<dyn ExecutionBackend>,
    limits: ServerLimits,
    key: Option<&Path>,
) -> TestServer {
    start_holding(world, backend, limits, key).await.0
}

/// [`start`], keeping a handle on the listener's state (its artifact slots).
async fn start_holding(
    world: &TestWorld,
    backend: Arc<dyn ExecutionBackend>,
    limits: ServerLimits,
    key: Option<&Path>,
) -> (TestServer, Arc<AppState>) {
    let resident = ResidentConfig::new(&world.state).with_limits(limits);
    let authority = ResidentAuthority::open(resident, backend)
        .await
        .expect("authority");
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &world.workflows,
        &world.token,
    );
    let bound = BoundServer::attach(config, &authority).await.expect("bind");
    if let Some(key) = key {
        bound
            .state
            .artifacts
            .key
            .set(key.to_path_buf())
            .expect("one key");
    }
    let address = bound.local_addr().expect("local address");
    let shutdown_probe = authority.state.store.shutdown_test_probe();
    let state = Arc::clone(&bound.state);
    let (shutdown, receiver) = oneshot::channel::<()>();
    let join = tokio::spawn(authority.serve_with_http(bound, async move {
        let _stopped = receiver.await;
    }));
    let server = TestServer {
        address,
        shutdown: Some(shutdown),
        join,
        shutdown_probe,
    };
    (server, state)
}

fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn frame(kind: EventKind, fields: &[(&str, String)]) -> Event {
    let fields = fields
        .iter()
        .map(|(key, value)| KeyValue::new(*key, FieldValue::string(value.clone())))
        .collect();
    Event::new(
        EventId::generate(),
        Timestamp::from_unix_ms(1_792_000_000_000),
        kind,
    )
    .with_fields(fields)
}

/// The frame `nika:chart` leaves when it saved `bytes` at `path`.
fn chart_saved(task: &str, path: &str, bytes: &[u8]) -> Event {
    let value = json!({"path": path, "sha256": hex(bytes), "bytes": bytes.len(), "wrote": true});
    let outcome =
        json!({"class": "success", "cause": "normal", "payload": {"attempts": 1, "value": value}});
    frame(
        EventKind::TaskCompleted,
        &[
            ("task", task.to_owned()),
            ("note", "invoke · nika:chart".to_owned()),
            ("outcome", outcome.to_string()),
        ],
    )
}

/// The frame a harness image leaves once the store kept it as the blob `blob`.
fn image_received(task: &str, blob: &str, bytes: &[u8]) -> Event {
    let observation = json!({
        "schema": "nika/harness-image-observation@1", "source": "harness_reported",
        "tool_call_id": "call-1", "mime_type": "image/png", "received_bytes": bytes.len(),
        "blob": {"hash": format!("blake3:{blob}"), "mime_type": "image/png", "size": bytes.len()},
        "storage": "stored", "storage_failure": null, "received_sha256": hex(bytes),
        "reported_saved_path": "/elsewhere/codex/image.png", "file_verified": false,
        "permission_evidence": "separate permit_checked frames"
    });
    frame(
        EventKind::AgentImageObserved,
        &[
            ("task", task.to_owned()),
            ("harness_image", observation.to_string()),
        ],
    )
}

/// What a scripted run left in the project: a chart file and one agent image's blob.
struct Left {
    chart: Vec<u8>,
    image: Vec<u8>,
    blob: PathBuf,
    events: Vec<Event>,
}

/// Write `chart` at `out/chart.svg` and an image blob under `.nika/blobs`, with the journal
/// frames that record them between a run's start and end.
fn leave(world: &TestWorld, chart: Vec<u8>) -> Left {
    let out = world.workflows.join("out");
    std::fs::create_dir_all(&out).expect("out");
    std::fs::write(out.join("chart.svg"), &chart).expect("chart");
    // A PNG signature, then bytes that are not UTF-8: the wire carries bytes, never text.
    let image: Vec<u8> = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        .into_iter()
        .chain((0..4096_u32).map(|n| u8::try_from(n % 256).expect("a byte")))
        .collect();
    // The store names a blob by its BLAKE3; the door only reads that name as a place.
    let name = hex(&image);
    let shard = world.workflows.join(".nika/blobs").join(&name[..2]);
    std::fs::create_dir_all(&shard).expect("blob shard");
    let blob = shard.join(&name[2..]);
    std::fs::write(&blob, &image).expect("blob");
    let events = vec![
        frame(
            EventKind::WorkflowStarted,
            &[("workflow", "root".to_owned())],
        ),
        chart_saved("plot", "./out/chart.svg", &chart),
        image_received("draw", &name, &image),
        frame(
            EventKind::WorkflowCompleted,
            &[("workflow", "root".to_owned())],
        ),
    ];
    Left {
        chart,
        image,
        blob,
        events,
    }
}

/// A resident whose one job, already settled, left `left`, sealed with `key` (unsealed
/// without one); the server, the job id.
async fn settled(
    world: &TestWorld,
    left: &Left,
    key: Option<&Key>,
    limits: ServerLimits,
) -> (TestServer, String) {
    let (server, id, _state) = settled_holding(world, left, key, limits).await;
    (server, id)
}

/// [`settled`], keeping a handle on the listener's state.
async fn settled_holding(
    world: &TestWorld,
    left: &Left,
    key: Option<&Key>,
    limits: ServerLimits,
) -> (TestServer, String, Arc<AppState>) {
    let run = Run {
        seal: key,
        weighed: key.map(|key| key.file.as_path()),
        head: None,
    };
    run_settled(world, left.events.clone(), run, limits).await
}

/// How a scripted run seals its journal and what the door weighs.
struct Run<'a> {
    /// The key the journal is sealed with (unsealed without one).
    seal: Option<&'a Key>,
    /// The public key file the door weighs beside the custody defaults.
    weighed: Option<&'a Path>,
    /// A chain head the backend reports instead of the journal's.
    head: Option<String>,
}

/// A resident whose one job, already settled, journaled `events` as `run` says.
async fn run_settled(
    world: &TestWorld,
    events: Vec<Event>,
    run: Run<'_>,
    limits: ServerLimits,
) -> (TestServer, String, Arc<AppState>) {
    let backend = Arc::new(JournalingBackend {
        project: world.workflows.clone(),
        events,
        seal: run.seal.map(|key| (key.secret.clone(), key.public.clone())),
        head: run.head,
    });
    let (server, state) = start_holding(world, backend, limits, run.weighed).await;
    let id = admit(&server, "artifact-job").await;
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("settled");
    (server, id, state)
}

async fn admit(server: &TestServer, key: &str) -> String {
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika"}"#,
            key,
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    created.json()["id"].as_str().expect("id").to_owned()
}

/// One raw exchange: status, header block, body bytes (a binary body is not UTF-8, and a
/// stream cut short may end in a reset after its bytes).
async fn exchange(server: &TestServer, request: &str) -> (u16, String, Vec<u8>) {
    let mut stream = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("connect");
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut bytes = Vec::new();
    let _read = stream.read_to_end(&mut bytes).await;
    split(&bytes)
}

fn split(bytes: &[u8]) -> (u16, String, Vec<u8>) {
    let end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a header block");
    let headers = String::from_utf8(bytes[..end].to_vec()).expect("UTF-8 headers");
    let status = headers
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status");
    (status, headers, bytes[end + 4..].to_vec())
}

fn header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then_some(value.trim())
    })
}

fn get(path: &str) -> String {
    get_request(path)
}

/// The document declares `status` on `GET template`, the answer validates against it, and a
/// refusal's code is one the declaration names (401 is the shared envelope).
fn assert_declared(document: &Value, template: &str, status: u16, body: &[u8]) {
    assert_declared_on(document, "get", template, status, body);
    if status >= 400 && status != 401 {
        let declared = &document["paths"][template]["get"]["responses"][status.to_string()];
        let description = declared["description"].as_str().unwrap_or_default();
        let named = format!("`{}`", code(body));
        assert!(
            description.contains(&named),
            "GET {template} {status} declares no {named}: {description}"
        );
    }
}

fn code(body: &[u8]) -> String {
    let value: Value = serde_json::from_slice(body).expect("an error envelope");
    value["error"]["code"]
        .as_str()
        .expect("an error code")
        .to_owned()
}

/// Download `name` (raw, as the client sends it) and check the declared answer.
async fn download(
    server: &TestServer,
    document: &Value,
    id: &str,
    name: &str,
) -> (u16, String, Vec<u8>) {
    let answer = exchange(server, &get(&format!("/v1/jobs/{id}/artifacts/{name}"))).await;
    let (status, ref headers, ref body) = answer;
    if status == 200 {
        let declared = &document["paths"][DOWNLOAD]["get"]["responses"]["200"]["headers"];
        for name in [
            "Content-Length",
            "Nika-Artifact-Sha256",
            "Content-Disposition",
            "X-Content-Type-Options",
            "Cache-Control",
        ] {
            assert!(declared.get(name).is_some(), "{name} undeclared");
            assert!(header(headers, name).is_some(), "{name} not sent");
        }
    } else {
        assert_declared(document, DOWNLOAD, status, body);
    }
    answer
}

/// The manifest lists what the sealed journal records, bound to the job's receipt, and each
/// download is those exact bytes with the recorded type, length and digest.
#[tokio::test(flavor = "multi_thread")]
async fn a_settled_jobs_sealed_journal_lists_and_serves_its_artifacts() {
    let world = TestWorld::new();
    let key = test_key(&world);
    let left = leave(
        &world,
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n".to_vec(),
    );
    std::fs::write(world.workflows.join("secret.txt"), "never an artifact").expect("secret");
    let (server, id) = settled(&world, &left, Some(&key), limits()).await;
    let document = served_document(&server).await;

    let (status, headers, body) =
        exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    assert_eq!(header(&headers, "content-type"), Some("application/json"));
    assert_declared(&document, MANIFEST, 200, &body);
    let manifest: Value = serde_json::from_slice(&body).expect("manifest");
    let job = server.request(&get(&format!("/v1/jobs/{id}"))).await.json();
    assert_eq!(
        manifest["receipt"], job["receipt"],
        "bound to the job's receipt"
    );
    assert!(manifest["receipt"]["chain_head"].is_string(), "{manifest}");
    assert_eq!(manifest["verdict"], "sealed");
    assert_eq!(manifest["truncated"], false);
    assert_eq!(
        manifest["max_artifact_bytes"],
        crate::server::config::DEFAULT_MAX_ARTIFACT_BYTES
    );
    let artifacts = manifest["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 2, "{manifest}");
    let file = &artifacts[0];
    assert_eq!(file["name"], "file/out/chart.svg");
    assert_eq!(file["kind"], "file");
    assert_eq!(file["path"], "out/chart.svg");
    assert_eq!(file["size"], left.chart.len());
    assert_eq!(file["sha256"], hex(&left.chart));
    assert_eq!(file["mime"], "image/svg+xml");
    assert_eq!(file["task"], "plot");
    assert_eq!(file["event_id"], left.events[1].id.uuid.to_string());
    let image = &artifacts[1];
    assert_eq!(image["name"], "agent_image/1.png");
    assert_eq!(image["kind"], "agent_image");
    assert!(image.get("path").is_none(), "a blob's place never crosses");
    assert_eq!(image["size"], left.image.len());
    assert_eq!(image["sha256"], hex(&left.image));
    assert_eq!(
        (image["mime"].as_str(), image["task"].as_str()),
        (Some("image/png"), Some("draw"))
    );
    let rendered = String::from_utf8_lossy(&body);
    for private in [
        world.root.path().to_string_lossy().as_ref(),
        ".nika",
        "blake3",
        "secret",
    ] {
        assert!(!rendered.contains(private), "{private} leaked: {rendered}");
    }

    let (status, headers, bytes) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!(status, 200, "{headers}");
    assert_eq!(bytes, left.chart);
    assert_eq!(header(&headers, "content-type"), Some("image/svg+xml"));
    assert_eq!(
        header(&headers, "content-length"),
        Some(left.chart.len().to_string().as_str())
    );
    assert_eq!(
        header(&headers, "nika-artifact-sha256"),
        Some(hex(&left.chart).as_str())
    );
    assert_eq!(header(&headers, "content-disposition"), Some("attachment"));
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));
    assert_eq!(header(&headers, "cache-control"), Some("no-store"));

    let (status, headers, bytes) = download(&server, &document, &id, "agent_image/1.png").await;
    assert_eq!(status, 200, "{headers}");
    assert_eq!(bytes, left.image, "the blob's bytes, byte for byte");
    assert_eq!(header(&headers, "content-type"), Some("image/png"));
    assert_eq!(
        header(&headers, "nika-artifact-sha256"),
        Some(hex(&left.image).as_str())
    );

    // A segment the client percent-encodes addresses the same entry.
    let (status, _, bytes) = download(&server, &document, &id, "file/%6Fut/chart%2Esvg").await;
    assert_eq!((status, bytes), (200, left.chart.clone()));
    server.stop().await.expect("clean stop");
}

/// Names are looked up in the manifest, never resolved on the filesystem: a traversal, an
/// absolute path, an encoded separator or an unlisted project file is an unknown name.
#[tokio::test(flavor = "multi_thread")]
async fn a_name_outside_the_manifest_never_reaches_the_filesystem() {
    let world = TestWorld::new();
    let key = test_key(&world);
    let left = leave(&world, b"<svg/>".to_vec());
    std::fs::write(world.workflows.join("secret.txt"), "never an artifact").expect("secret");
    let (server, id) = settled(&world, &left, Some(&key), limits()).await;
    let document = served_document(&server).await;
    for name in [
        "secret.txt",
        "file/secret.txt",
        "file/../secret.txt",
        "file/out/../../serve.token",
        "file/%2e%2e/secret.txt",
        "file%2F%2e%2e%2Fsecret.txt",
        "file/out/chart.svg/",
        "file//out/chart.svg",
        "/etc/passwd",
        "file/%2Fetc%2Fpasswd",
        "agent_image/../../.nika/serve.token",
        "agent_image/2.png",
        "file/.nika/blobs",
        "file/out/chart.svg%00",
        "file/out/chart.sv%g",
        "",
    ] {
        let (status, _, body) = download(&server, &document, &id, name).await;
        assert_eq!(status, 404, "{name}: {}", String::from_utf8_lossy(&body));
        assert_eq!(code(&body), "artifact_not_found", "{name}");
        assert!(!String::from_utf8_lossy(&body).contains("never an artifact"));
    }
    // The form an `OpenAPI` client sends for one path parameter addresses the same entry.
    for name in ["file%2Fout%2Fchart.svg", "file/out%2Fchart.svg"] {
        let (status, _, bytes) = download(&server, &document, &id, name).await;
        assert_eq!((status, bytes), (200, left.chart.clone()), "{name}");
    }
    server.stop().await.expect("clean stop");
}

/// The recorded place must still hold the recorded bytes: a change of content, a truncation,
/// a removal, a link in place of the file or of a directory on the way is refused 409.
#[tokio::test(flavor = "multi_thread")]
async fn bytes_that_no_longer_match_the_record_are_refused() {
    let world = TestWorld::new();
    let key = test_key(&world);
    let left = leave(&world, b"<svg>recorded</svg>".to_vec());
    let (server, id) = settled(&world, &left, Some(&key), limits()).await;
    let document = served_document(&server).await;
    let chart = world.workflows.join("out/chart.svg");
    std::fs::write(&chart, b"<svg>tampered</svg>").expect("same length, other bytes");
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_changed"),
        "changed in place"
    );
    std::fs::write(&chart, b"<svg>").expect("shorter");
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_changed"),
        "truncated"
    );

    // An identical copy outside the project, linked in place of the file: still refused.
    let outside = world.root.path().join("outside");
    std::fs::create_dir_all(&outside).expect("outside");
    std::fs::write(outside.join("chart.svg"), &left.chart).expect("identical copy");
    std::fs::remove_file(&chart).expect("remove");
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_changed"),
        "removed"
    );
    std::os::unix::fs::symlink(outside.join("chart.svg"), &chart).expect("file link");
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_changed"),
        "a final link"
    );
    // The whole directory on the way replaced by a link to one holding the same bytes.
    std::fs::remove_dir_all(world.workflows.join("out")).expect("drop out");
    std::os::unix::fs::symlink(&outside, world.workflows.join("out")).expect("dir link");
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_changed"),
        "a link on the way"
    );

    // The blob too: a changed blob is not the image the harness sent.
    let mut changed = left.image.clone();
    changed[9] ^= 0xff;
    std::fs::write(&left.blob, &changed).expect("change the blob");
    let (status, _, body) = download(&server, &document, &id, "agent_image/1.png").await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_changed"),
        "blob"
    );
    // The manifest still says what the journal records.
    let (status, _, body) = exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!(status, 200);
    assert_declared(&document, MANIFEST, 200, &body);
    server.stop().await.expect("clean stop");
}

/// A file changed while it streams never arrives whole: the stream ends before its
/// `Content-Length` and the client sees a cut body, never a complete one.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_changed_while_it_streams_ends_short_of_its_length() {
    let world = TestWorld::new();
    let key = test_key(&world);
    // Far larger than the loopback socket buffers: the stream waits on the reader.
    let size = 48 * 1024 * 1024;
    let chart: Vec<u8> = (0..size)
        .map(|n| u8::try_from(n % 199).expect("a byte"))
        .collect();
    let left = leave(&world, chart);
    let (server, id, state) = settled_holding(&world, &left, Some(&key), limits()).await;
    let mut stream = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("connect");
    let request = get(&format!("/v1/jobs/{id}/artifacts/file/out/chart.svg"));
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut received = Vec::new();
    while !received.windows(4).any(|window| window == b"\r\n\r\n") {
        let mut chunk = [0_u8; 4096];
        let read = stream.read(&mut chunk).await.expect("headers");
        assert!(read > 0, "the response starts");
        received.extend_from_slice(&chunk[..read]);
    }
    let (status, headers, _) = split(&received);
    assert_eq!(status, 200, "{headers}");
    assert_eq!(
        header(&headers, "content-length"),
        Some(size.to_string().as_str())
    );
    assert_eq!(
        state.artifacts.slots.available_permits(),
        super::super::artifacts::ARTIFACT_SLOTS,
        "a stream held back by its reader holds no slot"
    );
    // Change the tail in place while the stream is held back by the unread socket.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(world.workflows.join("out/chart.svg"))
        .expect("open");
    let tail = u64::try_from(size - 16).expect("an offset");
    std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(tail)).expect("seek");
    std::io::Write::write_all(&mut file, b"tampered-in-tail").expect("change");
    drop(file);
    let _rest = stream.read_to_end(&mut received).await;
    let (_, _, body) = split(&received);
    assert!(
        body.len() < size,
        "a changed file must not arrive whole: {} of {size}",
        body.len()
    );
    assert_eq!(
        &body[..],
        &left.chart[..body.len()],
        "what arrived is the record's prefix"
    );
    server.stop().await.expect("clean stop");
}

/// The bound is the server's: a larger artifact stays listed, the manifest says the bound,
/// and its download is refused before a byte is read.
#[tokio::test(flavor = "multi_thread")]
async fn an_artifact_over_the_bound_stays_listed_and_is_refused() {
    let world = TestWorld::new();
    let key = test_key(&world);
    let left = leave(&world, vec![b'x'; 2048]);
    let tight = limits().with_max_artifact_bytes(1024);
    let (server, id) = settled(&world, &left, Some(&key), tight).await;
    let document = served_document(&server).await;
    let manifest = server
        .request(&get(&format!("/v1/jobs/{id}/artifacts")))
        .await
        .json();
    assert_eq!(manifest["max_artifact_bytes"], 1024);
    assert_eq!(manifest["artifacts"][0]["size"], 2048);
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!((status, code(&body).as_str()), (403, "artifact_too_large"));
    // The image (4104 bytes) is over the bound as well, and refused the same way.
    let (status, _, body) = download(&server, &document, &id, "agent_image/1.png").await;
    assert_eq!((status, code(&body).as_str()), (403, "artifact_too_large"));
    server.stop().await.expect("clean stop");
}

/// A journal that is not the sealed one the receipt names serves nothing: unsealed,
/// edited after settlement, removed, or never kept by the backend.
#[tokio::test(flavor = "multi_thread")]
async fn a_journal_that_is_not_sealed_and_intact_serves_nothing() {
    let world = TestWorld::new();
    let left = leave(&world, b"<svg/>".to_vec());
    let (server, id) = settled(&world, &left, None, limits()).await;
    let document = served_document(&server).await;
    let manifest = exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!(manifest.0, 409);
    assert_eq!(code(&manifest.2), "artifact_journal_unverified", "unsealed");
    assert_declared(&document, MANIFEST, 409, &manifest.2);
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_journal_unverified")
    );
    server.stop().await.expect("clean stop");

    let world = TestWorld::new();
    let key = test_key(&world);
    let left = leave(&world, b"<svg/>".to_vec());
    let (server, id) = settled(&world, &left, Some(&key), limits()).await;
    let document = served_document(&server).await;
    let journal = std::fs::read_dir(world.workflows.join(nika_dap::store::TRACE_DIR))
        .expect("traces")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .expect("the journal");
    let original = std::fs::read_to_string(&journal).expect("journal");
    let edited = original.replacen("\"plot\"", "\"plat\"", 1);
    assert_ne!(edited, original);
    std::fs::write(&journal, &edited).expect("edit a line");
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_journal_unverified"),
        "edited"
    );
    std::fs::remove_file(&journal).expect("remove the journal");
    let manifest = exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!(
        (manifest.0, code(&manifest.2).as_str()),
        (409, "artifact_journal_unavailable")
    );
    assert_declared(&document, MANIFEST, 409, &manifest.2);
    server.stop().await.expect("clean stop");

    // A backend that keeps no journal: the receipt names no head.
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = start(&world, backend, limits(), None).await;
    let id = admit(&server, "no-journal").await;
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("settled");
    let manifest = exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!(
        (manifest.0, code(&manifest.2).as_str()),
        (409, "artifact_journal_unavailable")
    );
    server.stop().await.expect("clean stop");
}

/// A job still queued or running has no artifacts yet; an unknown, foreign or malformed id
/// answers as the job door does; both routes need the bearer token.
#[tokio::test(flavor = "multi_thread")]
async fn only_an_authenticated_caller_reads_a_settled_jobs_artifacts() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::hangs());
    let server = start(&world, backend, short_shutdown_limits(), None).await;
    let document = served_document(&server).await;
    let id = admit(&server, "still-running").await;
    wait_for_status(&server, &id, "running")
        .await
        .expect("running");
    for path in [
        format!("/v1/jobs/{id}/artifacts"),
        format!("/v1/jobs/{id}/artifacts/file/out/chart.svg"),
    ] {
        let (status, _, body) = exchange(&server, &get(&path)).await;
        assert_eq!(
            (status, code(&body).as_str()),
            (409, "job_not_settled"),
            "{path}"
        );
        let template = if path.ends_with("/artifacts") {
            MANIFEST
        } else {
            DOWNLOAD
        };
        assert_declared(&document, template, 409, &body);
    }
    let unknown = server
        .request(&get(&format!("/v1/jobs/{}", JobId::random())))
        .await;
    for raw in [
        JobId::random().to_string(),
        "not-a-job-id".to_owned(),
        "..".to_owned(),
    ] {
        for path in [
            format!("/v1/jobs/{raw}/artifacts"),
            format!("/v1/jobs/{raw}/artifacts/file/x"),
        ] {
            let (status, _, body) = exchange(&server, &get(&path)).await;
            assert_eq!(status, 404, "{path}");
            assert_eq!(
                String::from_utf8_lossy(&body),
                unknown.body,
                "{path}: one shape"
            );
            let template = if path.ends_with("/artifacts") {
                MANIFEST
            } else {
                DOWNLOAD
            };
            assert_declared(&document, template, 404, &body);
        }
    }
    for path in [
        format!("/v1/jobs/{id}/artifacts"),
        format!("/v1/jobs/{id}/artifacts/file/x"),
    ] {
        let anonymous =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let wrong = format!(
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: Bearer {}\r\n\r\n",
            "w".repeat(TOKEN.len())
        );
        for request in [anonymous, wrong] {
            let (status, headers, body) = exchange(&server, &request).await;
            assert_eq!(
                (status, code(&body).as_str()),
                (401, "unauthorized"),
                "{path}"
            );
            assert!(header(&headers, "www-authenticate").is_some_and(|v| v.starts_with("Bearer")));
            let template = if path.ends_with("/artifacts") {
                MANIFEST
            } else {
                DOWNLOAD
            };
            assert_declared(&document, template, 401, &body);
        }
    }
    assert!(matches!(
        server.stop().await,
        Err(ServerError::ShutdownTimeout)
    ));
    // Interrupted is settled, but its journal (none here) records nothing.
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = start(&world, backend, limits(), None).await;
    wait_for_status(&server, &id, "interrupted")
        .await
        .expect("interrupted");
    let (status, _, body) = exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_journal_unavailable")
    );
    server.stop().await.expect("clean stop");
}

/// Every artifact slot busy: both routes refuse at once (nothing queues), as declared, and the
/// next request after a slot frees is served.
#[tokio::test(flavor = "multi_thread")]
async fn a_full_set_of_artifact_slots_refuses_the_next_read() {
    let world = TestWorld::new();
    let key = test_key(&world);
    let left = leave(&world, b"<svg/>".to_vec());
    let (server, id, state) = settled_holding(&world, &left, Some(&key), limits()).await;
    let document = served_document(&server).await;
    let held = Arc::clone(&state.artifacts.slots)
        .try_acquire_many_owned(
            u32::try_from(super::super::artifacts::ARTIFACT_SLOTS).expect("slots"),
        )
        .expect("every slot");
    let (status, _, body) = exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!((status, code(&body).as_str()), (503, "artifact_capacity"));
    assert_declared(&document, MANIFEST, 503, &body);
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!((status, code(&body).as_str()), (503, "artifact_capacity"));
    drop(held);
    let (status, _, bytes) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!((status, bytes), (200, left.chart.clone()));
    assert_eq!(
        state.artifacts.slots.available_permits(),
        super::super::artifacts::ARTIFACT_SLOTS,
        "a finished stream holds no slot"
    );
    server.stop().await.expect("clean stop");
}

/// A paused job may resume: its observation ended, but it has not settled, so it has no
/// artifacts yet.
#[tokio::test(flavor = "multi_thread")]
async fn a_paused_job_has_no_artifacts_yet() {
    let world = TestWorld::new();
    let pausing = Arc::new(TestBackend::completes(ExecutionDisposition::Paused));
    let server = start(&world, pausing, limits(), None).await;
    let document = served_document(&server).await;
    let id = admit(&server, "paused-job").await;
    wait_for_status(&server, &id, "paused")
        .await
        .expect("paused");
    let (status, _, body) = exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!((status, code(&body).as_str()), (409, "job_not_settled"));
    assert_declared(&document, MANIFEST, 409, &body);
    server.stop().await.expect("clean stop");
}

/// A journal the server cannot attribute (sealed with a key it does not weigh), or whose head
/// is not the one the receipt names, serves nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_journal_the_server_cannot_attribute_or_bind_serves_nothing() {
    let world = TestWorld::new();
    let key = test_key(&world);
    let left = leave(&world, b"<svg/>".to_vec());
    let foreign = Run {
        seal: Some(&key),
        weighed: None,
        head: None,
    };
    let (server, id, _state) = run_settled(&world, left.events.clone(), foreign, limits()).await;
    let document = served_document(&server).await;
    let (status, _, body) = exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_journal_unverified"),
        "a seal no weighed key attributes"
    );
    assert_declared(&document, MANIFEST, 409, &body);
    server.stop().await.expect("clean stop");

    let world = TestWorld::new();
    let key = test_key(&world);
    let left = leave(&world, b"<svg/>".to_vec());
    let rebound = Run {
        seal: Some(&key),
        weighed: Some(key.file.as_path()),
        head: Some("0".repeat(64)),
    };
    let (server, id, _state) = run_settled(&world, left.events.clone(), rebound, limits()).await;
    let job = server.request(&get(&format!("/v1/jobs/{id}"))).await.json();
    assert_eq!(job["receipt"]["chain_head"], "0".repeat(64));
    let (status, _, body) = download(&server, &document, &id, "file/out/chart.svg").await;
    assert_eq!(
        (status, code(&body).as_str()),
        (409, "artifact_journal_unverified"),
        "a sealed journal whose head is not the receipt's"
    );
    server.stop().await.expect("clean stop");
}

/// A zero-byte artifact downloads as an empty body, and an entry past the listed bound is
/// still served by its name.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_artifact_and_one_past_the_listed_bound_are_served() {
    let world = TestWorld::new();
    let key = test_key(&world);
    let out = world.workflows.join("out");
    std::fs::create_dir_all(&out).expect("out");
    std::fs::write(out.join("empty.svg"), b"").expect("empty");
    let last = b"<svg>past the bound</svg>".to_vec();
    std::fs::write(out.join("last.svg"), &last).expect("last");
    let listed = super::super::artifacts::MAX_LISTED;
    let mut events = vec![chart_saved("plot", "out/empty.svg", b"")];
    events.extend((1..listed).map(|n| chart_saved("plot", &format!("out/{n}.svg"), b"x")));
    events.push(chart_saved("plot", "out/last.svg", &last));
    let run = Run {
        seal: Some(&key),
        weighed: Some(key.file.as_path()),
        head: None,
    };
    let (server, id, _state) = run_settled(&world, events, run, limits()).await;
    let document = served_document(&server).await;
    let manifest = server
        .request(&get(&format!("/v1/jobs/{id}/artifacts")))
        .await
        .json();
    assert_eq!(manifest["truncated"], true);
    assert_eq!(manifest["artifacts"].as_array().map(Vec::len), Some(listed));
    assert_eq!(manifest["artifacts"][0]["size"], 0);
    let (status, headers, bytes) = download(&server, &document, &id, "file/out/empty.svg").await;
    assert_eq!(status, 200, "{headers}");
    assert_eq!(header(&headers, "content-length"), Some("0"));
    assert!(bytes.is_empty());
    let (status, _, bytes) = download(&server, &document, &id, "file/out/last.svg").await;
    assert_eq!((status, bytes), (200, last));
    server.stop().await.expect("clean stop");
}

/// The production backend's own journal: a `nika:chart` run's frame is the shape the door
/// reads, the seal its custody seam mints verifies, and the served SVG is the file on disk.
#[tokio::test(flavor = "multi_thread")]
async fn a_real_chart_run_lists_and_serves_its_svg() {
    let world = TestWorld::new();
    let key = test_key(&world);
    std::fs::write(world.workflows.join("chart.nika"), CHART_WORKFLOW).expect("workflow");
    let backend = Arc::new(
        ResidentExecutionBackend::new(&world.workflows).with_journal_seal(Arc::new(TestSeal {
            secret: key.secret.clone(),
            public: key.public.clone(),
        })),
    );
    let long = ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(30),
        Duration::from_millis(200),
        4,
        16,
        64,
        32,
    );
    let server = start(&world, backend, long, Some(&key.file)).await;
    let document = served_document(&server).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"chart.nika"}"#,
            "chart-run",
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("id").to_owned();
    let mut status = String::new();
    for _ in 0..600 {
        status = server
            .request(&get(&format!("/v1/jobs/{id}/status")))
            .await
            .json()["status"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if status != "queued" && status != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(status, "succeeded");
    let written = std::fs::read(world.workflows.join("out/totals.svg")).expect("the chart");
    let (code_, _, body) = exchange(&server, &get(&format!("/v1/jobs/{id}/artifacts"))).await;
    assert_eq!(code_, 200, "{}", String::from_utf8_lossy(&body));
    assert_declared(&document, MANIFEST, 200, &body);
    let manifest: Value = serde_json::from_slice(&body).expect("manifest");
    assert_eq!(manifest["verdict"], "sealed", "{manifest}");
    let entry = &manifest["artifacts"][0];
    assert_eq!(entry["name"], "file/out/totals.svg", "{manifest}");
    assert_eq!(entry["task"], "draw");
    assert_eq!(entry["size"], written.len());
    assert_eq!(entry["sha256"], hex(&written));
    assert_eq!(manifest["artifacts"].as_array().map(Vec::len), Some(1));
    let (status, headers, bytes) = download(&server, &document, &id, "file/out/totals.svg").await;
    assert_eq!(status, 200, "{headers}");
    assert_eq!(bytes, written);
    server.stop().await.expect("clean stop");
}
