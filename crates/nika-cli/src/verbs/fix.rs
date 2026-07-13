// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check --fix` — apply the machine-applicable renames and converge.
//!
//! The check ladder was DESIGNED for a repair loop: every rename finding
//! carries a typed `suggestion` (the deterministic did-you-mean — never a
//! guess past the threshold), and the code comments promise « the agent
//! repair loop pattern-matches once and converges ». This verb is that
//! loop, in-binary (the `cargo clippy --fix` / `eslint --fix` shape):
//!
//! 1. parse — an [`UnknownField`](nika_schema::SchemaError) with a typed
//!    suggestion (`promt:` → `prompt:`) is spliced and the round restarts
//!    (parse aborts at the first defect, so parse-level repairs land one
//!    per round);
//! 2. check — every `unknown_tools[]` / `unknown_args[]` finding with a
//!    typed suggestion (`nika:raed` → `nika:read` · `inpit` → `input` ·
//!    `expr` → `expression`) is spliced in the same pass;
//! 3. re-parse + re-check until a round applies nothing (capped).
//!
//! SAFETY over reach — a repair is applied ONLY when:
//! - the suggestion is TYPED (never regex-scraped from a human message);
//! - the old token occurs EXACTLY ONCE in the source as a whole word
//!   (word-boundary guarded — ambiguity or a second occurrence skips the
//!   repair with an honest note, the file is never guessed at);
//! - the file re-parses/re-checks after (the loop converging IS the
//!   proof; a repair that made things worse cannot survive the rounds).
//!
//! The file is rewritten only when at least one repair applied; the verb
//! then renders the NORMAL check report of the final state — `--fix` is
//! check plus a converging pen, not a different audit.

use std::fmt::Write as _;

use nika_schema::{ParseMode, SchemaError};

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, exit};

/// One applied (or skipped) repair, for the summary.
struct Repair {
    old: String,
    new: String,
    kind: &'static str,
    applied: bool,
}

/// Rounds cap — each parse-level repair costs one round (parse aborts at
/// the first defect), so this bounds pathological inputs, not real files.
const MAX_ROUNDS: usize = 16;

