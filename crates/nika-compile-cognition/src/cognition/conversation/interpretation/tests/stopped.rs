// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Stop at the new audit boundary drops preparation before any program judgment or trial.
use super::*;
use nika_providers::authoring::preparation::PreparationCosts;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Notify;

struct WaitingAudit {
    requests: Mutex<Vec<InferRequest>>,
    completed: AtomicUsize,
    audit_started: Notify,
    audit_dropped: AtomicBool,
}

struct InFlight<'a>(&'a AtomicBool);
impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl ProviderInferDyn for WaitingAudit {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let call = {
            let mut requests = self.requests.lock().unwrap();
            requests.push(request);
            requests.len()
        };
        if call == 1 {
            self.completed.fetch_add(1, Ordering::SeqCst);
            return Ok(InferResponse::new(
                vec![ContentBlock::Text {
                    text: reading().to_string(),
                }],
                TokenUsage::new(7, 3),
                StopReason::EndTurn,
            ));
        }
        assert_eq!(call, 2, "nothing may continue beyond the pending audit");
        let _in_flight = InFlight(&self.audit_dropped);
        self.audit_started.notify_one();
        std::future::pending().await
    }
}

#[tokio::test]
async fn stop_during_interpretation_audit_drops_it_before_trial_or_decision() {
    let provider = WaitingAudit {
        requests: Mutex::new(Vec::new()),
        completed: AtomicUsize::new(0),
        audit_started: Notify::new(),
        audit_dropped: AtomicBool::new(false),
    };
    let seat = Seat::new(false);
    let room = Room(Mutex::new(0));
    let request = request();
    let mut costs = PreparationCosts::default();
    let cancel = costs.begin_turn();
    let _scope = costs.enter();
    let verification = Box::pin(verify_document(
        &request,
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        Some(&room),
    ));
    let stop = async {
        provider.audit_started.notified().await;
        assert_eq!(provider.completed.load(Ordering::SeqCst), 1);
        assert_eq!(*room.0.lock().unwrap(), 0);
        assert!(seat.asked.lock().unwrap().is_empty());
        cancel.cancel();
    };
    let (outcome, ()) = Box::pin(tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(PreparationCosts::while_active(verification), stop)
    }))
    .await
    .expect("audit must start and Stop must return");
    assert!(outcome.is_none(), "Stop must not return a program verdict");
    assert!(provider.audit_dropped.load(Ordering::SeqCst));
    assert_eq!(provider.completed.load(Ordering::SeqCst), 1);
    assert_eq!(provider.requests.lock().unwrap().len(), 2);
    assert_eq!(*room.0.lock().unwrap(), 0);
    assert!(seat.asked.lock().unwrap().is_empty());
    assert!(PreparationCosts::interrupted());
    tokio::task::yield_now().await;
    assert_eq!(
        provider.requests.lock().unwrap().len(),
        2,
        "no detached continuation"
    );
}
