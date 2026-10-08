// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The job door over a scripted resident: one admission per run, the resident's own words when
//! it refuses, and an admitted run whose end is unknown never reported as observed.

use std::sync::Mutex;

use super::*;

/// One admission asked: the workflow, its pairs and its pin.
type Asked = (String, Vec<String>, Option<String>);

/// A resident that answers from a script and records what it was asked.
#[derive(Default)]
struct Scripted {
    admitted: Mutex<Vec<Asked>>,
    refuse: Option<String>,
    lost: Option<String>,
}

impl Jobs for Scripted {
    fn admit<'a>(
        &'a self,
        name: &'a str,
        vars: &'a [String],
        access: Option<&'a str>,
    ) -> JobFuture<'a, Result<String, String>> {
        Box::pin(async move {
            if let Some(why) = &self.refuse {
                return Err(why.clone());
            }
            let mut admitted = self.admitted.lock().expect("admitted");
            admitted.push((name.to_owned(), vars.to_vec(), access.map(str::to_owned)));
            Ok(format!("job-{}", admitted.len()))
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
    }
}

/// Run `request` through a door over `jobs`, from a plain thread as a Session's worker does.
fn run(jobs: Arc<Scripted>) -> (RunStep, Vec<String>) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("runtime");
    let mut door = JobDoor::new(runtime.handle().clone(), jobs);
    let told = Told::default();
    let step = std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .spawn_scoped(scope, || door.run(Path::new("/project"), &request(), &told))
            .expect("worker");
        worker.join().expect("joined")
    });
    let lines = told.0.lock().expect("told").clone();
    (step, lines)
}

#[test]
fn a_run_is_one_admission_by_name_then_its_observed_end() {
    let jobs = Arc::new(Scripted::default());
    let (step, told) = run(Arc::clone(&jobs));
    let RunStep::Observed { exit, trace } = step else {
        panic!("observed: {step:?}");
    };
    assert_eq!(exit, 4, "the resident's end, as the exit nika run gives");
    assert_eq!(trace, Some(PathBuf::from(".nika/traces/job-1.ndjson")));
    let admitted = jobs.admitted.lock().expect("admitted").clone();
    assert_eq!(
        admitted,
        [(
            "workflows/copy.nika".to_owned(),
            vec!["city=Paris".to_owned()],
            Some("api".to_owned())
        )],
        "one admission of the saved workflow by name, inputs and pin unchanged"
    );
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
    let (step, told) = run(jobs);
    let RunStep::NotStarted { why } = step else {
        panic!("not started: {step:?}");
    };
    assert_eq!(
        why,
        "no workflow by that name under the served registry · nothing ran"
    );
    assert!(told.is_empty());
}

#[test]
fn an_admitted_run_whose_end_is_lost_is_never_reported_observed() {
    let jobs = Arc::new(Scripted {
        lost: Some("the job store refused".to_owned()),
        ..Scripted::default()
    });
    let (step, _) = run(jobs);
    let RunStep::Unobserved { why } = step else {
        panic!("unobserved: {step:?}");
    };
    assert!(why.starts_with("job job-1 was admitted"), "{why}");
}
