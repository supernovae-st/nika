// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A seated harness stores received image bytes in the admitted project held
//! by descriptor, never through an ambient path. Each store operation owns a
//! finite room, so the same runtime runs again; a refused anchor refuses the
//! seat. Disposable directories only, no network, no provider.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nika_kernel::BlobError;
use nika_kernel::ai::harness::{
    DynAgentBackend, HarnessError, HarnessEvent, HarnessEventStream, HarnessImage, HarnessOutcome,
    HarnessRequest,
};
use nika_kernel::blob::BlobStoreDyn;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_runtime_laws::image_room::ImageRoom;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

use super::*;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nreceived fixture bytes";

/// Every delegated turn receives a distinct image, then the unchanged text.
struct ImageTape {
    turns: AtomicUsize,
    reported_path: String,
    requested_models: std::sync::Mutex<Vec<Option<String>>>,
}

impl ImageTape {
    fn new(reported_path: &str) -> Self {
        Self {
            turns: AtomicUsize::new(0),
            reported_path: reported_path.to_owned(),
            requested_models: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl DynAgentBackend for ImageTape {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>,
    > {
        self.requested_models
            .lock()
            .unwrap()
            .push(request.requested_model);
        let turn = self.turns.fetch_add(1, Ordering::SeqCst);
        let bytes = [PNG, format!(" turn {turn}").as_bytes()].concat();
        let mut image = HarnessImage::new(format!("image-{turn}"));
        image.received_bytes = Some(bytes.len() as u64);
        image.data = Some(bytes.into());
        image.mime_type = Some("image/png".into());
        image.reported_saved_path = Some(self.reported_path.clone());
        let events = vec![
            Ok(HarnessEvent::ImageActivityObserved {
                tool_call_id: format!("image-{turn}"),
            }),
            Ok(HarnessEvent::ImageObserved {
                image: Box::new(image),
            }),
            Ok(HarnessEvent::Completed {
                outcome: Box::new(HarnessOutcome::new("unchanged text")),
            }),
        ];
        Box::pin(
            async move { Ok(Box::pin(futures_util::stream::iter(events)) as HarnessEventStream) },
        )
    }
}

type Seated = Runtime<
    MockShell,
    MockToolExecutor,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
>;

fn runtime(root: &std::path::Path) -> Seated {
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(nika_providers::ProviderRegistry::without_http(
                nika_providers::ProvidersConfig::new(),
            )),
            "mock/echo",
        ),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default().with_sandbox_root(root.to_path_buf()),
    )
}

/// A fresh disposable world: `project` (admitted) and `outside` (a sentinel).
struct World {
    base: std::path::PathBuf,
    project: std::path::PathBuf,
    outside: std::path::PathBuf,
}

impl World {
    fn new(name: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("nika-image-room-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (project, outside) = (base.join("project"), base.join("outside"));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), b"unchanged").unwrap();
        Self {
            base,
            project,
            outside,
        }
    }

    fn outside_intact(&self) -> bool {
        std::fs::read(self.outside.join("sentinel")).unwrap() == b"unchanged"
            && std::fs::read_dir(&self.outside).unwrap().count() == 1
    }

    fn blob(&self, hash: &str) -> Vec<u8> {
        let raw = hash.strip_prefix("blake3:").expect("CAS key");
        std::fs::read(
            self.project
                .join(".nika/blobs")
                .join(&raw[..2])
                .join(&raw[2..]),
        )
        .unwrap()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn image_frames(events: &[Event]) -> Vec<serde_json::Value> {
    events
        .iter()
        .filter(|e| e.kind == EventKind::AgentImageObserved)
        .map(|e| {
            let raw = e.fields.iter().find(|f| f.key == "harness_image").unwrap();
            let FieldValue::String(raw) = &raw.value else {
                panic!("image metadata is JSON text");
            };
            serde_json::from_str(raw).unwrap()
        })
        .collect()
}

async fn run_once(runtime: &Seated) -> (RunOutcome, Vec<Event>) {
    let yaml = "nika: image-room\nmodel: mock/echo\npermits: {}\ntasks:\n  draw:\n    agent:\n      \
                prompt: \"draw\"\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{report:?}");
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles");
    (outcome, sink.into_events())
}

#[tokio::test]
async fn received_bytes_land_in_the_held_project_and_the_runtime_runs_again() {
    let world = World::new("runs");
    let backend = Arc::new(ImageTape::new("/outside/never-opened.png"));
    let runtime = runtime(&world.project)
        .with_harness_backend(backend.clone(), "tape".into())
        .expect("the project is held as the image room");
    assert!(
        !world.project.join(".nika").exists(),
        "seating writes nothing"
    );
    let mut hashes = Vec::new();
    for turn in 0..2 {
        let (outcome, events) = run_once(&runtime).await;
        assert!(outcome.ok, "run {turn} on the same runtime: {events:?}");
        let frames = image_frames(&events);
        assert_eq!(frames.len(), 1, "{events:?}");
        let hash = frames[0]["blob"]["hash"].as_str().expect("stored locator");
        let expected = [PNG, format!(" turn {turn}").as_bytes()].concat();
        assert_eq!(world.blob(hash), expected, "the received bytes, exactly");
        assert_eq!(
            frames[0]["reported_saved_path"],
            "/outside/never-opened.png"
        );
        assert_eq!(frames[0]["file_verified"], false);
        assert_eq!(
            frames[0]["storage"], "stored",
            "the store answer settled the receipt"
        );
        let terminal = events
            .iter()
            .rfind(|e| e.kind == EventKind::TaskCompleted)
            .unwrap();
        assert!(
            terminal
                .fields
                .iter()
                .any(|f| f.key == "harness_media_count" && f.value == FieldValue::Int(1))
        );
        // The run returned after its terminal: nothing is published afterwards.
        let settled = tree(&world.project);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            tree(&world.project),
            settled,
            "no blob write after the run returned"
        );
        hashes.push(hash.to_owned());
    }
    assert_ne!(hashes[0], hashes[1], "two runs, two stored images");
    assert_eq!(
        *backend.requested_models.lock().unwrap(),
        vec![Some("mock/echo".to_owned()), Some("mock/echo".to_owned())],
        "each model-less agent task delegates the effective envelope model"
    );
    assert!(world.outside_intact());
}

#[cfg(unix)]
#[tokio::test]
async fn a_linked_project_root_refuses_the_seat_without_any_fallback() {
    let world = World::new("link-root");
    let linked = world.base.join("linked");
    std::os::unix::fs::symlink(&world.outside, &linked).unwrap();
    let refused = runtime(&linked).with_harness_backend(
        Arc::new(ImageTape::new("/outside/never-opened.png")),
        "tape".into(),
    );
    let Err(error) = refused else {
        panic!("a symlinked root is never held as the image room");
    };
    assert!(error.to_string().contains("image room"), "{error}");
    assert!(world.outside_intact(), "nothing written outside");
}

#[tokio::test]
async fn each_operation_owns_a_finite_room_and_the_store_stays_usable() {
    let world = World::new("ops");
    let room = ImageRoom::open(&world.project).unwrap();
    assert!(!world.project.join(".nika").exists());
    let first = room
        .put(bytes::Bytes::from_static(b"one"), "image/png")
        .await
        .unwrap();
    let second = room
        .put(bytes::Bytes::from_static(b"two"), "image/png")
        .await
        .unwrap();
    assert_ne!(first.hash, second.hash);
    assert_eq!(room.get(&first.hash).await.unwrap().as_ref(), b"one");
    assert_eq!(room.stat(&second.hash).await.unwrap().size, 3);
    assert!(room.exists(&first.hash).await);
    room.delete(&first.hash).await.unwrap();
    assert!(!room.exists(&first.hash).await);
    let max = usize::try_from(nika_runtime_laws::image_room::IMAGE_MAX_BYTES).unwrap();
    assert!(matches!(
        room.put(vec![0_u8; max + 1].into(), "image/png").await,
        Err(BlobError::TooLarge { .. })
    ));
    assert!(world.outside_intact());
}

/// Every file below the project, with its bytes: what a late write would change.
fn tree(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// Occupy the sole blocking worker before a real image write is registered.
/// Dropping the sender releases it even on a failed assertion; expiration is
/// reported as false and can never qualify the cancellation proof.
fn hold_image_worker() -> (std::sync::mpsc::Sender<()>, tokio::task::JoinHandle<bool>) {
    let (entered, started) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let worker = tokio::task::spawn_blocking(move || {
        entered.send(()).unwrap();
        released
            .recv_timeout(std::time::Duration::from_secs(10))
            .is_ok()
    });
    started
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("HARNESS_INVALID: the sole blocking worker did not enter");
    (release, worker)
}

fn cancel_held_image_put_and_drain(room: &ImageRoom) {
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let mut put = Box::pin(room.put(bytes::Bytes::from_static(PNG), "image/png"));
    assert!(std::future::Future::poll(put.as_mut(), &mut cx).is_pending());
    drop(put);
    assert_eq!(room.pending_drains(), 1, "the cancelled put is sealed");
    let mut drain = Box::pin(room.drain_dropped());
    assert!(std::future::Future::poll(drain.as_mut(), &mut cx).is_pending());
    assert_eq!(room.pending_drains(), 0, "the drain owns its held join");
    drop(drain);
    assert_eq!(room.pending_drains(), 1, "cancellation returns that join");
}

struct ObserveHeldImageRun {
    kinds: Arc<std::sync::Mutex<Vec<EventKind>>>,
    task_settled: Option<tokio::sync::oneshot::Sender<()>>,
    shard: std::path::PathBuf,
}

impl EventSink for ObserveHeldImageRun {
    fn emit(&mut self, event: Event) {
        if event.kind.is_terminal() {
            assert!(
                self.shard.is_dir(),
                "the Run terminal must follow the held directory write"
            );
        }
        self.kinds.lock().unwrap().push(event.kind);
        if event.kind == EventKind::TaskCompleted
            && let Some(settled) = self.task_settled.take()
        {
            let _ = settled.send(());
        }
    }
}

/// No timing-dependent branch: the put and both drains must be Pending.
/// The held effect is the real put's first `RootedFs` write (create shard), not
/// a simulated receipt or a claim that blob publication has already begun.
#[test]
fn held_image_write_delays_run_terminal_and_cancelled_drain_resumes() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(async {
        let world = World::new("held-drain");
        let room = Arc::new(ImageRoom::open(&world.project).unwrap());
        let (release, worker) = hold_image_worker();
        cancel_held_image_put_and_drain(&room);
        assert!(!world.project.join(".nika").exists());
        let mut runner = runtime(&world.project);
        runner.shell = ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("ok").enqueue_ok("ok")));
        runner.image_room = Some(Arc::clone(&room));
        let wf = nika_schema::parse(
            "nika: held-image\npermits: { exec: [\"true\"] }\ntasks:\n  done:\n    exec: { command: [\"true\"] }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        ).unwrap();
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "{report:?}");
        let (settled, task_settled) = tokio::sync::oneshot::channel();
        let kinds = Arc::new(std::sync::Mutex::new(Vec::new()));
        let raw = blake3::hash(PNG).to_hex().to_string();
        let mut sink = ObserveHeldImageRun {
            kinds: Arc::clone(&kinds),
            task_settled: Some(settled),
            shard: world.project.join(".nika/blobs").join(&raw[..2]),
        };
        let mut stamper = DeterministicStamper::new();
        let mut run = Box::pin(runner.run(&wf, &report, &mut stamper, &mut sink));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::select! {
                result = run.as_mut() => panic!("Run answered before releasing the write: {result:?}"),
                result = task_settled => result.expect("task terminal must be observed"),
            }
        }).await.expect("the unrelated mock task settles while the image write is held");
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(std::future::Future::poll(run.as_mut(), &mut cx).is_pending());
        assert_eq!(room.pending_drains(), 0, "the real Run owns the resumed drain");
        assert!(kinds.lock().unwrap().contains(&EventKind::TaskCompleted));
        assert!(!kinds.lock().unwrap().iter().any(EventKind::is_terminal));
        assert!(!world.project.join(".nika").exists());
        release.send(()).expect("the test, not the watchdog, releases the worker");
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), run)
            .await.expect("the released Run drains").unwrap();
        assert!(worker.await.unwrap(), "HARNESS_INVALID: the hold expired");
        assert!(outcome.ok);
        assert_eq!(room.pending_drains(), 0);
        assert_eq!(kinds.lock().unwrap().iter().filter(|kind| kind.is_terminal()).count(), 1);
        assert!(sink.shard.is_dir(), "the abandoned put's registered write completed");
        assert!(!sink.shard.join(&raw[2..]).exists(), "the cancelled future never reached blob publication");
        let stored = room.put(bytes::Bytes::from_static(PNG), "image/png").await.unwrap();
        assert_eq!(world.blob(&stored.hash), PNG, "a fresh lease remains writable");
        let mut second = VecSink::new();
        let outcome = runner.run(&wf, &report, &mut DeterministicStamper::new(), &mut second).await.unwrap();
        assert!(outcome.ok, "the same runtime remains reusable");
        assert_eq!(room.pending_drains(), 0);
        assert!(world.outside_intact());
    });
}

