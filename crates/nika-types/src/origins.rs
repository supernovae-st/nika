// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The closed origin vocabulary of a bound input (NEP-0014 law 2 ·
//! kebab-case on the wire) — descended from `nika-runtime` at the 15k
//! wall (ADR-110 · #889): the types ride their leaf home, the derivation
//! (`input_origins`, over the file's declarations) stays with the runtime.

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InputOrigin {
    /// `--var name=value` typed by the operator at a terminal.
    CliOperator,
    /// `--var name=value` supplied through the CI context (the `CI`
    /// environment marks it — the pipeline, not a human, is the caller).
    CiContext,
    /// `--var name=@env:VAR` — the declared environment channel: the
    /// value was read from the OS environment through the explicit
    /// spelling, never an ambient guess.
    Env,
    /// The declared `default:` in the workflow file filled the input.
    File,
}

impl InputOrigin {
    /// The wire form (the journal's field values).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CliOperator => "cli-operator",
            Self::CiContext => "ci-context",
            Self::Env => "env",
            Self::File => "file",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_forms_are_the_laws_vocabulary() {
        assert_eq!(InputOrigin::CliOperator.as_str(), "cli-operator");
        assert_eq!(InputOrigin::CiContext.as_str(), "ci-context");
        assert_eq!(InputOrigin::Env.as_str(), "env");
        assert_eq!(InputOrigin::File.as_str(), "file");
        assert_eq!(
            serde_json::to_value(InputOrigin::CiContext).expect("serializes"),
            serde_json::Value::from("ci-context"),
            "serde speaks the same kebab-case"
        );
    }
}