/// The `nika check <file> --fix` verb. Single real file only (the caller
/// refuses stdin and multi-file — a rewrite needs a place to write).
#[must_use]
pub fn run(path: &str, native_strict: bool, model: Option<&str>, theme: Theme) -> VerbOutput {
    let Ok(original) = std::fs::read_to_string(path) else {
        return VerbOutput::env(format!("cannot read {path}"));
    };
    let mut source = original.clone();
    let mut repairs: Vec<Repair> = Vec::new();

    for _ in 0..MAX_ROUNDS {
        let mut round_applied = false;
        match nika_schema::parse(&source, nika_schema::FileId::new(0), ParseMode::Strict) {
            Err(SchemaError::UnknownField {
                field,
                suggestion: Some(to),
                ..
            }) => {
                round_applied |= splice(&mut source, &field, &to, "field", &mut repairs);
                // A parse-fatal we cannot splice (ambiguous token): the
                // loop cannot progress past parse — stop honestly.
                if !round_applied {
                    break;
                }
            }
            // W1 « the map » dead forms (PARSE-020..023): ONE structural
            // repair — the shared migration (comment-preserving ·
            // idempotent) — then the round restarts and the re-parse is
            // the proof. The old form is repairable, never executable.
            Err(
                SchemaError::W1WorkflowScalar { .. }
                | SchemaError::W1TopLevelDescription { .. }
                | SchemaError::W1TasksSequence { .. }
                | SchemaError::W1TaskIdField { .. },
            ) => match nika_schema::migrate::w1(&source) {
                Some(migrated) => {
                    source = migrated;
                    repairs.push(Repair {
                        old: "the pre-W1 envelope (workflow scalar · tasks list)".to_owned(),
                        new: "workflow object + task map".to_owned(),
                        kind: "w1-map",
                        applied: true,
                    });
                }
                None => break, // ambiguous document — the teaching names it
            },
            Err(_) => break, // not a rename-shaped parse error — check will tell
            Ok(wf) => {
                let report = nika_schema::check(&wf);
                // Collect this round's typed renames FIRST (splicing
                // invalidates nothing — each token is unique by the gate).
                let mut renames: Vec<(String, String, &'static str)> = Vec::new();
                for t in &report.unknown_tools {
                    if let Some(s) = &t.suggestion {
                        renames.push((t.tool.clone(), s.clone(), "tool"));
                    }
                }
                for a in &report.unknown_args {
                    if let Some(s) = &a.suggestion {
                        renames.push((a.arg.clone(), s.clone(), "arg"));
                    }
                }
                // Conformance renames (typed `offending`/`suggestion` —
                // unknown depends_on target · unresolved `${{ }}` ref,
                // both fully qualified so a splice keeps the namespace).
                for v in &report.conformance {
                    if let (Some(o), Some(s)) = (&v.offending, &v.suggestion) {
                        renames.push((o.clone(), s.clone(), "ref"));
                    }
                }
                renames.sort();
                renames.dedup();
                for (old, new, kind) in renames {
                    round_applied |= splice(&mut source, &old, &new, kind, &mut repairs);
                }
                if !round_applied {
                    break; // converged — nothing left this loop can repair
                }
            }
        }
    }

    let applied = repairs.iter().filter(|r| r.applied).count();
    if applied > 0
        && let Err(e) = write_atomic(path, &source)
    {
        return VerbOutput::env(format!("cannot write {path}: {e}"));
    }
    // The final truth is the NORMAL check of what is now on disk —
    // --fix is check plus a pen, never a different audit.
    let verdict = super::check::run(path, false, native_strict, model, theme);
    VerbOutput {
        text: format!("{}{}", summary(&repairs, applied, theme), verdict.text),
        code: verdict.code,
    }
}

/// Render the per-repair lines + the closing verdict line (applied count
/// or the honest nothing-applicable note).
fn summary(repairs: &[Repair], applied: usize, theme: Theme) -> String {
    let mut out = String::new();
    for r in repairs {
        if r.applied {
            let _ = writeln!(
                out,
                " {} {}  {} `{}` → `{}`",
                theme.paint(Role::Good, "✔"),
                theme.paint(Role::Strong, "FIX"),
                r.kind,
                r.old,
                r.new,
            );
        } else {
            let _ = writeln!(
                out,
                " {} {}  {} `{}` → `{}` skipped — `{}` is not unique in the file \
                 (a blind splice could rewrite the wrong site)",
                theme.paint(Role::Dim, "○"),
                theme.paint(Role::Strong, "FIX"),
                r.kind,
                r.old,
                r.new,
                r.old,
            );
        }
    }
    if applied == 0 {
        let _ = writeln!(
            out,
            " {} {}  no machine-applicable repairs (typed rename suggestions only \
             — structural findings stay yours)",
            theme.paint(Role::Dim, "○"),
            theme.paint(Role::Strong, "FIX"),
        );
    } else {
        let plural = if applied == 1 { "repair" } else { "repairs" };
        let _ = writeln!(
            out,
            " {} {}  {applied} {plural} applied · re-audit below",
            theme.paint(Role::Good, "✔"),
            theme.paint(Role::Strong, "FIX"),
        );
    }
    out
}

/// Publish the healed source ATOMICALLY: write a temp sibling, then one
/// `rename` (POSIX-atomic within a filesystem — the same contract the
/// `nika-fs` effect crate documents for `nika:write`). A crash or ENOSPC
/// mid-write leaves the ORIGINAL file untouched and at most a
/// `.nika-fix-tmp.*` sibling to sweep — never a truncated workflow. The
/// temp lands in the target's own directory (rename across filesystems
/// is not atomic); any failure removes it best-effort at this one site.
fn write_atomic(path: &str, contents: &str) -> std::io::Result<()> {
    let target = std::path::Path::new(path);
    let dir = target.parent().filter(|p| !p.as_os_str().is_empty());
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("write path has no file name"))?;
    let tmp_name = format!(".nika-fix-tmp.{}", name.to_string_lossy());
    let tmp = dir.map_or_else(
        || std::path::PathBuf::from(&tmp_name),
        |d| d.join(&tmp_name),
    );
    let publish = std::fs::write(&tmp, contents).and_then(|()| std::fs::rename(&tmp, target));
    if publish.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    publish
}

/// Splice `old` → `new` when `old` occurs EXACTLY ONCE in `source` as a
/// whole word (neighbors are non-word chars or the string edges). Records
/// the outcome either way; returns whether it applied.
fn splice(
    source: &mut String,
    old: &str,
    new: &str,
    kind: &'static str,
    repairs: &mut Vec<Repair>,
) -> bool {
    // An APPLIED token never re-applies. A SKIPPED one stays retryable:
    // an earlier round's splice can make it unique (the two-site case —
    // `buidl` in `depends_on` is ambiguous while `tasks.buidl` exists;
    // once the reference heals, the dependency token stands alone and
    // the next round heals it too). Convergence, not one-shot.
    if repairs
        .iter()
        .any(|r| r.applied && r.old == old && r.kind == kind)
    {
        return false;
    }
    let sites = word_sites(source, old);
    let applied = if let [at] = sites[..] {
        source.replace_range(at..at + old.len(), new);
        true
    } else {
        false
    };
    // One log row per (old, kind): a retry that succeeds UPGRADES its
    // earlier skip row (the summary reports final outcomes, not rounds).
    if let Some(row) = repairs.iter_mut().find(|r| r.old == old && r.kind == kind) {
        row.applied = row.applied || applied;
        new.clone_into(&mut row.new);
    } else {
        repairs.push(Repair {
            old: old.to_owned(),
            new: new.to_owned(),
            kind,
            applied,
        });
    }
    applied
}

