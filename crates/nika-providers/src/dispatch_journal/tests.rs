// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The journal's states, driven by hand: one poll, then the drop a
//! `fail_fast` sibling or a `timeout:` gets.

use std::cell::RefCell;
use std::future::Future;
use std::task::{Context, Waker};

use nika_kernel::ai::provider::TokenUsage;
use nika_types::cost::InferenceCall;

use super::{DispatchJournal, open, sent};

fn requested(endpoint: &str) -> InferenceCall {
    let mut call = InferenceCall::new();
    call.requested_endpoint = Some(endpoint.to_owned());
    call
}

fn answered(endpoint: &str) -> InferenceCall {
    let mut call = requested(endpoint);
    call.usage = Some(TokenUsage::new(10, 5));
    call.usage_complete = true;
    call
}

/// Poll once (the dispatch must be in flight), then drop it.
fn poll_then_drop<F: Future>(dispatch: F) {
    let mut dispatch = std::pin::pin!(dispatch);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(dispatch.as_mut().poll(&mut cx).is_pending(), "in flight");
}

/// The lost requests each drop reported.
fn lost_of<F: Future>(dispatch: impl FnOnce() -> F) -> Vec<Vec<InferenceCall>> {
    let seen = RefCell::new(Vec::new());
    poll_then_drop(DispatchJournal::observe(dispatch(), |lost| {
        seen.borrow_mut().push(lost);
    }));
    seen.into_inner()
}

#[test]
fn nothing_is_recorded_outside_a_scope() {
    assert!(open().is_none(), "no journal, no entry");
    sent(Some(&requested("https://a.example/v1")));
}

#[test]
fn a_returned_dispatch_never_reports_its_requests() {
    let dispatch = DispatchJournal::observe(
        async {
            let entry = open().expect("scoped");
            sent(Some(&requested("https://a.example/v1")));
            entry.settle(Some(&answered("https://a.example/v1")), true);
            7
        },
        |lost| panic!("a returned dispatch carries its own evidence: {lost:?}"),
    );
    let mut dispatch = std::pin::pin!(dispatch);
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(dispatch.as_mut().poll(&mut cx), std::task::Poll::Ready(7));
}

#[test]
fn a_dropped_dispatch_reports_its_sent_request_once_and_bare() {
    let lost = lost_of(|| async {
        let _entry = open().expect("scoped");
        sent(Some(&requested("https://a.example/v1")));
        std::future::pending::<()>().await;
    });
    assert_eq!(lost.len(), 1, "one report");
    assert_eq!(lost[0].len(), 1, "one request");
    let call = &lost[0][0];
    assert_eq!(
        call.requested_endpoint.as_deref(),
        Some("https://a.example/v1")
    );
    assert!(
        call.usage.is_none() && call.known_estimate().is_none(),
        "charge unknown"
    );
}

#[test]
fn a_pre_send_request_is_never_reported_as_sent() {
    let lost = lost_of(|| async {
        let _entry = open().expect("scoped");
        std::future::pending::<()>().await;
    });
    assert!(lost.is_empty(), "nothing crossed the transport: {lost:?}");
}

#[test]
fn returned_evidence_and_the_unanswered_request_are_both_reported() {
    let lost = lost_of(|| async {
        let first = open().expect("scoped");
        sent(Some(&requested("https://a.example/v1")));
        first.settle(Some(&answered("https://a.example/v1")), true);
        let _second = open().expect("scoped");
        sent(Some(&requested("https://a.example/v1")));
        std::future::pending::<()>().await;
    });
    assert_eq!(lost.len(), 1);
    assert_eq!(lost[0].len(), 2, "the answered retry and the one in flight");
    assert!(lost[0][0].usage.is_some(), "evidence kept");
    assert!(lost[0][1].usage.is_none(), "unanswered stays bare");
}

#[test]
fn a_withdrawn_request_is_not_reported() {
    let lost = lost_of(|| async {
        let refused = open().expect("scoped");
        refused.settle(None, false);
        let _entry = open().expect("scoped");
        sent(Some(&requested("https://b.example/v1")));
        std::future::pending::<()>().await;
    });
    assert_eq!(lost.len(), 1);
    assert_eq!(lost[0].len(), 1, "only the request that crossed");
    assert_eq!(
        lost[0][0].requested_endpoint.as_deref(),
        Some("https://b.example/v1")
    );
}

#[test]
fn a_sent_request_without_evidence_stays_sent() {
    let lost = lost_of(|| async {
        let entry = open().expect("scoped");
        sent(Some(&requested("https://a.example/v1")));
        entry.settle(None, false);
        std::future::pending::<()>().await;
    });
    assert_eq!(lost.len(), 1);
    assert_eq!(lost[0].len(), 1, "sent, never withdrawn");
}

#[test]
fn a_nested_scope_keeps_its_own_requests() {
    let inner_seen = RefCell::new(Vec::new());
    let outer_seen = RefCell::new(Vec::new());
    poll_then_drop(DispatchJournal::observe(
        async {
            DispatchJournal::observe(
                async {
                    let _entry = open().expect("the inner scope");
                    sent(Some(&requested("https://child.example/v1")));
                    std::future::pending::<()>().await;
                },
                |lost| inner_seen.borrow_mut().push(lost),
            )
            .await;
        },
        |lost| outer_seen.borrow_mut().push(lost),
    ));
    assert_eq!(
        inner_seen.borrow().len(),
        1,
        "the child's request is the child's"
    );
    assert!(
        outer_seen.borrow().is_empty(),
        "the parent sent nothing itself"
    );
}
