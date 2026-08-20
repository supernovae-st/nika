// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project-file verdict `nika check` prints.
//!
//! Renders from PRIMITIVES, never from `nika-vocab`'s types: this crate
//! renders and owes no dependency on the grammar it renders. The caller
//! reads the parsed project and hands over the pieces.

/// The green verdict, and why this document was judged as a project.
#[must_use]
pub fn verdict(path: &str, name: &str, governs: &str) -> String {
    format!(
        "nika check · {path}\n \
         ✔ PROJECT  {name} · {governs}\n   \
         the type discriminant is `tasks:` — this document declares none, \
         so it is judged as a project, never as a workflow\n"
    )
}

/// A refusal in the project grammar's own vocabulary — never the
/// workflow envelope's, which would demand a `tasks:` map this file must
/// not grow.
#[must_use]
pub fn refusal(path: &str, line: Option<usize>, slug: &str, detail: &str, remedy: &str) -> String {
    let at = line.map_or_else(String::new, |l| format!(":{l}"));
    format!(
        "nika check · {path}\n \
         ✖ PROJECT  {path}{at} · {slug} · {detail}\n   fix: {remedy}\n"
    )
}

/// What the file governs, named from what it actually declared.
///
/// A project that governs nothing says so, rather than rendering an
/// empty list that reads like a measurement.
#[must_use]
pub fn governs(ceiling: Option<f64>, traces: bool, registry: bool, beats: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(c) = ceiling {
        parts.push(format!("ceiling ${c:.2}"));
    }
    if traces {
        parts.push("trace retention".to_owned());
    }
    if registry {
        parts.push("a registry floor".to_owned());
    }
    match beats {
        0 => {}
        1 => parts.push("1 beat armed".to_owned()),
        n => parts.push(format!("{n} beats armed")),
    }
    if parts.is_empty() {
        "governs nothing (every knob falls to its built-in default)".to_owned()
    } else {
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_that_declares_nothing_says_so() {
        assert!(governs(None, false, false, 0).contains("governs nothing"));
    }

    #[test]
    fn every_declared_knob_is_named_once() {
        let g = governs(Some(0.5), true, true, 3);
        assert!(g.contains("ceiling $0.50"), "{g}");
        assert!(g.contains("trace retention"), "{g}");
        assert!(g.contains("a registry floor"), "{g}");
        assert!(g.contains("3 beats armed"), "{g}");
    }

    #[test]
    fn one_beat_is_singular() {
        assert!(governs(None, false, false, 1).contains("1 beat armed"));
    }

    #[test]
    fn a_refusal_never_demands_tasks() {
        let r = refusal("nika.yaml", Some(2), "project.unknown-key", "d", "r");
        assert!(!r.contains("tasks"), "{r}");
        assert!(r.contains("nika.yaml:2"), "{r}");
    }
}
