// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--var KEY=VALUE` input seam, in `run`'s voice: the parse and the
//! origin law live in `nika_cli_host::var_inputs` (one door for `run` and
//! the golden test's cases); this file keeps the env-refusal wrap.

pub(crate) use nika_cli_host::var_inputs::{ValidatedInputs, parse_var_overrides};

use nika_schema::raw::RawWorkflow;

use super::epilogue;

/// [`parse_var_overrides`] + the admission preflight (#603 · the runtime's
/// ONE constructor) — both ENV-class refusals (stderr + the envelope).
pub(super) fn validated_var_overrides(
    vars: &[String],
    wf: &RawWorkflow,
    output_json: bool,
) -> Result<ValidatedInputs, u8> {
    let validated = parse_var_overrides(vars, wf)
        .map_err(|message| epilogue::env_refusal(&message, output_json))?;
    if let Some(err) = nika_runtime::required_inputs_refusal(wf, &validated.values) {
        return Err(epilogue::env_refusal(&err.to_string(), output_json));
    }
    Ok(validated)
}
