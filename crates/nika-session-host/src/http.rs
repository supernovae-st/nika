// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The HTTP door of the Session (`nika serve`): the routes under `/v1/sessions`, after the
//! server's own bearer check and body limit. One live Session per served project, because the
//! Session's history is one per (HOME, project). A command that reached the Session settles in
//! its worker whatever happens to the request that carried it: a client that disconnects or
//! times out re-sends the same command and gets the recorded result. No route here applies a
//! request deadline of its own.

use std::convert::Infallible;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt as _, Full};
use hyper::body::Frame as BodyFrame;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue};
use hyper::{HeaderMap, Method, Response, StatusCode};

use nika_onboard::compile::room::JqHelper;
use nika_session::SessionRuntime;

use crate::host::{Dispatch, SessionHost};
use crate::run::RunDoor;
use crate::wire::{Body, CONTRACT, Command, Frame, Refused};

/// The routes and frames of this door, as an RFC 7386 merge patch of the server's `OpenAPI`
/// document (merged on a server that serves sessions).
pub const OPENAPI: &str = include_str!("http/openapi.json");

/// The body type every route answers with (the server's own).
pub type ResponseBody = UnsyncBoxBody<Bytes, Infallible>;

/// How the server opens its project's Session: the runtime and its opening notices, or the
/// Session's refusal.
pub type Opener = Arc<dyn Fn() -> Result<(SessionRuntime, Vec<String>), String> + Send + Sync>;

/// The run door each opened Session is lent.
pub type Doors = Box<dyn Fn() -> Box<dyn RunDoor> + Send + Sync>;

/// The served project's Session, at most one live at a time.
pub struct Sessions {
    live: tokio::sync::Mutex<Option<Arc<SessionHost>>>,
    opener: Opener,
    doors: Doors,
}

impl std::fmt::Debug for Sessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sessions").finish_non_exhaustive()
    }
}

/// The opening request: nothing, or the contract it speaks.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Open {
    contract: String,
}

impl Sessions {
    /// Sessions opened by `opener`, each lent a run door from `doors`.
    #[must_use]
    pub fn new(opener: Opener, doors: Doors) -> Self {
        Self {
            live: tokio::sync::Mutex::new(None),
            opener,
            doors,
        }
    }

    /// Answer one request under `/v1/sessions`; `None` when `path` is not one of its routes
    /// (the server answers it as any unknown route).
    pub async fn route(
        &self,
        method: &Method,
        path: &str,
        headers: &HeaderMap,
        body: Bytes,
    ) -> Option<Response<ResponseBody>> {
        let rest = path.strip_prefix("/v1/sessions")?;
        if rest.is_empty() {
            if *method != Method::POST {
                return None;
            }
            return Some(self.open(&body).await);
        }
        let rest = rest.strip_prefix('/')?;
        let (id, tail) = rest.split_once('/').unwrap_or((rest, ""));
        let known = matches!(
            (method, tail),
            (&Method::GET | &Method::DELETE, "")
                | (&Method::GET, "details" | "events")
                | (&Method::POST, "commands")
        );
        if id.is_empty() || !known {
            return None;
        }
        let Some(host) = self.live(id).await else {
            let frame = Frame::refused(
                id,
                Refused::SessionNotFound,
                "no live Session by that identity on this server",
                None,
                None,
                None,
            );
            return Some(respond(&frame));
        };
        Some(match (method, tail) {
            (&Method::GET, "") => respond(&host.snapshot()),
            (&Method::GET, "details") => respond(&host.details()),
            (&Method::GET, "events") => events(host, headers),
            (&Method::POST, _) => command(&host, &body).await,
            _ => self.close(&host).await,
        })
    }

