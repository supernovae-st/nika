// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/v1/sessions` — the project's native Session (nika-session-host) behind this server's own
//! checks, outside the generic deadline. A Session's run is a job of THIS resident: admitted by
//! name through the job door's own law (the served registry, literal inputs, the cost review
//! this server holds when its operator seated one), then observed in the job store until it
//! settles or pauses. A paused job is not resumed here (the resident has no resume route): the
//! answer to its gate is `run_not_started`, never an invented resume.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};

use bytes::Bytes;
use hyper::body::Incoming;
use hyper::{Method, Request, Response};
use nika_cli_host::output::exit;
use nika_session_host::http::{Doors, Sessions, job_id, refusal_words};
use nika_session_host::run::{
    Admitted, JobDoor, JobEnd, JobFuture, Jobs, NoRunDoor, RunDoor, RunRequest,
};
use sha2::{Digest as _, Sha256};

use super::error::{ApiError, ResponseBody};
use super::{AppState, ServerError, cost_review, route};
use crate::{IdempotencyKey, JobId, JobStatus, JobStoreError, RequestDigest};

/// The Session of the project at `root`, opened by this process as bare `nika` opens it, its
/// runs lent to this resident's own job admission.
pub(super) fn open(root: PathBuf, state: Weak<AppState>) -> Sessions {
    let doors: Doors = Box::new(move || -> Box<dyn RunDoor> {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return Box::new(NoRunDoor::new(
                "this server has no runtime to admit a run on",
            ));
        };
        let state = state.clone();
        let held = Mutex::default();
        let resident = Resident { state, held };
        Box::new(JobDoor::new(handle, Arc::new(resident)))
    });
    #[cfg(test)]
    if let Some(opener) = TEST_OPENERS
        .lock()
        .ok()
        .and_then(|mut map| map.remove(&root))
    {
        return Sessions::new(opener, doors);
    }
    Sessions::bare(root, doors)
}

/// The scripted Session a test opens for its project root, in place of the machine's own.
#[cfg(test)]
pub(super) static TEST_OPENERS: std::sync::Mutex<
    std::collections::BTreeMap<PathBuf, nika_session_host::http::Opener>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());

pub(super) async fn route(request: Request<Incoming>, state: &AppState) -> Response<ResponseBody> {
    let (method, headers) = (request.method().clone(), request.headers().clone());
    let path = request.uri().path().to_owned();
    let body = match method {
        Method::POST => match super::route::intake(request, state, |_| Ok(())).await {
            Ok(((), _, body)) => body,
            Err(refused) => return refused,
        },
        _ => Bytes::new(),
    };
    let sessions = state.sessions.as_deref();
    let routed = match sessions {
        Some(sessions) => sessions.route(&method, &path, &headers, body).await,
        None => None,
    };
    routed.unwrap_or_else(|| ApiError::route_not_found().into_response())
}

/// This resident's job admission, lent to one Session's run door: it holds at most the cost
/// review that Session's run waits at, with the exact request that review's admission repeats.
pub(super) struct Resident {
    state: Weak<AppState>,
    held: Mutex<Option<(String, Bytes)>>,
}

#[cfg(test)]
impl Resident {
    /// The port a test drives directly, as a Session's door is lent it.
    pub(super) fn lent(state: &Arc<AppState>) -> Self {
        let state = Arc::downgrade(state);
        let held = Mutex::default();
        Self { state, held }
    }
}

impl Jobs for Resident {
    fn admit<'a>(&'a self, run: &'a RunRequest) -> JobFuture<'a, Result<Admitted, String>> {
        Box::pin(self.admitted(run))
    }

    fn decide<'a>(
        &'a self,
        review: &'a str,
        approve: bool,
    ) -> JobFuture<'a, Result<Option<String>, String>> {
        Box::pin(self.decided(review, approve))
    }

    fn settled<'a>(&'a self, id: &'a str) -> JobFuture<'a, Result<JobEnd, String>> {
        Box::pin(self.ended(id))
    }
}

impl Resident {
    fn state(&self) -> Result<Arc<AppState>, String> {
        (self.state.upgrade()).ok_or_else(|| "this server is stopping".to_owned())
    }