#[tokio::test]
async fn a_put_cancelled_midway_is_sealed_joined_and_never_publishes_later() {
    let world = World::new("dropped");
    let room = ImageRoom::open(&world.project).unwrap();
    let max = usize::try_from(nika_runtime_laws::image_room::IMAGE_MAX_BYTES).unwrap();
    let data = bytes::Bytes::from(vec![7_u8; max]);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let mut put = Box::pin(room.put(data.clone(), "image/png"));
    let first = std::future::Future::poll(put.as_mut(), &mut cx);
    drop(put);
    if first.is_pending() {
        assert_eq!(
            room.pending_drains(),
            1,
            "the cancelled lease is sealed and listed"
        );
    }
    room.drain_dropped().await;
    assert_eq!(
        room.pending_drains(),
        0,
        "the registry is empty after the drain"
    );
    let settled = tree(&world.project);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        tree(&world.project),
        settled,
        "nothing is published after the drain"
    );
    // A blob is either absent or whole: publication is exclusive and atomic.
    let raw = blake3::hash(&data).to_hex().to_string();
    let path = world
        .project
        .join(".nika/blobs")
        .join(&raw[..2])
        .join(&raw[2..]);
    if let Ok(bytes) = std::fs::read(&path) {
        assert_eq!(bytes, data.as_ref());
    }
    let again = room.put(data.clone(), "image/png").await.unwrap();
    assert_eq!(room.get(&again.hash).await.unwrap(), data);
    assert_eq!(room.pending_drains(), 0);
    assert!(world.outside_intact());
}

