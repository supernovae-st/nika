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

/// Quote one shell word for the taught line: bare when it is already a
/// safe word, single-quoted otherwise (embedded single quotes splice
/// through the POSIX `'\''` idiom — paste-able in sh/bash/zsh). A word
/// that STARTS with `=` is never bare: zsh's EQUALS expansion rewrites
/// `=name` to a command path or aborts the line. A later `=` is literal
/// in every shell, so `key=value` stays byte-identical.
#[must_use]
pub fn sh_word(word: &str) -> std::borrow::Cow<'_, str> {
    let safe = !word.is_empty()
        && !word.starts_with('=')
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_=./:@+-".contains(c));
    if safe {
        return std::borrow::Cow::Borrowed(word);
    }
    std::borrow::Cow::Owned(format!("'{}'", word.replace('\'', "'\\''")))
}

#[cfg(test)]
mod tests {
    use super::sh_word;
    use std::borrow::Cow;

    /// What sh, bash and zsh all read back from one rendered word: `'…'`
    /// is literal, `\'` outside quotes is one quote, and a bare character
    /// must be one no shell expands — including zsh's EQUALS rule, which
    /// rewrites a word that STARTS with a bare `=`. `None` = unsafe.
    fn shell_reads(rendered: &str) -> Option<String> {
        const SPECIAL: &str = "|&;<>()$`\"' \t\n*?[]#~%!{}";
        if rendered.starts_with('=') {
            return None;
        }
        let mut out = String::new();
        let mut chars = rendered.chars();
        while let Some(c) = chars.next() {
            match c {
                '\'' => loop {
                    match chars.next()? {
                        '\'' => break,
                        inner => out.push(inner),
                    }
                },
                '\\' => match chars.next()? {
                    '\'' => out.push('\''),
                    _ => return None,
                },
                bare if SPECIAL.contains(bare) => return None,
                bare => out.push(bare),
            }
        }
        Some(out)
    }

    #[test]
    fn a_safe_word_stays_bare_and_borrowed() {
        for word in [
            "./out/draft.nika",
            "page=wifi",
            "openai/gpt-5.2",
            "a@b:c+d-e_f",
        ] {
            assert!(
                matches!(sh_word(word), Cow::Borrowed(same) if same == word),
                "{word}"
            );
        }
    }

    #[test]
    fn an_unsafe_word_is_quoted_and_reads_back_verbatim() {
        for word in [
            "",
            "two words",
            "it's",
            "''",
            "$(rm -rf ~)",
            "`id`",
            "a;b|c&d",
            "*.nika",
            "tab\there",
            "line\nbreak",
            "say \"hi\"",
            "back\\slash",
            "~/draft.nika",
            "café.nika",
            "#comment",
            "!history{a,b}",
        ] {
            let rendered = sh_word(word);
            assert!(rendered.starts_with('\''), "{word:?} stayed bare");
            assert_eq!(shell_reads(&rendered).as_deref(), Some(word), "{rendered}");
        }
    }

    #[test]
    fn every_ascii_character_reads_back_verbatim() {
        for c in (1u8..128).map(char::from) {
            // Mid-word and first position: zsh judges the leading one alone.
            for word in [format!("a{c}b"), format!("{c}b")] {
                let rendered = sh_word(&word);
                assert_eq!(shell_reads(&rendered).as_deref(), Some(&*word), "{word:?}");
            }
        }
    }

    #[test]
    fn a_leading_equals_is_quoted_and_a_later_one_stays_bare() {
        // zsh rewrites `=ls` to the path of ls and aborts on `=value`.
        assert_eq!(sh_word("=ls"), "'=ls'");
        assert_eq!(sh_word("=value"), "'=value'");
        assert_eq!(sh_word("=draft.nika.yaml"), "'=draft.nika.yaml'");
        assert_eq!(sh_word("="), "'='");
        assert_eq!(sh_word("==x"), "'==x'");
        // A later `=` is literal in every shell: the carry stays byte-identical.
        for word in ["page=wifi", "a==b", "k="] {
            assert!(
                matches!(sh_word(word), Cow::Borrowed(same) if same == word),
                "{word}"
            );
        }
    }

    #[test]
    fn the_embedded_quote_splices_through_the_posix_idiom() {
        assert_eq!(sh_word("it's"), r"'it'\''s'");
        assert_eq!(sh_word(""), "''");
    }
}
