// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! [`CelError`] — the spec-plane conformance error for the
//! `cel-subset/0.1` engine.
//!
//! CEL errors carry SPEC wire codes (`NIKA-VAR-001` · `NIKA-VAR-005` ·
//! `NIKA-VAR-006`) whose canon is the spec 05 error table (resolvable
//! via `nika_pack::error_codes()`) — NOT a `nika-error` registry range.
//! Same plane as the runtime's `NIKA-TIMEOUT-001`: these are
//! conformance codes the engine emits, not engine-internal enum codes.

use std::fmt;

/// The spec-plane class of a CEL failure (each maps to a `NIKA-VAR-NNN`
/// wire code · spec 05).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CelErrorKind {
    /// NIKA-VAR-001 · unresolved reference (unknown root · `.field` of a
    /// null/absent value · `[i]` out of range).
    Unresolved,
    /// NIKA-VAR-005 · static grammar violation (bad token · chained
    /// relation · unknown call · a statically-non-boolean `when:` root).
    Static,
    /// NIKA-VAR-006 · type error at compute time (cross-type compare ·
    /// non-bool boolean operand · `size()` of a scalar).
    Type,
}

impl CelErrorKind {
    /// The spec wire code (`NIKA-VAR-NNN` · resolvable in the embedded
    /// canon via `nika_pack::error_codes()`).
    #[must_use]
    pub const fn spec_code(self) -> &'static str {
        match self {
            Self::Unresolved => "NIKA-VAR-001",
            Self::Static => "NIKA-VAR-005",
            Self::Type => "NIKA-VAR-006",
        }
    }
}

/// One `cel-subset/0.1` failure · the spec code + a message + the byte
/// span into the source (for the host's diagnostic rendering).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{} · {message} (at {}..{})", kind.spec_code(), span.0, span.1)]
pub struct CelError {
    kind: CelErrorKind,
    message: String,
    span: (usize, usize),
}

impl CelError {
    /// A static (parse-time · NIKA-VAR-005) error.
    #[must_use]
    pub fn static_err(message: impl Into<String>, span: (usize, usize)) -> Self {
        Self {
            kind: CelErrorKind::Static,
            message: message.into(),
            span,
        }
    }

    /// A type (compute-time · NIKA-VAR-006) error.
    #[must_use]
    pub fn type_err(message: impl Into<String>, span: (usize, usize)) -> Self {
        Self {
            kind: CelErrorKind::Type,
            message: message.into(),
            span,
        }
    }

    /// An unresolved-reference (NIKA-VAR-001) error.
    #[must_use]
    pub fn unresolved(message: impl Into<String>, span: (usize, usize)) -> Self {
        Self {
            kind: CelErrorKind::Unresolved,
            message: message.into(),
            span,
        }
    }

    /// The spec wire code (`NIKA-VAR-NNN`).
    #[must_use]
    pub fn spec_code(&self) -> &'static str {
        self.kind.spec_code()
    }

    /// The failure class.
    #[must_use]
    pub const fn kind(&self) -> CelErrorKind {
        self.kind
    }

    /// The human message (without the code/span prefix).
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The byte span `(start, end)` into the parsed source.
    #[must_use]
    pub const fn span(&self) -> (usize, usize) {
        self.span
    }
}

impl fmt::Display for CelErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spec_code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_maps_to_its_spec_code() {
        assert_eq!(CelErrorKind::Unresolved.spec_code(), "NIKA-VAR-001");
        assert_eq!(CelErrorKind::Static.spec_code(), "NIKA-VAR-005");
        assert_eq!(CelErrorKind::Type.spec_code(), "NIKA-VAR-006");
    }

    #[test]
    fn constructors_carry_kind_message_and_span() {
        let e = CelError::static_err("bad token", (3, 7));
        assert_eq!(e.kind(), CelErrorKind::Static);
        assert_eq!(e.spec_code(), "NIKA-VAR-005");
        assert_eq!(e.message(), "bad token");
        assert_eq!(e.span(), (3, 7));
    }

    #[test]
    fn display_is_code_first() {
        let e = CelError::type_err("cross-type compare", (0, 5));
        let s = e.to_string();
        assert!(s.starts_with("NIKA-VAR-006 · "), "code-first: {s}");
        assert!(s.contains("cross-type compare"));
    }

    #[test]
    fn kind_display_writes_the_wire_code() {
        // CelErrorKind's OWN Display (distinct from CelError's) writes the
        // wire code · a blanking `fmt -> Ok(())` mutant leaves these empty.
        assert_eq!(CelErrorKind::Unresolved.to_string(), "NIKA-VAR-001");
        assert_eq!(CelErrorKind::Static.to_string(), "NIKA-VAR-005");
        assert_eq!(CelErrorKind::Type.to_string(), "NIKA-VAR-006");
    }

    #[test]
    fn the_full_error_display_reports_the_exact_span() {
        // The span numbers ARE rendered (the host's diagnostic anchor) ·
        // pins the `(at {}..{})` shape against a blanked span.
        let e = CelError::static_err("bad token", (3, 7));
        assert!(e.to_string().contains("(at 3..7)"), "got: {e}");
    }
}
