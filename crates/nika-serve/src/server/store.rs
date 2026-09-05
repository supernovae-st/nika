// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread::JoinHandle;

#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

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

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShutdownPhase {
    SchedulerJoin = 1,
    StoreShutdown = 2,
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct ShutdownTestProbe {
    gate_observation: AtomicBool,
    observation_blocked: AtomicBool,
    observation_release: std::sync::Mutex<bool>,
    observation_gate: std::sync::Condvar,
    observation_notify: Notify,
    shutdown_loop_observed: AtomicBool,
    shutdown_loop_notify: Notify,
    terminal_settled: AtomicBool,
    terminal_settled_notify: Notify,
    first_phase: AtomicU8,
    phase_notify: Notify,
}

#[cfg(test)]
impl ShutdownTestProbe {
    pub(super) fn gate_observation(&self) {
        self.gate_observation.store(true, Ordering::SeqCst);
    }

    fn hold_observation(&self) {
        if !self.gate_observation.load(Ordering::SeqCst) {
            return;
        }
        self.observation_blocked.store(true, Ordering::SeqCst);
        self.observation_notify.notify_waiters();
        let mut released = self
            .observation_release
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*released {
            released = self
                .observation_gate
                .wait(released)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(super) async fn wait_observation_blocked(&self) {
        loop {
            let notified = self.observation_notify.notified();
            if self.observation_blocked.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }

    pub(super) fn release_observation(&self) {
        let mut released = self
            .observation_release
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *released = true;
        self.observation_gate.notify_all();
    }

    pub(super) fn mark_shutdown_loop_observed(&self) {
        self.shutdown_loop_observed.store(true, Ordering::SeqCst);
        self.shutdown_loop_notify.notify_waiters();
    }

    pub(super) async fn wait_shutdown_loop_observed(&self) {
        loop {
            let notified = self.shutdown_loop_notify.notified();
            if self.shutdown_loop_observed.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }

    fn mark_terminal_settled(&self) {
        self.terminal_settled.store(true, Ordering::SeqCst);
        self.terminal_settled_notify.notify_waiters();
    }

    pub(super) async fn wait_terminal_settled(&self) {
        loop {
            let notified = self.terminal_settled_notify.notified();
            if self.terminal_settled.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }

    pub(super) fn mark_phase(&self, phase: ShutdownPhase) {
        if self
            .first_phase
            .compare_exchange(0, phase as u8, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.phase_notify.notify_waiters();
        }
    }

    pub(super) async fn wait_first_phase(&self) -> ShutdownPhase {
        loop {
            let notified = self.phase_notify.notified();
            match self.first_phase.load(Ordering::SeqCst) {
                1 => return ShutdownPhase::SchedulerJoin,
                2 => return ShutdownPhase::StoreShutdown,
                _ => notified.await,
            }
        }
    }
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
    Replay {
        key: IdempotencyKey,
        digest: RequestDigest,
        reply: Reply<Option<Admission>>,
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
    ExecutionStatus {
        id: JobId,
        reply: Reply<Option<JobStatus>>,
    },
    RefuseQueued {
        id: JobId,
        event: Value,
        reply: Reply<JobRecord>,
    },
    CancelQueued {
        id: JobId,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
        event: Value,
        receipt: Box<JobReceipt>,
        reply: Reply<JobRecord>,
    },
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
    #[cfg(test)]
    shutdown_probe: Arc<ShutdownTestProbe>,
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

    /// The job already bound to this key, without creating one (ADR-132).
    pub(super) async fn replay(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
    ) -> Result<Option<Admission>, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_request(RequestCommand::Replay { key, digest, reply })?;
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

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn cancel_queued(
        &self,
        id: JobId,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
        event: Value,
        receipt: JobReceipt,
    ) -> Result<JobRecord, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control(ControlCommand::CancelQueued {
            id,
            execution_id,
            trace_id,
            snapshot_digest,
            event,
            receipt: Box::new(receipt),
            reply,
        })?;
        receive(answer).await
    }

    pub(super) async fn start_execution_reliable(
        &self,
        id: JobId,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
        event: Value,
    ) -> Result<JobRecord, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control_blocking(ControlCommand::StartExecution {
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

    /// Lifecycle reads share the reserved control lane with their mutations.
    /// A full HTTP request queue cannot turn a stale replay into a fatal run.
    pub(super) async fn execution_status(
        &self,
        id: JobId,
    ) -> Result<Option<JobStatus>, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control_blocking(ControlCommand::ExecutionStatus { id, reply })?;
        receive(answer).await
    }

    pub(super) fn get_blocking(&self, id: JobId) -> Result<Option<JobRecord>, ServerError> {
        #[cfg(test)]
        self.shutdown_probe.hold_observation();
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

    #[cfg(test)]
    pub(super) fn shutdown_test_probe(&self) -> Arc<ShutdownTestProbe> {
        Arc::clone(&self.shutdown_probe)
    }

    pub(super) async fn refuse_queued(
        &self,
        id: JobId,
        event: Value,
    ) -> Result<JobRecord, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control_blocking(ControlCommand::RefuseQueued { id, event, reply })?;
        receive(answer).await
    }

    pub(super) async fn settle_with_result_reliable(
        &self,
        id: JobId,
        status: JobStatus,
        event: Value,
        outputs: Option<BTreeMap<String, Value>>,
        receipt: Option<JobReceipt>,
    ) -> Result<JobRecord, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control_blocking(ControlCommand::Transition {
            id,
            status,
            event,
            outputs,
            receipt: receipt.map(Box::new),
            reply,
        })?;
        let result = receive(answer).await;
        #[cfg(test)]
        if result.is_ok() {
            self.shutdown_probe.mark_terminal_settled();
        }
        result
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
        self.send_control_blocking(ControlCommand::Transition {
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
        self.send_control_blocking(ControlCommand::Interrupt {
            id,
            reply: Some(reply),
        })?;
        receive(answer).await
    }

    pub(super) fn interrupt_detached(&self, id: JobId) {
        let _result = self.send_control_blocking(ControlCommand::Interrupt { id, reply: None });
    }

    pub(super) async fn settle_interrupted(&self) -> Result<usize, ServerError> {
        let (reply, answer) = oneshot::channel();
        self.send_control_blocking(ControlCommand::SettleInterrupted { reply })?;
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

    fn send_control_blocking(&self, command: ControlCommand) -> Result<(), ServerError> {
        self.controls
            .send(command)
            .map_err(|_| ServerError::BlockingTask)
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
        #[cfg(test)]
        let shutdown_probe = Arc::new(ShutdownTestProbe::default());
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
                #[cfg(test)]
                shutdown_probe,
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
            .controls
            .send(ControlCommand::Shutdown { reply })
            .map_err(|_| ServerError::BlockingTask)?;
        answer.await.map_err(|_| ServerError::BlockingTask)?;
        let thread = self.thread.take().ok_or(ServerError::BlockingTask)?;
        let result = tokio::task::spawn_blocking(move || thread.join())
            .await
            .map_err(|_| ServerError::BlockingTask)?
            .map_err(|_| ServerError::BlockingTask);
        #[cfg(test)]
        self.handle
            .shutdown_probe
            .mark_phase(ShutdownPhase::StoreShutdown);
        result
    }
}

impl Drop for StoreActor {
    fn drop(&mut self) {
        let (reply, _answer) = oneshot::channel();
        let _result = self
            .handle
            .controls
            .send(ControlCommand::Shutdown { reply });
        if let Some(thread) = self.thread.take() {
            let _result = thread.join();
        }
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
        RequestCommand::Replay { key, digest, reply } => {
            let result = store.replay(&key, &digest);
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
        ControlCommand::ExecutionStatus { id, reply } => {
            let result = store
                .get(&id)
                .map(|record| record.map(|record| record.status()));
            let _result = reply.send(result);
        }
        ControlCommand::RefuseQueued { id, event, reply } => {
            let result = store
                .refuse_queued(&id, &event)
                .map(|mutation| mutation.record().clone());
            notify_persisted(&result, events);
            let _result = reply.send(result);
        }
        ControlCommand::CancelQueued {
            id,
            execution_id,
            trace_id,
            snapshot_digest,
            event,
            receipt,
            reply,
        } => {
            let result = store
                .cancel_queued(
                    &id,
                    execution_id,
                    trace_id,
                    snapshot_digest,
                    &event,
                    *receipt,
                )
                .map(|mutation| mutation.record().clone());
            notify_persisted(&result, events);
            let _result = reply.send(result);
        }
        ControlCommand::Transition {
            id,
            status,
            event,
            outputs,
            receipt,
            reply,
        } => {
            let result = transition_job(store, &id, status, &event, outputs, receipt);
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
                .start_queued_execution(
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

fn transition_job(
    store: &JobStore,
    id: &JobId,
    status: JobStatus,
    event: &Value,
    outputs: Option<BTreeMap<String, Value>>,
    receipt: Option<Box<JobReceipt>>,
) -> Result<JobRecord, JobStoreError> {
    let payloads = std::slice::from_ref(event);
    let mutation = if outputs.is_some() || receipt.is_some() {
        store.settle_with_events(
            id,
            status,
            payloads,
            outputs,
            receipt.map(|receipt| *receipt),
        )
    } else {
        store.transition_with_events(id, status, payloads)
    }?;
    Ok(mutation.record().clone())
}

#[cfg(test)]
mod tests {
    use std::future::{Future, poll_fn};
    use std::task::Poll;

    use super::*;

    #[tokio::test]
    async fn execution_status_does_not_compete_with_a_full_request_queue() {
        let (requests, request_receiver) = std::sync::mpsc::sync_channel(1);
        let (controls, control_receiver) = std::sync::mpsc::sync_channel(1);
        let handle = StoreHandle {
            requests,
            controls,
            events: Arc::new(Notify::new()),
            shutdown_probe: Arc::new(ShutdownTestProbe::default()),
        };
        let (reply, _answer) = oneshot::channel();
        assert!(
            handle
                .send_request(RequestCommand::Queued { reply })
                .is_ok()
        );
        let id = JobId::random();
        let status = handle.execution_status(id.clone());
        tokio::pin!(status);
        assert!(
            poll_fn(|cx| Poll::Ready(status.as_mut().poll(cx)))
                .await
                .is_pending(),
            "lifecycle observation must await the store, not fail at HTTP ingress"
        );
        let (observed, reply) = match control_receiver.try_recv().expect("reserved control lane") {
            ControlCommand::ExecutionStatus { id, reply } => Some((id, reply)),
            _ => None,
        }
        .expect("an execution status read");
        assert_eq!(observed, id);
        reply
            .send(Ok(Some(JobStatus::Succeeded)))
            .expect("observer");
        assert_eq!(status.await.expect("status"), Some(JobStatus::Succeeded));
        assert!(matches!(
            request_receiver.try_recv(),
            Ok(RequestCommand::Queued { .. })
        ));
        assert!(request_receiver.try_recv().is_err());
    }
}
