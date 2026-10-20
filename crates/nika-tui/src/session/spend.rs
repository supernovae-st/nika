// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What this conversation's preparation spent since it opened, read from the
//! Session's own cost observations: the known subtotal of its preparation
//! scopes now, less what they held when it opened (an earlier session's
//! restored scopes cancel out). Unpriced calls add nothing; an observation
//! without a known subtotal leaves the whole spend unknown, never zero. A
//! display estimate, never billing.

use serde_json::Value;

/// The schema of a preparation scope's observation.
const PREPARATION: &str = "nika/preparation-cost-observation@1";

/// The known subtotal, in nano-USD, of every preparation scope `observations`
/// hold; `None` when one of them knows none.
pub(super) fn prepared_nano(observations: &[Value]) -> Option<i128> {
    (observations.iter())
        .filter(|observation| observation["schema"] == PREPARATION)
        .try_fold(0_i128, |sum, observation| {
            let known = observation["known_subtotal_nano_usd"].as_str()?;
            sum.checked_add(known.parse::<i128>().ok()?)
        })
}

/// `nano` USD as a displayed amount.
#[expect(
    clippy::cast_precision_loss,
    reason = "a displayed estimate, never billing"
)]
pub(crate) fn usd(nano: i128) -> f64 {
    nano as f64 / 1e9
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn only_preparation_scopes_with_known_subtotals_are_summed() {
        let scope = |nano: &str| json!({"schema": PREPARATION, "known_subtotal_nano_usd": nano});
        let other =
            json!({"schema": "nika/inference-cost-observation@2", "known_subtotal_nano_usd": "7"});
        assert_eq!(prepared_nano(&[]), Some(0));
        assert_eq!(
            prepared_nano(&[scope("120000000"), other, scope("790000000")]),
            Some(910_000_000)
        );
        let unknown = json!({"schema": PREPARATION, "known_subtotal_nano_usd": null});
        assert_eq!(prepared_nano(&[scope("1"), unknown]), None, "never zero");
        assert!((usd(910_000_000) - 0.91).abs() < 1e-9);
    }
}
