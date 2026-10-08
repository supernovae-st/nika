// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The job door over a scripted resident: one admission per run, the resident's own words when
//! it refuses, its fresh cost review held for the human's one decision, and an admitted run whose
//! end is unknown never reported as observed.

use std::sync::Mutex;

use super::*;

/// A resident that answers from a script and records what it was asked and decided.
#[derive(Default)]
struct Scripted {
    admitted: Mutex<Vec<RunRequest>>,
    decided: Mutex<Vec<(String, bool)>>,
    refuse: Option<String>,
    review: bool,
    lost: Option<String>,
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

    fn settled<'a>(&'a self, id: &'a str) -> JobFuture<'a, Result<(u8, Option<PathBuf>), String>> {
        Box::pin(async move {
            match &self.lost {
                Some(why) => Err(why.clone()),
                None => Ok((4, Some(PathBuf::from(format!(".nika/traces/{id}.ndjson"))))),
            }
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
    let RunStep::Observed { exit, trace } = step else {
        panic!("observed: {step:?}");
    };
    assert_eq!(exit, 4, "the resident's end, as the exit nika run gives");
    assert_eq!(trace, Some(PathBuf::from(".nika/traces/job-1.ndjson")));
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
