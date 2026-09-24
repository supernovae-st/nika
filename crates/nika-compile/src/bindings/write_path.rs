// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The path question of a write whose target names no file. Its key is the output's place
//! in the plan, never the state of the other answers: a plan with one such write keeps
//! `const.output_path`; several are numbered in plan order (`const.output_1_path`, …), so
//! two outputs never share a question and no key moves when another output is answered. A
//! file another output of the request already receives is refused, never shared.

use crate::paths::{self, PathShape};
use crate::plan::{Effect, EffectVerb, Plan};
use crate::support::{answer, reject};
use crate::{CompileOutcome, CompileRequest};
use std::collections::BTreeSet;
use std::path::{Component, Path};

/// The question key of every write effect that names no single file, indexed like
/// `plan.effects`; `None` for every other effect.
pub(super) fn keys(plan: &Plan) -> Vec<Option<String>> {
    let unnamed: Vec<usize> = plan
        .effects
        .iter()
        .enumerate()
        .filter(|(_, effect)| {
            effect.verb == EffectVerb::Write && super::file_write(effect).is_none()
        })
        .map(|(index, _)| index)
        .collect();
    let mut keys = vec![None; plan.effects.len()];
    for (position, index) in unnamed.iter().enumerate() {
        let key = if unnamed.len() == 1 {
            "const.output_path".to_owned()
        } else {
            format!("const.output_{}_path", position + 1)
        };
        if let Some(slot) = keys.get_mut(*index) {
            *slot = Some(key);
        }
    }
    keys
}

/// The exact file of one write that names none, asked under its stable key. An answer that is
/// no file (a directory, a glob, a placeholder, or prose that names no path at all) keeps the
/// question asked, and so does a file another output already receives: one answer never
/// binds two outputs.
pub(super) fn ask(
    effect: &Effect,
    key: &str,
    taken: &BTreeSet<String>,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
) -> Option<String> {
    recognized.insert(key.to_owned());
    let target = effect.target.trim();
    let label = format!(
        "Which exact file path should receive `{target}`? One path-shaped token (for example ./out/result.md), no prose; a directory is not a file."
    );
    let value = answer(request, out, key, &label, true)?;
    let Some(PathShape::File(path)) = value.as_str().and_then(paths::token) else {
        reject(
            out,
            key,
            &label,
            true,
            "Name one exact file, not a directory, a glob or a placeholder.",
        );
        return None;
    };
    if taken.iter().any(|other| same_lexical_path(other, &path)) {
        reject(
            out,
            key,
            &label,
            true,
            &format!(
                "`{path}` already receives another output of this request; name a different file for `{target}`."
            ),
        );
        return None;
    }
    Some(path)
}

/// Compare lexical components without filesystem I/O or resolving symlinks. Dot components
/// and repeated separators cannot turn one destination into two outputs; parent components
/// retain their meaning and the runtime still owns real filesystem confinement.
fn same_lexical_path(left: &str, right: &str) -> bool {
    Path::new(left)
        .components()
        .filter(|part| !matches!(part, Component::CurDir))
        .eq(Path::new(right)
            .components()
            .filter(|part| !matches!(part, Component::CurDir)))
}
