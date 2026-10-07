// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Private authoring capture: the one host serializer and writer of returned application
//! Text, into a reserved log below a held directory the trusted host selected. Diagnostic
//! evidence only, never a workflow, permit, semantic record or replay input; no raw data in
//! public compile documents, local status or Debug output.

mod context;
mod policy;
mod project;
#[cfg(test)]
mod tests;
pub use context::CaptureContext;
pub use policy::{CapturePolicy, TextAdmission};

use nika_fs::{OwnedDir, ReservedLog};
use nika_onboard::compile::{CompileOutcome, observe};
use serde_json::{Value, json};
use std::io::{self, Read as _};
use std::os::unix::fs::MetadataExt as _;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const ROUND_BYTES: u64 = 1024 * 1024;
const CONTAINER_BYTES: u64 = 16 * ROUND_BYTES;
const CLOSING_BYTES: u64 = 16 * 1024;
const TRACKED_CALLS: usize = 64;

/// Local persistence state, never a compiler verdict or a provider send receipt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaptureState {
    #[default]
    Off,
    Armed,
    Open,
    Unavailable,
    Withheld,
    WriteFailed,
    Saved,
}

/// Bounded local facts. No paths, provider errors, model text or secret labels.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct CaptureStatus {
    pub state: CaptureState,
    /// Opaque host-generated identity; never a path or authority to open one.
    pub identity: Option<[u64; 2]>,
    pub received: u64,
    pub saved: u64,
    pub withheld: u64,
    pub omitted: u64,
    /// Later `CompileOutcome` correlations; `returned_metadata` already belongs to saved calls.
    pub outcome_metadata_saved: u64,
    pub finished: bool,
    pub close_saved: bool,
    /// True only when this scope received its `CompileOutcome` through `finish(Some(_))`.
    /// Saved and finished alone describe capture persistence, not compile completion.
    pub outcome_returned: bool,
}

/// A trusted local status listener, never an HTTP/SSE or model sink.
/// It must not panic, re-enter capture, or interpret status as compile authority.
pub type CaptureListener = Arc<dyn Fn(CaptureStatus) + Send + Sync>;

/// Independently retained local status, even when persistence admission fails.
#[derive(Clone, Default)]
pub struct CaptureReport {
    state: Arc<Mutex<CaptureStatus>>,
    listener: Option<CaptureListener>,
}

impl std::fmt::Debug for CaptureReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CaptureReport")
            .field(&self.status())
            .finish()
    }
}

impl CaptureReport {
    /// Observe only this report's facts; concurrent rounds use fresh reports.
    #[must_use]
    pub fn with_listener(listener: CaptureListener) -> Self {
        Self {
            listener: Some(listener),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn status(&self) -> CaptureStatus {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn fresh(&self) -> Self {
        Self {
            state: Arc::default(),
            listener: self.listener.clone(),
        }
    }

    fn publish(&self, status: CaptureStatus) {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = status;
        if let Some(listener) = &self.listener {
            listener(status);
        }
    }
}

impl CaptureStatus {
    /// Bounded local presentation: no raw text, path, key, model name or error string.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "private capture {:?} · id {:?} · received {} · saved {} · withheld {} · omitted {} · outcome metadata {} · finished {} · close saved {} · outcome returned {}",
            self.state,
            self.identity,
            self.received,
            self.saved,
            self.withheld,
            self.omitted,
            self.outcome_metadata_saved,
            self.finished,
            self.close_saved,
            self.outcome_returned,
        )
    }
}

/// Operator opt-in on `CompileCommand`, preserving the legacy `CompileArgs` literal.
#[derive(Debug, Default, clap::Args)]
#[non_exhaustive]
pub struct CaptureFlags {
    /// Privately record bounded authoring Text in this explicitly admitted host context.
    #[arg(long = "private-authoring-capture", requires = "authoring_model")]
    pub enabled: bool,
    #[arg(skip)]
    report: OnceLock<CaptureReport>,
}

impl CaptureFlags {
    pub(super) fn begin(&self) {
        if self.enabled || self.report.get().is_some() {
            self.report
                .get_or_init(CaptureReport::default)
                .publish(CaptureStatus {
                    state: if self.enabled {
                        CaptureState::Unavailable
                    } else {
                        CaptureState::Off
                    },
                    ..CaptureStatus::default()
                });
        }
    }

    /// The default is off; construction reads no filesystem or configuration.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            enabled: false,
            report: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn status(&self) -> CaptureStatus {
        self.report
            .get()
            .map_or_else(CaptureStatus::default, CaptureReport::status)
    }
}

struct State {
    log: Option<ReservedLog>,
    policy: CapturePolicy,
    status: CaptureStatus,
    report: CaptureReport,
    identities: Vec<project::Identity>,
}

impl State {
    fn append(&mut self, record: &Value) -> bool {
        let Some(log) = self.log.as_mut() else {
            return false;
        };
        let result = log.append_encoded(|writer| {
            serde_json::to_writer(writer, record).map_err(io::Error::other)
        });
        match result {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(_) => {
                self.status.state = CaptureState::WriteFailed;
                self.log = None;
                false
            }
        }
    }

