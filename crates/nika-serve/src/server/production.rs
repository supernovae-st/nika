// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production composition for the resident authority.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nika_error::prelude::{NikaCode, NikaErrorCode, codes};
use nika_service_execution::{
    ServiceExecutionDriver, ServiceExecutionOptions, ServiceExecutionStatus,
};

use super::{
    BoundServer, CredentialRefuse, ExecutionBackend, ExecutionDisposition, ExecutionOutcome,
    ResidentAuthority, ResidentConfig, ServerConfig, ServerError,
};

/// Production adapter from admitted execution snapshots to the shared service driver.
#[non_exhaustive]
pub struct ResidentExecutionBackend {
    display_root: PathBuf,
}

impl ResidentExecutionBackend {
    /// Bind resident output rendering to the held workflow root.
    #[must_use]
    pub fn new(display_root: impl Into<PathBuf>) -> Self {
        Self {
            display_root: display_root.into(),
        }
    }
}

impl ExecutionBackend for ResidentExecutionBackend {
    fn execute<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        let display_root = self.display_root.clone();
        Box::pin(async move { drive_resident_execution(display_root, context, None).await })
    }

    fn execute_with_max_cost<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        let display_root = self.display_root.clone();
        Box::pin(async move { drive_resident_execution(display_root, context, max_cost_usd).await })
    }
}

/// Dropping an execution future stops its blocking worker.
struct CancelOnDrop(Option<tokio::sync::oneshot::Sender<()>>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

async fn drive_resident_execution(
    display_root: PathBuf,
    context: nika_execution::ExecutionContext<'_>,
    max_cost_usd: Option<f64>,
) -> ExecutionOutcome {
    let Some(driver) = ServiceExecutionDriver::new(context, display_root) else {
        return ExecutionOutcome::failed(
            "admission_refused",
            "workflow world could not be composed",
        );
    };
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    let _cancel = CancelOnDrop(Some(cancel_tx));
    match tokio::task::spawn_blocking(move || {
        run_admitted_resident_job(driver, cancel_rx, max_cost_usd)
    })
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => ExecutionOutcome::failed("NIKA-COMP-001", "execution worker did not finish"),
    }
}

fn run_admitted_resident_job(
    driver: ServiceExecutionDriver,
    cancel: tokio::sync::oneshot::Receiver<()>,
    max_cost_usd: Option<f64>,
) -> ExecutionOutcome {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return ExecutionOutcome::failed("NIKA-COMP-001", "execution runtime could not start");
    };
    rt.block_on(async move {
        tokio::select! {
            result = driver.execute(ServiceExecutionOptions::new().with_max_cost_usd(max_cost_usd)) => match result {
                Ok(outcome) => {
                    let disposition = match outcome.status() {
                        ServiceExecutionStatus::Succeeded => ExecutionDisposition::Succeeded,
                        ServiceExecutionStatus::Paused => ExecutionDisposition::Paused,
                        _ => ExecutionDisposition::Failed,
                    };
                    let mut mapped = ExecutionOutcome::from(disposition);
                    if !outcome.outputs().is_empty() {
                        mapped = mapped.with_outputs(outcome.outputs().clone());
                    }
                    if let Some((code, message)) = outcome.error() {
                        mapped = mapped.with_error(code, message);
                    }
                    mapped
                }
                Err(_) => ExecutionOutcome::failed(
                    "NIKA-COMP-001",
                    "service runtime could not be composed",
                ),
            },
            _ = cancel => ExecutionDisposition::Failed.into(),
        }
    })
}

/// Why optional HTTP launch flags could not form one complete listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ServerLaunchRefuse {
    /// `--bind` and `--workflows` were not both supplied.
    #[error("serve · --bind and --workflows are an inseparable pair")]
    MissingBindOrWorkflows,
    /// A listener was requested without an owner-only token file.
    #[error("serve · --bind requires --token-file")]
    MissingTokenFile,
    /// Bounded rehearsal mode cannot own a network listener.
    #[error("serve · --once/--dry cannot bind a listener")]
    RehearsalWithListener,
    /// The injected rehearsal clock cannot drive a persistent listener.
    #[error("serve · scripted clock is the resident firer harness, not HTTP")]
    ScriptedClockWithListener,
    /// The bind string is not a socket address.
    #[error("serve · server configuration refused: bind address is invalid")]
    InvalidBind,
}

impl NikaErrorCode for ServerLaunchRefuse {
    fn nika_code(&self) -> NikaCode {
        codes::NIKA_001
    }
}

