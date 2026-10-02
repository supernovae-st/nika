// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Words derived only from a copy qualification or observed witness; formatting grants no effects.

use super::{CopyLowering, Qualified, Seen, Witness};

/// The preview's line on execution for a rehearsed copy: where it ran and what it read back
/// there, from the user's own world only.
#[must_use]
pub fn rehearsed_lines(q: &Qualified) -> String {
    let p = &q.preview;
    let published = if p.published {
        "published by the run"
    } else {
        "not published by the run"
    };
    let before = p.replaced.map_or_else(
        || format!("`{}` did not exist", p.target),
        |bytes| format!("replaces `{}` ({bytes} B)", p.target),
    );
    let lowering = match q.lowering {
        CopyLowering::Text => "text",
        CopyLowering::Bytes => "byte",
        _ => "closed",
    };
    format!(
        "Rehearsed once on a copy of your files · nothing ran on the originals · `yes` saves these exact bytes and checks them · running is its own line (« run it »)\n  read back · `{}` {published} · {} B · sha256 {} · the whole text\n    « {} »\n  from `{}` · {} B · sha256 {} · {before}\n  the {lowering} copy held on every world: {}\n",
        p.target,
        p.bytes,
        short(&p.sha256),
        p.excerpt,
        p.source,
        p.source_bytes,
        short(&p.source_sha256),
        p.worlds.join(" · ")
    )
}

/// What the yes found of the rehearsed world, in words.
#[must_use]
pub fn held_words(witness: &Witness) -> String {
    let parts: Vec<String> = witness
        .world()
        .iter()
        .map(|(path, seen)| match seen {
            Seen::File(digest) => format!("`{path}` the same {} B", digest.bytes),
            Seen::Absent => format!("`{path}` still absent"),
        })
        .collect();
    format!(
        "the rehearsed world holds: {} (the bytes are the selected ones)",
        parts.join(" · ")
    )
}

/// The first twelve characters of a digest.
fn short(sha256: &str) -> &str {
    sha256.get(..12).unwrap_or(sha256)
}
