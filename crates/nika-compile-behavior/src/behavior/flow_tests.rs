// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What each task of a rehearsal received and returned rides with the run as the host counted
//! it: a fact the judge reads, never a verdict of its own. A task is emptied only when it
//! returned nothing from items it did receive; an uncounted side states nothing.

use super::{Run, RunEnd, StepFlow, Usage};

#[test]
fn a_task_that_returned_nothing_from_what_it_received_is_emptied() {
    assert!(StepFlow::new("filter_today", Some(12), Some(0)).emptied());
    for (received, returned) in [
        (Some(0), Some(0)),
        (Some(12), Some(3)),
        (None, Some(0)),
        (Some(12), None),
    ] {
        let step = StepFlow::new("filter_today", received, returned);
        assert!(!step.emptied(), "{step:?}");
    }
}

#[test]
fn the_flow_rides_with_its_run_in_the_run_s_order() {
    let flow = vec![
        StepFlow::new("fetch", None, Some(12)),
        StepFlow::new("filter_today", Some(12), Some(0)),
    ];
    let run = Run::new("observed", RunEnd::Completed, Usage::default());
    assert!(run.flow.is_empty());
    assert_eq!(run.with_flow(flow.clone()).flow, flow);
}