    /// The Session of the project at `root`, opened as bare `nika` opens it by this process
    /// ([`crate::open::open_bare`]: its HOME, its census, this binary as the jq helper).
    #[must_use]
    pub fn bare(root: PathBuf, doors: Doors) -> Self {
        let opener: Opener = Arc::new(move || {
            let home = nika_cli_host::probe::home_dir();
            let jq = std::env::current_exe().ok().map(JqHelper::new);
            crate::open::open_bare(&root, home.as_deref(), jq)
        });
        Self::new(opener, doors)
    }

    /// The live Session, whatever its identity (tests hold its worker).
    #[cfg(test)]
    pub(crate) async fn any_live(&self) -> Option<Arc<SessionHost>> {
        self.live.lock().await.clone()
    }

    /// The live Session named `id`, when it is.
    async fn live(&self, id: &str) -> Option<Arc<SessionHost>> {
        let live = self.live.lock().await;
        (live.as_ref())
            .filter(|host| host.session() == id && !host.is_closed())
            .cloned()
    }

    /// Open the project's Session, unless one is live: then its identity, to attach to.
    async fn open(&self, body: &[u8]) -> Response<ResponseBody> {
        if !body.is_empty() {
            let stated = (serde_json::from_slice::<Open>(body).ok()).map(|open| open.contract);
            if stated.as_deref() != Some(CONTRACT) {
                let message =
                    format!("open a Session with no body or {{\"contract\":\"{CONTRACT}\"}}");
                let frame = Frame::refused("", Refused::Malformed, message, None, None, None);
                return respond(&frame);
            }
        }
        let mut live = self.live.lock().await;
        if let Some(host) = live.as_ref().filter(|host| !host.is_closed()) {
            let frame = Frame::refused(
                host.session(),
                Refused::SessionLive,
                "a Session is already live for this project · attach to it by its identity",
                None,
                None,
                Some(host.current()),
            );
            return respond(&frame);
        }
        let session_opener = Arc::clone(&self.opener);
        let made = match tokio::task::spawn_blocking(move || session_opener()).await {
            Ok(made) => made,
            Err(error) => Err(format!("the Session could not open: {error}")),
        };
        let started = made.and_then(|(runtime, notices)| {
            SessionHost::start(runtime, (self.doors)(), notices)
                .map_err(|error| format!("the Session could not start: {error}"))
        });
        match started {
            Ok(host) => {
                let host = Arc::new(host);
                let first = host.opened();
                *live = Some(host);
                first.map_or_else(
                    || internal("the opened frame is missing"),
                    |frame| respond(&frame),
                )
            }
            Err(why) => {
                let frame = Frame::refused("", Refused::SessionUnavailable, why, None, None, None);
                respond(&frame)
            }
        }
    }

    async fn close(&self, host: &SessionHost) -> Response<ResponseBody> {
        let _closing = host.dispatch(Command::Close);
        let closed = host.closed().await;
        let mut live = self.live.lock().await;
        if live
            .as_ref()
            .is_some_and(|live| live.session() == host.session())
        {
            *live = None;
        }
        closed.map_or_else(|| internal("the closed frame is missing"), |f| respond(&f))
    }
}

/// One command: a refusal or a replay answers at once, a submit once it settled.
async fn command(host: &SessionHost, body: &[u8]) -> Response<ResponseBody> {
    let refused = |message: String| {
        let frame = Frame::refused(
            host.session(),
            Refused::Malformed,
            message,
            None,
            None,
            None,
        );
        respond(&frame)
    };
    let command = match Command::parse(body) {
        Ok(command @ (Command::Submit { .. } | Command::Stop { .. })) => command,
        Ok(_) => {
            return refused(
                "only submit and stop are commands here · read with GET, close with DELETE"
                    .to_owned(),
            );
        }
        Err(why) => return refused(why),
    };
    match host.dispatch(command) {
        Dispatch::Reply(frame) | Dispatch::Logged(frame) => respond(&frame),
        Dispatch::Accepted { command, repeated } => match host.result(&command).await {
            Some(frame) if repeated => respond(&frame.replayed()),
            Some(frame) => respond(&frame),
            None => {
                let frame = Frame::refused(
                    host.session(),
                    Refused::SessionNotFound,
                    "the Session ended before this command settled",
                    Some(&command),
                    None,
                    None,
                );
                respond(&frame)
            }
        },
        _ => refused("this Session is closing".to_owned()),
    }
}

