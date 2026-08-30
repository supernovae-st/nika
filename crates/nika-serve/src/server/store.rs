// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread::JoinHandle;

use serde_json::Value;
use tokio::sync::{Notify, oneshot};

use crate::{
    Admission, EventPageLimit, IdempotencyKey, JobEvent, JobId, JobOrigin, JobReceipt, JobRecord,
    JobStatus, JobStore, JobStoreError, RequestDigest, ServerIncarnation,
};

use super::ServerError;

type Reply<T> = oneshot::Sender<Result<T, JobStoreError>>;

pub(super) struct EventPage {
    pub events: Vec<JobEvent>,
    pub record: JobRecord,
    pub terminal_sequence: Option<u64>,
}

enum RequestCommand {
    Create {
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
        workflow: String,
        world: String,
        reply: Reply<Admission>,
    },
    PrepareScheduled {
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
        workflow: String,
        world: String,
        origin: Box<JobOrigin>,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
        event: Value,
        reply: Reply<Admission>,
    },
    Get {
        id: JobId,
        reply: Reply<Option<JobRecord>>,
    },
    LoadWorld {
        id: JobId,
        reply: Reply<String>,
    },
    Queued {
        reply: Reply<Vec<(JobId, String)>>,
    },
    EventsAfter {
        id: JobId,
        after: u64,
        limit: EventPageLimit,
        reply: Reply<EventPage>,
    },
}

enum ControlCommand {
    Transition {
        id: JobId,
        status: JobStatus,
        event: Value,
        outputs: Option<BTreeMap<String, Value>>,
        receipt: Option<Box<JobReceipt>>,
        reply: Reply<JobRecord>,
    },
    StartExecution {
        id: JobId,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
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
    events: Arc<Notify>,
}

impl StoreHandle {
    pub(super) async fn create_or_replay(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
        workflow: String,
        world: String,
    ) -> Result<Admission, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::Create {
            key,
            digest,
            max_jobs,
            workflow,
            world,
            reply,
        })?;
        receive(answer).await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_scheduled_blocking(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
        workflow: String,
        world: String,
        origin: JobOrigin,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
        event: Value,
    ) -> Result<Admission, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::PrepareScheduled {
            key,
            digest,
            max_jobs,
            workflow,
            world,
            origin: Box::new(origin),
            execution_id,
            trace_id,
            snapshot_digest,
            event,
            reply,
        })?;
        receive_blocking(answer)
    }

    pub(super) async fn load_world(&self, id: JobId) -> Result<String, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::LoadWorld { id, reply })?;
        receive(answer).await
    }

    pub(super) async fn start_execution(
        &self,
        id: JobId,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
        event: Value,
    ) -> Result<JobRecord, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control(ControlCommand::StartExecution {
            id,
            execution_id,
            trace_id,
            snapshot_digest,
            event,
            reply,
        })?;
        receive(answer).await
    }

    pub(super) async fn queued_jobs(&self) -> Result<Vec<(JobId, String)>, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::Queued { reply })?;
        receive(answer).await
    }

    pub(super) async fn get(&self, id: JobId) -> Result<Option<JobRecord>, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::Get { id, reply })?;
        receive(answer).await
    }

    pub(super) fn get_blocking(&self, id: JobId) -> Result<Option<JobRecord>, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::Get { id, reply })?;
        receive_blocking(answer)
    }

    pub(super) async fn events_after(
        &self,
        id: JobId,
        after: u64,
        limit: EventPageLimit,
    ) -> Result<EventPage, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::EventsAfter {
            id,
            after,
            limit,
            reply,
        })?;
        receive(answer).await
    }

    pub(super) fn event_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.events)
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
            outputs: None,
            receipt: None,
            reply,
        })?;
        receive(answer).await
    }

    pub(super) async fn settle_with_result(
        &self,
        id: JobId,
        status: JobStatus,
        event: Value,
        outputs: Option<BTreeMap<String, Value>>,
        receipt: Option<JobReceipt>,
    ) -> Result<JobRecord, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control(ControlCommand::Transition {
            id,
            status,
            event,
            outputs,
            receipt: receipt.map(Box::new),
            reply,
        })?;
        receive(answer).await
    }

    pub(super) fn settle_with_result_blocking(
        &self,
        id: JobId,
        status: JobStatus,
        event: Value,
        outputs: Option<BTreeMap<String, Value>>,
        receipt: Option<JobReceipt>,
    ) -> Result<JobRecord, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control(ControlCommand::Transition {
            id,
            status,
            event,
            outputs,
            receipt: receipt.map(Box::new),
            reply,
        })?;
        receive_blocking(answer)
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

