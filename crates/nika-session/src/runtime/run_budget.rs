// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Run shares monetary parsing with Prepare, but an unbound numeric Run
//! condition must be clarified rather than silently granting execution.

/// Why a Run line's number was not read as money, in English then in French: the person reads
/// it in their language (live gauntlet wave 8, r10: this English line in a French conversation).
pub(super) const UNBOUND: (&str, &str) = (
    "that number has no explicit monetary meaning — name a ceiling in USD, or describe the timing/change first; the run was not started",
    "ce nombre ne dit pas un montant — donnez un plafond en dollars US, ou dites d'abord le moment ou le changement voulus ; le run n'a pas été lancé",
);

pub(super) fn ceiling_in(input: &str) -> Result<Option<f64>, &'static str> {
    let parsed = super::money_parse::parse(input)?;
    if parsed.unbound_number {
        return Err(UNBOUND.0);
    }
    Ok(parsed.amount)
}