#[tokio::test]
async fn a_cancelled_drain_hands_its_joins_back_and_the_next_drain_waits() {
    let world = World::new("drain-cancel");
    let room = ImageRoom::open(&world.project).unwrap();
    let max = usize::try_from(nika_runtime_laws::image_room::IMAGE_MAX_BYTES).unwrap();
    let data = bytes::Bytes::from(vec![9_u8; max]);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let mut put = Box::pin(room.put(data.clone(), "image/png"));
    let put_first = std::future::Future::poll(put.as_mut(), &mut cx);
    drop(put);
    let listed = room.pending_drains();
    assert_eq!(
        listed,
        usize::from(put_first.is_pending()),
        "a dropped lease is listed sealed"
    );
    let mut drain = Box::pin(room.drain_dropped());
    let drain_first = std::future::Future::poll(drain.as_mut(), &mut cx);
    drop(drain);
    if drain_first.is_pending() {
        assert_eq!(
            room.pending_drains(),
            listed,
            "an abandoned drain hands its joins back"
        );
    }
    room.drain_dropped().await;
    assert_eq!(room.pending_drains(), 0, "the resumed drain joined them");
    let settled = tree(&world.project);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        tree(&world.project),
        settled,
        "nothing is published after the drain"
    );
    assert!(world.outside_intact());
}