/// Validate the optional HTTP flag group without opening credentials or sockets.
///
/// `rehearsal` is true for `--once` or `--dry`; `scripted_clock` is true for
/// either injected clock bound. The validation order is part of the CLI contract.
///
/// # Errors
/// Returns a typed refusal for an incomplete or incompatible flag group.
pub fn optional_server_config(
    bind: Option<&str>,
    workflow_root: Option<&Path>,
    token_file: Option<&Path>,
    allow_remote: bool,
    rehearsal: bool,
    scripted_clock: bool,
) -> Result<Option<ServerConfig>, ServerLaunchRefuse> {
    if bind.is_none() && workflow_root.is_none() && token_file.is_none() && !allow_remote {
        return Ok(None);
    }
    let bind = bind.ok_or(ServerLaunchRefuse::MissingBindOrWorkflows)?;
    let workflow_root = workflow_root.ok_or(ServerLaunchRefuse::MissingBindOrWorkflows)?;
    let token_file = token_file.ok_or(ServerLaunchRefuse::MissingTokenFile)?;
    if rehearsal {
        return Err(ServerLaunchRefuse::RehearsalWithListener);
    }
    if scripted_clock {
        return Err(ServerLaunchRefuse::ScriptedClockWithListener);
    }
    let bind = bind.parse().map_err(|_| ServerLaunchRefuse::InvalidBind)?;
    Ok(Some(
        ServerConfig::new(bind, workflow_root, token_file).with_allow_remote(allow_remote),
    ))
}

/// Open the resident authority, optionally attach HTTP, print readiness, and
/// drive both surfaces under one shutdown boundary.
///
/// # Errors
/// Returns the typed authority, listener, execution, or shutdown refusal.
#[allow(clippy::disallowed_macros, clippy::print_stderr)]
pub async fn serve_resident(
    resident: ResidentConfig,
    server: Option<ServerConfig>,
    backend: Arc<dyn ExecutionBackend>,
    shutdown: impl Future<Output = ()>,
) -> Result<(), ServerError> {
    let authority = ResidentAuthority::open(resident, backend).await?;
    let Some(config) = server else {
        return authority.serve_until(shutdown).await;
    };
    let server = BoundServer::attach(config, &authority).await?;
    let line = match server.listen_line() {
        Ok(line) => line,
        Err(error) => {
            drop(server);
            return authority.serve_until(async {}).await.and(Err(error));
        }
    };
    eprintln!("{line}");
    authority.serve_with_http(server, shutdown).await
}

/// Compose and run the production resident process on a current-thread runtime.
///
/// # Errors
/// Returns a bounded operator-facing startup or lifecycle refusal.
pub fn serve_resident_process(
    workflow_root: &Path,
    state_root: PathBuf,
    server: Option<ServerConfig>,
) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("serve · the signal runtime refused: {error}"))?;
    let backend = Arc::new(ResidentExecutionBackend::new(workflow_root));
    let resident = ResidentConfig::new(state_root).with_workflow_root(workflow_root.to_path_buf());
    runtime
        .block_on(serve_resident(
            resident,
            server,
            backend,
            process_shutdown(),
        ))
        .map_err(server_operator_message)
}

const TOKEN_FILE_MINT: &str =
    "umask 077 && openssl rand -hex 24 > .nika/serve.token && chmod 600 .nika/serve.token";
const TOKEN_FILE_RULE: &str = "32–512 visible ASCII bytes, mode 0600, never argv";

fn token_file_refused(prefix: &str) -> String {
    format!("serve · {prefix} ({TOKEN_FILE_RULE})\n  {TOKEN_FILE_MINT}")
}

fn credential_prefix(kind: CredentialRefuse) -> &'static str {
    match kind {
        CredentialRefuse::Unreadable => "token file unreadable",
        CredentialRefuse::FollowRefused => "token file must be a regular file, not a symlink",
        CredentialRefuse::InsecureMode => "token file must be mode 0600",
        CredentialRefuse::InvalidMaterial => "token file must be 32–512 visible ASCII",
    }
}

/// Render a launch-flag refusal with the same token-file mint guidance used
/// for credential acquisition failures.
#[must_use]
pub fn launch_operator_message(error: ServerLaunchRefuse) -> String {
    match error {
        ServerLaunchRefuse::MissingTokenFile => token_file_refused("--bind requires --token-file"),
        other => other.to_string(),
    }
}

/// Render one bounded operator-facing refusal without paths or secret bytes.
#[must_use]
pub fn server_operator_message(error: ServerError) -> String {
    match error {
        ServerError::Credential(kind) => token_file_refused(credential_prefix(kind)),
        other => format!("serve · {other}"),
    }
}

/// Resolve on Ctrl-C or SIGTERM for a production resident process.
pub async fn process_shutdown() {
    #[cfg(unix)]
    {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = async {
                if let Some(signal) = term.as_mut() {
                    signal.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use nika_error::prelude::{NikaErrorCode as _, codes};

    use super::ServerLaunchRefuse;

    #[test]
    fn launch_refusals_share_the_validation_wire_code() {
        let refusals = [
            ServerLaunchRefuse::MissingBindOrWorkflows,
            ServerLaunchRefuse::MissingTokenFile,
            ServerLaunchRefuse::RehearsalWithListener,
            ServerLaunchRefuse::ScriptedClockWithListener,
            ServerLaunchRefuse::InvalidBind,
        ];
        assert!(
            refusals
                .into_iter()
                .all(|refusal| refusal.nika_code() == codes::NIKA_001)
        );
    }
}
