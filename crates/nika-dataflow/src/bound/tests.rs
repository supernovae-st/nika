// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::ValueBound;
use crate::expr::{render, render_json};
use crate::{DataflowError, Scope};

fn inputs(text: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([("text".to_owned(), json!(text))])
}

/// Two islands joining one input into a string over the bound are refused where the string is
/// built, and the crossing is kept; under the bound, or with no bound, the string is built.
#[test]
fn a_string_built_over_the_bound_is_refused_where_it_is_built() {
    let (records, consts, secrets) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let inputs = inputs(&"x".repeat(600));
    let base = Scope::workflow_with_value_authorities(&records, &inputs, &consts, &secrets);
    let joined = "${{ inputs.text }}${{ inputs.text }}";
    let bound = ValueBound::new(1_000);
    let refused = render(joined, &base.with_value_bound(Some(&bound)));
    assert!(matches!(
        refused,
        Err(DataflowError::ValueBound { limit: 1_000, .. })
    ));
    assert!(bound.crossed().is_some_and(|bytes| bytes > 1_000));
    let roomy = ValueBound::new(2_000);
    let built = render(joined, &base.with_value_bound(Some(&roomy))).ok();
    assert_eq!(built.map(|s| s.len()), Some(1_200));
    assert_eq!(roomy.crossed(), None);
    assert_eq!(render(joined, &base).ok().map(|s| s.len()), Some(1_200));
}

/// An array or an object holding templates is counted whole, as built: over the bound it is
/// refused; a run with no bound builds it.
#[test]
fn a_value_built_over_the_bound_is_refused() {
    let (records, consts, secrets) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let inputs = inputs(&"y".repeat(400));
    let base = Scope::workflow_with_value_authorities(&records, &inputs, &consts, &secrets);
    let held = json!({"rows": ["${{ inputs.text }}", "${{ inputs.text }}", "${{ inputs.text }}"]});
    let bound = ValueBound::new(1_000);
    let refused = render_json(&held, &base.with_value_bound(Some(&bound)));
    assert!(matches!(refused, Err(DataflowError::ValueBound { .. })));
    assert!(render_json(&held, &base).is_ok());
}

/// A value over the bound says how to clear it: shrink the value, or run outside the room.
#[test]
fn a_value_over_the_bound_says_how_to_clear_it() {
    let refused = DataflowError::ValueBound {
        bytes: 1_200,
        limit: 1_000,
    };
    let said = refused.to_string();
    assert!(said.contains("over the room's bound of 1000"), "{said}");
    assert!(said.contains("shrink the value this task builds"), "{said}");
}