    fn received(&mut self, call: &observe::AuthoringObservation<'_>) {
        self.status.received = self.status.received.saturating_add(1);
        if self.status.finished || self.log.is_none() {
            self.status.omitted = self.status.omitted.saturating_add(1);
            self.report.publish(self.status);
            return;
        }
        let Some((mut record, identity, withheld)) = project::observation(call, &self.policy)
        else {
            self.status.omitted = self.status.omitted.saturating_add(1);
            self.report.publish(self.status);
            return;
        };
        if withheld {
            self.status.withheld = self.status.withheld.saturating_add(1);
        }
        if self.identities.len() < TRACKED_CALLS {
            self.identities.push(identity);
        }
        if self.append(&record) {
            self.status.saved = self.status.saved.saturating_add(1);
        } else if self.log.is_some() {
            // Ok(None) guarantees no disk write. This is a different small record,
            // not a retry of an uncertain write. Retain original identity/counts.
            record["response"]["text"] = Value::Null;
            record["response"]["withheld_reason"] = json!("serialized_round_bound");
            self.status.omitted = self.status.omitted.saturating_add(1);
            if self.append(&record) {
                self.status.saved = self.status.saved.saturating_add(1);
            }
        } else {
            self.status.omitted = self.status.omitted.saturating_add(1);
        }
        self.report.publish(self.status);
    }
}

/// A capture scope is not cloneable; its sink borrows observations synchronously.
pub struct Capture(Arc<Mutex<State>>);

impl Capture {
    /// Start only after the host chose the held directory and admitted its context.
    /// Ordinary storage failures produce local status and do not fail compilation.
    #[must_use]
    pub fn start(
        directory: Option<&OwnedDir>,
        policy: CapturePolicy,
        report: CaptureReport,
        requested_model: Option<&str>,
        max_tokens: u32,
        timeout_ms: u64,
    ) -> Self {
        let (model, model_withheld) = policy.metadata(requested_model);
        let log = directory.and_then(|directory| {
            policy
                .ready()
                .then(|| reserve(directory))
                .transpose()
                .ok()
                .flatten()
        });
        let identity = log.as_ref().map(|(_, identity)| *identity);
        let log = log.map(|(log, _)| log);
        let status = CaptureStatus {
            identity,
            state: if log.is_some() {
                CaptureState::Open
            } else {
                CaptureState::Unavailable
            },
            ..CaptureStatus::default()
        };
        let mut state = State {
            log,
            policy,
            status,
            report,
            identities: Vec::new(),
        };
        if state.log.is_some() && !state.append(&json!({
            "capture_version": 1,
            "kind": "scope",
            "identity": identity,
            "requested_model": model,
            "requested_model_withheld": model_withheld,
            "max_output_tokens": max_tokens,
            "timeout_ms": timeout_ms,
            "scope_opened_unix_ms": SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis()),
            "physical_send": "unknown",
            "retention": "reservation_retained_until_operator_removal",
        })) {
            state.status.state = CaptureState::WriteFailed;
            state.log = None;
        }
        state.report.publish(state.status);
        Self(Arc::new(Mutex::new(state)))
    }

