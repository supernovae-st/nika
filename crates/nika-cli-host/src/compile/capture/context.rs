// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Held host context only; no model, HTTP caller or semantic record chooses a directory.

use super::{Capture, CapturePolicy, CaptureReport, CaptureState, CaptureStatus};
use nika_fs::OwnedDir;
use nika_onboard::compile::CompileRequest;
use std::sync::Arc;

/// Reusable trusted configuration; ordinary storage failure never refuses authoring.
/// No Debug or serialization: its policy contains withheld values.
#[derive(Clone)]
pub struct CaptureContext {
    directory: Option<Arc<OwnedDir>>,
    policy: CapturePolicy,
    report: CaptureReport,
}

impl CaptureContext {
    /// The host already selected the held container and admitted its whole context.
    #[must_use]
    pub fn new(directory: Option<OwnedDir>, policy: CapturePolicy, report: CaptureReport) -> Self {
        report.publish(CaptureStatus {
            state: CaptureState::Armed,
            ..CaptureStatus::default()
        });
        Self {
            directory: directory.map(Arc::new),
            policy,
            report,
        }
    }

    /// Start another independent round, preserving only configuration and its local listener.
    #[must_use]
    pub fn fresh(&self) -> Self {
        Self {
            directory: self.directory.clone(),
            policy: self.policy.clone(),
            report: self.report.fresh(),
        }
    }

    /// Keep the host's resolved credential private before the callback sees returned Text.
    #[must_use]
    pub fn with_withheld(mut self, value: &str) -> Self {
        self.policy = self.policy.with_withheld(value);
        self
    }

    /// Scope the actual authoring invocation only; a request with no policy makes no capture.
    #[must_use]
    pub fn start(&self, request: &CompileRequest) -> Option<Capture> {
        let policy = request.authoring.as_ref()?;
        Some(Capture::start(
            self.directory.as_deref(),
            self.policy.clone(),
            self.report.clone(),
            Some(&policy.model),
            policy.max_tokens,
            u64::try_from(policy.timeout.as_millis()).unwrap_or(u64::MAX),
        ))
    }

    /// Descriptor-relative creation with publication synced at every newly traversed parent.
    /// Failure remains local unavailability; the caller never uses a display path as authority.
    #[must_use]
    pub fn directory(root: &OwnedDir, components: &[&str]) -> Option<OwnedDir> {
        let (first, rest) = components.split_first()?;
        let mut directory = root.create_below(&[*first]).ok()?;
        root.as_file().sync_all().ok()?;
        for component in rest {
            let child = directory.create_below(&[*component]).ok()?;
            directory.as_file().sync_all().ok()?;
            directory = child;
        }
        Some(directory)
    }
}

impl Capture {
    /// Poll the existing observer in the future's real runtime, including blocking host work.
    pub async fn observe<F: std::future::Future>(capture: Option<&Self>, work: F) -> F::Output {
        match capture {
            Some(capture) => {
                nika_onboard::compile::observe::observe_authoring(capture.sink(), work).await
            }
            None => work.await,
        }
    }
}
