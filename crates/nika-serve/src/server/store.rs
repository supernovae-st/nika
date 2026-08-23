// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::sync::Arc;
use std::thread::JoinHandle;

use serde_json::Value;
use tokio::sync::oneshot;

use crate::{
    Admission, IdempotencyKey, JobId, JobRecord, JobStatus, JobStore, JobStoreError, RequestDigest,
    ServerIncarnation,
};

use super::ServerError;

type Reply<T> = oneshot::Sender<Result<T, JobStoreError>>;

enum RequestCommand {
    Create {
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
        reply: Reply<Admission>,
    },
    Get {
        id: JobId,
        reply: Reply<Option<JobRecord>>,
    },
}

enum ControlCommand {
    Transition {
        id: JobId,
        status: JobStatus,
        event: Value,
        reply: Reply<JobRecord>,
    },
    Interrupt {
        id: JobId,
        reply: Option<Reply<JobRecord>>,
    },
    SettleInterrupted {
        reply: Reply<usize>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

/// Async facade over the one blocking durable-state owner.
#[derive(Clone)]
pub(super) struct StoreHandle {
    requests: std::sync::mpsc::SyncSender<RequestCommand>,
    controls: std::sync::mpsc::SyncSender<ControlCommand>,
}

impl StoreHandle {
    pub(super) async fn create_or_replay(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
    ) -> Result<Admission, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::Create {
            key,
            digest,
            max_jobs,
            reply,
        })?;
        receive(answer).await
    }

    pub(super) async fn get(&self, id: JobId) -> Result<Option<JobRecord>, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::Get { id, reply })?;
        receive(answer).await
    }

    pub(super) async fn transition_with_events(
        &self,
        id: JobId,
        status: JobStatus,
        event: Value,
    ) -> Result<JobRecord, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control(ControlCommand::Transition {
            id,
            status,
            event,
            reply,
        })?;
        receive(answer).await
    }

    pub(super) async fn interrupt(&self, id: JobId) -> Result<JobRecord, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control(ControlCommand::Interrupt {
            id,
            reply: Some(reply),
        })?;
        receive(answer).await
    }

    pub(super) fn interrupt_detached(&self, id: JobId) {
        let _result = self.send_control(ControlCommand::Interrupt { id, reply: None });
    }

    pub(super) async fn settle_interrupted(&self) -> Result<usize, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control(ControlCommand::SettleInterrupted { reply })?;
        receive(answer).await
    }

    fn send_request(&self, command: RequestCommand) -> Result<(), ServerError> {
        self.requests
            .try_send(command)
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => ServerError::StoreQueueFull,
                std::sync::mpsc::TrySendError::Disconnected(_) => ServerError::BlockingTask,
            })
    }

    fn send_control(&self, command: ControlCommand) -> Result<(), ServerError> {
        self.controls
            .try_send(command)
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => ServerError::StoreQueueFull,
                std::sync::mpsc::TrySendError::Disconnected(_) => ServerError::BlockingTask,
            })
    }
}

async fn receive<T>(answer: oneshot::Receiver<Result<T, JobStoreError>>) -> Result<T, ServerError> {
    answer
        .await
        .map_err(|_| ServerError::BlockingTask)?
        .map_err(ServerError::JobStore)
}

/// Join authority for the blocking owner; shutdown never detaches mutations.
pub(super) struct StoreActor {
    handle: StoreHandle,
    thread: Option<JoinHandle<()>>,
}

impl StoreActor {
    pub(super) fn start(
        store: Arc<JobStore>,
        incarnation: ServerIncarnation,
        request_capacity: usize,
        control_capacity: usize,
    ) -> Result<Self, ServerError> {
        let (requests, request_receiver) = std::sync::mpsc::sync_channel(request_capacity);
        let (controls, control_receiver) = std::sync::mpsc::sync_channel(control_capacity);
        let thread = std::thread::Builder::new()
            .name("nika-serve-store".to_owned())
            .spawn(move || {
                serve_store(&request_receiver, &control_receiver, &store, &incarnation);
            })
            .map_err(|_| ServerError::BlockingTask)?;
        Ok(Self {
            handle: StoreHandle { requests, controls },
            thread: Some(thread),
        })
    }

    pub(super) fn handle(&self) -> StoreHandle {
        self.handle.clone()
    }

    pub(super) async fn shutdown(mut self) -> Result<(), ServerError> {
        let (reply, answer) = oneshot::channel();
        self.handle
            .send_control(ControlCommand::Shutdown { reply })?;
        answer.await.map_err(|_| ServerError::BlockingTask)?;
        let thread = self.thread.take().ok_or(ServerError::BlockingTask)?;
        tokio::task::spawn_blocking(move || thread.join())
            .await
            .map_err(|_| ServerError::BlockingTask)?
            .map_err(|_| ServerError::BlockingTask)
    }
}

fn serve_store(
    requests: &std::sync::mpsc::Receiver<RequestCommand>,
    controls: &std::sync::mpsc::Receiver<ControlCommand>,
    store: &JobStore,
    incarnation: &ServerIncarnation,
) {
    loop {
        if let Ok(command) = controls.try_recv() {
            if !serve_control(command, store, incarnation) {
                break;
            }
            continue;
        }
        let command = match requests.recv_timeout(std::time::Duration::from_millis(5)) {
            Ok(command) => command,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => match controls.recv() {
                Ok(command) => {
                    if !serve_control(command, store, incarnation) {
                        break;
                    }
                    continue;
                }
                Err(_) => break,
            },
        };
        dispatch_request(command, store);
    }
}

fn dispatch_request(command: RequestCommand, store: &JobStore) {
    match command {
        RequestCommand::Create {
            key,
            digest,
            max_jobs,
            reply,
        } => {
            let result = store.create_or_replay_bounded(key, digest, max_jobs);
            let _result = reply.send(result);
        }
        RequestCommand::Get { id, reply } => {
            let _result = reply.send(store.get(&id));
        }
    }
}

fn serve_control(
    command: ControlCommand,
    store: &JobStore,
    incarnation: &ServerIncarnation,
) -> bool {
    match command {
        ControlCommand::Transition {
            id,
            status,
            event,
            reply,
        } => {
            let result = store
                .transition_with_events(&id, status, std::slice::from_ref(&event))
                .map(|mutation| mutation.record().clone());
            let _result = reply.send(result);
        }
        ControlCommand::Interrupt { id, reply } => {
            let payload = serde_json::json!({
                "kind": "execution.interrupted",
                "status": "interrupted"
            });
            let result = store.interrupt_running(&id, incarnation, &payload);
            if let Some(reply) = reply {
                let _result = reply.send(result);
            }
        }
        ControlCommand::SettleInterrupted { reply } => {
            let _result = reply.send(store.settle_interrupted_jobs(incarnation));
        }
        ControlCommand::Shutdown { reply } => {
            let _result = reply.send(());
            return false;
        }
    }
    true
}
