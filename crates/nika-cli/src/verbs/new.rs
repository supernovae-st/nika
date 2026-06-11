// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika new --from <template> <dest>` — instantiate one of the embedded
//! skeletons (spec §2). Refuses to overwrite (the human keeps the hand);
//! `--force` is the explicit override. The written file is the template
//! VERBATIM — slots stay visible so the author fills them deliberately.

use std::path::Path;

use crate::verbs::{VerbOutput, exit};

/// The `nika new` verb.
#[must_use]
pub fn run(template: &str, dest: &str, force: bool) -> VerbOutput {
    let Some(body) = nika_pack::template(template) else {
        return VerbOutput {
            text: format!(
                "unknown template `{template}` — embedded set: {}",
                nika_pack::template_names().join(" · ")
            ),
            code: exit::FILE,
        };
    };
    if Path::new(dest).exists() && !force {
        return VerbOutput {
            text: format!("{dest} exists — pass --force to overwrite"),
            code: exit::ENV,
        };
    }
    if let Err(e) = std::fs::write(dest, body) {
        return VerbOutput {
            text: format!("cannot write {dest}: {e}"),
            code: exit::ENV,
        };
    }
    VerbOutput {
        text: format!(
            "{dest} ← template `{template}` · fill the `# SLOT:` lines then `nika check {dest}`"
        ),
        code: exit::OK,
    }
}
