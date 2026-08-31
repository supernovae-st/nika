// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! `nika serve` — LE TIREUR RÉSIDENT (W5) plus loopback HTTP (W06).
//! Default: the SAME `fire` (D2), the wall clock in place of the OS.
//! Gate 1: the resident firer reads ONLY `nika.yaml` (vocab + cadence
//! judge it before any shot) and its own sidecar. Durable execution authority
//! is always present in persistent mode; HTTP is an optional second door.
// A server's whole job is its log lines (the run/mod.rs precedent).
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use super::arm::{
    self,
    fire::{ExecutionRunSeam, FireCtx, Wait, WaitSeam, fire_beat, labels},
    state::ArmState,
};
use super::{VerbOutput, exit};
use jiff::{SignedDuration, Zoned};
use nika_cadence::registry::{ArmRegistry, Locus};
use std::path::{Path, PathBuf};
/// `nika serve` — the resident firer's args, plus the explicit HTTP pair.
#[derive(Debug, Clone, clap::Args)]
pub struct ServeArgs {
    /// Fire what is due once, then exit — the rehearsal.
    #[arg(long)]
    pub once: bool,
    /// Say what WOULD fire, run nothing.
    #[arg(long)]
    pub dry: bool,
    /// Inject the clock (RFC 3339 · D5) — the harness.
    #[arg(long, hide = true, value_name = "RFC3339")]
    pub now: Option<String>,
    /// Stop the loop at this instant (RFC 3339) — the harness.
    #[arg(long, hide = true, value_name = "RFC3339")]
    pub until: Option<String>,
    /// Bind an authenticated HTTP listener. Requires `--workflows` and `--token-file`.
    #[arg(long, value_name = "ADDR")]
    pub bind: Option<String>,
    /// Held registry root of `.nika.yaml` workflows. Requires `--bind`.
    #[arg(long, value_name = "DIR")]
    pub workflows: Option<PathBuf>,
    /// Acknowledge a non-loopback `--bind`. Authentication is unchanged.
    /// TLS is a reverse proxy — this process does not terminate it.
    #[arg(long)]
    pub allow_remote: bool,
    /// Owner-only Bearer file (32–512 visible ASCII bytes, mode 0600). Never argv.
    /// Mint: umask 077 && openssl rand -hex 24 > .nika/serve.token && chmod 600 .nika/serve.token
    #[arg(long, value_name = "FILE")]
    pub token_file: Option<PathBuf>,
    /// Durable job-state root. Defaults to `<cwd>/.nika/serve`.
    #[arg(long, value_name = "DIR")]
    pub state_root: Option<PathBuf>,
}
/// The injected edges — `Zoned::now` + `tokio::time::sleep`, or the
/// harness's scripted clock whose sleep ADVANCES it (trap ② avoided).
/// The scripted cell is shared with the overlap-wait seam: a
/// `chevauchement: file` wait advances the SAME clock, never the wall.
struct Clock {
    now: Box<dyn Fn() -> Zoned>,
    sleep: SleepFn,
    scripted: Option<std::rc::Rc<std::cell::RefCell<Zoned>>>,
}
/// The sleeper's shape: a span in, a future out (factored for the lint).
type SleepFn = Box<dyn Fn(SignedDuration) -> std::pin::Pin<Box<dyn Future<Output = ()>>>>;
type LifecycleWait = std::rc::Rc<dyn Fn(SignedDuration, &SleepFn) -> bool>;
/// The shared SIGTERM stream cell (W5-bis) — the overlap wait and the
/// between-fires race take turns owning the receiver: the stream is
/// TAKEN out of the cell for the await and put back after, so no
/// `RefCell` borrow ever lives across an await point.
#[cfg(unix)]
type TermCell = std::rc::Rc<std::cell::RefCell<Option<tokio::signal::unix::Signal>>>;
/// The verb edge: both lanes carry a `VerbOutput` (0 clean · 1 otherwise).
#[must_use]
pub fn run(args: &ServeArgs) -> VerbOutput {
    match go(args) {
        Ok(out) | Err(out) => out,
    }
}
fn go(args: &ServeArgs) -> Result<VerbOutput, VerbOutput> {
    let fail = |text: String| VerbOutput {
        text,
        code: exit::WORKFLOW,
    };
    let bounded_rehearsal = args.once || args.dry || args.now.is_some() || args.until.is_some();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let http = nika_serve::optional_server_config(
        args.bind.as_deref(),
        args.workflows.as_deref(),
        args.token_file.as_deref(),
        args.allow_remote,
        args.once || args.dry,
        args.now.is_some() || args.until.is_some(),
    )
    .map_err(nika_serve::launch_operator_message)
    .map_err(&fail)?;
    let now = instant(args.now.as_deref()).map_err(&fail)?;
    let until = instant(args.until.as_deref()).map_err(&fail)?;
    if now.is_some() && !args.once && until.is_none() {
        return Err(fail(
            "serve · --now sans --once exige --until — une horloge scriptée sans borne tourne à vide"
                .to_owned(),
        ));
    }
    let (path, registry) = arm::load(&cwd).map_err(|out| fail(out.text))?;
    let root = path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    recover_resident(&root, &registry, args.dry).map_err(&fail)?;
    if bounded_rehearsal {
        let run = ResidentRun::Direct(std::rc::Rc::new(arm::fire::prod_run));
        let lifecycle = ResidentLifecycle::process().map_err(&fail)?;
        serve(
            &root,
            registry,
            args,
            until.as_ref(),
            &clock(now),
            &run,
            &lifecycle,
        )
        .map_err(fail)?;
    } else {
        let state_root = args
            .state_root
            .clone()
            .unwrap_or_else(|| cwd.join(".nika/serve"));
        nika_serve::serve_resident_process(&root, state_root, http).map_err(fail)?;
    }
    Ok(VerbOutput::ok(String::new()))
}

