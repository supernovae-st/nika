// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Run shares monetary parsing with Prepare, but an unbound numeric Run
//! condition must be clarified rather than silently granting execution.

pub(super) fn ceiling_in(input: &str) -> Result<Option<f64>, &'static str> {
    let parsed = super::money_parse::parse(input)?;
    if parsed.unbound_number {
        return Err(
            "that number has no explicit monetary meaning — name a ceiling in USD, or describe the timing/change first; the run was not started",
        );
    }
    Ok(parsed.amount)
}
