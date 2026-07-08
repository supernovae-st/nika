// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The static verb suite — everything auditable BEFORE a run (spec §2).
//!
//! Every verb here is a pure function `(args, file) → (text, exit code)`
//! over the shipped engine layers (`nika-schema` ladder · `nika-error`
//! registry · `nika-pack` embedded surface). No network, no effects, no
//! runtime — the `run` verb arrives with L3 and is refused honestly until
//! then. The bin (`main.rs`) stays a thin dispatcher so every surface is
//! testable as a library call.

pub mod catalog;
pub mod check;
pub mod dap;
pub mod doctor;
pub mod explain;
pub mod explain_file;
pub mod graph;
pub mod init;
pub mod inspect;
pub mod new;
pub mod pack_surface;
pub(crate) mod probe;
pub mod run;
pub mod test;
pub mod tools;
pub mod trace;
pub mod trace_otel;
pub mod trace_reproduce;
pub mod trace_verify;
pub mod welcome;
pub mod wire;

use nika_schema::check::CheckReport;
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode};

/// Exit-code contract (spec §4 · LOCKED · additive-only forever).
pub mod exit {
    /// Success (run completed · check clean · verb done).
    pub const OK: u8 = 0;
    /// A workflow RAN and FAILED (a task failed unrecovered · `nika run`
    /// only · distinct from a static FILE finding · spec §4).
    pub const WORKFLOW: u8 = 1;
    /// Validation findings — the FILE has errors (CI gates on this).
    pub const FILE: u8 = 2;
    /// Environment error — config · I/O · missing resource.
    pub const ENV: u8 = 3;
    /// The run PAUSED on a blocking `nika:prompt` (ADR-099 rider · run
    /// state `paused` · additive per the locked contract). NOT a
    /// failure — but non-zero on purpose: `nika run … && next` must not
    /// proceed past an unanswered human gate. Resume with
    /// `--resume <trace> --answer <task>=<value>`.
    pub const PAUSED: u8 = 4;
}

/// One verb invocation's outcome: the text to print + the exit code.
#[derive(Debug)]
pub struct VerbOutput {
    /// Human or machine text (the caller owns the stream choice).
    pub text: String,
    /// Spec §4 exit code.
    pub code: u8,
}

impl VerbOutput {
    fn ok(text: String) -> Self {
        Self {
            text,
            code: exit::OK,
        }
    }

    fn file(text: String) -> Self {
        Self {
            text,
            code: exit::FILE,
        }
    }

    fn env(text: String) -> Self {
        Self {
            text,
            code: exit::ENV,
        }
    }
}

/// Best-effort hostname for `file://` links (iTerm2 opens them only when
/// the host names this machine). Env-only — no libc, no subprocess; an
/// empty host degrades to RFC 8089 localhost. The read is presentation
/// state, not a secret — the same scoped exemption as `env_flag` in
/// `main.rs` (the workspace `disallowed_methods` ban routes SECRET reads
/// through the kernel vault seam, which has no business here).
#[allow(clippy::disallowed_methods)]
pub(crate) fn link_host() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_default()
}

/// Did the terminal PROVE truecolor (`COLORTERM=truecolor|24bit`)?
/// Presentation env — the same scoped exemption as [`link_host`].
#[allow(clippy::disallowed_methods)]
pub(crate) fn truecolor_env() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit")
}

/// Render one on-disk path as an OSC-8 `file://` hyperlink when the
/// theme's `links` capability resolved on — the TEXT stays the path the
/// verb already prints (byte-identical registers when links are off).
/// A path that will not canonicalize (deleted mid-run · unsaved) stays
/// plain: a link that cannot open is worse than no link.
pub(crate) fn linked_path(theme: crate::Theme, path: &str) -> String {
    if !theme.links {
        return path.to_owned();
    }
    match std::fs::canonicalize(path) {
        Ok(abs) => {
            let url = crate::display::format::file_url(&link_host(), &abs.to_string_lossy());
            theme.link(&url, path)
        }
        Err(_) => path.to_owned(),
    }
}

