// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Refresh only engine-owned instructions on an explicit turn. Historical bytes stay intact.
use super::{EntryKind, SYSTEM, Tree, TreeFile, instructions, unix_ms};
use nika_session_agent::{EntryId, Store};
use serde_json::json;

const FACT: &str = "engine_instructions";
const SCHEMA: &str = "nika/session-engine-instructions@1";
// Exact engine instruction assets before conversation/recovery guidance was introduced.
// Their dynamic suffix must also match the engine renderer, excluding any custom addition.
const LEGACY: [&str; 2] = [
    "4244df8a72f15c2f8a2039d1b4a4d3320db2a3ad169c1859b12903527dda3be4",
    "c529c155b2cc0e93c929d76113e47bb1118b85488a5a0362d7f91af8ebbbfd92",
];

fn digest(text: &str) -> String {
    nika_event::source_id::sha256_hex(text.as_bytes())
}

fn legacy(text: &str) -> bool {
    let Some((body, tail)) = text.split_once("\nThis Session: the person chose `") else {
        return false;
    };
    if !LEGACY.contains(&digest(body).as_str()) {
        return false;
    }
    let Some((model, _)) = tail.split_once("` to prepare with;") else {
        return false;
    };
    let current = instructions(model);
    current
        .strip_prefix(SYSTEM)
        .is_some_and(|suffix| text == format!("{body}{suffix}"))
}

/// Journal ownership with the exact System entry and bytes. Facts never enter model messages.
pub(super) fn mark(
    tree: &mut Tree,
    store: &mut TreeFile,
    id: &EntryId,
    text: &str,
) -> Result<(), String> {
    tree.append(
        EntryKind::Fact {
            name: FACT.to_owned(),
            data: json!({"schema": SCHEMA, "entry": id.as_str(), "sha256": digest(text)}),
        },
        unix_ms(),
        |line| Store::append(store, line),
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn refresh(tree: &mut Tree, store: &mut TreeFile, chosen: &str) -> Result<(), String> {
    let branch = tree.branch();
    let Some((entry, text)) = branch.iter().rev().find_map(|entry| match &entry.kind {
        EntryKind::System { text } => Some((entry.id.clone(), text.clone())),
        _ => None,
    }) else {
        return Ok(());
    };
    let current = instructions(chosen);
    let hash = digest(&text);
    let owned = branch.iter().any(|record| {
        matches!(&record.kind,
        EntryKind::Fact { name, data } if name == FACT && data["schema"] == SCHEMA
            && data["entry"] == entry.as_str() && data["sha256"] == hash)
    });
    if text == current {
        // A failed Fact append may have left the exact engine System durable. Restore its
        // ownership witness once; customized bytes never meet this equality.
        return if owned {
            Ok(())
        } else {
            mark(tree, store, &entry, &text)
        };
    }
    if !owned && !legacy(&text) {
        return Ok(());
    }
    let id = tree
        .append(
            EntryKind::System {
                text: current.clone(),
            },
            unix_ms(),
            |line| Store::append(store, line),
        )
        .map_err(|error| error.to_string())?;
    mark(tree, store, &id, &current)
}
