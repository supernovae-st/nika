// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The job door over a scripted resident: one admission per run, the resident's own words when
//! it refuses, its fresh cost review held for the human's one decision, and an admitted run whose
//! end is unknown never reported as observed.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::*;

/// A resident that answers from a script and records what it was asked and decided.
#[derive(Default)]
struct Scripted {
    admitted: Mutex<Vec<RunRequest>>,
    decided: Mutex<Vec<(String, bool)>>,
    refuse: Option<String>,
    review: bool,
    lost: Option<String>,
    /// The job runs until it is cancelled.
    hold: bool,
    /// The door waits on the job's end.
    waiting: AtomicBool,
    cancelled: Mutex<Vec<String>>,
}

impl Jobs for Scripted {
    fn admit<'a>(&'a self, run: &'a RunRequest) -> JobFuture<'a, Result<Admitted, String>> {
        Box::pin(async move {
            if let Some(why) = &self.refuse {
                return Err(why.clone());
            }
            let mut admitted = self.admitted.lock().expect("admitted");
            admitted.push(run.clone());
            let n = admitted.len();
            Ok(if self.review {
                Admitted::Review {
                    review: format!("rev-{n}"),
                    question: "Run once at an unknown cost?".to_owned(),
                    details: "route deepseek/x · price unknown".to_owned(),
                }
            } else {
                Admitted::Job(format!("job-{n}"))
            })
        })
    }

    fn decide<'a>(
        &'a self,
        review: &'a str,
        approve: bool,
    ) -> JobFuture<'a, Result<Option<String>, String>> {
        Box::pin(async move {
            self.decided
                .lock()
                .expect("decided")
                .push((review.to_owned(), approve));
            Ok(approve.then(|| format!("job-of-{review}")))
        })
    }

    fn settled<'a>(&'a self, id: &'a str) -> JobFuture<'a, Result<JobEnd, String>> {
        Box::pin(async move {
            self.waiting.store(true, Ordering::SeqCst);
            let mut waited = 0;
            while self.hold && self.cancelled.lock().expect("cancelled").is_empty() && waited < 2000
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
                waited += 1;
            }
            if self.hold {
                return Ok(JobEnd::new(130, None));
            }
            match &self.lost {
                Some(why) => Err(why.clone()),
                None => Ok(JobEnd::new(
                    4,
                    Some(PathBuf::from(format!(".nika/traces/{id}.ndjson"))),
                )),
            }
        })
    }

    fn cancel<'a>(&'a self, id: &'a str) -> JobFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.cancelled
                .lock()
                .expect("cancelled")
                .push(id.to_owned());
            Ok(())
        })
    }
}

/// Every line the run told its sink.
#[derive(Default)]
struct Told(Mutex<Vec<String>>);

impl RunSink for Told {
    fn said(&self, line: String) {
        self.0.lock().expect("told").push(line);
    }
}

fn request() -> RunRequest {
    RunRequest {
        workflow: PathBuf::from("workflows/copy.nika"),
        vars: vec!["city=Paris".to_owned()],
        max_cost_usd: 0.5,
        access_pin: Some("api".to_owned()),
        bytes: None,
        closure: None,
    }
}

/// A door over `jobs` and the runtime it waits on.
fn door(jobs: &Arc<Scripted>) -> (tokio::runtime::Runtime, JobDoor) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("runtime");
    let door = JobDoor::new(runtime.handle().clone(), Arc::clone(jobs) as Arc<dyn Jobs>);
    (runtime, door)
}

/// `act` on `door` from a plain thread, as a Session's worker does.
fn on_worker<T: Send>(act: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .spawn_scoped(scope, act)
            .expect("worker");
        worker.join().expect("joined")
    })
}

#[test]
fn a_run_is_one_admission_by_name_then_its_observed_end() {
    let jobs = Arc::new(Scripted::default());
    let (_runtime, mut door) = door(&jobs);
    let told = Told::default();
    let step = on_worker(|| door.run(Path::new("/project"), &request(), &told));
    let RunStep::Observed { exit, trace, leg } = step else {
        panic!("observed: {step:?}");
    };
    assert_eq!(exit, 4, "the resident's end, as the exit nika run gives");
    assert_eq!(trace, Some(PathBuf::from(".nika/traces/job-1.ndjson")));
    assert_eq!(leg, None, "a receipt naming nothing names no identity");
    let admitted = jobs.admitted.lock().expect("admitted").clone();
    assert_eq!(
        admitted,
        [request()],
        "one admission: name, inputs, pin and ceiling unchanged"
    );
    let told = told.0.lock().expect("told").clone();
    assert_eq!(
        told,
        ["run admitted as job job-1", "job job-1 ended with exit 4"]
    );
}