#[cfg(unix)]
#[tokio::test]
async fn a_linked_store_parent_refuses_without_outside_effects() {
    let world = World::new("link-parent");
    std::os::unix::fs::symlink(&world.outside, world.project.join(".nika")).unwrap();
    let room = ImageRoom::open(&world.project).unwrap();
    for _ in 0..2 {
        assert!(matches!(
            room.put(bytes::Bytes::from_static(b"x"), "image/png").await,
            Err(BlobError::Io { .. })
        ));
    }
    assert!(world.outside_intact());
}

/// A real fan-out: 256 agent iterations, each receiving one image whose reported
/// path is 4 KiB, so the receipt rows exceed 1 MiB in aggregate. Every row rides
/// its own small frame with its iteration; the terminal only counts them.
#[tokio::test]
async fn a_fan_out_above_one_mebibyte_keeps_every_row_in_bounded_frames() {
    let world = World::new("fan-out");
    let long_path = format!("/outside/{}", "p".repeat(4096));
    let runtime = runtime(&world.project)
        .with_harness_backend(Arc::new(ImageTape::new(&long_path)), "tape".into())
        .expect("the project is held as the image room");
    let items: Vec<String> = (0..256).map(|i| i.to_string()).collect();
    let yaml = format!(
        "nika: image-fan-out\nmodel: mock/echo\npermits: {{}}\nconst:\n  items: [{}]\ntasks:\n  \
         draw:\n    for_each: {{ items: \"${{{{ const.items }}}}\", max_parallel: 8 }}\n    agent:\n      \
         prompt: \"draw ${{{{ item }}}}\"\n",
        items.join(", ")
    );
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{report:?}");
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles");
    let events = sink.into_events();
    assert!(outcome.ok, "{:?}", events.last());
    for event in &events {
        let line = serde_json::to_vec(event).unwrap().len();
        assert!(line < 64 * 1024, "{:?} frame of {line} bytes", event.kind);
    }
    let frames: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == EventKind::AgentImageObserved)
        .collect();
    assert_eq!(frames.len(), 256);
    let mut iterations: Vec<i64> = frames
        .iter()
        .map(|e| {
            match &e
                .fields
                .iter()
                .find(|f| f.key == "iteration")
                .unwrap()
                .value
            {
                FieldValue::Int(i) => *i,
                other => panic!("iteration {other:?}"),
            }
        })
        .collect();
    iterations.sort_unstable();
    assert_eq!(
        iterations,
        (0..256).collect::<Vec<i64>>(),
        "each item names its row"
    );
    let observed = image_frames(&events);
    let mut hashes: Vec<&str> = observed
        .iter()
        .map(|f| f["blob"]["hash"].as_str().unwrap())
        .collect();
    hashes.sort_unstable();
    hashes.dedup();
    assert_eq!(hashes.len(), 256, "every received image was stored");
    assert!(
        observed
            .iter()
            .all(|f| f["reported_saved_path"] == long_path.as_str())
    );
    let terminal = events
        .iter()
        .rfind(|e| e.kind == EventKind::TaskCompleted)
        .unwrap();
    assert!(
        terminal
            .fields
            .iter()
            .any(|f| f.key == "harness_media_count" && f.value == FieldValue::Int(256))
    );
    assert!(!terminal.fields.iter().any(|f| f.key == "harness_media"));
    let record = &outcome.records["draw"];
    assert_eq!(
        record.harness_media.len(),
        256,
        "the embedder keeps every row"
    );
    assert!(serde_json::to_vec(&record.harness_media).unwrap().len() > 1024 * 1024);
    assert_eq!(
        record.output,
        serde_json::json!(vec!["unchanged text"; 256])
    );
    assert!(world.outside_intact());
}
