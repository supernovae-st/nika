// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `GET /v1/access-offers/{name}` (capability `accessSessionOffer`): what the
//! sessions of a served workflow's own declared ACP route offer now.
//!
//! The route and the models are the file's. The door admits the workflow by
//! name exactly as the job door does, reads `run.access` from it, and asks
//! only when the file selects ACP sessions (`run.access.via` under
//! `run.access.protocol: acp`): one session per shape its tasks run in and
//! per model they resolve to. A caller names a served workflow and nothing
//! else, so no route, model, adapter or frame of its choosing ever reaches a
//! harness, and a workflow without such a route opens no session at all.
//!
//! Each session is opened as a run opens it and ended before any prompt
//! (`nika_service_execution::offer`): it lists the models, the reasoning
//! efforts and the service tiers the route has for the model, and costs it
//! no turn. What comes back is carried as bounded tokens, never as text the
//! adapter wrote.
//!
//! A reading is an observation with a time, kept for [`FRESH`] ([`RETRY`]
//! after a failed one) and served again meanwhile, so a page that asks often
//! opens a session rarely. One reading runs at a time; a request beyond
//! [`WAITERS`] is refused `access_offer_busy` and nothing queues past them.
//!
//! An offer authorizes nothing. `POST /v1/jobs` admits and runs as before:
//! the run selects, reads back and refuses on its own session, whatever an
//! offer said a moment earlier.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use hyper::{Response, StatusCode};
use jiff::Timestamp;
use nika_error::NikaErrorCode as _;
use nika_kernel::ai::harness::HarnessError;
use nika_service_execution::offer::{
    self, EffortOffer, ModelOffer, OfferProfile, OfferTarget, ServiceTierOffer, SessionOffer,
};
use serde::Serialize;
use tokio::sync::{Mutex, Semaphore};

use super::AppState;
use super::error::{ApiError, ResponseBody, bound_message};
use super::route::json_response;

/// The route's prefix; what follows is a served name, `/` separators literal.
pub(super) const PREFIX: &str = "/v1/access-offers/";

/// The document's schema word.
const SCHEMA: &str = "nika/access-offer@1";

/// Requests this door holds at once: the one that reads and those waiting
/// for it. The next one is refused `access_offer_busy`.
pub(super) const WAITERS: usize = 8;

/// The sessions one request reads at most; a workflow that names more has
/// the rest counted in `routes_omitted`.
pub(super) const MAX_ROUTES: usize = 8;

/// One request's bound: its wait for the reading slot and every reading it makes.
const DEADLINE: Duration = Duration::from_secs(90);

/// One session's bound, the adapter's start included.
const READING: Duration = Duration::from_secs(30);

/// How long an observed offer is served again before the route is asked anew.
pub(super) const FRESH: Duration = Duration::from_secs(60);

/// How long a failed reading is served again: a broken adapter is not asked
/// on every request, and a transient failure does not hide the route for long.
pub(super) const RETRY: Duration = Duration::from_secs(15);

/// The longest token an answer carries (a model id, an effort, an option id,
/// an adapter name or release), in bytes.
const MAX_TOKEN_BYTES: usize = 160;

pub(super) type Reading = Pin<Box<dyn Future<Output = Result<SessionOffer, HarnessError>> + Send>>;

/// How a session is read: the seat, the target, the bound. The production
/// reader is [`offer::read_offer`]; a test seats its own.
pub(super) type Reader = Arc<dyn Fn(String, OfferTarget, Duration) -> Reading + Send + Sync>;

/// One session's key: the seat, the shape and the model as the file names it.
type Key = (String, OfferProfile, String);

/// What the door holds for one server: its waiter slots, what it read lately
/// behind the lock that makes one reading at a time, and its reader.
pub(super) struct Door {
    pub(super) waiters: Arc<Semaphore>,
    read: Mutex<BTreeMap<Key, Kept>>,
    reader: Reader,
    /// Under test, the scripted reader a test seats in place of the harness.
    #[cfg(test)]
    pub(super) scripted: std::sync::OnceLock<Reader>,
}

