// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Speaker attribution derived from existing journal boundaries, never serialized.
//! A full tuple window has no entry IDs: uncertain occurrence alignment stays neutral.

use super::Operation;
use crate::runtime::RECENT_TURNS;

#[derive(Default)]
pub(super) struct Presentation {
    human: Vec<bool>,
}

impl Presentation {
    pub(super) fn is_human(&self, index: usize) -> bool {
        self.human.get(index).copied().unwrap_or(false)
    }

    pub(super) fn push(&mut self, human: bool) {
        self.human.push(human);
        if self.human.len() > RECENT_TURNS {
            self.human.remove(0);
        }
    }

    pub(super) fn completed(
        &mut self,
        before: &[(String, String)],
        after: &[(String, String)],
        started: Option<&(Operation, String)>,
    ) {
        let said = started.and_then(|(operation, input)| {
            matches!(operation, Operation::Turn | Operation::Run).then_some(input)
        });
        // Below capacity, append-only retention has one possible alignment. At
        // capacity, every compatible suffix is possible, including full replacement.
        // Preserve Human only when every possible occurrence has that provenance.
        let retained: Vec<usize> = (0..=before.len().min(after.len()))
            .filter(|&len| {
                (after.len() == RECENT_TURNS || len == before.len())
                    && before[before.len() - len..] == after[..len]
            })
            .collect();
        self.human = (0..after.len())
            .map(|index| {
                !retained.is_empty()
                    && retained.iter().all(|&len| {
                        if index < len {
                            self.is_human(before.len() - len + index)
                        } else {
                            said == Some(&after[index].0)
                        }
                    })
            })
            .collect();
    }
}

#[cfg(test)]
mod tests;
