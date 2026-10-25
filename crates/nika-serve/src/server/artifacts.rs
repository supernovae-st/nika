// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `GET /v1/jobs/{id}/artifacts` and `GET /v1/jobs/{id}/artifacts/{name}` — a settled job's
//! artifacts, as its sealed journal records them (NIK-265).
//!
//! The manifest is read from the journal the job's receipt names, and only from it: the
//! journal must verify sealed and intact with the receipt's head ([`journal`]), and each entry
//! is an event that records a written byte stream with its sha256 ([`manifest`]). A download
//! looks the requested name up in that manifest — the request never names a filesystem path —
//! opens the recorded place below the held project root (no link at any component), checks
//! the whole file against the record before the first byte leaves, and checks it again while
//! it streams ([`stream`]). A file changed since the run is refused, never served as recorded.

mod journal;
mod manifest;
mod stream;

use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::Arc;

use http_body_util::BodyExt as _;
use hyper::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, HeaderName, HeaderValue,
    X_CONTENT_TYPE_OPTIONS,
};
use hyper::{Response, StatusCode};
use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

use crate::{JobId, JobReceipt, JobStoreError};

use super::error::{ApiError, ResponseBody};
use super::route::json_response;
use super::{AppState, ServerError};
use manifest::{Artifact, Kind};

/// Concurrent journal verifications and pre-download hash passes: each holds one blocking
/// worker and at most one journal in memory. The next one is refused `artifact_capacity`;
/// nothing queues. A stream holds no slot once its bytes start (the connection limit bounds
/// streams).
pub(super) const ARTIFACT_SLOTS: usize = 4;

/// The most entries one manifest lists, in journal order (`truncated` says more exist; a
/// download still finds every recorded name).
pub(super) const MAX_LISTED: usize = 1024;

/// The response header carrying the downloaded bytes' recorded SHA-256 (lowercase hex).
const SHA256_HEADER: &str = "nika-artifact-sha256";

/// What the door holds for one server: its read slots, and under test the run public key it
/// weighs beside the custody defaults (a test's in-memory key, never `~/.nika/keys`).
#[derive(Debug)]
pub(super) struct Door {
    pub(super) slots: Arc<Semaphore>,
    #[cfg(test)]
    pub(super) key: std::sync::OnceLock<PathBuf>,
}

impl Default for Door {
    fn default() -> Self {
        Self {
            slots: Arc::new(Semaphore::new(ARTIFACT_SLOTS)),
            #[cfg(test)]
            key: std::sync::OnceLock::new(),
        }
    }
}

/// `/v1/jobs/{id}/artifacts` (no name) or `/v1/jobs/{id}/artifacts/{name}` (the raw rest).
fn route(path: &str) -> Option<(&str, Option<&str>)> {
    let (id, rest) = path.strip_prefix("/v1/jobs/")?.split_once('/')?;
    let rest = rest.strip_prefix("artifacts")?;
    if id.is_empty() {
        return None;
    }
    if rest.is_empty() {
        Some((id, None))
    } else {
        Some((id, Some(rest.strip_prefix('/')?)))
    }
}

/// Whether `path` is a job's artifact manifest.
pub(super) fn is_manifest(path: &str) -> bool {
    matches!(route(path), Some((_, None)))
}

/// Whether `path` names one of a job's artifacts.
pub(super) fn is_download(path: &str) -> bool {
    matches!(route(path), Some((_, Some(_))))
}

/// The manifest of the job `path` names.
pub(super) async fn list(path: &str, state: &AppState) -> Response<ResponseBody> {
    let Some((id, None)) = route(path) else {
        return ApiError::route_not_found().into_response();
    };
    let settled = match settled_job(id, state).await {
        Ok(settled) => settled,
        Err(refused) => return refused,
    };
    match judged(settled, state).await {
        Ok((verified, _slot)) => json_response(
            StatusCode::OK,
            &ManifestBody::new(&verified, state.limits.max_artifact_bytes()),
        ),
        Err(refused) => refused,
    }
}

