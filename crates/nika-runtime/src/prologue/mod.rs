// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `workflow_started` prologue's field helpers + test module (A5
//! evidence fields — the tests split from the crate-root `tests.rs`,
//! which rides the 1,500-LOC ceiling; the field helpers descend here
//! from `lib.rs` under the same ceiling, cohesion intact: a prologue
//! field belongs to the prologue module).

use nika_types::resource::Value as FieldValue;

use crate::s;

#[cfg(test)]
mod tests;

/// The F-P18 cost fields (NEP-0017 · la table de prix DANS le pin) —
/// additive, the `permits_json` posture: older readers ignore unknown
/// fields, newer readers find them absent where no claim exists.
///
/// - `pricing` — the versioned pricing table that gives sense to the
///   run's ρ (usd), journaled as ONE compact JSON text
///   (`{"as_of","schema","sha256_16"}` — the `outcome` idiom: a nested
///   object rides as a JSON string, the wire's `Value` having no object
///   variant). This IS the law's pin: a future cost-replay reads the
///   run's costs against THIS table — « un coût rejoué en 2031 se lit
///   contre la table 2026 pinnée ». Compile-time statics only
///   ([`nika_catalog::pricing_snapshot`] + [`nika_catalog::PRICING_SCHEMA`]
///   — the `spec_pin` posture: no clock, no I/O, determinism intact);
/// - `budget` — the resolved operator budget (`--max-cost-usd`), ONE
///   compact JSON text (`{"max_cost_usd":dollars}` — dollars ride as a
///   JSON number, the `total_cost_usd` float convention). ABSENT when
///   the run carries no budget (absent is honest — an unbounded run
///   journals no claim, never a fake zero/unbounded). A non-finite
///   budget cannot reach here (the CLI refuses NaN/inf at the flag);
///   the guard stays so the journal can never carry `null` where a
///   number was claimed.
pub(crate) fn cost_pin_fields(max_cost_usd: Option<f64>) -> Vec<(&'static str, FieldValue)> {
    let snapshot = nika_catalog::pricing_snapshot();
    let pin = serde_json::json!({
        "schema": nika_catalog::PRICING_SCHEMA,
        "as_of": snapshot.as_of,
        "sha256_16": snapshot.source_sha256_16,
    });
    let mut fields = vec![("pricing", s(&pin.to_string()))];
    if let Some(budget) = max_cost_usd.filter(|b| b.is_finite()) {
        let resolved = serde_json::json!({ "max_cost_usd": budget });
        fields.push(("budget", s(&resolved.to_string())));
    }
    fields
}
