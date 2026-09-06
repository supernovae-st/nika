// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verb-outcome contract shared by both members of the `nika-cli`
//! unit — exit codes (spec §4 · LOCKED), [`VerbOutput`], and the OSC-8
//! link seam. Lifted verbatim from `nika-cli::verbs` at the ADR-110
//! split; `nika-cli` re-exports these at their historical paths.

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
    /// The operator CANCELLED the run (Ctrl-C · SIGTERM · #1438): in-flight
    /// work completed and was counted, the unstarted tasks settled as
    /// cancelled, the trace ends with `workflow_cancelled`. 128 + SIGINT ·
    /// the code every shell and CI reader already treats as « interrupted »
    /// · never the WORKFLOW failure code (a decision is not a defect).
    pub const CANCELLED: u8 = 130;
    /// The journal never reached a terminal frame (`trace verify` · ADR-129
    /// · #1442): the chain holds, the lifecycle end is unattested — a run
    /// in flight or a writer that died. Non-zero on purpose: a monitor
    /// wired on the exit code must never green a dead run. Distinct from
    /// FILE (a broken or forged chain) and ENV (a missing input).
    pub const INCOMPLETE: u8 = 5;
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
    #[must_use]
    pub fn ok(text: String) -> Self {
        Self {
            text,
            code: exit::OK,
        }
    }

    #[must_use]
    pub fn file(text: String) -> Self {
        Self {
            text,
            code: exit::FILE,
        }
    }

    #[must_use]
    pub fn env(text: String) -> Self {
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
#[must_use]
pub fn link_host() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_default()
}

/// Did the terminal PROVE truecolor (`COLORTERM=truecolor|24bit`)?
/// Presentation env — the same scoped exemption as [`link_host`].
#[allow(clippy::disallowed_methods)]
#[must_use]
pub fn truecolor_env() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit")
}

/// Render one on-disk path as an OSC-8 `file://` hyperlink when the
/// theme's `links` capability resolved on — the TEXT stays the path the
/// verb already prints (byte-identical registers when links are off).
/// A path that will not canonicalize (deleted mid-run · unsaved) stays
/// plain: a link that cannot open is worse than no link.
#[must_use]
pub fn linked_path(theme: crate::Theme, path: &str) -> String {
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
