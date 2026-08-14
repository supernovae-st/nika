// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pre-C2 dead value forms (the E-split · R3a) — the closed
//! vocabulary of the forms the flag-day killed, plus their teaching
//! texts, byte-mirrored from the reference oracle
//! (`nika-spec/reference/values_core.py`).
//!
//! Descended out of `nika-schema`'s error door at the C2 wall (the 15k
//! prod-LOC budget — the teachings are message vocabulary, and the
//! vocabulary crate is its home · the `keys.rs` precedent). The
//! `SchemaError::DeadValueForm` variant carries this enum as its payload
//! — the variant and its spec-code arm stay in `nika-schema` (the error
//! one-voice), the vocabulary lives here.

/// The pre-C2 dead value forms (the E-split · R3a) — the payload the
/// spec-code arm and the teachings key on (no stringly-typed form).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeadForm {
    /// `vars` — dead since C2 (`NIKA-VALUES-001`).
    Vars,
    /// `env` — dead since C2 (`NIKA-VALUES-002`).
    Env,
}

impl DeadForm {
    /// The `NIKA-VALUES-<num>` wire number of the form (the spec
    /// registry · `canon/laws/values.yaml` LAW-GRAMMAR-0201/0202).
    #[must_use]
    pub fn spec_num(self) -> u16 {
        match self {
            Self::Vars => 1,
            Self::Env => 2,
        }
    }

    /// The envelope-field teaching, byte-mirrored from the reference
    /// oracle (`reference/values_core.py`) plus the codemod pointer.
    #[must_use]
    pub fn field_teaching(self) -> String {
        let (field, classify) = match self {
            Self::Vars => (
                "vars:",
                "a typed parameter is an `inputs:` declaration, a fixed value is a `const:` entry",
            ),
            Self::Env => (
                "env:",
                "non-sensitive runtime configuration is an `inputs:` declaration with `required: false` and a `default:`, a governed store reference is a `secrets:` entry",
            ),
        };
        format!(
            "{field} is a dead envelope field (R3a · the E-split) · {classify} \
             (classify-not-rename · equivalence-or-stop · never a bulk rename) \
             — `nika check --fix` migrates"
        )
    }

    /// The `${{ form.X }}` read teaching, byte-mirrored from the oracle.
    #[must_use]
    pub fn ref_teaching(self, reference: &str) -> String {
        let form = match self {
            Self::Vars => "vars",
            Self::Env => "env",
        };
        format!(
            "${{{{ {reference} }}}} reads the dead `{form}` namespace (R3a · the E-split) · \
             read the value authority its role commands (inputs · const · secrets)"
        )
    }
}

/// The `${{ <root>.X }}` foreign-namespace teaching (`NIKA-VALUES-003`),
/// byte-mirrored from the reference oracle (`reference/values_core.py`).
#[must_use]
pub fn foreign_namespace_teaching(root: &str) -> String {
    format!(
        "${{{{ {root}.X }}}} reads `{root}`, a value namespace outside the \
         four-authority family (R3a · LAW-SURFACE-0201) · the authorities are \
         exactly inputs · const · secrets"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_teaching_names_the_classification_and_the_repair() {
        let v = DeadForm::Vars.field_teaching();
        assert!(v.contains("vars: is a dead envelope field"), "{v}");
        assert!(v.contains("`inputs:`") && v.contains("`const:`"), "{v}");
        assert!(v.contains("classify-not-rename"), "{v}");
        assert!(v.contains("nika check --fix"), "{v}");
        let e = DeadForm::Env.field_teaching();
        assert!(e.contains("`inputs:`") && e.contains("`secrets:`"), "{e}");
        // `config:` died with the 9-key envelope: the env teaching may
        // never send an author to it again.
        assert!(!e.contains("`config:`"), "{e}");
    }

    #[test]
    fn ref_teaching_names_the_dead_namespace_and_the_family() {
        let v = DeadForm::Vars.ref_teaching("vars.topic");
        assert!(v.contains("${{ vars.topic }}"), "{v}");
        assert!(v.contains("dead `vars` namespace"), "{v}");
        assert!(v.contains("inputs · const · secrets"), "{v}");
    }

    #[test]
    fn foreign_teaching_names_the_root_and_the_closed_family() {
        let f = foreign_namespace_teaching("params");
        assert!(f.contains("${{ params.X }}"), "{f}");
        assert!(f.contains("outside the four-authority family"), "{f}");
        assert!(f.contains("inputs · const · secrets"), "{f}");
    }
}
