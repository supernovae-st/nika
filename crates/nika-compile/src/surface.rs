// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The deterministic contracts shared by members of the Compile unit (ADR-140).
//! Seat orchestration lives above the core; replay and native record application stay here.

pub use crate::doors::{
    lexical_rest_is_explicit, native_apply, plan_record, record_ledger, record_retrieval,
    record_route, replay, unresolved,
};
pub use crate::edit::literal_projection;
pub use crate::laws::{LINES, SELECT_BY_FIELD};
pub use crate::ledger::Ledger;
pub use crate::types::{EditChange, Input};

/// Host-supplied field observations; this module performs no I/O.
pub mod observed {
    pub use crate::observed::{columns, field_answer, for_intent, record, world};
}
pub use crate::{finding, finish, initial, literal_answer, parse, question};

/// Durable unresolved computation and its exact field-choice context.
pub mod pending_transform {
    pub use crate::pending_transform::{PendingTransform, PendingTransformError, invalid, present};
}

/// A deterministic admission rejection, preserving the original ordered reasons.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{}", .reasons.join("; "))]
#[non_exhaustive]
pub struct AdmissionError {
    reasons: Vec<String>,
}

impl AdmissionError {
    /// Wrap the admission's reasons without changing their text or order.
    #[must_use]
    pub fn new(reasons: Vec<String>) -> Self {
        Self { reasons }
    }

    /// The individual reasons in the order judged by the deterministic admission.
    #[must_use]
    pub fn reasons(&self) -> &[String] {
        &self.reasons
    }
}

/// Admit a reading under the requested deterministic policy.
///
/// # Errors
/// Returns every rejection of the selected HOT admission, in its original order.
pub fn admit_hot(
    intent: &str,
    reading: &nika_compile_reader::lexicon::Reading,
    hot: crate::HotPolicy,
) -> Result<(), AdmissionError> {
    crate::doors::admit_hot(intent, reading, hot).map_err(AdmissionError::new)
}

/// The assembler's entry and its unfed-plan law.
pub mod assemble {
    pub use crate::assemble::{assemble, unfed};
}

/// The bounded support clauses: resolution and assembly.
pub mod support {
    pub use crate::support::{Plan, assemble};

    /// The original rejection of a partially recognized bounded support request.
    #[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
    #[error("{reason}")]
    #[non_exhaustive]
    pub struct ResolveError {
        reason: String,
    }

    impl ResolveError {
        /// Retain the resolver's rejection text verbatim.
        #[must_use]
        pub fn new(reason: String) -> Self {
            Self { reason }
        }
    }

    /// Resolve only the complete bounded support grammar; unrelated intents return `None`.
    ///
    /// # Errors
    /// Returns the unchanged reason when recognized support clauses cannot form a plan.
    pub fn resolve(intent: &str) -> Result<Option<Plan>, ResolveError> {
        crate::support::resolve(intent).map_err(ResolveError::new)
    }
}

/// The embedded specification identity shared by deterministic outcomes and seat receipts.
#[must_use]
pub fn spec_pin() -> &'static str {
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../SPEC_PIN"))
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("")
}

/// The lowercase SHA-256 identity shared by intent records and knowledge receipts.
#[must_use]
pub fn sha256(text: &str) -> String {
    use sha2::Digest as _;
    use std::fmt::Write as _;
    sha2::Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}
