// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The deterministic contracts shared by members of the Compile unit (ADR-140).
//! Seat orchestration lives above the core; replay and native record application stay here.

pub use crate::doors::{
    lexical_rest_is_explicit, native_apply, plan_record, record_ledger, record_retrieval,
    record_route, replay, replay_judged, unresolved,
};
pub use crate::edit::literal_projection;
pub use crate::laws::{LINES, SELECT_BY_FIELD};
pub use crate::ledger::{Binding, Disposition, Judgment, Ledger};
pub use crate::seat_cap::output_caps;
pub use crate::types::{EditChange, Input};

/// Host-supplied field observations; this module performs no I/O.
pub mod observed {
    pub use crate::observed::{columns, field_answer, for_intent, record, world};
    use unicode_normalization::UnicodeNormalization;

    /// The observed spellings canonically equivalent (Unicode NFC) to `literal` and not
    /// byte-identical to it, in the order observed: the bounded canonical-spelling law (R4 A5).
    /// A typed text equality also matches each exactly beside its literal (`decision.spellings`);
    /// a program a seat writes must treat each as the literal (R4 A11). No case folding, no
    /// compatibility (NFKC) folding, no accent stripping.
    #[must_use]
    pub fn equivalent_spellings(literal: &str, observed: &[String]) -> Vec<String> {
        let canonical = |text: &str| text.nfc().collect::<String>();
        let target = canonical(literal);
        observed
            .iter()
            .filter(|value| value.as_str() != literal && canonical(value) == target)
            .cloned()
            .collect()
    }

    /// The literals `clause` states that the host observed spelled with other bytes, as
    /// `(stated, observed)` pairs in the order observed (R4 A11). Each stated value retains
    /// its actual bytes, including partly composed forms, and is canonically equivalent to
    /// the observed value. Its exact token boundaries exclude letters, digits and combining
    /// marks on either side. Column names and byte-identical values bind nothing.
    #[must_use]
    pub fn stated_spellings(
        clause: &str,
        observed: &[String],
        columns: &[String],
    ) -> Vec<(String, String)> {
        observed
            .iter()
            .filter_map(|value| {
                stated(clause, value, columns).map(|literal| (literal.to_owned(), value.clone()))
            })
            .collect()
    }

    /// Find the original span, not just its NFC and NFD renderings. Canonical decomposition
    /// length bounds the scan: every source character contributes at least one code point,
    /// and equivalent spans must have the same decomposed length, even when marks reorder.
    fn stated<'a>(clause: &'a str, observed: &str, columns: &[String]) -> Option<&'a str> {
        let expected: String = observed.nfd().collect();
        let limit = expected.chars().count();
        if limit == 0 {
            return None;
        }
        for (start, _) in clause.char_indices() {
            if !boundary(clause[..start].chars().next_back()) {
                continue;
            }
            let mut decomposed = 0;
            for (offset, character) in clause[start..].char_indices() {
                let mut bytes = [0; 4];
                decomposed += character.encode_utf8(&mut bytes).nfd().count();
                if decomposed > limit {
                    break;
                }
                let end = start + offset + character.len_utf8();
                let literal = &clause[start..end];
                if decomposed == limit {
                    if literal != observed
                        && boundary(clause[end..].chars().next())
                        && !columns.iter().any(|column| column == literal)
                        && literal.nfd().eq(expected.chars())
                    {
                        return Some(literal);
                    }
                    break;
                }
            }
        }
        None
    }

    fn boundary(character: Option<char>) -> bool {
        character.is_none_or(|c| {
            !c.is_alphanumeric() && !unicode_normalization::char::is_combining_mark(c)
        })
    }
}

/// The money a caller admitted or its operator stated, read before any strategy (R4 B15).
pub mod admitted {
    pub use crate::admitted::{read, record, refused, replacement};
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

/// The assembler's entry (also under the judgments made in this compile, R4 A11), its
/// unfed-plan law and its contradiction refusal, and the assembly under one lowering of an
/// exact copy's read.
pub mod assemble {
    pub use crate::assemble::{
        CopyLowering, assemble, assemble_judged, assemble_lowered, refuse_contradiction, unfed,
    };
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
