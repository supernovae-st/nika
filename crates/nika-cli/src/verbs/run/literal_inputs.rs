// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Stdin ownership and machine rendering for the literal input channel.
use nika_cli_host::literal_inputs::Refusal;
pub(super) use nika_cli_host::literal_inputs::validate;
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn refuse(error: &Refusal, machine: bool) -> u8 {
    super::epilogue::typed_env_refusal(error.code(), error.message(), machine)
}

pub(super) fn capture(
    channel: Option<&str>,
    vars: &[String],
    file: &str,
    machine: bool,
) -> Result<Option<BTreeMap<String, Value>>, u8> {
    let Some(channel) = channel else {
        return Ok(None);
    };
    if channel != "-" {
        return Err(super::epilogue::typed_env_refusal(
            "invalid_inputs_channel",
            "--inputs-json accepts only `-` (JSON-object stdin)",
            machine,
        ));
    }
    if !vars.is_empty() || file == "-" {
        return Err(super::epilogue::typed_env_refusal(
            "input_channel_conflict",
            "--inputs-json cannot share --var or workflow-source stdin (`run -`)",
            machine,
        ));
    }
    nika_cli_host::literal_inputs::read(std::io::stdin().lock())
        .map(Some)
        .map_err(|error| refuse(&error, machine))
}