#[cfg(test)]
type ResidentMain =
    Box<dyn FnOnce(tokio::sync::watch::Receiver<bool>) -> Result<(), String> + Send>;

enum ResidentRun {
    Direct(ExecutionRunSeam),
}

/// Joined owner of the resident ARM loop. The worker starts dormant: HTTP
/// recovery and bind must succeed before `activate`, so readiness never
/// describes only half of the process. The stop channel is the explicit seam
/// where the next wave can inject one shared execution coordinator.
#[cfg(test)]
struct ResidentSupervisor {
    activate: Option<std::sync::mpsc::SyncSender<()>>,
    stop: tokio::sync::watch::Sender<bool>,
    finished: Option<tokio::sync::oneshot::Receiver<()>>,
    worker: Option<std::thread::JoinHandle<Result<(), String>>>,
}

#[cfg(test)]
impl ResidentSupervisor {
    fn start(main: ResidentMain) -> Result<Self, String> {
        let (activate, ready) = std::sync::mpsc::sync_channel(0);
        let (stop, receiver) = tokio::sync::watch::channel(false);
        let (finished, completion) = tokio::sync::oneshot::channel();
        let worker = std::thread::Builder::new()
            .name("nika-resident-arm".to_owned())
            .spawn(move || {
                let result = if ready.recv().is_err() {
                    Ok(())
                } else {
                    main(receiver)
                };
                let _ = finished.send(());
                result
            })
            .map_err(|error| format!("serve · resident supervisor refused: {error}"))?;
        Ok(Self {
            activate: Some(activate),
            stop,
            finished: Some(completion),
            worker: Some(worker),
        })
    }

    fn activate(&mut self) -> Result<(), String> {
        let activate = self
            .activate
            .take()
            .ok_or_else(|| "serve · resident supervisor activated twice".to_owned())?;
        activate
            .send(())
            .map_err(|_| "serve · resident supervisor ended before readiness".to_owned())
    }

    fn shutdown_and_join(mut self) -> Result<(), String> {
        let _ = self.stop.send(true);
        self.activate.take();
        self.finished.take();
        let worker = self
            .worker
            .take()
            .ok_or_else(|| "serve · resident supervisor lost its worker".to_owned())?;
        worker
            .join()
            .map_err(|_| "serve · resident supervisor worker failed".to_owned())?
    }
}

#[cfg(test)]
impl Drop for ResidentSupervisor {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        self.activate.take();
        self.finished.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// An RFC 3339 instant — the zoned form keeps its zone, a bare one rides
/// UTC (the arm fire `--now` precedent).
fn instant(raw: Option<&str>) -> Result<Option<Zoned>, String> {
    raw.map_or(Ok(None), |text| {
        text.parse::<Zoned>()
            .or_else(|_| {
                text.parse::<jiff::Timestamp>()
                    .map(|t| t.to_zoned(jiff::tz::TimeZone::UTC))
            })
            .map(Some)
            .map_err(|_| format!("serve · `{text}` · RFC 3339 attendu — 2026-08-19T03:02:00Z"))
    })
}
/// The two edges: the wall clock + a real sleep, or the scripted harness
/// (start at the injected instant · a sleep ADVANCES it, never waits).
fn clock(start: Option<Zoned>) -> Clock {
    match start {
        None => Clock {
            now: Box::new(Zoned::now),
            sleep: Box::new(|d| {
                Box::pin(tokio::time::sleep(
                    std::time::Duration::try_from(d).unwrap_or_default(),
                ))
            }),
            scripted: None,
        },
        Some(t) => {
            let cell = std::rc::Rc::new(std::cell::RefCell::new(t));
            let advance = cell.clone();
            let shared = cell.clone();
            Clock {
                now: Box::new(move || cell.borrow().clone()),
                sleep: Box::new(move |d| {
                    let next = advance.borrow().clone() + d;
                    *advance.borrow_mut() = next;
                    Box::pin(async {})
                }),
                scripted: Some(shared),
            }
        }
    }
}
/// One lifecycle authority for the resident loop. Standalone mode listens
/// to process signals; composed mode listens to the supervisor's shared
/// stop channel. Both race the injected sleeper, preserving scripted-clock
/// law without ever lending that clock to HTTP.
#[derive(Clone)]
struct ResidentLifecycle {
    wait: LifecycleWait,
}

impl ResidentLifecycle {
    fn process() -> Result<Self, String> {
        let rt = signal_runtime()?;
        #[cfg(unix)]
        let term: TermCell = std::rc::Rc::new(std::cell::RefCell::new(rt.block_on(async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok()
        })));
        Ok(Self {
            wait: std::rc::Rc::new(move |span, sleep| {
                #[cfg(unix)]
                let mut stream = term.borrow_mut().take();
                let heard = rt.block_on(async {
                    #[cfg(unix)]
                    let sig = async {
                        if let Some(signal) = stream.as_mut() {
                            signal.recv().await;
                        } else {
                            std::future::pending::<()>().await;
                        }
                    };
                    #[cfg(unix)]
                    let heard = tokio::select! {
                        () = sleep(span) => false,
                        _ = tokio::signal::ctrl_c() => true,
                        () = sig => true,
                    };
                    #[cfg(not(unix))]
                    let heard = tokio::select! {
                        () = sleep(span) => false,
                        _ = tokio::signal::ctrl_c() => true,
                    };
                    heard
                });
                #[cfg(unix)]
                {
                    *term.borrow_mut() = stream;
                }
                heard
            }),
        })
    }

