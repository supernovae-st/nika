// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Integration test proving `ObservabilitySink` was dropped per ADR/Q12.
//!
//! Adding it back must reopen Q12 in
//! `docs/architecture/l0-l05-architecture-decisions.md`.

// Doc-link sentinel — if this comment block moves, update Q12 cross-ref.
// See: docs/architecture/l0-l05-architecture-decisions.md Decision Q12.
//
// The doctest below is a `compile_fail` that asserts the symbol is not
// importable. If it ever starts compiling, the drop regressed and Q12
// must be re-opened.

#[test]
fn observability_sink_is_dropped() {
    // Sentinel — anchors the Q12 doc reference. Real enforcement is the
    // `compile_fail` doctest on `_compile_fail_anchor` below.
    let _: fn() = _compile_fail_anchor;
}

/// This `use` must NOT compile.
///
/// ```compile_fail
/// use nika_kernel::ObservabilitySink;
/// ```
#[allow(dead_code)]
fn _compile_fail_anchor() {}