impl std::fmt::Debug for Door {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Door").finish_non_exhaustive()
    }
}

impl Default for Door {
    fn default() -> Self {
        Self {
            waiters: Arc::new(Semaphore::new(WAITERS)),
            read: Mutex::new(BTreeMap::new()),
            reader: Arc::new(|seat, target, bound| {
                Box::pin(async move { offer::read_offer(&seat, &target, bound).await })
            }),
            #[cfg(test)]
            scripted: std::sync::OnceLock::new(),
        }
    }
}

impl Door {
    /// The reader this door asks sessions with.
    fn reader(&self) -> &Reader {
        #[cfg(test)]
        if let Some(scripted) = self.scripted.get() {
            return scripted;
        }
        &self.reader
    }
}

/// One reading, kept with its time: the offer, or why the session gave none.
#[derive(Debug, Clone)]
struct Kept {
    observed: Timestamp,
    until: Timestamp,
    answer: Result<SessionOffer, Refused>,
}

/// A reading that failed: the engine's code and a bounded reason.
#[derive(Debug, Clone)]
struct Refused {
    code: String,
    reason: String,
}

/// Whether `path` addresses this door.
pub(super) fn is_route(path: &str) -> bool {
    path.starts_with(PREFIX)
}

/// Answer the offer of the workflow `path` names. The door applies its own
/// deadline: a reading opens a session, which outlives the request deadline.
pub(super) async fn handle(path: &str, state: Arc<AppState>) -> Response<ResponseBody> {
    let Some(name) = path.strip_prefix(PREFIX).filter(|name| !name.is_empty()) else {
        return ApiError::route_not_found().into_response();
    };
    // Fail fast past the waiters instead of queueing without bound.
    let Ok(_waiter) = Arc::clone(&state.access_offers.waiters).try_acquire_owned() else {
        return busy().into_response();
    };
    match tokio::time::timeout(DEADLINE, answer(name, &state)).await {
        Ok(response) => response,
        Err(_) => super::route::request_timeout().into_response(),
    }
}

fn busy() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "access_offer_busy",
        "access offers are being read for other requests; retry shortly",
    )
}

async fn answer(name: &str, state: &AppState) -> Response<ResponseBody> {
    let admitted = match super::route::admit_by_name(name, state).await {
        Ok(admitted) => admitted,
        Err(refused) => return refused,
    };
    let requirement = nika_service_execution::access::declared_requirement(admitted.workflow());
    let declared = requirement
        .as_ref()
        .map(nika_types::access::AccessRequirement::to_json);
    let seat = match offer::declared_acp_seat(requirement.as_ref()) {
        Ok(seat) => seat.to_owned(),
        Err(reason) => {
            return json_response(
                StatusCode::OK,
                &Document::not_applicable(name, declared, reason),
            );
        }
    };
    let mut targets = offer::offer_targets(admitted.workflow(), admitted.check());
    if targets.is_empty() {
        let reason = "the workflow has no `infer:` or `agent:` task with a static model: it \
                      opens no session";
        return json_response(
            StatusCode::OK,
            &Document::not_applicable(name, declared, reason),
        );
    }
    let omitted = targets.len().saturating_sub(MAX_ROUTES);
    targets.truncate(MAX_ROUTES);
    let door = &state.access_offers;
    // One reading at a time: the lock is held across each session, so a request for a
    // key another request is reading finds it kept when its own turn comes.
    let mut read = door.read.lock().await;
    let mut routes = Vec::with_capacity(targets.len());
    for target in targets {
        let now = state.clock.now().timestamp();
        read.retain(|_, kept| kept.until > now);
        let key = (seat.clone(), target.profile, target.model.clone());
        let kept = if let Some(kept) = read.get(&key) {
            kept.clone()
        } else {
            let answer = (door.reader())(seat.clone(), target.clone(), READING).await;
            let observed = state.clock.now().timestamp();
            let kept = Kept::of(observed, answer);
            read.insert(key, kept.clone());
            kept
        };
        routes.push(Route::of(&seat, target, &kept));
    }
    drop(read);
    json_response(
        StatusCode::OK,
        &Document {
            schema: SCHEMA,
            workflow: name,
            status: "read",
            reason: None,
            declared,
            prompts_sent: 0,
            routes,
            routes_omitted: omitted,
        },
    )
}