    fn locked(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|poisoned| {
            let mut state = poisoned.into_inner();
            state.log = None;
            state.status.state = CaptureState::WriteFailed;
            state
        })
    }

    #[must_use]
    pub fn sink(&self) -> observe::Sink {
        let state = Arc::clone(&self.0);
        Arc::new(move |call| match state.lock() {
            Ok(mut state) => state.received(call),
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.log = None;
                state.status.state = CaptureState::WriteFailed;
                state.report.publish(state.status);
            }
        })
    }

    /// Correlate only the exact invocation's closed result. None means interrupted/unknown.
    pub fn finish(&self, outcome: Option<&CompileOutcome>) {
        let mut state = self.locked();
        if state.status.finished {
            return;
        }
        state.status.outcome_returned = outcome.is_some();
        // The observer excludes judges; use the same projection before ordinal matching.
        let entries = outcome
            .and_then(|out| out.provenance.authoring.as_ref())
            .map(|receipt| {
                receipt.context.iter().filter(|entry| {
                    entry["call"]
                        .as_str()
                        .is_some_and(|role| !role.starts_with("judge"))
                })
            });
        // Count the full population without retaining a reference for every entry.
        let entry_count = entries
            .as_ref()
            .and_then(|entries| u64::try_from(entries.clone().count()).ok());
        let identities = std::mem::take(&mut state.identities);
        // Resolve only the at-most-64 retained ordinals; this is not a call limit.
        for identity in identities {
            let entry = entries
                .as_ref()
                .filter(|_| entry_count == Some(state.status.received))
                .and_then(|entries| {
                    identity
                        .ordinal
                        .checked_sub(1)
                        .and_then(|n| usize::try_from(n).ok())
                        .and_then(|n| entries.clone().nth(n))
                })
                .filter(|entry| identity.matches(entry));
            if let Some(entry) = entry {
                let metadata = identity.metadata(entry, &state.policy);
                if state.append(&metadata) {
                    state.status.outcome_metadata_saved =
                        state.status.outcome_metadata_saved.saturating_add(1);
                }
            }
        }
        let close = json!({
            "capture_version": 1,
            "kind": "close",
            "outcome_returned": state.status.outcome_returned,
            "received": state.status.received,
            "saved_call_records": state.status.saved,
            "withheld_payloads": state.status.withheld,
            "omitted_payloads": state.status.omitted,
            "outcome_metadata_saved": state.status.outcome_metadata_saved,
            "outcome_metadata_unknown": state.status.received.saturating_sub(state.status.outcome_metadata_saved),
            "unreceived_or_unsynchronized_work": "unknown",
        });
        if let Some(log) = state.log.as_mut() {
            match log.finish_encoded(|writer| {
                serde_json::to_writer(writer, &close).map_err(io::Error::other)
            }) {
                Ok(Some(_)) => {
                    state.status.close_saved = true;
                    state.status.state = if state.status.omitted > 0 || state.status.withheld > 0 {
                        CaptureState::Withheld
                    } else {
                        CaptureState::Saved
                    };
                }
                Ok(None) | Err(_) => state.status.state = CaptureState::WriteFailed,
            }
        }
        state.status.finished = true;
        state.log = None;
        state.report.publish(state.status);
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // A best-effort explicit unknown close; no retry after a poisoned write.
        // This does not promise completion on SIGKILL or during unwinding failure.
        self.finish(None);
    }
}

fn reserve(directory: &OwnedDir) -> io::Result<(ReservedLog, [u64; 2])> {
    // The held container is this user's and private before the host writes its marker, and the
    // marker is exact before a round is reserved: a refused marker reserves nothing and is never
    // rewritten. The reservation re-checks the container and every entry on its descriptors.
    let held = directory.as_file().metadata()?;
    if held.uid() != nix::unistd::Uid::effective().as_raw() || held.mode() & 0o077 != 0 {
        return Err(io::Error::other(
            "capture container is not private to this user",
        ));
    }
    match directory.write_once(".gitignore", "*\n") {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let mut bytes = Vec::new();
            directory
                .open_relative(std::path::Path::new(".gitignore"))?
                .take(3)
                .read_to_end(&mut bytes)?;
            if bytes != b"*\n" {
                return Err(io::Error::other("capture ignore protection unavailable"));
            }
        }
        Err(error) => return Err(error),
    }
    let instant = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    let identity = [
        u64::from(std::process::id()),
        u64::try_from(instant.as_nanos()).map_err(io::Error::other)?,
    ];
    let name = format!("{}-{}.capture", identity[0], identity[1]);
    // The marker now exists before the inventory, which charges its two bytes; the container
    // budget keeps its historical two-byte allowance, so rounds are charged as before.
    let log = directory.reserve_private_log(
        &name,
        ROUND_BYTES,
        CONTAINER_BYTES - 2,
        CLOSING_BYTES,
        256,
    )?;
    Ok((log, identity))
}

fn cli_directory() -> Option<OwnedDir> {
    let root = std::env::current_dir().ok()?;
    let root = OwnedDir::open(&root).ok()?;
    CaptureContext::directory(&root, &[".nika", "compile", "capture"])
}

/// The CLI's opt-in capture around its actual compile future: the resolved `keys` are
/// withheld, the context is admitted only when `admitted` (no harness seat), and the round
/// finishes on the returned result, before any later rendering or sidecar failure.
pub(super) async fn cli_observe<'k, T>(
    flags: &CaptureFlags,
    keys: impl Iterator<Item = &'k str>,
    admitted: bool,
    (model, max_tokens, timeout): (Option<&str>, u32, std::time::Duration),
    work: impl std::future::Future<Output = Result<CompileOutcome, T>>,
) -> Result<CompileOutcome, T> {
    let capture = flags.enabled.then(|| {
        let mut admitted = admitted;
        let mut withheld = Vec::new();
        for key in keys {
            if key.len() <= 32 * 1024 {
                withheld.push(key.to_owned());
            } else {
                admitted = false;
            }
        }
        let policy = CapturePolicy::admitted(withheld, Arc::new(move |_| admitted));
        let report = flags.report.get_or_init(CaptureReport::default).clone();
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        let directory = cli_directory();
        Capture::start(
            directory.as_ref(),
            policy,
            report,
            model,
            max_tokens,
            timeout_ms,
        )
    });
    let result = Capture::observe(capture.as_ref(), work).await;
    if let Some(capture) = &capture {
        capture.finish(result.as_ref().ok());
    }
    result
}
