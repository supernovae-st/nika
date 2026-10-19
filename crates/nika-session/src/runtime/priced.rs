// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A host's typed activity, each returned call with what it settled: the requests the
//! preparation scope active on the reporting thread returned since the last call ended, read
//! from that scope's own journal ([`PreparationCosts::active_settled`]). A call whose returned
//! requests all settled at a known estimate is priced at their sum; one with a request that
//! returned without a price is unpriced; a call that settled nothing, or one reported off the
//! driver's thread, states no cost. The sum of every priced call is the scope's known
//! subtotal: nothing is counted twice or invented. A display estimate, never billing.

use std::sync::{Arc, Mutex};

use nika_providers::authoring::preparation::{PreparationCosts, SettledCalls};

use crate::activity::{Activity, ActivityHook, CallCost, CallState};

/// `hook`, its finished calls priced as their scope settled them.
pub(super) fn calls(hook: ActivityHook) -> ActivityHook {
    let last: Mutex<Option<SettledCalls>> = Mutex::new(None);
    Arc::new(move |activity: &Activity| {
        let ended = (activity.call.as_ref())
            .filter(|call| call.cost.is_none())
            .is_some_and(|call| matches!(call.state, CallState::Finished | CallState::Cancelled));
        let Some(now) = ended.then(PreparationCosts::active_settled).flatten() else {
            return hook(activity);
        };
        let before = match last.lock() {
            Ok(mut last) => last.replace(now.clone()),
            Err(poisoned) => poisoned.into_inner().replace(now.clone()),
        };
        let finished =
            (activity.call.as_ref()).is_some_and(|call| call.state == CallState::Finished);
        match settled_between(before.as_ref(), &now).filter(|_| finished) {
            Some(cost) => {
                let mut priced = activity.clone();
                priced.call = priced.call.map(|mark| mark.costing(cost));
                hook(&priced);
            }
            None => hook(activity),
        }
    })
}

/// What settled between two readings of one scope: unpriced when a request returned without a
/// price, else priced at what the known estimates added; nothing when nothing settled. A reading
/// of another scope (or the first one) counts from that scope's start.
fn settled_between(before: Option<&SettledCalls>, now: &SettledCalls) -> Option<CallCost> {
    let (known, unpriced) = match before.filter(|before| before.scope == now.scope) {
        Some(before) => (
            now.known_nano_usd.saturating_sub(before.known_nano_usd),
            now.unpriced.saturating_sub(before.unpriced),
        ),
        None => (now.known_nano_usd, now.unpriced),
    };
    if unpriced > 0 {
        Some(CallCost::Unpriced)
    } else if known > 0 {
        Some(CallCost::Priced(known))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(scope: &str, known_nano_usd: i128, unpriced: usize) -> SettledCalls {
        let mut settled = SettledCalls::default();
        settled.scope = scope.to_owned();
        settled.known_nano_usd = known_nano_usd;
        settled.unpriced = unpriced;
        settled
    }

    /// Between two readings of one scope a call is priced at what its known estimates added,
    /// unpriced when a request returned without one, and states nothing when nothing settled;
    /// a reading of another scope counts from that scope's start.
    #[test]
    fn a_call_is_priced_by_what_settled_since_the_last_one() {
        let first = reading("s1", 120_000_000, 0);
        assert_eq!(
            settled_between(None, &first),
            Some(CallCost::Priced(120_000_000))
        );
        let second = reading("s1", 310_000_000, 0);
        assert_eq!(
            settled_between(Some(&first), &second),
            Some(CallCost::Priced(190_000_000))
        );
        let unpriced = reading("s1", 400_000_000, 1);
        assert_eq!(
            settled_between(Some(&second), &unpriced),
            Some(CallCost::Unpriced)
        );
        assert_eq!(settled_between(Some(&unpriced), &unpriced), None);
        let other = reading("s2", 5_000_000, 0);
        assert_eq!(
            settled_between(Some(&unpriced), &other),
            Some(CallCost::Priced(5_000_000))
        );
    }
}
