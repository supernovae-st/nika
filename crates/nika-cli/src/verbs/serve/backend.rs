// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::path::PathBuf;

pub(super) struct ResidentBackend {
    display_root: PathBuf,
}

impl ResidentBackend {
    pub(super) fn new(display_root: PathBuf) -> Self {
        Self { display_root }
    }
}

impl nika_serve::ExecutionBackend for ResidentBackend {
    fn execute<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = nika_serve::ExecutionOutcome> + Send + 'a>,
    > {
        let display_root = self.display_root.clone();
        Box::pin(async move { drive_resident_execution(display_root, context).await })
    }
}

/// Dropping the execute future stops its blocking worker.
struct CancelOnDrop(Option<tokio::sync::oneshot::Sender<()>>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

pub(super) async fn drive_resident_execution(
    display_root: PathBuf,
    context: nika_execution::ExecutionContext<'_>,
) -> nika_serve::ExecutionOutcome {
    use nika_service_execution::ServiceExecutionDriver;

    let Some(driver) = ServiceExecutionDriver::new(context, display_root) else {
        return nika_serve::ExecutionOutcome::failed(
            "admission_refused",
            "workflow world could not be composed",
        );
    };
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    let _cancel = CancelOnDrop(Some(cancel_tx));
    match tokio::task::spawn_blocking(move || run_admitted_resident_job(driver, cancel_rx)).await {
        Ok(outcome) => outcome,
        Err(_) => {
            nika_serve::ExecutionOutcome::failed("NIKA-COMP-001", "execution worker did not finish")
        }
    }
}

fn run_admitted_resident_job(
    driver: nika_service_execution::ServiceExecutionDriver,
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> nika_serve::ExecutionOutcome {
    use nika_service_execution::{ServiceExecutionOptions, ServiceExecutionStatus};

    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return nika_serve::ExecutionOutcome::failed(
            "NIKA-COMP-001",
            "execution runtime could not start",
        );
    };
    rt.block_on(async move {
        tokio::select! {
            result = driver.execute(ServiceExecutionOptions::new()) => match result {
                Ok(outcome) => {
                    let disposition = match outcome.status() {
                        ServiceExecutionStatus::Succeeded => {
                            nika_serve::ExecutionDisposition::Succeeded
                        }
                        ServiceExecutionStatus::Paused => nika_serve::ExecutionDisposition::Paused,
                        _ => nika_serve::ExecutionDisposition::Failed,
                    };
                    let mut mapped = nika_serve::ExecutionOutcome::from(disposition);
                    if !outcome.outputs().is_empty() {
                        mapped = mapped.with_outputs(outcome.outputs().clone());
                    }
                    if let Some((code, message)) = outcome.error() {
                        mapped = mapped.with_error(code, message);
                    }
                    mapped
                }
                Err(_) => nika_serve::ExecutionOutcome::failed(
                    "NIKA-COMP-001",
                    "service runtime could not be composed",
                ),
            },
            _ = cancel => nika_serve::ExecutionDisposition::Failed.into(),
        }
    })
}
