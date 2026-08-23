// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! SLOT rung — an unfilled scaffold is not a workflow (#1066).

use nika_check::SlotMarker;

use super::{Theme, section_list};

/// Stamp the SLOT rows after the report header. Silent when empty.
pub fn stamp_unfilled_slots(text: &mut String, slots: &[SlotMarker], t: Theme) {
    if slots.is_empty() {
        return;
    }
    let mut block = String::new();
    let mut rows = vec![nika_check::slot_refusal_message(slots)];
    for slot in slots {
        rows.push(format!("line {} · {}", slot.line, slot.label()));
    }
    section_list(&mut block, t, "SLOT", "", rows);
    if let Some(at) = text.find('\n') {
        text.insert_str(at + 1, &block);
    } else {
        text.push('\n');
        text.push_str(&block);
    }
}