    /// The run by name, its `name=value` pairs bound by the resident's literal law (never its
    /// environment): the review this server holds first when it seats one, else the job.
    async fn admitted(&self, run: &RunRequest) -> Result<Admitted, String> {
        let state = self.state()?;
        let name = run.workflow.to_string_lossy().into_owned();
        let admitted = match route::admit_by_name(&name, &state).await {
            Ok(admitted) => admitted,
            Err(refused) => return Err(refusal_words(refused).await),
        };
        // Only the world the Session checked runs (the root's bytes, and the closure of every
        // workflow and skill it reaches): every admission below executes, or frames its review
        // over, this very capture.
        let snapshot = admitted.snapshot();
        let checked = (snapshot.text(snapshot.root())).is_some_and(|source| run.admits(source));
        if !checked || !run.admits_world(snapshot) {
            return Err(UNCHECKED.to_owned());
        }
        let pairs = (run.vars.iter()).map(|var| var.split_once('=').unwrap_or((var.as_str(), "")));
        let inputs = super::schedule_inputs::bind_literals(&admitted, pairs)
            .map_err(|refused| refused.message)?;
        let mut request = serde_json::json!({"workflow": name, "inputs": inputs});
        if let Some(pin) = &run.access_pin {
            request["access"] = pin.as_str().into();
        }
        let body = Bytes::from(request.to_string());
        let world = admitted.snapshot().encode();
        let world = world.map_err(|_| "the workflow's world could not be captured".to_owned())?;
        if let Some(door) = &state.cost_review {
            let Ok(Some(job)) = route::named_job(&body) else {
                return Err("the run's request is not one this server's review reads".to_owned());
            };
            // Framed over this very world, the Session's ceiling as the review's default.
            let asked = (2, Some(run.max_cost_usd));
            match cost_review::review_admitted(&state, door, job, admitted, asked).await {
                Ok(Ok(review)) => return self.hold(door, review, body),
                Ok(Err(_not_required)) => {}
                Err(refused) => return Err(refusal_words(refused).await),
            }
        }
        // The admission binds the run's ceiling with its request: it runs under it, restricted by
        // this server's, never raised.
        let bound = serde_json::json!({"request": request, "max_cost_usd": run.max_cost_usd});
        let (key, digest) = identity(bound.to_string().as_bytes())?;
        let (access, ceiling) = (run.access_pin.clone(), Some(run.max_cost_usd));
        let admission = (state.coordinator)
            .admit_manual_inputs(key, digest, name, world, access, inputs, None, ceiling)
            .await;
        let response = match admission {
            Ok(admission) => route::admission_response(admission),
            Err(error) => route::admission_error(&error),
        };
        job_id(response).await.map(Admitted::Job)
    }

    /// Keep `review` for this Session: its question, its public document on demand.
    fn hold(
        &self,
        door: &cost_review::Door,
        review: String,
        body: Bytes,
    ) -> Result<Admitted, String> {
        let shown = cost_review::shown(door, &review);
        let (question, details) = shown.ok_or_else(|| "the review is no longer held".to_owned())?;
        if let Ok(mut held) = self.held.lock() {
            *held = Some((review.clone(), body));
        }
        Ok(Admitted::Review {
            review,
            question,
            details,
        })
    }

    /// The human's decision on the review this door holds; approved, its one admission.
    async fn decided(&self, review: &str, approve: bool) -> Result<Option<String>, String> {
        let state = self.state()?;
        let held = (self.held.lock().ok()).and_then(|mut held| held.take_if(|h| h.0 == review));
        let (Some(door), Some((_, body))) = (state.cost_review.clone(), held) else {
            return Err("no such review waits on this Session's door".to_owned());
        };
        let witness = cost_review::decide_held(&door, review, approve)?;
        if !approve {
            return Ok(None);
        }
        let Ok(Some(job)) = route::named_job(&body) else {
            return Err("the reviewed request could not be read again".to_owned());
        };
        let (key, digest) = identity(&body)?;
        let reviewed = (review.to_owned(), witness);
        let response = cost_review::admit(Arc::clone(&state), key, digest, job, reviewed).await;
        job_id(response).await.map(Some)
    }

    /// The job's end once it settled or paused, as the exit `nika run` gives for that end, and
    /// its journal; an interrupted job's effects are unknown, never an observed end.
    async fn ended(&self, id: &str) -> Result<JobEnd, String> {
        let state = self.state()?;
        let id = JobId::parse(id).map_err(|_| "not a job of this resident".to_owned())?;
        let notify = state.store.event_notify();
        loop {
            let notified = Arc::clone(&notify).notified_owned();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let record = match state.store.get(id.clone()).await {
                Ok(Some(record)) => record,
                Ok(None) => return Err("the job store no longer holds it".to_owned()),
                Err(ServerError::JobStore(JobStoreError::Busy) | ServerError::StoreQueueFull) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                }
                Err(_) => return Err("the job store refused to read it".to_owned()),
            };
            let exit = match record.status() {
                JobStatus::Queued | JobStatus::Running => {
                    notified.await;
                    continue;
                }
                JobStatus::Succeeded => exit::OK,
                JobStatus::Failed => exit::WORKFLOW,
                JobStatus::Paused => exit::PAUSED,
                JobStatus::Cancelled => exit::CANCELLED,
                JobStatus::Interrupted => {
                    return Err("the resident lost its execution: its effects are unknown".into());
                }
            };
            let journal = (state.journal_dir.as_deref())
                .and_then(|dir| super::trace_verdict::JournalKey::of(dir, &record));
            let end = JobEnd::new(exit, journal.and_then(|key| key.path()));
            // The receipt's opaque identities: the trace one the job's trace door resolves.
            let named = |id: Option<&str>| id.map(str::to_owned);
            let head = record.receipt().and_then(crate::JobReceipt::chain_head);
            let receipt = (named(record.execution_id()), named(record.trace_id()));
            return Ok(end.with_receipt(receipt.0, receipt.1, named(head)));
        }
    }
}

/// Why a run whose captured world is not the one the Session checked starts nothing.
const UNCHECKED: &str = "the workflow on disk, or a workflow or skill it uses, is not what the Session checked for this run · nothing ran · ask for the run again so the Session checks it";

/// A fresh admission identity for one Session run: its own key, the digest of its request.
fn identity(body: &[u8]) -> Result<(IdempotencyKey, RequestDigest), String> {
    let key = IdempotencyKey::new(format!("session-run-{}", uuid::Uuid::new_v4()));
    let key = key.map_err(|_| "no admission identity could be drawn".to_owned())?;
    Ok((key, RequestDigest::from_bytes(Sha256::digest(body).into())))
}