fn receive_blocking<T>(
    answer: oneshot::Receiver<Result<T, JobStoreError>>,
) -> Result<T, ServerError> {
    answer
        .blocking_recv()
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
        let events = Arc::new(Notify::new());
        let thread_events = Arc::clone(&events);
        let thread = std::thread::Builder::new()
            .name("nika-serve-store".to_owned())
            .spawn(move || {
                serve_store(
                    &request_receiver,
                    &control_receiver,
                    &store,
                    &incarnation,
                    &thread_events,
                );
            })
            .map_err(|_| ServerError::BlockingTask)?;
        Ok(Self {
            handle: StoreHandle {
                requests,
                controls,
                events,
            },
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
    events: &Notify,
) {
    loop {
        if let Ok(command) = controls.try_recv() {
            if !serve_control(command, store, incarnation, events) {
                break;
            }
            continue;
        }
        let command = match requests.recv_timeout(std::time::Duration::from_millis(5)) {
            Ok(command) => command,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => match controls.recv() {
                Ok(command) => {
                    if !serve_control(command, store, incarnation, events) {
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
            workflow,
            world,
            reply,
        } => {
            let result = store.create_or_replay_captured(key, digest, max_jobs, workflow, &world);
            let _result = reply.send(result);
        }
        RequestCommand::PrepareScheduled {
            key,
            digest,
            max_jobs,
            workflow,
            world,
            origin,
            execution_id,
            trace_id,
            snapshot_digest,
            event,
            reply,
        } => {
            let result = store.prepare_scheduled_captured(
                key,
                digest,
                max_jobs,
                workflow,
                &world,
                *origin,
                execution_id,
                trace_id,
                snapshot_digest,
                &event,
            );
            let _result = reply.send(result);
        }
        RequestCommand::Get { id, reply } => {
            let _result = reply.send(store.get(&id));
        }
        RequestCommand::LoadWorld { id, reply } => {
            let _result = reply.send(store.load_world(&id));
        }
        RequestCommand::Queued { reply } => {
            let _result = reply.send(store.queued_jobs());
        }
        RequestCommand::EventsAfter {
            id,
            after,
            limit,
            reply,
        } => {
            let result =
                store
                    .event_page(&id, after, limit)
                    .map(|(events, record, terminal_sequence)| EventPage {
                        events,
                        record,
                        terminal_sequence,
                    });
            let _result = reply.send(result);
        }
    }
}

fn notify_persisted<T>(result: &Result<T, JobStoreError>, events: &Notify) {
    if result.is_ok() {
        events.notify_waiters();
    }
}

fn serve_control(
    command: ControlCommand,
    store: &JobStore,
    incarnation: &ServerIncarnation,
    events: &Notify,
) -> bool {
    match command {
        ControlCommand::Transition {
            id,
            status,
            event,
            outputs,
            receipt,
            reply,
        } => {
            let result = if outputs.is_some() || receipt.is_some() {
                store.settle_with_events(
                    &id,
                    status,
                    std::slice::from_ref(&event),
                    outputs,
                    receipt.map(|receipt| *receipt),
                )
            } else {
                store.transition_with_events(&id, status, std::slice::from_ref(&event))
            }
            .map(|mutation| mutation.record().clone());
            notify_persisted(&result, events);
            let _result = reply.send(result);
        }
        ControlCommand::StartExecution {
            id,
            execution_id,
            trace_id,
            snapshot_digest,
            event,
            reply,
        } => {
            let result = store
                .start_execution(
                    &id,
                    execution_id,
                    trace_id,
                    snapshot_digest,
                    std::slice::from_ref(&event),
                )
                .map(|mutation| mutation.record().clone());
            notify_persisted(&result, events);
            let _result = reply.send(result);
        }
        ControlCommand::Interrupt { id, reply } => {
            let payload = serde_json::json!({
                "kind": "execution.interrupted",
                "status": "interrupted"
            });
            let result = store.interrupt_running(&id, incarnation, &payload);
            notify_persisted(&result, events);
            if let Some(reply) = reply {
                let _result = reply.send(result);
            }
        }
        ControlCommand::SettleInterrupted { reply } => {
            let result = store.settle_interrupted_jobs(incarnation);
            notify_persisted(&result, events);
            let _result = reply.send(result);
        }
        ControlCommand::Shutdown { reply } => {
            let _result = reply.send(());
            return false;
        }
    }
    true
}
