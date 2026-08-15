// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The EXPRESSION boundary — what a jq program is allowed to SEE.
//!
//! D-2026-08-11-N26 · « **une expression ne voit que son ENTRÉE** · le monde de
//! jq est la réponse du verbe, celui de CEL ses bindings · ni le processus, ni
//! l'horloge, ni le disque, ni l'environnement. C'est une **SOUSTRACTION**. »
//!
//! **This is not a permit route.** `permits.env` grants a spawned CHILD PROCESS
//! its environment ([`crate::env`]); it has never governed an in-process
//! expression, and no grant here turns a withheld native back on — a
//! subtraction has no dial. That asymmetry is the point: the boundary an author
//! declares is about what the workflow *reaches out to*, and an expression
//! reaches nothing.
//!
//! The list is pure data with zero `jaq` dependency, because THREE seams build
//! a jaq function set independently and must withhold the same names — the
//! runtime's `extract:` bindings (`nika-runtime`), the `nika:jq` builtin
//! (`nika-builtin`), and the static compile-check (`nika-check`). A divergence
//! between them is exactly the defect class this closes: before 2026-08-15 the
//! three agreed on `env` by agreeing to expose it, and the check certificate
//! printed « pure compute · nothing escapes » over a body that read the
//! operator's environment under an ABSENT `permits:` block (measured on the
//! shipped 0.108.0 binary, canary in the trace, run green).

/// One native WITHHELD from every expression seam.
///
/// Construct through [`WITHHELD_JQ_NATIVES`] — the set is closed and lives in
/// this crate so no seam can grow its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct WithheldNative {
    /// The jaq native's name, exactly as the compiler reports it undefined.
    pub name: &'static str,
    /// What it would let the expression see, in the diagnostic's voice.
    pub reads: &'static str,
    /// The governed way to obtain the same value — the half that makes the
    /// refusal teach instead of merely refuse.
    pub instead: &'static str,
}

impl WithheldNative {
    /// Assemble one row (invariant #19 — constructor on `#[non_exhaustive]`).
    #[must_use]
    pub const fn new(name: &'static str, reads: &'static str, instead: &'static str) -> Self {
        Self {
            name,
            reads,
            instead,
        }
    }
}

/// The natives an expression never receives.
///
/// Every row is a native that reads state OTHER than the value piped into it.
/// The set was derived, not guessed: `funs()` over the workspace-pinned stack
/// (jaq-core 3.1 · jaq-std 3.0 · jaq-json 2.0) exposes 114 natives, and these
/// four are the ones whose result depends on the host rather than the input.
///
/// Its cost was measured before removal (2026-08-15 · 410 `.nika.yaml` files
/// across the engine, spec, plugins, docs, audit-workflow, control-tower and
/// the atelier's own workflows · 184 jq programs extracted): **zero** call
/// sites.
///
/// # What is deliberately ABSENT, and who owns it
///
/// - **The clock family** (`now` · `localtime` · `strflocaltime`) reads the
///   host too, and measures zero uses as well — but **D-2026-08-11-N27 (active)
///   prescribes a different remedy**: `now` MUST RESOLVE TO THE RUN'S START
///   INSTANT, the one already in the trace, so a replay yields the same value
///   forever. That is a rebinding, not a subtraction, and shipping a removal
///   here would pre-empt a locked decision with a mechanism it did not choose.
///   The debt is pinned by a test in `nika-builtin`.
/// - **`debug` · `stderr` · `halt`** EMIT to the host or act on the process
///   rather than SEE beyond the input — a different class from N26, and
///   `jaq-std`'s own `defs.jq` builds on their natives.
pub const WITHHELD_JQ_NATIVES: &[WithheldNative] = &[WithheldNative::new(
    "env",
    "the ambient process environment",
    "pass the value in — `inputs:` (the caller), `const:` (the author) or \
     `secrets:` (a governed store reference); a CHILD process receives its \
     environment through `permits.env` on an `exec:` task",
)];

/// The withheld row for `name`, when there is one.
#[must_use]
pub fn withheld_jq_native(name: &str) -> Option<&'static WithheldNative> {
    WITHHELD_JQ_NATIVES.iter().find(|w| w.name == name)
}

/// Whether `name` is withheld — the predicate the three seams filter with.
#[must_use]
pub fn is_withheld_jq_native(name: &str) -> bool {
    withheld_jq_native(name).is_some()
}

/// The one-line refusal for a withheld native, NAMING the class it would have
/// read — the sentence that turns jaq's bare « undefined filter » into the
/// reason the author needs.
///
/// `None` when `name` is simply undefined (a typo), so a caller keeps jaq's own
/// wording for that case and never claims a boundary it did not enforce.
#[must_use]
pub fn withheld_jq_reason(name: &str) -> Option<String> {
    withheld_jq_native(name).map(|w| {
        format!(
            "`{}` is withheld — it reads {}, and an expression sees only its input; {}",
            w.name, w.reads, w.instead
        )
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn env_is_withheld_and_the_reason_names_the_class() {
        assert!(is_withheld_jq_native("env"));
        let reason = withheld_jq_reason("env").expect("env is withheld");
        assert!(reason.contains("ambient process environment"), "{reason}");
        assert!(reason.contains("sees only its input"), "{reason}");
        // It teaches the governed route rather than only refusing.
        assert!(reason.contains("inputs:"), "{reason}");
    }

    /// The clock family is NOT here, and that is a decision with an owner.
    ///
    /// D-2026-08-11-N27 (active) rebinds `now` to the run's start instant
    /// rather than removing it. When that ships, this test is the reminder
    /// that the list was left alone on purpose — flip it only WITH N27, never
    /// as a drive-by.
    #[test]
    fn the_clock_family_belongs_to_n27_not_to_this_list() {
        for name in ["now", "localtime", "strflocaltime"] {
            assert!(
                !is_withheld_jq_native(name),
                "{name} · N27 prescribes a REBINDING (the run's start instant), \
                 not a subtraction — removing it here pre-empts a locked decision"
            );
        }
    }

    #[test]
    fn an_ordinary_native_is_not_withheld() {
        // The pure date family STAYS — it computes from its argument.
        for name in ["strftime", "gmtime", "mktime", "map", "select", "length"] {
            assert!(!is_withheld_jq_native(name), "{name} must stay");
            assert!(withheld_jq_reason(name).is_none(), "{name} must stay");
        }
    }

    #[test]
    fn a_typo_keeps_jaqs_own_wording() {
        // No reason means the caller renders « undefined filter » — we never
        // dress a typo as a boundary refusal.
        assert!(withheld_jq_reason("envv").is_none());
        assert!(withheld_jq_reason("").is_none());
    }

    #[test]
    fn every_row_is_complete_and_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for w in WITHHELD_JQ_NATIVES {
            assert!(!w.name.is_empty());
            assert!(!w.reads.is_empty(), "{} has no class", w.name);
            assert!(!w.instead.is_empty(), "{} teaches nothing", w.name);
            assert!(seen.insert(w.name), "{} listed twice", w.name);
        }
    }
}
