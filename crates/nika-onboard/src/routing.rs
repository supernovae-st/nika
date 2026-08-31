// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The one routing door, exposed for sibling surfaces (RAMS-11): the
//! MCP oracle follows the SAME door the CLI walks — one router, one
//! calibration, zero drift between what `nika new` routes and what an
//! agent's `nika_examples`/`nika_template` tool routes.

use crate::intent::RoutingOutcome;

/// Where a plain-words query landed.
#[derive(Debug, PartialEq, Eq)]
pub enum RoutedEntry {
    /// A complete example (a lesson — lands verbatim on the CLI side).
    Example(String),
    /// A skeleton template (SLOTs to fill).
    Skeleton(String),
    /// Below the confidence bar — the closest names, for recognition.
    Clarify(Vec<String>),
}

/// Route plain words through the WHOLE catalog (examples + skeletons) —
/// the `nika new <intent>` door (G-16).
#[must_use]
pub fn route_query(query: &str) -> RoutedEntry {
    classify(crate::intent::route(query))
}

/// Route plain words within the SKELETON set only — the wizard's door
/// (its whole gesture is a file of SLOTs to fill).
#[must_use]
pub fn route_skeleton_query(query: &str) -> RoutedEntry {
    classify(crate::intent::route_skeletons(query))
}

fn classify(outcome: RoutingOutcome) -> RoutedEntry {
    match outcome {
        RoutingOutcome::Routed { template: name, .. } => {
            if nika_pack::example(&name).is_some() {
                RoutedEntry::Example(name)
            } else {
                RoutedEntry::Skeleton(name)
            }
        }
        RoutingOutcome::NeedsClarification { candidates, .. } => RoutedEntry::Clarify(candidates),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two doors agree with the CLI's: a job phrase routes to the
    /// example (whole catalog) · the same phrase in the skeleton door
    /// stays within skeletons · nonsense clarifies, never guesses.
    #[test]
    fn the_doors_route_like_the_cli() {
        assert_eq!(
            route_query("chase unpaid invoices"),
            RoutedEntry::Example("invoice-chaser".to_owned())
        );
        assert!(
            !matches!(
                route_skeleton_query("chase unpaid invoices"),
                RoutedEntry::Example(_)
            ),
            "the skeleton door must never leak an example"
        );
        assert!(
            matches!(route_query("zzz qqq xxx"), RoutedEntry::Clarify(_)),
            "nonsense must clarify, never guess"
        );
    }

    /// B01 · `hello` is the 01-hello lesson, never a second file.
    #[test]
    fn hello_is_the_numbered_lesson() {
        assert_eq!(
            route_query("hello"),
            RoutedEntry::Example("01-hello".to_owned())
        );
        assert_eq!(
            route_query("01-hello"),
            RoutedEntry::Example("01-hello".to_owned())
        );
    }

    /// C09 · a weekend URL summary writes 05-fetch-chain or names it.
    #[test]
    fn a_weekend_summary_of_three_urls_names_fetch_chain() {
        let outcome = route_query("a weekend summary of three URLs");
        assert!(
            !matches!(outcome, RoutedEntry::Skeleton(_)),
            "must not land a skeleton: {outcome:?}"
        );
        match outcome {
            RoutedEntry::Example(name) => assert_eq!(
                name, "05-fetch-chain",
                "the weekend URL summary is the fetch-chain lesson"
            ),
            RoutedEntry::Clarify(candidates) => assert!(
                candidates.iter().any(|c| c == "05-fetch-chain"),
                "must name 05-fetch-chain, got {candidates:?}"
            ),
            RoutedEntry::Skeleton(_) => {}
        }
    }
}