    #[cfg(test)]
    fn supervised_on(
        receiver: tokio::sync::watch::Receiver<bool>,
        runtime: tokio::runtime::Runtime,
    ) -> Self {
        let rt = std::rc::Rc::new(runtime);
        Self {
            wait: std::rc::Rc::new(move |span, sleep| {
                let mut stop = receiver.clone();
                rt.block_on(async {
                    tokio::select! {
                        () = sleep(span) => false,
                        () = wait_for_supervisor(&mut stop) => true,
                    }
                })
            }),
        }
    }

    fn wait(&self, span: SignedDuration, sleep: &SleepFn) -> bool {
        (self.wait)(span, sleep)
    }
}

#[cfg(test)]
async fn wait_for_supervisor(stop: &mut tokio::sync::watch::Receiver<bool>) {
    while !*stop.borrow() {
        if stop.changed().await.is_err() {
            return;
        }
    }
}
/// The reload on change (« le fichier propose »): a fresher file is
/// re-read — a broken edit is told and the last-good registry keeps
/// serving (the stale mtime is kept, so the next tick retries).
fn reload_on_change(
    root: &Path,
    path: &Path,
    reg: ArmRegistry,
    mtime: Option<std::time::SystemTime>,
) -> (ArmRegistry, Option<std::time::SystemTime>) {
    let Ok(fresh) = std::fs::metadata(path).and_then(|m| m.modified()) else {
        return (reg, mtime);
    };
    if Some(fresh) == mtime {
        return (reg, mtime);
    }
    match arm::load(root) {
        Ok((_, reloaded)) => (reloaded, Some(fresh)),
        Err(out) => {
            eprintln!("nika: {}", out.text);
            (reg, mtime)
        }
    }
}
/// One due beat through the one firer: the line prints (D8), the
/// registry comes back (a reload may have swapped it), and `true` says
/// a signal broke the beat's overlap wait — the caller stops the loop.
fn fire_due(
    root: &Path,
    reg: ArmRegistry,
    index: usize,
    now: &Zoned,
    wait: WaitSeam,
    run: &ResidentRun,
    stop: &std::rc::Rc<std::cell::Cell<bool>>,
) -> (ArmRegistry, bool) {
    let context = match run {
        ResidentRun::Direct(run) => FireCtx::new_with_execution(
            root.to_path_buf(),
            reg,
            index,
            now.clone(),
            std::process::id(),
            std::rc::Rc::clone(run),
        ),
    };
    let ctx = match context {
        Ok(ctx) => ctx.with_wait(wait),
        Err(error) => {
            println!("failed serve · {error}");
            return (error.into_registry(), false);
        }
    };
    let (line, _) = fire_beat(&ctx).into_parts();
    println!("{line}");
    (ctx.into_registry(), stop.get())
}
/// The loop: the clock, the reload on change (« le fichier propose » —
/// re-read, never cache; a broken edit is told and the last-good registry
/// keeps serving), the SAME firer for what is due, then the sleep to the
/// next slot (≤ 60 s) racing the signals — a signal breaks clean (the
/// current fire, synchronous, finishes first). A `chevauchement: file`
/// wait rides the wait seam (W5-bis): the scripted clock advances, or
/// the span races ctrl-c/SIGTERM on the runtime — the loop's thread
/// never blocks, and a broken wait sets the stop flag the loop checks
/// after each fire.
fn signal_runtime() -> Result<std::rc::Rc<tokio::runtime::Runtime>, String> {
    build_runtime().map(std::rc::Rc::new)
}

fn build_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("serve · the signal runtime refused: {error}"))
}

fn next_sleep_seconds(registry: &ArmRegistry, now: &Zoned) -> Result<i64, String> {
    let next = nika_cadence::earliest_next(registry, now)
        .map_err(|error| format!("serve · a validated registry refuses: {error}"))?;
    Ok(next.map_or(60, |(_, slot)| {
        (slot.at.timestamp().as_second() - now.timestamp().as_second()).clamp(1, 60)
    }))
}