impl Kept {
    fn of(observed: Timestamp, answer: Result<SessionOffer, HarnessError>) -> Self {
        let (span, answer) = match answer {
            Ok(session) => (FRESH, Ok(session)),
            Err(error) => {
                let code = error.nika_code().to_string();
                let reason = public_reason(&error.to_string());
                (RETRY, Err(Refused { code, reason }))
            }
        };
        Self {
            observed,
            // A time this far out cannot be represented: the reading is then not kept.
            until: observed.checked_add(span).unwrap_or(observed),
            answer,
        }
    }
}

/// A reason as a client may read it: the engine's sentence without any path in it. The shared
/// bound drops a bare path; a harness names commands and directories between quotes, so a
/// token that is a path once its opening quote or bracket is set aside is dropped here first.
fn public_reason(raw: &str) -> String {
    let kept: Vec<&str> = raw
        .split_whitespace()
        .filter(|word| {
            let bare = word.trim_start_matches(['`', '"', '\'', '(', '[', '<']);
            !(bare.starts_with('/')
                || bare.starts_with("~/")
                || bare.starts_with("./")
                || bare.starts_with("../")
                || bare.contains(":\\"))
        })
        .collect();
    bound_message(&kept.join(" "))
}

/// A value an adapter listed, carried only as a short graphic-ASCII token.
fn token(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= MAX_TOKEN_BYTES
        && value.bytes().all(|byte| byte.is_ascii_graphic()))
    .then(|| value.to_owned())
}

/// The tokens of `values`, in order, and how many were not carried on top of
/// those the reading already left out.
fn tokens(values: &[String], already: usize) -> (Vec<String>, usize) {
    let kept: Vec<String> = values.iter().filter_map(|value| token(value)).collect();
    let omitted = already + (values.len() - kept.len());
    (kept, omitted)
}

/// The offer document (`nika/access-offer@1`).
#[derive(Debug, Serialize)]
struct Document<'a> {
    schema: &'static str,
    workflow: &'a str,
    /// `read`: the routes below were asked. `not_applicable`: no session was
    /// opened, and `reason` says why.
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    /// The file's `run.access` and `run.reasoning.effort`, as the engine
    /// reads them; absent when the file declares neither.
    #[serde(skip_serializing_if = "Option::is_none")]
    declared: Option<serde_json::Value>,
    /// Always 0: this door holds no prompt.
    prompts_sent: u8,
    routes: Vec<Route>,
    routes_omitted: usize,
}

impl<'a> Document<'a> {
    fn not_applicable(
        workflow: &'a str,
        declared: Option<serde_json::Value>,
        reason: &'static str,
    ) -> Self {
        Self {
            schema: SCHEMA,
            workflow,
            status: "not_applicable",
            reason: Some(reason),
            declared,
            prompts_sent: 0,
            routes: Vec::new(),
            routes_omitted: 0,
        }
    }
}

/// One session's answer.
#[derive(Debug, Serialize)]
struct Route {
    via: String,
    protocol: &'static str,
    profile: &'static str,
    tasks: Vec<String>,
    /// `observed`: the session answered. `unavailable`: it gave no offer;
    /// `code` and `reason` say why.
    status: &'static str,
    observed_at: String,
    fresh_until: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter: Option<Adapter>,
    model: Model,
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<Listed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_tier: Option<Listed>,
}