#[test]
fn a_refused_admission_starts_nothing_and_says_the_residents_words() {
    let jobs = Arc::new(Scripted {
        refuse: Some("no workflow by that name under the served registry".to_owned()),
        ..Scripted::default()
    });
    let (_runtime, mut door) = door(&jobs);
    let told = Told::default();
    let step = on_worker(|| door.run(Path::new("/project"), &request(), &told));
    let RunStep::NotStarted { why } = step else {
        panic!("not started: {step:?}");
    };
    assert_eq!(
        why,
        "no workflow by that name under the served registry · nothing ran"
    );
    assert!(told.0.lock().expect("told").is_empty());
}

#[test]
fn an_admitted_run_whose_end_is_lost_is_never_reported_observed() {
    let jobs = Arc::new(Scripted {
        lost: Some("the job store refused".to_owned()),
        ..Scripted::default()
    });
    let (_runtime, mut door) = door(&jobs);
    let told = Told::default();
    let step = on_worker(|| door.run(Path::new("/project"), &request(), &told));
    let RunStep::Unobserved { why } = step else {
        panic!("unobserved: {step:?}");
    };
    assert!(why.starts_with("job job-1 was admitted"), "{why}");
}

#[test]
fn a_held_review_admits_once_on_approval_and_nothing_on_decline() {
    let jobs = Arc::new(Scripted {
        review: true,
        ..Scripted::default()
    });
    let (_runtime, mut door) = door(&jobs);
    let told = Told::default();
    let step = on_worker(|| door.run(Path::new("/project"), &request(), &told));
    let RunStep::Review { question, details } = step else {
        panic!("review: {step:?}");
    };
    assert_eq!(question, "Run once at an unknown cost?");
    assert!(details.contains("price unknown"));
    assert!(
        jobs.decided.lock().expect("decided").is_empty(),
        "nothing decided for the human"
    );
    let step = on_worker(|| door.answer_review(true, &told));
    assert!(
        matches!(step, RunStep::Observed { exit: 4, .. }),
        "{step:?}"
    );
    let again = on_worker(|| door.answer_review(true, &told));
    assert!(
        matches!(again, RunStep::NotStarted { .. }),
        "one approval, once: {again:?}"
    );
    let step = on_worker(|| door.run(Path::new("/project"), &request(), &told));
    assert!(matches!(step, RunStep::Review { .. }));
    let declined = on_worker(|| door.answer_review(false, &told));
    assert!(matches!(declined, RunStep::Declined), "{declined:?}");
    let decided = jobs.decided.lock().expect("decided").clone();
    assert_eq!(
        decided,
        [("rev-1".to_owned(), true), ("rev-2".to_owned(), false)]
    );
}

#[test]
fn a_review_left_waiting_is_declined_when_replaced_or_dropped() {
    let jobs = Arc::new(Scripted {
        review: true,
        ..Scripted::default()
    });
    let (runtime, mut door) = door(&jobs);
    let told = Told::default();
    let _ = on_worker(|| door.run(Path::new("/project"), &request(), &told));
    let _ = on_worker(|| door.run(Path::new("/project"), &request(), &told));
    on_worker(move || drop(door));
    drop(runtime);
    let decided = jobs.decided.lock().expect("decided").clone();
    assert_eq!(
        decided,
        [("rev-1".to_owned(), false), ("rev-2".to_owned(), false)],
        "a superseded review and the one held at the end are declined, never left pending"
    );
}

const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
const OTHER: &str = "01a0ef11-0212-70de-a8b3-99de94270000";

/// One journal line (or lane frame) of `exec`: the runtime's own event, its chain field kept.
fn event_line(exec: &str, n: u32, kind: &str, fields: &str) -> String {
    format!(
        r#"{{"chain":"ab","correlation":null,"execution":{{"uuid":"{exec}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
    )
}