fn resident_last_fired(
    root: &Path,
    registry: &ArmRegistry,
    dry: bool,
) -> Result<Vec<Option<Zoned>>, String> {
    let state = ArmState::at_project(root);
    let names = labels(registry);
    registry
        .beats()
        .zip(&names)
        .map(|(beat, label)| {
            if beat.locus() == Locus::Cloud {
                Ok(None)
            } else if dry {
                state.peek_last_fired(label)
            } else {
                state.last_fired(label)
            }
        })
        .collect::<std::io::Result<_>>()
        .map_err(|error| format!("serve · corrupt arm sidecar refused: {error}"))
}

/// Recover every resident beat before any composed door may announce ready.
fn recover_resident(root: &Path, registry: &ArmRegistry, dry: bool) -> Result<(), String> {
    resident_last_fired(root, registry, dry).map(|_| ())
}

fn serve(
    root: &Path,
    mut reg: ArmRegistry,
    args: &ServeArgs,
    until: Option<&Zoned>,
    clock: &Clock,
    run: &ResidentRun,
    lifecycle: &ResidentLifecycle,
) -> Result<(), String> {
    // Set when a signal broke an overlap wait — the loop stops after
    // the current beat's line (a beat mid-run still finishes first).
    let stop = std::rc::Rc::new(std::cell::Cell::new(false));
    // The overlap-wait seam, per lane: the harness advances the scripted
    // clock (the wait is instant, the loop never blocks); prod races the
    // span against the signals on the runtime.
    let make_wait = || -> WaitSeam {
        if let Some(cell) = clock.scripted.clone() {
            return Box::new(move |span| {
                let next = cell.borrow().clone() + span;
                *cell.borrow_mut() = next;
                Wait::Elapsed
            });
        }
        let lifecycle = lifecycle.clone();
        let stop = std::rc::Rc::clone(&stop);
        Box::new(move |span| {
            let sleep: SleepFn = Box::new(|duration| {
                Box::pin(tokio::time::sleep(
                    std::time::Duration::try_from(duration).unwrap_or_default(),
                ))
            });
            if lifecycle.wait(span, &sleep) {
                stop.set(true);
                Wait::Interrupted
            } else {
                Wait::Elapsed
            }
        })
    };
    let path = root.join(nika_vocab::project::FILE_NAME);
    let mut mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    loop {
        let now = (clock.now)();
        if until.is_some_and(|u| now >= *u) {
            return Ok(());
        }
        let (reloaded, fresh_mtime) = reload_on_change(root, &path, reg, mtime);
        reg = reloaded;
        mtime = fresh_mtime;
        let names = labels(&reg);
        let last_fired = resident_last_fired(root, &reg, args.dry)?;
        let dues: Vec<(usize, nika_cadence::Slot)> =
            nika_cadence::due(&reg, &now, &|i| last_fired.get(i).and_then(Clone::clone))
                .map_err(|e| format!("serve · a validated registry refuses: {e}"))?
                .map(|d| (d.index, d.slot))
                .collect();
        for (index, slot) in dues {
            let label = names[index].clone();
            if args.dry {
                println!("would fire {label} · slot {}", slot.at.timestamp());
                continue;
            }
            let (returned, broken) = fire_due(root, reg, index, &now, make_wait(), run, &stop);
            reg = returned;
            if broken {
                return Ok(());
            }
        }
        if args.once {
            return Ok(());
        }
        let secs = next_sleep_seconds(&reg, &now)?;
        if lifecycle.wait(SignedDuration::from_secs(secs), &clock.sleep) {
            return Ok(());
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::verbs::arm::fire::{RunShot, RunUpshot};
    use crate::verbs::arm::state::{FireKind, HistoryEntry};

    mod resident_tests;

    fn at(text: &str) -> Zoned {
        text.parse::<jiff::Timestamp>()
            .expect("ts")
            .to_zoned(jiff::tz::TimeZone::UTC)
    }

    #[tokio::test]
    async fn production_resident_backend_carries_declared_outputs() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = "nika: http-output\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  value:\n    invoke: { tool: nika:jq, args: { input: 42, expression: \".\" } }\noutputs:\n  answer: ${{ tasks.value.output }}\n";
        std::fs::write(directory.path().join("flow.nika.yaml"), source).expect("workflow");
        let project = nika_fs::OwnedDir::open(directory.path()).expect("owned project");
        let service = nika_execution::ExecutionService::default();
        let admitted = service
            .admit(&project, Path::new("flow.nika.yaml"))
            .expect("admitted");
        let session = service.begin(admitted);

        let backend = nika_serve::ResidentExecutionBackend::new(PathBuf::new());
        let outcome = nika_serve::ExecutionBackend::execute(&backend, session.context()).await;

        assert_eq!(
            outcome.disposition(),
            nika_serve::ExecutionDisposition::Succeeded
        );
        assert_eq!(
            outcome.outputs().expect("declared outputs")["answer"],
            serde_json::json!(42)
        );
    }

    /// A tempdir project (registry + workflow shelf) — the arm precedent.
    fn project(tag: &str, registry: &str) -> tempfile::TempDir {
        let dir = tempfile::Builder::new()
            .prefix(&format!("nika-serve-{tag}-"))
            .tempdir()
            .expect("tmp dir");
        std::fs::create_dir_all(dir.path().join("workflows")).expect("workflows");
        std::fs::write(dir.path().join("nika.yaml"), registry).expect("registry");
        dir
    }

    fn write_workflow(root: &Path, name: &str) {
        let id = name.strip_suffix(".nika.yaml").unwrap_or(name);
        let body = format!(
            "nika: {id}\npermits: {{ exec: true }}\ntasks:\n  ok:\n    exec: {{ shell: \"true\" }}\n"
        );
        std::fs::write(root.join("workflows").join(name), body).expect("workflow");
    }

    /// Hourly beats — mid-hour nothing is ever ON TIME, so the loop only
    /// ever SKIPS (an in-process run chdirs: the binary tests' ground —
    /// parallel tests race on the process CWD).
    const HOURLY_A: &str = concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 * * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
    );

    /// The v2: a second beat appears between two ticks.
    const HOURLY_AB: &str = concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 * * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
        "  - workflow: workflows/nightly.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 * * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
    );

    /// Seed a decided slot (a past skip) so the planner counts a silence.
    fn seed_last(root: &Path, label: &str, slot: &str) {
        let slot = slot.parse::<jiff::Timestamp>().expect("slot");
        let mut entry = HistoryEntry::new(Some(slot), slot, FireKind::Skipped);
        entry.reason = Some("test-seed".to_owned());
        entry.exit = Some(0);
        ArmState::at_project(root)
            .record_fixture(label, &entry)
            .expect("seed ledger truth");
    }

    fn history(root: &Path, label: &str) -> String {
        std::fs::read_to_string(root.join(".nika/arm").join(label).join("history.ndjson"))
            .unwrap_or_default()
    }

    /// The scripted clock: starts at `start`, each `sleep` ADVANCES it
    /// (trap ② avoided by construction) and lands ready — the first sleep
    /// also runs the test's actor (the reload test rewrites the file there).
    fn scripted(start: &str, on_first_sleep: impl FnMut() + 'static) -> Clock {
        let cell = std::rc::Rc::new(std::cell::RefCell::new(at(start)));
        let advance = cell.clone();
        let shared = cell.clone();
        let actor = std::cell::RefCell::new(Some(on_first_sleep));
        Clock {
            now: Box::new(move || cell.borrow().clone()),
            sleep: Box::new(move |span| {
                let next = advance.borrow().clone() + span;
                *advance.borrow_mut() = next;
                if let Some(mut act) = actor.borrow_mut().take() {
                    act();
                }
                Box::pin(async {})
            }),
            scripted: Some(shared),
        }
    }

    fn serve_args() -> ServeArgs {
        ServeArgs {
            once: false,
            dry: false,
            now: None,
            until: None,
            bind: None,
            workflows: None,
            allow_remote: false,
            token_file: None,
            state_root: None,
        }
    }

    /// The run seam that must NEVER fire — the skip-only ticks (the
    /// reload + refusal tests) prove their point by never calling it.
    fn never_run() -> ResidentRun {
        ResidentRun::Direct(std::rc::Rc::new(|_, _| panic!("this tick runs nothing")))
    }

    fn test_lifecycle() -> (tokio::sync::watch::Sender<bool>, ResidentLifecycle) {
        let (stop, receiver) = tokio::sync::watch::channel(false);
        let lifecycle =
            ResidentLifecycle::supervised_on(receiver, build_runtime().expect("test runtime"));
        (stop, lifecycle)
    }

    /// A run stub that counts its shots (the real in-process run
    /// chdirs — parallel tests race on the process CWD, so the seam is
    /// stubbed; the binary tests own the real ground).
    fn stub_run() -> (std::rc::Rc<std::cell::Cell<u32>>, ResidentRun) {
        let count = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let seen = std::rc::Rc::clone(&count);
        let seam: ExecutionRunSeam = std::rc::Rc::new(move |_, _: &RunShot| {
            seen.set(seen.get() + 1);
            RunUpshot::new(exit::OK, None)
        });
        (count, ResidentRun::Direct(seam))
    }

    /// R7 · `chevauchement: file` under serve: the bounded wait rides
    /// the wait seam (the scripted clock advances — the loop's thread
    /// never blocks), the held beat ends `overlap-timeout`, and the
    /// OTHER beat's due slot still fires on the same tick.
    #[test]
    fn a_queued_beat_waits_without_blocking_the_other_beats() {
        let registry_text = concat!(
            "nika: proj\n",
            "arm:\n",
            "  - workflow: workflows/doctor.nika.yaml\n",
            "    cadence: \"TZ=UTC 0 * * * *\"\n",
            "    plafond: 0.05\n",
            "    manqué: sauter\n",
            "    chevauchement: file\n",
            "  - workflow: workflows/nightly.nika.yaml\n",
            "    cadence: \"TZ=UTC 0 * * * *\"\n",
            "    plafond: 0.05\n",
            "    manqué: sauter\n",
        );
        let dir = project("queue", registry_text);
        write_workflow(dir.path(), "doctor.nika.yaml");
        write_workflow(dir.path(), "nightly.nika.yaml");
        // Both beats' last DECIDED slot is 03:00 — at 04:02 both are ON
        // TIME for the 04:00 slot.
        seed_last(dir.path(), "doctor", "2026-08-19T03:00:00Z");
        seed_last(dir.path(), "nightly", "2026-08-19T03:00:00Z");
        // doctor's lock: a real kernel lease, held for the whole test.
        let lock_dir = dir.path().join(".nika/arm/doctor");
        std::fs::create_dir_all(&lock_dir).expect("lock dir");
        let lock_path = lock_dir.join("lock");
        std::fs::write(
            &lock_path,
            format!(
                "{{\"pid\":{},\"started_at\":\"2026-08-19T04:00:00Z\"}}\n",
                std::process::id()
            ),
        )
        .expect("lock metadata");
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .expect("lock file");
        let _lease =
            nix::fcntl::Flock::lock(lock_file, nix::fcntl::FlockArg::LockExclusiveNonblock)
                .expect("held for the whole test");
        let registry = match arm::load(dir.path()) {
            Ok((_, registry)) => registry,
            Err(out) => panic!("load: {}", out.text),
        };
        let (runs, seam) = stub_run();
        let clock = scripted("2026-08-19T04:02:00Z", || {});
        let until = at("2026-08-19T04:10:00Z");
        let (_stop, lifecycle) = test_lifecycle();
        let rc = serve(
            dir.path(),
            registry,
            &serve_args(),
            Some(&until),
            &clock,
            &seam,
            &lifecycle,
        );
        assert!(rc.is_ok(), "{rc:?}");
        // doctor: the wait burned the SCRIPTED clock, bounded by the
        // next slot — seed + ONE overlap-timeout line.
        let doctor = history(dir.path(), "doctor");
        assert_eq!(doctor.lines().count(), 2, "seed + timeout: {doctor}");
        assert!(
            doctor.contains("\"reason\":\"overlap-timeout\""),
            "{doctor}"
        );
        // nightly: its due slot STILL fired on the same tick — the loop
        // never blocked on doctor's wait (claim + receipt, the stub went
        // exactly once).
        let nightly = history(dir.path(), "nightly");
        assert_eq!(
            nightly.lines().count(),
            3,
            "seed + claim + receipt: {nightly}"
        );
        assert!(nightly.contains("\"kind\":\"claimed\""), "{nightly}");
        assert!(nightly.contains("\"kind\":\"fired\""), "{nightly}");
        assert_eq!(runs.get(), 1, "exactly one run went");
    }

    #[test]
    fn serve_reloads_the_registry_when_the_file_changes() {
        let dir = project("reload", HOURLY_A);
        write_workflow(dir.path(), "doctor.nika.yaml");
        write_workflow(dir.path(), "nightly.nika.yaml");
        seed_last(dir.path(), "doctor", "2026-08-19T00:00:00Z");
        seed_last(dir.path(), "nightly", "2026-08-19T00:00:00Z");
        let path = dir.path().join("nika.yaml");
        let registry = match arm::load(dir.path()) {
            Ok((_, registry)) => registry,
            Err(out) => panic!("load v1: {}", out.text),
        };
        let rewritten = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = rewritten.clone();
        let yaml = path.clone();
        let clock = scripted("2026-08-19T03:30:00Z", move || {
            // mtime must move: a real beat of the wall between the writes.
            std::thread::sleep(std::time::Duration::from_millis(5));
            std::fs::write(&yaml, HOURLY_AB).expect("rewrite");
            flag.set(true);
        });
        let until = at("2026-08-19T03:35:00Z");
        let (_stop, lifecycle) = test_lifecycle();
        let rc = serve(
            dir.path(),
            registry,
            &serve_args(),
            Some(&until),
            &clock,
            &never_run(),
            &lifecycle,
        );
        assert!(rc.is_ok(), "{rc:?}");
        assert!(rewritten.get(), "the actor ran");
        // Tick 1 (registry v1): doctor's silence is 3 slots — skipped.
        let doctor = history(dir.path(), "doctor");
        assert_eq!(doctor.lines().count(), 2, "seed + miss: {doctor}");
        assert!(doctor.contains("\"reason\":\"missed:3\""), "{doctor}");
        // Tick 2 (registry v2): nightly's line is the reload's proof —
        // v1 never named it, and the loop never restarted.
        let nightly = history(dir.path(), "nightly");
        assert_eq!(nightly.lines().count(), 2, "seed + reload: {nightly}");
        assert!(nightly.contains("\"reason\":\"missed:3\""), "{nightly}");
    }

    #[test]
    fn a_scripted_clock_without_a_bound_refuses() {
        let args = ServeArgs {
            once: false,
            dry: false,
            now: Some("2026-08-19T03:02:00Z".to_owned()),
            until: None,
            bind: None,
            workflows: None,
            allow_remote: false,
            token_file: None,
            state_root: None,
        };
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("--until"), "{}", out.text);
    }

    #[test]
    fn the_injected_instant_parses_rfc3339_and_refuses_garbage() {
        assert!(instant(Some("demain")).is_err());
        let zoned = instant(Some("2026-08-19T05:02:00+02:00[Europe/Paris]"))
            .expect("parses")
            .expect("some");
        assert_eq!(zoned.timestamp().to_string(), "2026-08-19T03:02:00Z");
    }

    /// Gate 1 (diamond-discipline §5, P0 · closed 2026-08-19): the resident
    /// firer reads ONLY the registry (through the ONE arm door, judged by
    /// vocab + cadence BEFORE any shot) and its own sidecar. HTTP is a
    /// second clap-gated door (`nika_serve::serve_http`) and must not
    /// mention sockets in its lifecycle slice. A static pin over the
    /// resident recovery + loop, while the HTTP door is judged separately.
    #[test]
    fn serve_has_no_input_but_the_registry_and_its_state() {
        let src = include_str!("serve.rs");
        let prod = src;
        assert!(prod.contains("arm::load"), "the registry's ONE door");
        assert!(prod.contains("ArmState"), "its own sidecar");
        let judged = prod.find("arm::load(").expect("the door's call");
        let fired = prod.find("fire_beat(").expect("the firer's call");
        assert!(judged < fired, "vocab + cadence judge BEFORE any shot");
        let backend = include_str!("../../../nika-serve/src/server/production.rs");
        assert!(
            backend.contains("BoundServer::attach")
                && backend.contains("CancelOnDrop")
                && backend.contains("driver.execute"),
            "resident shutdown must cancel a dropped blocking execution"
        );
        let resident = prod
            .split("fn resident_last_fired")
            .nth(1)
            .expect("resident lifecycle slice");
        for banned in [
            "reqwest",
            "std::net",
            "tokio::net",
            "TcpStream",
            "std::env::var",
            "env::args",
            "stdin",
        ] {
            assert!(
                !resident.contains(banned),
                "resident serve must not read {banned}"
            );
        }
    }

    #[test]
    fn http_remains_optional_and_cannot_replace_the_resident_plan() {
        let mut args = serve_args();
        args.state_root = Some(PathBuf::from("durable-state"));
        assert!(
            nika_serve::optional_server_config(
                args.bind.as_deref(),
                args.workflows.as_deref(),
                args.token_file.as_deref(),
                args.allow_remote,
                args.once || args.dry,
                args.now.is_some() || args.until.is_some(),
            )
            .expect("optional door")
            .is_none(),
            "state root alone opens no HTTP door"
        );

        let src = include_str!("serve.rs");
        let service = include_str!("../../../nika-serve/src/server/production.rs");
        let recovered = src.find("recover_resident(").expect("resident recovery");
        let delegated = src.find("nika_serve::serve_resident").expect("delegation");
        let opened = service
            .find("ResidentAuthority::open")
            .expect("resident authority");
        let bound = service.find("BoundServer::attach").expect("HTTP attach");
        let served = service
            .find("authority.serve_with_http")
            .expect("unified serve");
        assert!(
            recovered < delegated,
            "resident recovery precedes authority"
        );
        assert!(opened < bound, "authority recovery precedes HTTP attach");
        assert!(bound < served, "optional HTTP attach precedes activation");
        assert!(
            service.contains("with_workflow_root(workflow_root.to_path_buf())")
                && service.contains("process_shutdown()")
                && service.contains("authority.serve_until(shutdown)"),
            "persistent schedules run on the one resident authority even without HTTP"
        );
        assert!(
            src.contains("if bounded_rehearsal") && src.contains("ResidentRun::Direct"),
            "every bounded rehearsal selects direct execution"
        );
    }

    #[test]
    fn resident_supervisor_does_not_fire_before_activation_and_shutdown_is_joined() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let entered = std::sync::Arc::new(AtomicBool::new(false));
        let joined = std::sync::Arc::new(AtomicBool::new(false));
        let worker_entered = std::sync::Arc::clone(&entered);
        let worker_joined = std::sync::Arc::clone(&joined);
        let (ready, heard_ready) = std::sync::mpsc::sync_channel(0);
        let mut supervisor = ResidentSupervisor::start(Box::new(move |mut stop| {
            worker_entered.store(true, Ordering::SeqCst);
            ready.send(()).expect("ready receiver");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            rt.block_on(wait_for_supervisor(&mut stop));
            worker_joined.store(true, Ordering::SeqCst);
            Ok(())
        }))
        .expect("supervisor");

        assert!(!entered.load(Ordering::SeqCst), "worker starts dormant");
        supervisor.activate().expect("activate after all setup");
        heard_ready
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("resident entered");
        assert!(entered.load(Ordering::SeqCst));
        supervisor.shutdown_and_join().expect("joined shutdown");
        assert!(joined.load(Ordering::SeqCst), "join observes worker exit");
    }

    #[test]
    fn second_resident_authority_fails_closed_before_the_resident_can_fire() {
        let dir = project("second-authority", HOURLY_A);
        write_workflow(dir.path(), "doctor.nika.yaml");
        let token = dir.path().join("serve.token");
        std::fs::write(&token, "a".repeat(32)).expect("token");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).expect("mode");
        }
        let state_root = dir.path().join("serve-state");
        let workflow_root = dir.path().join("workflows");
        let backend =
            std::sync::Arc::new(nika_serve::ResidentExecutionBackend::new(&workflow_root));
        let rt = signal_runtime().expect("runtime");
        let first = rt
            .block_on(nika_serve::ResidentAuthority::open(
                nika_serve::ResidentConfig::new(&state_root),
                backend,
            ))
            .expect("first authority");
        let result = nika_serve::serve_resident_process(
            dir.path(),
            state_root,
            Some(nika_serve::ServerConfig::new(
                "127.0.0.1:0".parse().expect("address"),
                workflow_root,
                token,
            )),
        );
        assert!(result.is_err(), "second authority must refuse");
        assert!(
            !dir.path().join(".nika/arm/doctor/history.ndjson").exists(),
            "dormant resident leaves no firing behind"
        );
        rt.block_on(first.serve_until(async {}))
            .expect("first authority shutdown joins");
    }

    #[test]
    fn http_flags_are_an_inseparable_pair_and_refuse_the_firer_harness() {
        let mut args = serve_args();
        args.bind = Some("127.0.0.1:0".to_owned());
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("--workflows"), "{}", out.text);

        args.workflows = Some(PathBuf::from("workflows"));
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("--token-file"), "{}", out.text);

        args.token_file = Some(PathBuf::from("serve.token"));
        args.once = true;
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("--once"), "{}", out.text);

        let mut mint = serve_args();
        mint.bind = Some("127.0.0.1:0".to_owned());
        mint.workflows = Some(PathBuf::from("workflows"));
        let out = run(&mint);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("openssl rand -hex 24"), "{}", out.text);

        args.once = false;
        args.dry = true;
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("--dry"), "{}", out.text);

        args.dry = false;
        args.bind = Some("not-a-bind".to_owned());
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("bind address is invalid"), "{}", out.text);
    }

    #[test]
    fn a_short_token_file_teaches_the_mint_and_does_not_echo_the_secret() {
        let _cwd = crate::cwd::hold();
        let tmp = tempfile::tempdir().expect("tmp");
        let workflows = tmp.path().join("wf");
        std::fs::create_dir(&workflows).expect("wf");
        let token = tmp.path().join("short.token");
        std::fs::write(&token, "too-short\n").expect("token");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).expect("mode");
        }
        let mut args = serve_args();
        args.bind = Some("127.0.0.1:0".to_owned());
        args.workflows = Some(workflows);
        args.token_file = Some(token);
        args.state_root = Some(tmp.path().join("state"));
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("32–512 visible ASCII"), "{}", out.text);
        assert!(out.text.contains("openssl rand -hex 24"), "{}", out.text);
        assert!(!out.text.contains("too-short"), "{}", out.text);
    }

    #[cfg(unix)]
    #[test]
    fn a_world_readable_token_file_teaches_the_mint_and_does_not_echo_the_secret() {
        use std::os::unix::fs::PermissionsExt as _;
        let _cwd = crate::cwd::hold();
        let tmp = tempfile::tempdir().expect("tmp");
        let workflows = tmp.path().join("wf");
        std::fs::create_dir(&workflows).expect("wf");
        let token = tmp.path().join("open.token");
        let secret = "a".repeat(32);
        std::fs::write(&token, &secret).expect("token");
        std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o644)).expect("mode");
        let mut args = serve_args();
        args.bind = Some("127.0.0.1:0".to_owned());
        args.workflows = Some(workflows);
        args.token_file = Some(token);
        args.state_root = Some(tmp.path().join("state"));
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("mode 0600"), "{}", out.text);
        assert!(out.text.contains("openssl rand -hex 24"), "{}", out.text);
        assert!(!out.text.contains(&secret), "{}", out.text);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_token_file_teaches_regular_file_and_does_not_echo_the_secret() {
        let _cwd = crate::cwd::hold();
        let tmp = tempfile::tempdir().expect("tmp");
        let workflows = tmp.path().join("wf");
        std::fs::create_dir(&workflows).expect("wf");
        let real = tmp.path().join("real.token");
        let secret = "a".repeat(32);
        std::fs::write(&real, &secret).expect("token");
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).expect("mode");
        }
        let linked = tmp.path().join("linked.token");
        std::os::unix::fs::symlink(&real, &linked).expect("symlink");
        let mut args = serve_args();
        args.bind = Some("127.0.0.1:0".to_owned());
        args.workflows = Some(workflows);
        args.token_file = Some(linked);
        args.state_root = Some(tmp.path().join("state"));
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("regular file"), "{}", out.text);
        assert!(out.text.contains("openssl rand -hex 24"), "{}", out.text);
        assert!(!out.text.contains(&secret), "{}", out.text);
    }

    #[test]
    fn a_missing_token_file_teaches_unreadable_and_the_mint() {
        let _cwd = crate::cwd::hold();
        let tmp = tempfile::tempdir().expect("tmp");
        let workflows = tmp.path().join("wf");
        std::fs::create_dir(&workflows).expect("wf");
        let mut args = serve_args();
        args.bind = Some("127.0.0.1:0".to_owned());
        args.workflows = Some(workflows);
        args.token_file = Some(tmp.path().join("absent.token"));
        args.state_root = Some(tmp.path().join("state"));
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("unreadable"), "{}", out.text);
        assert!(out.text.contains("openssl rand -hex 24"), "{}", out.text);
    }
}