#[derive(Debug, Serialize)]
struct Adapter {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

/// The model the file names and what the session answered about it.
#[derive(Debug, Serialize)]
struct Model {
    requested: String,
    /// `confirmed` · `session_default` · `legacy_list` · `not_offered` ·
    /// `not_confirmed`; absent when the session gave no offer.
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'static str>,
    /// The native id the session holds for the model (`confirmed`,
    /// `legacy_list`), its own model (`session_default`) or the one it
    /// answered instead (`not_confirmed`).
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offered: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    omitted: Option<usize>,
}

/// One dependent offer: the reasoning efforts or the service tiers.
#[derive(Debug, Serialize)]
struct Listed {
    /// `offered` · `not_offered` · `unobserved`.
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    option: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offered: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    omitted: Option<usize>,
    /// The value the session holds now: what a run that names none keeps on
    /// a session like this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl Listed {
    fn state(status: &'static str, reason: Option<&str>) -> Self {
        Self {
            status,
            option: None,
            offered: None,
            omitted: None,
            current: None,
            reason: reason.map(public_reason),
        }
    }

    fn effort(offer: &EffortOffer) -> Self {
        match offer {
            EffortOffer::Offered {
                option,
                values,
                omitted,
                current,
            } => {
                let (offered, omitted) = tokens(values, *omitted);
                Self {
                    status: "offered",
                    option: token(option),
                    offered: Some(offered),
                    omitted: Some(omitted),
                    current: current.as_deref().and_then(token),
                    reason: None,
                }
            }
            EffortOffer::NotOffered => Self::state("not_offered", None),
            EffortOffer::Unobserved { reason } => Self::state("unobserved", Some(reason)),
            // A state this door does not know yet is reported as not read.
            _ => Self::state("unobserved", None),
        }
    }

    fn service_tier(offer: &ServiceTierOffer) -> Self {
        match offer {
            ServiceTierOffer::Offered {
                option,
                tiers,
                current,
            } => Self {
                status: "offered",
                option: token(option),
                offered: Some(tiers.iter().map(|tier| tier.as_str().to_owned()).collect()),
                omitted: Some(0),
                current: current.map(|tier| tier.as_str().to_owned()),
                reason: None,
            },
            ServiceTierOffer::NotOffered => Self::state("not_offered", None),
            ServiceTierOffer::Unobserved { reason } => Self::state("unobserved", Some(reason)),
            _ => Self::state("unobserved", None),
        }
    }
}

impl Model {
    fn unread(requested: String) -> Self {
        Self {
            requested,
            status: None,
            current: None,
            offered: None,
            omitted: None,
        }
    }

    fn of(requested: String, session: &SessionOffer) -> Self {
        let (status, current) = match &session.model {
            ModelOffer::SessionDefault { current } => ("session_default", current.as_deref()),
            ModelOffer::Confirmed { native } => ("confirmed", Some(native.as_str())),
            ModelOffer::LegacyList { native } => ("legacy_list", Some(native.as_str())),
            ModelOffer::NotOffered => ("not_offered", None),
            ModelOffer::NotConfirmed { reported } => ("not_confirmed", reported.as_deref()),
            // A state this door does not know yet says nothing about the model.
            _ => ("not_confirmed", None),
        };
        let (offered, omitted) = tokens(&session.models, session.models_omitted);
        Self {
            requested,
            status: Some(status),
            current: current.and_then(token),
            offered: Some(offered),
            omitted: Some(omitted),
        }
    }
}

impl Route {
    fn of(seat: &str, target: OfferTarget, kept: &Kept) -> Self {
        let mut route = Self {
            via: seat.to_owned(),
            protocol: "acp",
            profile: target.profile.as_str(),
            tasks: target.tasks,
            status: "unavailable",
            observed_at: kept.observed.to_string(),
            fresh_until: kept.until.to_string(),
            code: None,
            reason: None,
            adapter: None,
            model: Model::unread(target.model.clone()),
            effort: None,
            service_tier: None,
        };
        match &kept.answer {
            Ok(session) => {
                route.status = "observed";
                route.adapter = Some(Adapter {
                    name: session.agent_name.as_deref().and_then(token),
                    version: session.agent_version.as_deref().and_then(token),
                });
                route.model = Model::of(target.model, session);
                route.effort = Some(Listed::effort(&session.effort));
                route.service_tier = Some(Listed::service_tier(&session.service_tier));
            }
            Err(refused) => {
                route.code = Some(refused.code.clone());
                route.reason = Some(refused.reason.clone());
            }
        }
        route
    }
}

#[cfg(test)]
mod tests;
