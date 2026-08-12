// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check --fix` — apply the machine-applicable renames and converge.
//!
//! The verb keeps the I/O and the final `check` verdict here; the repair
//! LADDER (the dead-form arms · the splice machinery · the summary)
//! descended to [`nika_cli_host::fix_ladder`] at the 15k wall (ADR-110 ·
//! one architectural unit, two members).

use nika_cli_host::fix_ladder::{
    MAX_ROUNDS, Repair, StopNotes, apply_dead_form_arm, collect_typed_renames, render_stops,
    splice, summary, try_w2_hoist,
};
use nika_schema::{ParseMode, SchemaError};

use crate::display::theme::Theme;
use crate::verbs::{VerbOutput, exit};

/// The `nika check <file> --fix` verb. Single real file only (the caller
/// refuses stdin and multi-file — a rewrite needs a place to write).
#[must_use]
pub fn run(path: &str, native_strict: bool, model: Option<&str>, theme: Theme) -> VerbOutput {
    let Ok(original) = std::fs::read_to_string(path) else {
        return VerbOutput::env(format!("cannot read {path}"));
    };
    let mut source = original.clone();
    let mut repairs: Vec<Repair> = Vec::new();
    let mut stop_notes = StopNotes(Vec::new());

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
            Err(e) => match apply_dead_form_arm(&e, &mut source, &mut repairs, &mut stop_notes) {
                Some(true) => {}             // a migration applied — the round restarts
                Some(false) | None => break, // STOP, or not rename-shaped — check will tell
            },
            Ok(wf) => {
                let report = nika_check::check(&wf);
                if let Some(stop_or_continue) =
                    try_w2_hoist(&report, &mut source, &mut repairs, &mut stop_notes)
                {
                    if stop_or_continue {
                        continue; // re-parse + re-check the hoisted form
                    }
                    break;
                }
                // Collect this round's typed renames FIRST (splicing
                // invalidates nothing — each token is unique by the gate).
                for (old, new, kind) in collect_typed_renames(&report) {
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
        && let Err(e) = nika_migrate::repair::write_atomic(path, &source)
    {
        return VerbOutput::env(format!("cannot write {path}: {e}"));
    }
    // The final truth is the NORMAL check of what is now on disk —
    // --fix is check plus a pen, never a different audit.
    let verdict = super::check::run(path, false, native_strict, model, theme);
    let stops = render_stops(&stop_notes, theme);
    VerbOutput {
        text: format!(
            "{}{}{}",
            summary(&repairs, applied, theme),
            stops,
            verdict.text
        ),
        code: verdict.code,
    }
}

/// The env-shaped refusals for `--fix` combinations the loop cannot
/// honor (stdin · `--json`'s immutable audit · multi-file).
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
            "nika: w\nmodel: mock/echo\npermits: { tools: [\"nika:read\", \"nika:jq\"], fs: { read: [\"./x\"] } }\ntasks:\n  think:\n    infer: { promt: \"hi\", max_tokens: 10 }\n  read_it:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n  shape:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", inpit: 1 } }\n",
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
        let body = "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:hash\" }\n";
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
        // for: `buidl` occurs TWICE (bare as an `after:` control-edge
        // target · inside the qualified `tasks.buidl` outputs reference),
        // so the bare rename is ambiguous in round 1 — but the
        // fully-qualified reference rename (`tasks.buidl` →
        // `tasks.build`) is unique, applies, and leaves the bare token
        // standing alone for round 2. Both heal; a one-shot skip would
        // have left the file half-repaired.
        let dir = std::env::temp_dir().join(format!("nika-fix-conv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("two-site.nika.yaml");
        std::fs::write(
            &path,
            "nika: w\npermits: { tools: [\"nika:log\"] }\ninputs: { topic: { type: string, required: true } }\ntasks:\n  build:\n    invoke: { tool: \"nika:log\", args: { message: \"building ${{ inputs.topik }}\" } }\n  ship:\n    after:\n      buidl: success\n    invoke: { tool: \"nika:log\", args: { message: \"shipping\" } }\noutputs:\n  made: ${{ tasks.buidl.output }}\n",
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
            healed.contains("build: success"),
            "control edge healed: {healed}"
        );
        assert!(
            healed.contains("tasks.build.output"),
            "ref healed: {healed}"
        );
        assert!(
            healed.contains("inputs.topic"),
            "inputs ref healed too: {healed}"
        );
        assert!(!healed.contains("buidl") && !healed.contains("topik"));
        assert!(
            out.text.contains("ref `tasks.buidl` → `tasks.build`"),
            "{}",
            out.text
        );
        assert!(out.text.contains("ref `buidl` → `build`"), "{}", out.text);
        assert!(
            out.text.contains("ref `inputs.topik` → `inputs.topic`"),
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
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n",
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
    fn r5_dead_predicates_respell_and_converge_green() {
        // The R5 flag-day repair loop: a pre-R5 file (after: succeeded ·
        // after: failed across flow + block forms) is refused NIKA-DAG-005
        // at parse — --fix respells both spellings in ONE pass and the
        // final audit is clean (the codemod is whole-document, not
        // one-per-round).
        let dir = std::env::temp_dir().join(format!("nika-fix-r5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("prer5.nika.yaml");
        std::fs::write(
            &path,
            "nika: w\npermits: { exec: [\"true\"] }\ntasks:\n  build:\n    exec: { command: [\"true\"] }\n  test:\n    after: { build: succeeded }\n    exec: { command: [\"true\"] }\n  notify:\n    after:\n      test: failed\n    exec: { command: [\"true\"] }\n",
        )
        .expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        let healed = std::fs::read_to_string(&path).expect("re-read");
        assert!(healed.contains("after: { build: success }"), "{healed}");
        assert!(healed.contains("test: failure"), "{healed}");
        assert!(
            !healed.contains("succeeded") && !healed.contains(": failed"),
            "{healed}"
        );
        assert!(out.text.contains("r5-predicates"), "{}", out.text);
        assert_eq!(out.code, exit::OK, "clean after the respell: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_predicate_has_no_mechanical_repair() {
        // `passed` is NOT a dead spelling — the codemod stays out and the
        // closed-set teaching renders; the file is never guessed at.
        let dir = std::env::temp_dir().join(format!("nika-fix-r5noop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("unknown.nika.yaml");
        let body = "nika: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n  d:\n    after: { t: passed }\n    exec: { command: [\"true\"] }\n";
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
            "no rewrite without an applicable repair"
        );
        assert_ne!(out.code, exit::OK, "the unknown predicate still reds");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn esplit_migrates_the_dead_vars_block_and_converges_green() {
        // The C2 flag-day repair loop: a pre-C2 file (a `vars:` block +
        // `${{ vars.X }}` reads) is refused NIKA-VALUES-001 at parse —
        // --fix classifies the block into inputs:/const:, rewrites the
        // refs class-aware, and the final audit is clean.
        let dir = std::env::temp_dir().join(format!("nika-fix-esplit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("prec2.nika.yaml");
        std::fs::write(
            &path,
            "nika: w\nmodel: mock/echo\nvars:\n  topic:\n    type: string\n    required: true\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: \"${{ vars.topic }} · up to ${{ vars.retries }}\" }\n",
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
            healed.contains("inputs:\n  topic:\n    type: string\n    required: true"),
            "{healed}"
        );
        assert!(healed.contains("const:\n  retries: 3"), "{healed}");
        assert!(
            healed.contains("${{ inputs.topic }}") && healed.contains("${{ const.retries }}"),
            "{healed}"
        );
        assert!(
            !healed.contains("vars:") && !healed.contains("vars."),
            "{healed}"
        );
        assert!(out.text.contains("esplit"), "{}", out.text);
        assert_eq!(out.code, exit::OK, "clean after the E-split: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn esplit_stop_leaves_the_file_untouched_and_names_the_entry() {
        // Atomic-or-nothing: a credential-shaped entry is outside the
        // ratified rules — the file is NOT written and the diagnostic
        // names the entry (never guess).
        let dir = std::env::temp_dir().join(format!("nika-fix-estop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("stop.nika.yaml");
        let body = "nika: w\nvars:\n  api_token: abc123\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n";
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
            "atomic-or-nothing: a STOP writes nothing"
        );
        assert!(out.text.contains("STOP"), "{}", out.text);
        assert!(out.text.contains("vars.api_token"), "{}", out.text);
        assert_ne!(out.code, exit::OK, "the dead form still reds: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn w1_flow_items_converge_and_w2_follows_in_the_same_run() {
        // Issue #645 — the exact repro bytes: a W2 list-form file whose
        // FIRST item is single-line flow. The old codemod mapped the
        // second item but left the flow one a sequence entry — a mixed
        // collection, invalid YAML, and the loop stalled on an
        // intermediate no pass could parse. Now the whole list maps in
        // the same pass as the envelope (the flow item expands to block
        // entries), the next round's W2 migration drops the depends_on,
        // and the written file parses.
        let dir = std::env::temp_dir().join(format!("nika-fix-i645-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("w2-list.nika.yaml");
        std::fs::write(
            &path,
            "nika: daily-brief\nmodel: ollama/llama3.2:3b\ntasks:\n  - { id: notes, invoke: { tool: \"nika:read\", args: { path: ./notes/today.md } } }\n  - id: triage\n    depends_on: [notes]\n    with:\n      notes: ${{ tasks.notes.output }}\n    infer: { prompt: \"triage ${{ with.notes }}\" }\n",
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
            healed.contains(
                "  notes:\n    invoke: { tool: \"nika:read\", args: { path: ./notes/today.md } }\n"
            ),
            "the flow item expanded to block entries: {healed}"
        );
        assert!(healed.contains("  triage:\n"), "{healed}");
        assert!(
            !healed.contains("- id:") && !healed.contains("- { id:"),
            "{healed}"
        );
        assert!(
            !healed.contains("depends_on"),
            "the W2 round followed through: {healed}"
        );
        assert!(
            nika_schema::parse(&healed, nika_schema::FileId::new(0), ParseMode::Strict).is_ok(),
            "the written file parses strict: {healed}"
        );
        assert!(
            !out.text.contains("NIKA-PARSE-001"),
            "no unparseable intermediate survives: {}",
            out.text
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_list_with_a_non_mechanical_item_is_never_half_mapped() {
        // Atomicity: the second item's `id:` does not lead, so the WHOLE
        // list stays a sequence — the written file is valid YAML and the
        // re-audit names the dead form (never a mixed collection).
        let dir = std::env::temp_dir().join(format!("nika-fix-i645atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("atomic.nika.yaml");
        std::fs::write(
            &path,
            "nika: t\ntasks:\n  - { id: a, exec: { command: [\"true\"] } }\n  - { exec: { command: [\"true\"] }, id: b }\n",
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
            healed.contains("  - { id: a, exec: { command: [\"true\"] } }"),
            "the convertible item stays too — all or nothing: {healed}"
        );
        assert!(
            out.text.contains("NIKA-PARSE-022"),
            "the sequence teaching stands: {}",
            out.text
        );
        assert!(
            !out.text.contains("NIKA-PARSE-001"),
            "valid YAML throughout: {}",
            out.text
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn d1_string_command_migrates_and_the_file_converges_green() {
        // Issue #572 — the finding whose repair IS mechanical: the
        // 0.102 string command. The inert bare form lands the argv
        // rewrite; the re-audit is clean.
        let dir = std::env::temp_dir().join(format!("nika-fix-i572-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("string-cmd.nika.yaml");
        std::fs::write(
            &path,
            "nika: d1\npermits:\n  exec: [\"echo\"]\ntasks:\n  a:\n    exec:\n      command: echo hello\n",
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
            healed.contains("      command: [\"echo\", \"hello\"]\n"),
            "the inert bare string became argv: {healed}"
        );
        assert_eq!(out.code, exit::OK, "clean after the split: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn d1_shell_meta_renames_to_shell_and_the_teaching_moves_on() {
        // The same finding with shell-meta: the key rename preserves
        // the old implicit-shell semantics byte-for-byte.
        let dir = std::env::temp_dir().join(format!("nika-fix-i572sh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("shell-cmd.nika.yaml");
        std::fs::write(
            &path,
            "nika: d1sh\npermits:\n  exec: [\"sh\"]\ntasks:\n  a:\n    exec:\n      command: echo a | grep b\n",
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
            healed.contains("      shell: echo a | grep b\n"),
            "verbatim under shell:: {healed}"
        );
        assert!(
            !out.text.contains("argv-only"),
            "the D1 refusal is repaired: {}",
            out.text
        );
        let _ = std::fs::remove_file(&path);
    }
}