/// The verified bytes of the artifact `path` names. Outside the request deadline, like an
/// event stream: the size bound, the slots and the connection limit bound the work, not the
/// clock.
pub(super) async fn download(path: &str, state: Arc<AppState>) -> Response<ResponseBody> {
    let Some((id, Some(raw_name))) = route(path) else {
        return ApiError::route_not_found().into_response();
    };
    let settled = match settled_job(id, &state).await {
        Ok(settled) => settled,
        Err(refused) => return refused,
    };
    // A name no manifest can hold is refused before the journal is judged.
    let Some(name) = decode_name(raw_name) else {
        return not_found().into_response();
    };
    let (verified, slot) = match judged(settled, &state).await {
        Ok(held) => held,
        Err(refused) => return refused,
    };
    let Some(artifact) = verified
        .artifacts
        .into_iter()
        .find(|entry| entry.name == name)
    else {
        return not_found().into_response();
    };
    if artifact.size > state.limits.max_artifact_bytes() {
        return too_large().into_response();
    }
    let Some(expected) = digest_bytes(&artifact.sha256) else {
        return ApiError::internal().into_response();
    };
    let project = Arc::clone(&state.project);
    let (at, size) = (artifact.at.clone(), artifact.size);
    let opened = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        // The recorded place, below the held project root, no link at any component; then
        // every byte against the record, keeping each chunk's digest for the stream.
        let mut file = match project.open_relative(Path::new(&at)) {
            Ok(file) => file,
            Err(error) if moved_away(&error) => return Opened::Changed,
            Err(_) => return Opened::Unreadable,
        };
        match stream::verify_whole(&mut file, size, &expected) {
            Ok(Some(chunks)) => Opened::Verified(file, chunks),
            Ok(None) => Opened::Changed,
            Err(_) => Opened::Unreadable,
        }
    })
    .await;
    match opened {
        Ok(Opened::Verified(file, chunks)) => streamed(&artifact, file, chunks),
        Ok(Opened::Changed) => changed().into_response(),
        Ok(Opened::Unreadable) | Err(_) => ApiError::internal().into_response(),
    }
}

/// What the recorded place holds when a download opens it.
enum Opened {
    /// The recorded bytes, with the digest of each chunk the stream checks again.
    Verified(std::fs::File, Vec<[u8; 32]>),
    /// Other bytes, or no longer a regular file reached without a link.
    Changed,
    /// The server could not read it (permissions, descriptors, the disk).
    Unreadable,
}

/// Whether an open failed because the recorded file is no longer there as recorded: removed
/// or moved (`NotFound`), replaced by a link (`ELOOP`) or by a file where a directory stood
/// (`ENOTDIR`), or no longer a regular file.
fn moved_away(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    let link = [nix::errno::Errno::ELOOP, nix::errno::Errno::ENOTDIR]
        .iter()
        .any(|errno| error.raw_os_error() == Some(*errno as i32));
    link || matches!(
        error.kind(),
        ErrorKind::NotFound | ErrorKind::InvalidData | ErrorKind::NotADirectory
    )
}

/// A settled job's receipt and the artifacts its sealed journal records.
struct Verified {
    receipt: JobReceipt,
    verdict: &'static str,
    artifacts: Vec<Artifact>,
}

/// The receipt of the settled job `raw_id` names, and the directory its journal is under.
async fn settled_job(
    raw_id: &str,
    state: &AppState,
) -> Result<(JobReceipt, std::path::PathBuf), Response<ResponseBody>> {
    let Ok(id) = JobId::parse(raw_id) else {
        return Err(ApiError::job_not_found().into_response());
    };
    let record = match state.store.get(id).await {
        Ok(Some(record)) => record,
        Ok(None) => return Err(ApiError::job_not_found().into_response()),
        Err(ServerError::JobStore(JobStoreError::Busy) | ServerError::StoreQueueFull) => {
            return Err(ApiError::store_busy().into_response());
        }
        Err(_) => return Err(ApiError::internal().into_response()),
    };
    if !record.status().is_settled() {
        return Err(not_settled().into_response());
    }
    match (record.receipt().cloned(), state.journal_dir.clone()) {
        (Some(receipt), Some(dir)) => Ok((receipt, dir)),
        _ => Err(unavailable().into_response()),
    }
}

/// The settled job's journal held and judged under one artifact slot (returned, so a download
/// keeps it while it hashes the file).
async fn judged(
    (receipt, dir): (JobReceipt, std::path::PathBuf),
    state: &AppState,
) -> Result<(Verified, OwnedSemaphorePermit), Response<ResponseBody>> {
    let slot = match Arc::clone(&state.artifacts.slots).try_acquire_owned() {
        Ok(slot) => slot,
        Err(TryAcquireError::NoPermits) => return Err(capacity().into_response()),
        Err(TryAcquireError::Closed) => return Err(ApiError::internal().into_response()),
    };
    #[cfg(test)]
    let key = state.artifacts.key.get().cloned();
    #[cfg(not(test))]
    let key = None;
    // The slot lives inside the blocking closure: a request its deadline drops cannot free a
    // slot whose journal is still being judged.
    let held = tokio::task::spawn_blocking(move || {
        let verified = journal::hold(&dir, &receipt, key).map(|held| Verified {
            artifacts: manifest::derive(&held.events),
            verdict: held.verdict,
            receipt,
        });
        (verified, slot)
    })
    .await;
    match held {
        Ok((Ok(verified), slot)) => Ok((verified, slot)),
        Ok((Err(journal::Refusal::Unavailable), _)) => Err(unavailable().into_response()),
        Ok((Err(journal::Refusal::Unverified), _)) => Err(unverified().into_response()),
        Ok((Err(journal::Refusal::Unreadable), _)) | Err(_) => {
            Err(ApiError::internal().into_response())
        }
    }
}