fn started(exec: &str, n: u32, hash: &str) -> String {
    let fields = format!(r#"{{"key":"workflow_sha256","value":"{hash}"}}"#);
    event_line(exec, n, "workflow_started", &fields)
}

/// A job's end names its receipt's opaque identities and the source hash its own journal
/// started with, only when that journal is the receipt's execution; a receipt naming no
/// execution names no identity.
#[test]
fn a_job_end_names_its_receipt_and_its_own_journals_one_start() {
    let dir = tempfile::tempdir().expect("dir");
    let journal = dir.path().join("run.ndjson");
    let done = event_line(EXEC, 2, "workflow_completed", "");
    std::fs::write(&journal, format!("{}\n{done}\n", started(EXEC, 1, "aa"))).expect("journal");
    let receipt = |execution: &str| {
        JobEnd::new(0, Some(journal.clone())).with_receipt(
            Some(format!("exe-{execution}")),
            Some("trace-of-job-1".to_owned()),
            Some("cd".to_owned()),
        )
    };
    let leg = receipt(EXEC).leg().expect("an observed identity");
    assert_eq!(
        leg.execution.as_deref(),
        Some(EXEC),
        "as the run's frames name it"
    );
    assert_eq!(leg.workflow_sha256.as_deref(), Some("aa"));
    assert_eq!(leg.trace.as_deref(), Some("trace-of-job-1"));
    assert!(
        leg.trace_opaque,
        "the receipt's trace is an identity, never a path to read"
    );
    assert_eq!(
        (leg.chain_head.as_deref(), leg.chain_len),
        (Some("cd"), None)
    );
    let other = receipt(OTHER).leg().expect("the receipt's identity");
    assert_eq!(other.execution.as_deref(), Some(OTHER));
    assert_eq!(
        other.workflow_sha256, None,
        "another execution's journal names no source"
    );
    assert_eq!(JobEnd::new(0, Some(journal.clone())).leg(), None);
    let unspelled =
        JobEnd::new(0, Some(journal.clone())).with_receipt(Some(EXEC.to_owned()), None, None);
    assert_eq!(
        unspelled.leg(),
        None,
        "a receipt execution of another spelling names none"
    );
    let missing = JobEnd::new(0, Some(dir.path().join("gone.ndjson"))).with_receipt(
        Some(format!("exe-{EXEC}")),
        None,
        None,
    );
    let missing = missing.leg().expect("the receipt's identity");
    assert_eq!((missing.workflow_sha256, missing.trace), (None, None));
    assert!(!missing.trace_opaque);
}

/// A resumed leg is its own execution: without a start of its own it names no source hash,
/// never the paused leg's and never one derived from the bytes it was asked to run.
#[test]
fn a_resumed_leg_without_its_own_start_names_no_source() {
    let door = LaneRunDoor::new(PathBuf::from("nika"));
    let told = Told::default();
    let folding = Folding {
        sink: &told,
        identity: &door.identity,
        stop: None,
    };
    folding.frame(RunFrame::decode(&started(EXEC, 1, "aa")).expect("the paused leg's start"));
    door.fresh();
    let resumed = event_line(
        OTHER,
        2,
        "task_completed",
        r#"{"key":"task","value":"approve"}"#,
    );
    folding.frame(RunFrame::decode(&resumed).expect("the resumed leg's event"));
    let leg = door.leg().expect("the resumed execution");
    assert_eq!(leg.execution.as_deref(), Some(OTHER));
    assert_eq!(leg.workflow_sha256, None, "deliberately unproven");
}

/// Every frame a lane child relays reaches the turn's sink and is folded on the way: the run's
/// execution, its one start's source hash and its settlement's receipt, as the Session keeps it.
#[test]
fn the_lane_fold_relays_every_frame_and_keeps_the_runs_identity() {
    #[derive(Default)]
    struct Relayed(Mutex<(Vec<String>, usize)>);
    impl RunSink for Relayed {
        fn said(&self, line: String) {
            self.0.lock().expect("relayed").0.push(line);
        }
        fn frame(&self, _frame: RunFrame) {
            self.0.lock().expect("relayed").1 += 1;
        }
    }
    let relayed = Relayed::default();
    let identity = Mutex::default();
    let folding = Folding {
        sink: &relayed,
        identity: &identity,
        stop: None,
    };
    folding.said("running · copy.nika".to_owned());
    let settled = format!(
        r#"{{"kind":"run_settled","status":"succeeded","cause":"normal","execution":{{"uuid":"{EXEC}"}},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"unsealed","receipt":{{"trace_path":".nika/traces/t.ndjson","chain_head":"cd","chain_len":7}}}}"#
    );
    for line in [started(EXEC, 1, "aa"), started(OTHER, 2, "bb"), settled] {
        folding.frame(RunFrame::decode(&line).expect("a frame"));
    }
    assert_eq!(
        *relayed.0.lock().expect("relayed"),
        (vec!["running · copy.nika".to_owned()], 3)
    );
    let leg = kept(&identity.lock().expect("identity")).expect("an identity");
    assert_eq!(leg.execution.as_deref(), Some(EXEC));
    assert_eq!(leg.workflow_sha256.as_deref(), Some("aa"));
    assert_eq!(
        (leg.chain_head.as_deref(), leg.chain_len),
        (Some("cd"), Some(7))
    );
    assert_eq!(leg.trace, None, "the Session adds the trace it was told");
    assert_eq!(kept(&RunIdentity::new()), None);
}

/// A lane run's Stop waits for the run's start (its child's signal listener), sends the first
/// signal once, then only says where the run stands: a second Stop never signals again.
#[test]
#[allow(clippy::disallowed_types)]
fn a_lane_stop_waits_for_its_runs_start_then_interrupts_the_child_once() {
    use std::os::unix::process::ExitStatusExt as _;
    let mut door = LaneRunDoor::new(PathBuf::from("nika"));
    let stop = door.stopper().expect("the lane door stops its runs");
    assert_eq!(
        stop.stop(),
        Stopping::Pending,
        "no child yet: the Stop waits"
    );
    let mut child = std::process::Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .expect("a child in the run's place");
    *door.slot.lock().expect("slot") = Some(child.id());
    let told = Told::default();
    let folding = Folding {
        sink: &told,
        identity: &door.identity,
        stop: door.stop.as_deref(),
    };
    folding.frame(RunFrame::decode(&started(EXEC, 1, "aa")).expect("the run's start"));
    let status = child.wait().expect("the child ends");
    assert_eq!(status.signal(), Some(2), "SIGINT, a first Ctrl-C's signal");
    // Reaped, its pid still in the slot: a second Stop must not signal it again.
    assert_eq!(stop.stop(), Stopping::Signalled);
    door.stop.as_deref().expect("armed").ended();
    assert_eq!(
        stop.stop(),
        Stopping::Ended,
        "a run that ended takes no Stop"
    );
}

/// A Stop taken before the lane door spawns its child is applied there: nothing is spawned (the
/// door's binary does not even exist), and the next run arms afresh.
#[test]
fn a_stop_before_the_lane_door_spawns_starts_nothing() {
    let mut door = LaneRunDoor::new(PathBuf::from("/nonexistent/nika-run-lane"));
    let stop = door.stopper().expect("armed");
    assert_eq!(stop.stop(), Stopping::Pending);
    let told = Told::default();
    let refused = door.run(Path::new("/"), &request(), &told);
    assert!(
        matches!(&refused, RunStep::NotStarted { why } if why == STOPPED_BEFORE_START),
        "{refused:?}"
    );
    assert!(
        told.0.lock().expect("told").is_empty(),
        "no child said a word"
    );
    assert_eq!(
        stop.stop(),
        Stopping::Ended,
        "the refused run ended its Stop"
    );
    assert!(
        door.stop.is_none(),
        "no Stop of that run reaches the next one"
    );
}

/// The job door's Stop reaches the resident's own job cancellation once: asked while the job
/// runs it signals, a second Stop sends nothing more, and the run's end is the job's.
#[test]
fn a_stop_cancels_the_running_job_once() {
    let jobs = Arc::new(Scripted {
        hold: true,
        ..Scripted::default()
    });
    let (_runtime, mut door) = door(&jobs);
    let stopper = door.stopper().expect("the job door arms a Stop");
    let told = Told::default();
    let step = std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .spawn_scoped(scope, || door.run(Path::new("/project"), &request(), &told))
            .expect("worker");
        while !jobs.waiting.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(stopper.stop(), Stopping::Signalled);
        assert_eq!(
            stopper.stop(),
            Stopping::Signalled,
            "a second Stop sends nothing more"
        );
        worker.join().expect("joined")
    });
    let RunStep::Observed { exit, .. } = step else {
        panic!("observed: {step:?}");
    };
    assert_eq!(exit, 130);
    assert_eq!(*jobs.cancelled.lock().expect("cancelled"), ["job-1"]);
    assert_eq!(
        stopper.stop(),
        Stopping::Ended,
        "the run ended: nothing is sent"
    );
}

/// A Stop that came before the admission admits nothing: the run starts no job.
#[test]
fn a_stop_before_the_admission_admits_no_job() {
    let jobs = Arc::new(Scripted::default());
    let (_runtime, mut door) = door(&jobs);
    let stopper = door.stopper().expect("the job door arms a Stop");
    assert_eq!(stopper.stop(), Stopping::Pending);
    let told = Told::default();
    let step = on_worker(|| door.run(Path::new("/project"), &request(), &told));
    let RunStep::NotStarted { why } = step else {
        panic!("not started: {step:?}");
    };
    assert_eq!(why, "a Stop arrived before this run started · nothing ran");
    assert!(jobs.admitted.lock().expect("admitted").is_empty());
    assert!(jobs.cancelled.lock().expect("cancelled").is_empty());
}