/// Read + strict-parse + ladder-check one workflow file. The Unix dash
/// (`-`) reads stdin — the editor wire: a dirty buffer pipes straight
/// in, no tmp-file dance; every verb on this seam (check · graph ·
/// inspect · test) inherits it.
///
/// Failure mapping per spec §4: unreadable = environment (`3`) · parse
/// error = a finding in the FILE (`2`).
pub(crate) fn load_checked(path: &str) -> Result<(RawWorkflow, CheckReport), VerbOutput> {
    let (_, wf, report) = load_checked_with_source(path)?;
    Ok((wf, report))
}

/// [`load_checked`] plus the raw YAML — the painted-diagnostics surface
/// (check) frames findings on the source, so the text it was parsed
/// from rides along instead of being read twice. Carries the Unix-dash
/// contract: `-` reads stdin (the frames then label the origin `-`).
pub(crate) fn load_checked_with_source(
    path: &str,
) -> Result<(String, RawWorkflow, CheckReport), VerbOutput> {
    let yaml = if path == "-" {
        use std::io::Read as _;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| VerbOutput::env(format!("cannot read stdin: {e}")))?;
        buf
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))?
    };
    let wf = nika_schema::parse(&yaml, FileId::new(0), ParseMode::Strict)
        .map_err(|e| VerbOutput::file(format!("PARSE ✗  [{}] {e}", e.spec_code())))?;
    let report = nika_schema::check(&wf);
    Ok((yaml, wf, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pipe-parity pin: with the `links` capability OFF (every sober
    /// register), `linked_path` returns the path VERBATIM — zero escapes.
    /// With it on, the OSC-8 wrapper carries a `file://` URL and keeps
    /// the printed text unchanged; a path that will not canonicalize
    /// stays plain (a dead link is worse than no link).
    #[test]
    fn linked_path_is_byte_identical_when_links_are_off() {
        let dir = std::env::temp_dir().join("nika-cli-linkedpath-tests");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let file = dir.join("wf.nika.yaml");
        std::fs::write(&file, "nika: v1\n").expect("fixture");
        let path = file.to_str().expect("utf8 path");

        let plain = crate::Theme::new(false, false, false);
        assert_eq!(linked_path(plain, path), path, "sober register: verbatim");

        let mut linked = crate::Theme::new(false, false, false);
        linked.links = true;
        let out = linked_path(linked, path);
        assert!(
            out.starts_with("\x1b]8;;file://") && out.ends_with("\x1b]8;;\x1b\\"),
            "OSC-8 wrapper: {out:?}"
        );
        assert!(out.contains(path), "the printed text stays the path");

        let ghost = dir.join("never-written.yaml");
        let ghost = ghost.to_str().expect("utf8 path");
        assert_eq!(linked_path(linked, ghost), ghost, "no file → no link");
    }

    /// Regression · a parse-stage rejection must surface its spec wire code,
    /// exactly like the CONFORM stage. The multiple-verbs short-circuit used
    /// to render `PARSE ✗ <msg>` with no `[NIKA-PARSE-009]`, so an operator
    /// could not `nika explain` the failure (every other finding shows its
    /// code). `load_checked` now formats `e.spec_code()`.
    #[test]
    fn parse_error_carries_its_spec_code() {
        let path =
            std::env::temp_dir().join(format!("nika-parsecode-{}.nika.yaml", std::process::id(),));
        std::fs::write(
            &path,
            "nika: v1\nworkflow: two-verbs\nmodel: mock/echo\ntasks:\n  - id: a\n    infer: { prompt: \"x\" }\n    exec: { run: \"echo hi\" }\n",
        )
        .expect("fixture written");
        let err = load_checked(path.to_str().expect("utf-8 tmp path"))
            .expect_err("a task with two verbs must fail to parse");
        std::fs::remove_file(&path).ok();
        assert_eq!(err.code, exit::FILE, "{}", err.text);
        assert!(
            err.text.contains("NIKA-PARSE-009"),
            "parse error must carry its spec code · got: {}",
            err.text
        );
    }
}