/// The Session's log as server-sent events, from the `Last-Event-ID` of this Session when the
/// client names one; a cursor of another Session or incarnation, or beyond the log, gets one
/// `resync` frame with the current snapshot, then the live events.
fn events(host: Arc<SessionHost>, headers: &HeaderMap) -> Response<ResponseBody> {
    let session = host.session().to_owned();
    let last = host.last_event();
    let named = (headers.get("last-event-id"))
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            (value.split_once(':'))
                .filter(|(id, _)| *id == session)
                .and_then(|(_, n)| n.parse::<u64>().ok())
                .filter(|n| *n <= last)
        });
    let (from, resync) = match named {
        None => (0, None),
        Some(Some(n)) => (n, None),
        Some(None) => {
            // The cursor and the snapshot it stands for, read at one instant.
            let (from, snapshot) = host.resync_point();
            (
                from,
                Some(Frame::new(&session, None, Body::Resync { snapshot })),
            )
        }
    };
    #[cfg(test)]
    tests::after_resync_point(&session);
    let (sender, receiver) = tokio::sync::mpsc::channel::<Bytes>(16);
    tokio::spawn(async move {
        if let Some(frame) = resync
            && sender.send(sse(&session, from, &frame)).await.is_err()
        {
            return;
        }
        let mut cursor = from;
        loop {
            let (frames, complete) = host.next_events(cursor).await;
            for frame in &frames {
                cursor = frame.event().unwrap_or(cursor);
                if sender.send(sse(&session, cursor, frame)).await.is_err() {
                    return;
                }
            }
            if complete && frames.is_empty() {
                return;
            }
        }
    });
    let mut response = Response::new(EventBody(receiver).boxed_unsync());
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// One server-sent event: its id binds the cursor to this Session.
fn sse(session: &str, event: u64, frame: &Frame) -> Bytes {
    Bytes::from(format!(
        "id: {session}:{event}\ndata: {}\n\n",
        frame.to_line()
    ))
}

/// The events of one stream, as the pump sends them.
struct EventBody(tokio::sync::mpsc::Receiver<Bytes>);

impl hyper::body::Body for EventBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<BodyFrame<Bytes>, Infallible>>> {
        self.0
            .poll_recv(cx)
            .map(|data| data.map(|data| Ok(BodyFrame::data(data))))
    }
}

/// A frame as a JSON response, with the status its kind answers with.
fn respond(frame: &Frame) -> Response<ResponseBody> {
    let mut response = Response::new(Full::new(Bytes::from(frame.to_line())).boxed_unsync());
    *response.status_mut() =
        StatusCode::from_u16(frame.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

/// The words of a refusal a server answered (`{"error":{"message"}}`), never a guess.
pub async fn refusal_words(response: Response<ResponseBody>) -> String {
    let body = (response.into_body().collect().await).map(http_body_util::Collected::to_bytes);
    (body.ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "the server refused the run".to_owned())
}

/// The job a server's admission answered (`{"id"}`), or its refusal's words.
///
/// # Errors
/// The refusal's words, or that the answer named no job.
pub async fn job_id(response: Response<ResponseBody>) -> Result<String, String> {
    if !response.status().is_success() {
        return Err(refusal_words(response).await);
    }
    let body = (response.into_body().collect().await).map(http_body_util::Collected::to_bytes);
    (body.ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value["id"].as_str().map(str::to_owned))
        .ok_or_else(|| "the server's admission answered no job".to_owned())
}

/// An answer the host cannot give, said plainly.
fn internal(message: &'static str) -> Response<ResponseBody> {
    let frame = Frame::refused("", Refused::SessionUnavailable, message, None, None, None);
    respond(&frame)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