/// The 200 of a download: the recorded type, length and digest, then the verified stream.
fn streamed(
    artifact: &Artifact,
    file: std::fs::File,
    chunks: Vec<[u8; 32]>,
) -> Response<ResponseBody> {
    let body = stream::VerifiedBody::new(file, artifact.size, chunks).boxed_unsync();
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    let mime = HeaderValue::from_str(&artifact.mime)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    headers.insert(CONTENT_TYPE, mime);
    headers.insert(CONTENT_LENGTH, HeaderValue::from(artifact.size));
    if let Ok(digest) = HeaderValue::from_str(&artifact.sha256) {
        headers.insert(HeaderName::from_static(SHA256_HEADER), digest);
    }
    headers.insert(CONTENT_DISPOSITION, HeaderValue::from_static("attachment"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The artifact name a request path addresses: the whole remainder percent-decoded once, so
/// `file/out/a.png` and `file%2Fout%2Fa.png` (what an `OpenAPI` client sends for one path
/// parameter) name the same entry. It is only a key, matched exactly against the manifest's
/// names: `None` for an escape that is not two hexadecimal digits, bytes that are not UTF-8,
/// NUL, or nothing at all.
fn decode_name(raw: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(raw.len());
    let mut rest = raw.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'%' {
            let (hex, after) = tail.split_at_checked(2)?;
            bytes.push(hex_pair(hex)?);
            rest = after;
        } else {
            bytes.push(byte);
            rest = tail;
        }
    }
    let name = String::from_utf8(bytes).ok()?;
    (!name.is_empty() && !name.contains('\0')).then_some(name)
}

/// One byte from two hexadecimal digits (either case), nothing else.
fn hex_pair(pair: &[u8]) -> Option<u8> {
    let digit = |byte: u8| char::from(byte).to_digit(16);
    match pair {
        [high, low] => u8::try_from(digit(*high)? * 16 + digit(*low)?).ok(),
        _ => None,
    }
}

/// The 32 bytes of a lowercase hexadecimal SHA-256.
fn digest_bytes(hex: &str) -> Option<[u8; 32]> {
    let mut bytes = [0_u8; 32];
    let pairs = hex.as_bytes().chunks_exact(2);
    if hex.len() != 64 {
        return None;
    }
    for (slot, pair) in bytes.iter_mut().zip(pairs) {
        *slot = hex_pair(pair)?;
    }
    Some(bytes)
}

/// The manifest's wire body.
#[derive(Serialize)]
struct ManifestBody<'a> {
    receipt: &'a JobReceipt,
    verdict: &'static str,
    max_artifact_bytes: u64,
    truncated: bool,
    artifacts: Vec<EntryBody<'a>>,
}

impl<'a> ManifestBody<'a> {
    fn new(verified: &'a Verified, max_artifact_bytes: u64) -> Self {
        Self {
            receipt: &verified.receipt,
            verdict: verified.verdict,
            max_artifact_bytes,
            truncated: verified.artifacts.len() > MAX_LISTED,
            artifacts: (verified.artifacts.iter().take(MAX_LISTED))
                .map(EntryBody::from)
                .collect(),
        }
    }
}

/// One manifest entry on the wire.
#[derive(Serialize)]
struct EntryBody<'a> {
    name: &'a str,
    kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<&'a str>,
    size: u64,
    sha256: &'a str,
    mime: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<&'a str>,
    event_id: &'a str,
}

impl<'a> From<&'a Artifact> for EntryBody<'a> {
    fn from(artifact: &'a Artifact) -> Self {
        Self {
            name: &artifact.name,
            kind: artifact.kind,
            path: artifact.path(),
            size: artifact.size,
            sha256: &artifact.sha256,
            mime: &artifact.mime,
            task: artifact.task.as_deref(),
            event_id: &artifact.event_id,
        }
    }
}

const fn not_settled() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "job_not_settled",
        "the job has not settled: artifacts are served once it succeeded, failed, was cancelled or was interrupted",
    )
}

const fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "artifact_journal_unavailable",
        "no journal records this job's artifacts: its receipt names no chain head, or no journal by its identity exists",
    )
}

const fn unverified() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "artifact_journal_unverified",
        "the job's journal does not verify sealed and intact with the chain head its receipt names: nothing in it is served",
    )
}

const fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "artifact_not_found",
        "no artifact by that name in the job's manifest: GET /v1/jobs/{id}/artifacts lists the names",
    )
}

const fn changed() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "artifact_changed",
        "the bytes at the artifact's place no longer match the sha256 its journal records (changed, moved, removed or replaced): never served as recorded",
    )
}

const fn too_large() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "artifact_too_large",
        "the artifact is larger than this server's download bound (the manifest's max_artifact_bytes)",
    )
}

const fn capacity() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "artifact_capacity",
        "every artifact slot is busy; retry shortly",
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests;
