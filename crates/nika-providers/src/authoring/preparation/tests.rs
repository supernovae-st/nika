// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::*;
use nika_types::cost::InferenceCall;
use std::{
    pin::pin,
    task::{Context, Poll, Waker},
};
fn poll<F: Future>(future: std::pin::Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
fn call() -> InferenceCall {
    let mut call = InferenceCall::new();
    call.requested_endpoint = Some("https://example.test/private?q=secret".into());
    call.request_id = Some("https://example.test/private?q=secret".into());
    call
}
#[test]
fn continuous_observation_has_no_old_seven_ten_or_sixty_four_request_gate() {
    let costs = PreparationCosts::default();
    let _scope = costs.enter();
    let mut future = pin!(PreparationCosts::capture(async {
        for _ in 0..70 {
            let entry = crate::dispatch_journal::open().expect("scoped");
            crate::dispatch_journal::sent(Some(&call()));
            entry.settle(Some(&call()), true);
        }
    }));
    assert!(poll(future.as_mut()).is_ready());
    let observation = costs.observation().expect("observed");
    assert_eq!(observation["calls"].as_array().expect("calls").len(), 70);
    assert_eq!(observation["unknown_calls"], 70);
    assert_eq!(observation["state"], "Closed");
    assert_eq!(
        costs.uncertain_requests(),
        70,
        "unknown charges remain counted"
    );
    assert_eq!(
        PreparationCosts::uncertain_exposure(
            None,
            &crate::InferenceAdmission::unbudgeted(),
            &[],
            &[],
            Some(&costs),
        ),
        0,
        "returned unpriced responses are not incomplete operations"
    );
    assert_eq!(observation["authority"], "observation_only");
    assert!(!observation.to_string().contains("private?q=secret"));
    assert!(PreparationCosts::summary(&[observation]).contains("70 unpriced requests"));
}
#[test]
fn cancellation_keeps_sent_unknown_and_restores_the_parent_scope() {
    let costs = PreparationCosts::default();
    assert!(!PreparationCosts::active());
    {
        let _scope = costs.enter();
        let other = PreparationCosts::default();
        {
            let _inner = other.enter();
            assert!(PreparationCosts::active());
        }
        let mut future = pin!(PreparationCosts::capture(async {
            let _entry = crate::dispatch_journal::open().expect("scoped");
            crate::dispatch_journal::sent(Some(&call()));
            std::future::pending::<()>().await;
        }));
        assert!(poll(future.as_mut()).is_pending());
    }
    assert!(!PreparationCosts::active());
    let observation = costs.observation().expect("sent despite cancellation");
    assert_eq!(observation["unknown_calls"], 1);
    assert_eq!(observation["state"], "Uncertain");
    assert!(observation["calls"][0]["estimated_usd"].is_null());
}
#[test]
fn pre_send_withdrawal_never_becomes_a_paid_call() {
    let costs = PreparationCosts::default();
    let _scope = costs.enter();
    let mut future = pin!(PreparationCosts::capture(async {
        crate::dispatch_journal::open()
            .expect("scoped")
            .settle(None, false);
    }));
    assert!(poll(future.as_mut()).is_ready());
    assert!(costs.observation().is_none());
}

#[test]
fn each_cancelled_request_adds_uncertainty_in_the_same_preparation_scope() {
    let costs = PreparationCosts::default();
    let _scope = costs.enter();
    for expected in 1..=2 {
        {
            let mut future = pin!(PreparationCosts::capture(async {
                let _entry = crate::dispatch_journal::open().expect("scoped");
                crate::dispatch_journal::sent(Some(&call()));
                std::future::pending::<()>().await;
            }));
            assert!(poll(future.as_mut()).is_pending());
        }
        assert_eq!(costs.uncertain_requests(), expected);
        assert_eq!(
            PreparationCosts::uncertain_exposure(
                None,
                &crate::InferenceAdmission::unbudgeted(),
                &[],
                &[],
                Some(&costs),
            ),
            expected,
            "each unanswered request still contributes a new durable uncertainty"
        );
        assert_eq!(costs.observation().unwrap()["state"], "Uncertain");
    }
}
#[test]
fn retained_numeric_holds_and_both_jev_versions_stay_visible_without_a_gate() {
    let account =
        crate::InferenceAdmission::new(nika_types::cost::Cost::new(100_000_000_000)).unwrap();
    let mut attempt = account
        .reserve(
            "deepseek",
            "deepseek-v4-pro",
            "https://api.deepseek.com/chat/completions",
            32_768,
        )
        .unwrap();
    attempt.sent().unwrap();
    drop(attempt);
    let retained = account.snapshot().unwrap();
    let before = retained.durable_observation();
    let old = json!({"schema":"nika/session-decision-seat@1", "seat":"Jev", "attempts":[{"sent":true,"outcome":"chosen"}]});
    let new = json!({"schema":"nika/session-decision-seat@2", "seat":"Jev", "attempts":[{"sent":true,"outcome":"in_flight"}]});
    let line = PreparationCosts::summary(&[before.clone(), old, new]);
    assert!(
        line.contains(&format!("retained reservation {}", retained.held_unknown)),
        "{line}"
    );
    assert!(
        line.contains(&format!("old allowance {}", retained.limit)),
        "{line}"
    );
    assert!(
        line.contains("2 call(s) sent")
            && line.contains("1 answered")
            && line.contains("1 without a response"),
        "{line}"
    );
    assert!(
        !line.contains("blocked") && !line.contains("fresh review required"),
        "{line}"
    );
    assert_eq!(account.snapshot().unwrap().durable_observation(), before);
}

mod stop;

#[test]
fn a_returned_failure_keeps_uncertainty_even_when_its_call_evidence_is_recorded() {
    let costs = PreparationCosts::default();
    let _scope = costs.enter();
    let mut future = pin!(PreparationCosts::capture(async {
        let entry = crate::dispatch_journal::open().expect("scoped");
        crate::dispatch_journal::sent(Some(&call()));
        entry.settle(Some(&call()), false);
    }));
    assert!(poll(future.as_mut()).is_ready());
    let observation = costs.observation().expect("observed");
    assert_eq!(observation["unknown_calls"], 1);
    assert_eq!(costs.uncertain_requests(), 1);
    assert_eq!(
        PreparationCosts::uncertain_exposure(
            None,
            &crate::InferenceAdmission::unbudgeted(),
            &[],
            &[],
            Some(&costs),
        ),
        1,
        "a failed response does not establish a usable result or zero billing"
    );
}