/// Byte offsets where `needle` occurs in `hay` bounded by non-word
/// characters (or the ends) — `inpit` matches `inpit:` but never
/// `originpit`. Word chars: `[A-Za-z0-9_]` (the identifier alphabet every
/// spliceable token — field · arg key · `nika:` tool id — draws from;
/// `:` in a tool id is a non-word char, so the FULL `nika:raed` needle
/// still boundary-checks correctly at both ends).
fn word_sites(hay: &str, needle: &str) -> Vec<usize> {
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut sites = Vec::new();
    let mut from = 0;
    while let Some(pos) = hay[from..].find(needle) {
        let at = from + pos;
        let before_ok = at == 0 || !is_word(hay.as_bytes()[at - 1]);
        let end = at + needle.len();
        let after_ok = end >= hay.len() || !is_word(hay.as_bytes()[end]);
        if before_ok && after_ok {
            sites.push(at);
        }
        from = at + needle.len().max(1);
    }
    sites
}

/// The env-shaped refusals for `--fix` combinations the loop cannot
/// honor: stdin has no file to rewrite, `--json`'s `report_version`
/// contract is a single immutable audit, several files would interleave
/// rewrites with one summary.
#[must_use]
pub fn refuse(reason: &str) -> VerbOutput {
    VerbOutput {
        text: format!("check --fix: {reason}\n"),
        code: exit::ENV,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_sites_respects_boundaries_and_counts() {
        // whole-word only — substrings inside identifiers never match
        assert_eq!(word_sites("inpit: 1", "inpit"), vec![0]);
        assert_eq!(word_sites("originpit: 1", "inpit"), Vec::<usize>::new());
        assert_eq!(word_sites("a inpit b inpit", "inpit").len(), 2);
        // the full `nika:` tool id boundary-checks at both ends
        assert_eq!(word_sites("tool: \"nika:raed\"", "nika:raed"), vec![7]);
        // …and the `raed` HALF alone still bounds on the `:` (by design:
        // the splice always receives the full typed token, never a half)
        assert_eq!(word_sites("tool: \"nika:raed\"", "raed").len(), 1);
    }

    #[test]
    fn splice_applies_unique_and_skips_ambiguous() {
        let mut s = "invoke: { tool: \"nika:jq\", args: { inpit: 1 } }".to_owned();
        let mut log = Vec::new();
        assert!(splice(&mut s, "inpit", "input", "arg", &mut log));
        assert!(s.contains("input: 1") && !s.contains("inpit"));
        // ambiguous: two sites → untouched + logged skipped
        let mut s2 = "a: { promt: 1 }\nb: { promt: 2 }".to_owned();
        assert!(!splice(&mut s2, "promt", "prompt", "field", &mut log));
        assert!(s2.contains("promt: 1"), "ambiguous stays untouched");
        let skipped = log.iter().find(|r| r.old == "promt").expect("logged");
        assert!(!skipped.applied);
    }

    #[test]
    fn fix_converges_across_parse_and_check_levels() {
        // The battery's own author-error classes, stacked in one file:
        // a parse-fatal field typo (promt) + a tool typo (nika:raed) +
        // an arg typo (inpit). --fix heals all three across rounds and
        // the final audit is clean.
        let dir = std::env::temp_dir().join(format!("nika-fix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("broken.nika.yaml");
        std::fs::write(
            &path,
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\ntasks:\n  think:\n    infer: { promt: \"hi\", max_tokens: 10 }\n  read_it:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n  shape:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", inpit: 1 } }\n",
        )
        .expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        let healed = std::fs::read_to_string(&path).expect("re-read");
        assert!(healed.contains("prompt:"), "field healed: {healed}");
        assert!(healed.contains("nika:read"), "tool healed: {healed}");
        assert!(healed.contains("input: 1"), "arg healed: {healed}");
        assert!(
            out.text.contains("field `promt` → `prompt`"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("tool `nika:raed` → `nika:read`"),
            "{}",
            out.text
        );
        assert!(out.text.contains("arg `inpit` → `input`"), "{}", out.text);
        assert!(out.text.contains("3 repairs applied"), "{}", out.text);
        assert_eq!(out.code, exit::OK, "final audit is clean: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fix_without_applicable_repairs_leaves_the_file_alone() {
        // A structural finding (missing required arg) has no rename —
        // the file must be byte-identical after and the note honest.
        let dir = std::env::temp_dir().join(format!("nika-fix-noop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("structural.nika.yaml");
        let body =
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    invoke: { tool: \"nika:hash\" }\n";
        std::fs::write(&path, body).expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("re-read"),
            body,
            "no rewrite without an applied repair"
        );
        assert!(
            out.text.contains("no machine-applicable repairs"),
            "{}",
            out.text
        );
        assert_ne!(out.code, exit::OK, "the structural finding still reds");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dep_and_ref_renames_converge_across_rounds() {
        // The two-site convergence case the retryable-skip design exists
        // for: `buidl` occurs TWICE (bare in depends_on · inside the
        // `tasks.buidl` reference), so the dependency rename is ambiguous
        // in round 1 — but the fully-qualified reference rename
        // (`tasks.buidl` → `tasks.build`) is unique, applies, and leaves
        // the bare token standing alone for round 2. Both heal; a
        // one-shot skip would have left the file half-repaired.
        let dir = std::env::temp_dir().join(format!("nika-fix-conv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("two-site.nika.yaml");
        std::fs::write(
            &path,
            "nika: v1\nworkflow:\n  id: w\nvars: { topic: \"x\" }\ntasks:\n  build:\n    invoke: { tool: \"nika:log\", args: { message: \"building ${{ vars.topik }}\" } }\n  ship:\n    depends_on: [buidl]\n    invoke: { tool: \"nika:log\", args: { message: \"${{ tasks.buidl.output }}\" } }\n",
        )
        .expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        let healed = std::fs::read_to_string(&path).expect("re-read");
        assert!(
            healed.contains("depends_on: [build]"),
            "dep healed: {healed}"
        );
        assert!(
            healed.contains("tasks.build.output"),
            "ref healed: {healed}"
        );
        assert!(
            healed.contains("vars.topic"),
            "vars ref healed too: {healed}"
        );
        assert!(!healed.contains("buidl") && !healed.contains("topik"));
        assert!(
            out.text.contains("ref `tasks.buidl` → `tasks.build`"),
            "{}",
            out.text
        );
        assert!(out.text.contains("ref `buidl` → `build`"), "{}", out.text);
        assert!(
            out.text.contains("ref `vars.topik` → `vars.topic`"),
            "{}",
            out.text
        );
        assert!(!out.text.contains("skipped"), "no residual skip rows");
        assert_eq!(out.code, exit::OK, "clean after convergence: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fix_is_idempotent_and_atomic_publish_leaves_no_temp() {
        // Socratic hardening pins (2026-07-11): « what does --fix do when
        // there is NOTHING to do? » — the second run applies zero repairs
        // and leaves the file byte-identical (do-no-harm as a property,
        // not a hope). And the atomic publish never leaves its temp
        // sibling behind on the success path (`.nika-fix-tmp.*` is the
        // crash residue ONLY).
        let dir = std::env::temp_dir().join(format!("nika-fix-idem-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("idem.nika.yaml");
        std::fs::write(
            &path,
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n",
        )
        .expect("write fixture");
        let p = path.to_str().expect("utf8 path");
        let theme = Theme::new(false, true, false);
        let first = run(p, false, None, theme);
        assert!(first.text.contains("1 repair applied"), "{}", first.text);
        let healed = std::fs::read_to_string(&path).expect("re-read");
        // idempotence: the second run touches nothing
        let second = run(p, false, None, theme);
        assert!(
            second.text.contains("no machine-applicable repairs"),
            "{}",
            second.text
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("re-read 2"),
            healed,
            "second run is a byte-identical no-op"
        );
        // atomicity residue: no temp sibling survives a successful publish
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".nika-fix-tmp.")
            })
            .collect();
        assert!(leftovers.is_empty(), "temp residue: {leftovers:?}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_atomic_error_path_cleans_its_temp() {
        // The failure half of the atomic contract: renaming onto a path
        // whose parent DIRECTORY vanished fails — the original (absent)
        // target stays absent and the temp is swept, never leaked.
        let dir = std::env::temp_dir().join(format!("nika-fix-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let gone = dir.join("gone");
        std::fs::create_dir_all(&gone).expect("subdir");
        let target = gone.join("wf.nika.yaml");
        let target_str = target.to_str().expect("utf8").to_owned();
        std::fs::remove_dir_all(&gone).expect("vanish parent");
        assert!(
            write_atomic(&target_str, "nika: v1\n").is_err(),
            "no parent = publish fails"
        );
        // the temp would have lived in `gone/` — the whole dir is absent,
        // and the sweep must not have recreated anything under `dir`.
        let residue: Vec<_> = std::fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(Result::ok)
            .collect();
        assert!(residue.is_empty(), "no residue: {residue:?}");
    }
}
