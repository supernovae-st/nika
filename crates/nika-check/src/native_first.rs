// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native-first hints — `exec:` with a probable native path
//! (the `native-first/*` preference ruleset · spec 03 §native-first).
//!
//! An agent authoring a workflow reaches for the shell because the
//! shell always works — and `nika check` stayed silent about the
//! builtin that would have made the task portable, auditable and
//! sandboxed. This pass names the native path. Empirical trigger: a
//! generated site-asset workflow shipped FOUR avoidable `exec` tasks
//! (curl crawl · node upload helper · shell jq · curl image API) and
//! checked clean.
//!
//! Six deterministic rules, each keyed on LITERAL command fragments
//! (`RawCommand::argv_program` / leading shell token / fragments — a
//! templated head makes no claim):
//!
//! - `native-first/001 exec-http` — `curl`/`wget`/`xh`/`http(s)` (or an
//!   interpreter one-liner around `fetch(`/`axios`/`http.request`) →
//!   `nika:fetch` (uploads: `multipart:` · crawls: `traverse:`).
//! - `native-first/002 exec-file` — `cat`/`tee`/`cp`/`mv`/`mkdir`/
//!   `touch`/`head`/`tail`/`ls` → `nika:read`/`nika:write`/`nika:glob`.
//! - `native-first/003 exec-data` — `jq`/`sed`/`awk` → `nika:jq` (or an
//!   `output:` binding — the same jq engine, zero subprocess).
//! - `native-first/004 exec-media` — an image/speech provider endpoint
//!   in the command → `nika:image_generate`/`nika:tts_generate`.
//! - `native-first/006 exec-utility` — a utility with an EXACT builtin
//!   (`sleep`→`nika:wait` · `date`→`nika:date` · `uuidgen`→`nika:uuid` ·
//!   `sha256sum`→`nika:hash` · `yq`→`nika:convert` · `grep`→`nika:grep` ·
//!   `find`→`nika:glob`) — names the builtin AND its
//!   argument shape, because the mapping is one to one.
//! - `native-first/005 exec-helper` — an interpreter running a script
//!   file → inventory the helper (HTTP→fetch · files→read/write ·
//!   JSON→jq · product APIs→an MCP server) and keep only a genuine
//!   subprocess, recorded in the exec ledger.
//!
//! ADVISORY like every hint (`is_clean` ignores them) — the strict
//! posture is the CLI's: `nika check --native-strict` fails on any
//! `native-first` hint. Every shell segment is judged and a task keeps
//! every matching hint; within one segment the most specific rule wins
//! (helper ≻ media ≻ http ≻ file ≻ utility ≻ data). `nika run …` nested
//! invocations are never flagged (the sanctioned composition path).

use nika_schema::raw::{RawAction, RawCommand, RawWorkflow};

use super::hints::Hint;

/// The hint kind (agents route on it · `hints.rs` kind registry).
const KIND: &str = "native-first";

/// HTTP client programs (001).
const HTTP_PROGRAMS: [&str; 5] = ["curl", "wget", "xh", "http", "https"];
/// File-op programs (002) — the SAME list the B05 host-dump door walks
/// ([`nika_cap::FILE_PLUMBING_PROGRAMS`]), so a hint and a finding cannot
/// disagree about whether `cat` is file plumbing.
const FILE_PROGRAMS: &[&str] = nika_cap::FILE_PLUMBING_PROGRAMS;
/// Data-transform programs (003).
const DATA_PROGRAMS: [&str; 3] = ["jq", "sed", "awk"];
/// Script interpreters (005 · and the 001 one-liner carve-in).
const INTERPRETERS: [&str; 12] = [
    "node", "python", "python3", "pythonw", "bash", "sh", "dash", "zsh", "deno", "bun", "ruby",
    "perl",
];
/// Script-file suffixes an interpreter head promotes to 005.
const SCRIPT_SUFFIXES: [&str; 8] = [".mjs", ".cjs", ".js", ".ts", ".py", ".sh", ".rb", ".pl"];
/// Utility programs with an EXACT builtin counterpart (006).
///
/// The other four program families answer « what KIND of work is this »
/// and hand back a small catalogue. These rows are 1:1, so the advice
/// can name the builtin AND its argument shape — an author who typed
/// `sleep 3` does not need a menu, they need `duration: "3s"`.
///
/// Empirical trigger, measured 2026-08-20 against 0.111.0: ten utility
/// programs put through `nika check`, eight of them silent. Every one
/// of the eight had an exact builtin. The 005 helper advice, which is
/// what an author usually lands on, names five builtins and the catalog
/// holds twenty-eight — so `nika:wait` was unreachable from any hint,
/// and a three-second `sleep` survived in the studio's own radar beat
/// for as long as the beat existed, carrying a `permits.exec` grant
/// that existed for nothing else.
///
/// `echo` was in this table for one draft and came out. Its answer is
/// not a builtin — a literal value belongs in `const:` — and the table's
/// contract is an EXACT builtin counterpart. It also proved the cost of
/// breaking that contract immediately: three fixtures in this repo use
/// `echo` as the canonical harmless command, and under `--native-strict`
/// a hint is a refusal, so the row turned the placeholder every author
/// reaches for into a failure. A rule that fires on the universal
/// do-nothing command is friction, not teaching.
///
/// BARE HEAD ONLY, like 001/002/003: `./scripts/date` is the author's
/// own tool that merely shares a utility's name.
const UTILITY_PROGRAMS: [(&str, &str); 13] = [
    (
        "sleep",
        "a pause is `nika:wait` (`duration: \"3s\"`, or an absolute `until:`) — and it is \
         not an exec, so the order law that forbids a shell downstream of a net-effecting \
         task stops applying and the pause may finally sit where the intent wants it",
    ),
    (
        "date",
        "a timestamp is `nika:date` (`op: now|add|subtract|format|parse|diff` · strftime \
         grammar in, ISO 8601 out) — portable across platforms, where `date` is not",
    ),
    (
        "uuidgen",
        "an identifier is `nika:uuid` (`version: v7` sortable by default, `v4` random)",
    ),
    (
        "sha256sum",
        "a digest is `nika:hash` (`algo: blake3|sha256|sha512` · hex or base64) — in-process, \
         no subprocess and no platform-dependent output format",
    ),
    (
        "shasum",
        "a digest is `nika:hash` (`algo: sha256` · hex or base64)",
    ),
    (
        "md5sum",
        "a digest is `nika:hash` — and pick `blake3` or `sha256` while you are here",
    ),
    (
        "sha1sum",
        "a digest is `nika:hash` — and pick `blake3` or `sha256` while you are here",
    ),
    (
        "b3sum",
        "a digest is `nika:hash` (`algo: blake3` is the default)",
    ),
    (
        "yq",
        "YAML/TOML/CSV in or out is `nika:convert` (`from:`/`to:`), then `nika:jq` for the \
         shaping — one data language instead of two",
    ),
    (
        "grep",
        "a recursive regex search is `nika:grep` — returns `{path,line,match}` sorted, already structured",
    ),
    (
        "rg",
        "a recursive regex search is `nika:grep` — returns `{path,line,match}` sorted, already structured",
    ),
    (
        "ag",
        "a recursive regex search is `nika:grep` — returns `{path,line,match}` sorted, already structured",
    ),
    (
        "find",
        "a path listing is `nika:glob` (lexicographic, with `exclude:`) — for a traversal that \
         also RUNS something per hit, keep the subprocess and record it in the exec ledger",
    ),
];

/// Media provider endpoint markers (004) — endpoint fragments, not
/// bare words (a prompt mentioning "tts" must not fire).
const MEDIA_MARKERS: [&str; 4] = [
    "images/generations",
    "images/edits",
    "/v1/audio/speech",
    "api.elevenlabs.io",
];
/// HTTP one-liner markers inside interpreter code (001 carve-in).
const HTTP_CODE_MARKERS: [&str; 8] = [
    "fetch(",
    "axios",
    "http.request",
    "https.request",
    "requests.get(",
    "requests.post(",
    "urlopen(",
    "httpx.",
];

/// Scan every `exec:` task (and `on_finally` cleanups) for a native
/// path — every matching shell segment keeps its own site.
pub(super) fn scan(wf: &RawWorkflow) -> Vec<Hint> {
    let mut hints = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        push_native_first(&task.value.action, id, &mut hints);
    }
    hints
}

fn push_native_first(action: &RawAction, id: &str, hints: &mut Vec<Hint>) {
    let RawAction::Exec(exec) = action else {
        return;
    };
    for (rule, advice) in classify_all(&exec.command) {
        hints.push(Hint {
            kind: KIND,
            code: Some(rule),
            task: id.to_owned(),
            advice: format!("{rule} · {advice}"),
        });
    }
}

/// The rule ladder — most specific first inside each command segment.
/// Returns every stable rule id (`native-first/00N`) + advice body in
/// source order. ONE truth:
/// the check hint AND the reference linter ruleset
/// (`lints::native_first`) both classify HERE.
#[must_use]
pub fn classify_all(command: &RawCommand) -> Vec<(&'static str, String)> {
    command_segments(command)
        .into_iter()
        .filter_map(|segment| classify_segment(&segment))
        .collect()
}

/// Classify the first segment with a probable native path.
///
/// Kept as the compatibility surface for embedders that consumed the
/// original single-verdict API. New consumers should use
/// [`classify_all`] so later shell segments are never hidden.
#[must_use]
pub fn classify(command: &RawCommand) -> Option<(&'static str, String)> {
    classify_all(command).into_iter().next()
}

#[derive(Debug)]
struct CommandSegment {
    head: String,
    pathed: bool,
    fragments: Vec<String>,
}

fn classify_segment(segment: &CommandSegment) -> Option<(&'static str, String)> {
    let head = segment.head.as_str();
    let pathed = segment.pathed;
    if head == "nika" {
        return None; // nested `nika run …` — the sanctioned composition
    }
    let has_marker = |markers: &[&str]| {
        segment
            .fragments
            .iter()
            .any(|f| markers.iter().any(|m| f.contains(m)))
    };
    let is_interpreter = INTERPRETERS.contains(&head);

    // 005 · interpreter + script file — the helper umbrella.
    if is_interpreter
        && segment
            .fragments
            .iter()
            .any(|f| !f.contains("${{") && SCRIPT_SUFFIXES.iter().any(|suffix| f.ends_with(suffix)))
    {
        return Some(helper_script_hint(head));
    }
    // 004 · a media provider endpoint in the command. Bare head only —
    // a PATHED head (`./deploy.sh …`) is the author's own tool and must
    // not be second-guessed because a media-provider domain appears in
    // its args, exactly as 001/002/003 suppress a pathed head (the
    // rust-pro review's F2: `./deploy.sh --host api.elevenlabs.io`
    // false-fired 004 while the same pathed head is silent for HTTP).
    if !pathed && has_marker(&MEDIA_MARKERS) {
        return Some((
            "native-first/004",
            format!(
                "`{head}` calls a media provider endpoint — \
                 `nika:image_generate`/`nika:tts_generate` cover generation natively \
                 (provider-portable · provenance manifest · permits.fs-gated saves)"
            ),
        ));
    }
    // 001 · an HTTP client program (bare head only — `./scripts/curl` is
    // the author's own tool), or an interpreter one-liner around one.
    if (!pathed && HTTP_PROGRAMS.contains(&head))
        || (is_interpreter && has_marker(&HTTP_CODE_MARKERS))
    {
        return Some((
            "native-first/001",
            format!(
                "`{head}` is an HTTP fetch — `nika:fetch` covers \
                 GET/POST + extraction (SSRF-defended · permits-gated); uploads take \
                 `multipart:` · crawls take `traverse:` (builtins-v0.1.md §nika:fetch)"
            ),
        ));
    }
    // 002 · file plumbing (bare head only).
    if !pathed && FILE_PROGRAMS.contains(&head) {
        return Some((
            "native-first/002",
            format!(
                "`{head}` is file plumbing — `nika:read`/`nika:write` \
                 (`create_dirs: true` replaces mkdir) / `nika:glob` cover it inside the \
                 permits.fs boundary (no subprocess)"
            ),
        ));
    }
    // 006 · a utility with an EXACT builtin (bare head only). Sits at
    // the foot of the ladder because it never competes: none of its
    // programs appear in 001/002/003, and an interpreter head is 005
    // before it reaches here. It answers with ONE builtin and its
    // argument shape rather than a family, because the mapping is 1:1.
    if !pathed && let Some((_, advice)) = UTILITY_PROGRAMS.iter().find(|(prog, _)| *prog == head) {
        return Some((
            "native-first/006",
            format!("`{head}` has an exact native form — {advice}"),
        ));
    }
    // 003 · data transforms (bare head only).
    if !pathed && DATA_PROGRAMS.contains(&head) {
        return Some((
            "native-first/003",
            format!(
                "`{head}` reshapes data — `nika:jq` (or an `output:` \
                 jq binding) covers JSON in-process · `nika:edit` covers in-place \
                 literal file edits (one data language · no quoting traps)"
            ),
        ));
    }
    None
}

fn helper_script_hint(head: &str) -> (&'static str, String) {
    (
        "native-first/005",
        format!(
            "`{head}` runs a helper script — inventory it: \
             HTTP calls → `nika:fetch` (uploads: `multipart:` · crawls: `traverse:`) · \
             file I/O → `nika:read`/`nika:write` · JSON shaping → `nika:jq` · \
             YAML/TOML/CSV in or out → `nika:convert` (then `nika:jq`) · \
             a product API → wrap it as an MCP server (`mcp:<server>/<tool>`); \
             a helper script is not a genuine subprocess — under `--native-strict` \
             this fails, and a row in the exec ledger records the intent \
             without clearing it"
        ),
    )
}

fn command_segments(command: &RawCommand) -> Vec<CommandSegment> {
    match command {
        RawCommand::Argv(_) => command
            .argv_program()
            .and_then(|raw| {
                segment(
                    raw,
                    command
                        .text_fragments()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                )
            })
            .into_iter()
            .collect(),
        RawCommand::Shell(shell) => {
            if has_unquoted_heredoc(&shell.value) {
                return Vec::new();
            }
            split_shell_segments(&shell.value)
                .into_iter()
                .filter_map(|text| {
                    let words = shell_words(&text);
                    let raw = first_program(&words)?.to_owned();
                    segment(&raw, words)
                })
                .collect()
        }
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown exec command form: {other:?}"),
    }
}

fn segment(raw: &str, fragments: Vec<String>) -> Option<CommandSegment> {
    if raw.contains("${{") {
        return None; // templated — runtime business, no static claim
    }
    let pathed = raw.contains('/');
    let head = raw.rsplit('/').next().unwrap_or(raw);
    (!head.is_empty()).then(|| CommandSegment {
        head: head.to_owned(),
        pathed,
        fragments,
    })
}

fn first_program(words: &[String]) -> Option<&str> {
    let mut index = 0;
    let mut env_wrapper = false;
    while let Some(token) = words.get(index).map(String::as_str) {
        if is_assignment(token) {
            index += 1;
            continue;
        }
        if token == "env" && !env_wrapper {
            env_wrapper = true;
            index += 1;
            continue;
        }
        if env_wrapper && matches!(token, "-i" | "--ignore-environment") {
            index += 1;
            continue;
        }
        // `env -- NAME=value command` ends env's option scan; the
        // assignments that follow still belong to the wrapper, not to a
        // literal program named `--`.
        if env_wrapper && token == "--" {
            index += 1;
            continue;
        }
        if env_wrapper && matches!(token, "-u" | "--unset") {
            index += 2;
            continue;
        }
        if env_wrapper && token.starts_with("--unset=") {
            index += 1;
            continue;
        }
        if redirection_takes_operand(token) {
            index += 2;
            continue;
        }
        if is_redirection(token) {
            index += 1;
            continue;
        }
        if matches!(token, "{" | "then" | "do" | "else" | "!") {
            index += 1;
            continue;
        }
        if matches!(token, "if" | "elif" | "while" | "until") {
            index += 1;
            continue;
        }
        if matches!(
            token,
            "for" | "case" | "select" | "function" | "fi" | "done" | "esac" | "}"
        ) {
            return None;
        }
        return Some(token);
    }
    None
}

fn is_assignment(token: &str) -> bool {
    token.split_once('=').is_some_and(|(name, _)| {
        let mut chars = name.chars();
        chars
            .next()
            .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
            && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
    })
}

fn redirection_body(token: &str) -> &str {
    token.trim_start_matches(|c: char| c.is_ascii_digit())
}

fn redirection_takes_operand(token: &str) -> bool {
    matches!(
        redirection_body(token),
        ">" | ">>" | ">|" | "<" | "<>" | "<&" | ">&" | "<<<"
    )
}

fn is_redirection(token: &str) -> bool {
    matches!(redirection_body(token).chars().next(), Some('<' | '>'))
}

/// Split only on unquoted, unescaped shell control operators. This is
/// intentionally a segmenter, not a shell evaluator: it recognizes
/// physical-line comments/newlines and a small compound-command prefix
/// vocabulary, but never expands variables or substitutions. Heredocs
/// make the whole shell command opaque so their bodies cannot false-fire.
fn split_shell_segments(shell: &str) -> Vec<String> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Quote {
        None,
        Single,
        Double,
    }

    let chars: Vec<char> = shell.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote = Quote::None;
    let mut escaped = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escaped {
            current.push(c);
            escaped = false;
            i += 1;
            continue;
        }
        if c == '\\' && quote != Quote::Single {
            current.push(c);
            escaped = true;
            i += 1;
            continue;
        }
        match (quote, c) {
            (Quote::None, '\'') => quote = Quote::Single,
            (Quote::None, '"') => quote = Quote::Double,
            (Quote::Single, '\'') | (Quote::Double, '"') => quote = Quote::None,
            _ => {}
        }
        if quote == Quote::None
            && c == '#'
            && current.chars().last().is_none_or(char::is_whitespace)
        {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_owned());
            }
            current.clear();
            i += 1;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        // `>|` is one redirection operator, not a pipeline boundary.
        let is_clobber_redirection = c == '|' && i > 0 && chars.get(i - 1) == Some(&'>');
        if quote == Quote::None && !is_clobber_redirection && matches!(c, '|' | '&' | ';' | '\n') {
            let width = match c {
                '|' if chars.get(i + 1) == Some(&'|') => 2,
                '&' if chars.get(i + 1) == Some(&'&') => 2,
                ';' | '|' | '\n' => 1,
                _ => 0,
            };
            if width > 0 {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_owned());
                }
                current.clear();
                i += width;
                continue;
            }
        }
        current.push(c);
        i += 1;
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_owned());
    }
    parts
}

fn has_unquoted_heredoc(shell: &str) -> bool {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Quote {
        None,
        Single,
        Double,
    }

    let chars: Vec<char> = shell.chars().collect();
    let mut quote = Quote::None;
    let mut escaped = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if c == '\\' && quote != Quote::Single {
            escaped = true;
            i += 1;
            continue;
        }
        match (quote, c) {
            (Quote::None, '\'') => quote = Quote::Single,
            (Quote::None, '"') => quote = Quote::Double,
            (Quote::Single, '\'') | (Quote::Double, '"') => quote = Quote::None,
            _ => {}
        }
        if quote == Quote::None
            && c == '#'
            && (i == 0
                || chars
                    .get(i.wrapping_sub(1))
                    .is_some_and(|c| c.is_whitespace()))
        {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if quote == Quote::None
            && c == '<'
            && chars.get(i + 1) == Some(&'<')
            && chars.get(i + 2) != Some(&'<')
        {
            return true;
        }
        i += 1;
    }
    false
}

/// Tokenize one already-isolated segment just far enough to find its
/// literal program and marker fragments. Quotes and escapes protect
/// whitespace; no interpolation or shell expansion occurs.
fn shell_words(segment: &str) -> Vec<String> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Quote {
        None,
        Single,
        Double,
    }

    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = Quote::None;
    let mut escaped = false;
    for c in segment.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' && quote != Quote::Single {
            escaped = true;
            continue;
        }
        match (quote, c) {
            (Quote::None, '\'') => quote = Quote::Single,
            (Quote::None, '"') => quote = Quote::Double,
            (Quote::Single, '\'') | (Quote::Double, '"') => quote = Quote::None,
            (Quote::None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn hints_of(yaml: &str) -> Vec<Hint> {
        scan(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    /// Every utility with a 1:1 builtin NAMES it, and names the argument
    /// shape with it. Measured 2026-08-20 against 0.111.0: these eight
    /// were silent, every one had an exact builtin, and the advice an
    /// author usually lands on (005) mentions five of twenty-eight — so
    /// `nika:wait` was unreachable from any hint and a three-second
    /// `sleep` outlived the beat that needed it.
    ///
    /// The assertions name the BUILTIN, never the sentence: a pin that
    /// quotes its own prose passes on a reword into nonsense.
    #[test]
    fn a_utility_with_an_exact_builtin_names_it() {
        for (argv, builtin) in [
            (r#"["sleep", "3"]"#, "nika:wait"),
            (r#"["date", "+%s"]"#, "nika:date"),
            (r#"["uuidgen"]"#, "nika:uuid"),
            (r#"["sha256sum", "f.txt"]"#, "nika:hash"),
            (r#"["b3sum", "f.txt"]"#, "nika:hash"),
            (r#"["yq", ".a", "f.yaml"]"#, "nika:convert"),
            (r#"["grep", "-r", "x", "."]"#, "nika:grep"),
            (r#"["rg", "x"]"#, "nika:grep"),
            (r#"["find", ".", "-name", "x"]"#, "nika:glob"),
        ] {
            let hints = hints_of(&exec_wf(argv));
            let advice = &hints.first().expect("a utility fires 006").advice;
            assert!(
                advice.starts_with("native-first/006"),
                "{argv} is rule 006 · {advice}"
            );
            assert!(
                advice.contains(builtin),
                "{argv} names {builtin} · {advice}"
            );
        }

        // `echo` is NOT in the table, on purpose: its answer is `const:`,
        // not a builtin, and it is the universal harmless placeholder. A
        // rule that refuses it under `--native-strict` is friction.
        assert!(
            hints_of(&exec_wf(r#"["echo", "hello"]"#)).is_empty(),
            "the canonical do-nothing command gains no hint"
        );
    }

    /// 006 sits at the foot of the ladder and never steals a verdict.
    /// Its program set is disjoint from 001/002/003 by construction, and
    /// an interpreter head is 005 before it can reach here.
    #[test]
    fn the_exact_utility_rule_never_outranks_a_more_specific_one() {
        for (argv, rule) in [
            (r#"["curl", "https://example.com"]"#, "native-first/001"),
            (r#"["cat", "f.txt"]"#, "native-first/002"),
            (r#"["jq", ".a"]"#, "native-first/003"),
            (r#"["bash", "helper.sh"]"#, "native-first/005"),
        ] {
            let hints = hints_of(&exec_wf(argv));
            let advice = &hints.first().expect("a known family fires").advice;
            assert!(advice.starts_with(rule), "{argv} stays {rule} · {advice}");
        }

        // A PATHED head is the author's own tool that merely shares a
        // utility's name — the same suppression 001/002/003 already make.
        assert!(
            hints_of(&exec_wf(r#"["./scripts/date", "--weekly"]"#)).is_empty(),
            "a pathed head makes no claim"
        );

        // And a genuine subprocess with no builtin counterpart stays
        // silent: a rule that fires on everything advises nothing.
        assert!(
            hints_of(&exec_wf(r#"["rsync", "-a", "a/", "b/"]"#)).is_empty(),
            "a program with no native form gains no hint"
        );
    }

    fn exec_wf(command_yaml: &str) -> String {
        // D1 (0.103): the string form lives in `shell:`, argv in `command:` —
        // the fixture router mirrors the field split the parser enforces.
        let field = if command_yaml.trim_start().starts_with('[') {
            "command"
        } else {
            "shell"
        };
        format!("nika: w\ntasks:\n  t:\n    exec: {{ {field}: {command_yaml} }}\n")
    }

    fn sole_native_hint(yaml: &str) -> Hint {
        let hints = hints_of(yaml);
        let native: Vec<&Hint> = hints.iter().filter(|h| h.kind == "native-first").collect();
        assert_eq!(native.len(), 1, "exactly one per task: {hints:?}");
        native[0].clone()
    }

    #[test]
    fn fires_on_curl_naming_fetch_001() {
        let h = sole_native_hint(&exec_wf(
            "\"curl -s https://api.test/items -H 'x-api-key: k'\"",
        ));
        assert_eq!(h.task, "t");
        assert!(h.advice.contains("native-first/001"), "{h:?}");
        assert!(h.advice.contains("nika:fetch"), "{h:?}");
    }

    #[test]
    fn pathed_program_names_never_fire_the_name_families() {
        // The author's own tool sharing a utility name (review FP class):
        // path-qualified heads make no program-name claim.
        for command in [
            "[\"./scripts/cat\", \"--render\", \"template.json\"]",
            "[\"/usr/local/bin/curl\", \"https://x.test\"]",
            "\"./tools/jq-like transform.json\"",
        ] {
            let hints = hints_of(&exec_wf(command));
            assert!(
                !hints.iter().any(|h| h.kind == "native-first"),
                "{command} must stay silent: {hints:?}"
            );
        }
        // …but a pathed INTERPRETER + script is still the helper class.
        let h = sole_native_hint(&exec_wf("[\"/usr/bin/python3\", \"scripts/upload.py\"]"));
        assert!(h.advice.contains("native-first/005"), "{h:?}");
    }

    #[test]
    fn fires_on_python_requests_one_liner_001() {
        let h = sole_native_hint(&exec_wf(
            "\"python3 -c 'import requests; requests.get(\\\"https://x.test\\\")'\"",
        ));
        assert!(h.advice.contains("native-first/001"), "{h:?}");
    }

    #[test]
    fn fires_on_node_helper_script_005_not_001() {
        // An interpreter + script file is the HELPER class even when the
        // script name suggests HTTP — the umbrella advice owns it.
        let h = sole_native_hint(&exec_wf(
            "[\"node\", \"workflows/site/bin/crawl-and-upload.mjs\", \"--url\", \"${{ inputs.site }}\"]",
        ));
        assert!(h.advice.contains("native-first/005"), "{h:?}");
        assert!(h.advice.contains("exec ledger"), "{h:?}");
        // #475 · the parse lane is IN the inventory: the single most
        // common reason a helper survives it is "my input is YAML/CSV/
        // TOML" — a hole here becomes an exec in a user's boundary.
        assert!(h.advice.contains("nika:convert"), "{h:?}");
    }

    #[test]
    fn fires_on_python_inline_fetch_as_001() {
        let h = sole_native_hint(&exec_wf(
            "\"python3 -c 'import urllib; fetch(\\\"https://x.test\\\")'\"",
        ));
        assert!(h.advice.contains("native-first/001"), "{h:?}");
    }

    #[test]
    fn pathed_head_never_fires_media_004() {
        // The author's own script whose ARG names a media-provider domain
        // must not be second-guessed into `nika:image_generate` — a
        // pathed head is silent for 001/002/003 and now for 004 too
        // (the review's F2 false-dirty: `./deploy.sh --host
        // api.elevenlabs.io` fired 004 while a pathed curl stays silent).
        for command in [
            "[\"./deploy.sh\", \"--host\", \"api.elevenlabs.io\"]",
            "\"./tools/publish.sh https://api.openai.com/v1/images/generations\"",
        ] {
            let hints = hints_of(&exec_wf(command));
            assert!(
                !hints.iter().any(|h| h.advice.contains("native-first/004")),
                "pathed head must not fire media 004: {hints:?}"
            );
        }
    }

    #[test]
    fn fires_on_media_endpoint_004_over_http_001() {
        let h = sole_native_hint(&exec_wf(
            "\"curl -X POST https://api.openai.com/v1/images/generations -d @body.json\"",
        ));
        assert!(h.advice.contains("native-first/004"), "{h:?}");
        assert!(h.advice.contains("nika:image_generate"), "{h:?}");
    }

    #[test]
    fn fires_on_file_and_data_programs() {
        let file = sole_native_hint(&exec_wf("\"cat out/manifest.json\""));
        assert!(file.advice.contains("native-first/002"), "{file:?}");
        let data = sole_native_hint(&exec_wf(
            "\"jq '.items | map(.name)' out/crawl.json > out/names.json\"",
        ));
        assert!(data.advice.contains("native-first/003"), "{data:?}");
        let env_prefixed = sole_native_hint(&exec_wf("\"LC_ALL=C sed s/a/b/ in.txt\""));
        assert!(
            env_prefixed.advice.contains("native-first/003"),
            "assignments are skipped to the real head: {env_prefixed:?}"
        );
    }

    #[test]
    fn silent_on_genuine_subprocesses() {
        for command in [
            "\"cargo test --workspace --lib\"",
            "\"git commit -m 'x'\"",
            "[\"qrt\", \"product\", \"create\", \"--json\"]",
            "\"make release\"",
            "\"nika run subroutine.nika.yaml\"",
            "\"${{ inputs.tool }} --flag\"",
        ] {
            let hints = hints_of(&exec_wf(command));
            assert!(
                !hints.iter().any(|h| h.kind == "native-first"),
                "{command} must stay silent: {hints:?}"
            );
        }
    }

    #[test]
    fn silent_on_interpreter_without_script_or_http() {
        // A bare interpreter computation is a legitimate subprocess.
        let hints = hints_of(&exec_wf("\"python3 -c 'print(6*7)'\""));
        assert!(!hints.iter().any(|h| h.kind == "native-first"), "{hints:?}");
    }

    #[test]
    fn the_site_asset_regression_fixture_yields_all_four_hints() {
        // The genericized empirical trigger: a site-asset workflow whose
        // four exec tasks are all natively expressible. Spec-VALID (it
        // parses + checks) yet every task earns its native-first hint.
        let yaml = r#"nika: site-asset
model: mock/echo
tasks:
  crawl_site:
    exec: { command: ["curl", "-s", "https://acme.test", "-o", "out/site.html"] }
  upload_background:
    after: { crawl_site: success }
    exec:
      command: ["node", "workflows/site/bin/helper.mjs", "upload", "--file", "out/bg.png"]
  render_background:
    after: { crawl_site: success }
    exec: { command: ["curl", "-X", "POST", "https://api.openai.com/v1/images/generations"] }
  write_manifest:
    after: { upload_background: success, render_background: success }
    exec: { shell: "jq -n '{done: true}' > out/manifest.json" }
"#;
        let hints = hints_of(yaml);
        let by_task: Vec<(&str, &str)> = hints
            .iter()
            .filter(|h| h.kind == "native-first")
            .map(|h| {
                let rule = h
                    .advice
                    .split_once(" · ")
                    .map(|(rule, _)| rule)
                    .unwrap_or_default();
                (h.task.as_str(), rule)
            })
            .collect();
        assert_eq!(
            by_task,
            vec![
                ("crawl_site", "native-first/001"),
                ("upload_background", "native-first/005"),
                ("render_background", "native-first/004"),
                ("write_manifest", "native-first/003"),
            ],
            "{hints:?}"
        );
    }

    #[test]
    fn every_shell_segment_is_judged_in_order() {
        let hints = hints_of(&exec_wf("\"cat input.json | jq '.items' | tee out.json\""));
        let codes: Vec<Option<&str>> = hints
            .iter()
            .filter(|h| h.kind == "native-first")
            .map(|h| h.code)
            .collect();
        assert_eq!(
            codes,
            vec![
                Some("native-first/002"),
                Some("native-first/003"),
                Some("native-first/002"),
            ],
            "the head must not hide later pipeline segments: {hints:?}"
        );
    }

    #[test]
    fn shell_segmentation_respects_quotes_escapes_assignments_and_paths() {
        let hints = hints_of(&exec_wf(
            "\"printf 'left|right' \\\\| literal; MODE=x jq '.' && ./scripts/cat input || /usr/bin/date\"",
        ));
        let codes: Vec<Option<&str>> = hints
            .iter()
            .filter(|h| h.kind == "native-first")
            .map(|h| h.code)
            .collect();
        assert_eq!(
            codes,
            vec![Some("native-first/003")],
            "quoted/escaped separators and pathed utility heads stay literal: {hints:?}"
        );
    }

    #[test]
    fn date_and_digest_programs_name_registered_builtins() {
        let hints = hints_of(&exec_wf(
            "\"date; sha256sum a; shasum a; md5sum a; sha1sum a\"",
        ));
        assert_eq!(
            hints
                .iter()
                .filter(|h| h.code == Some("native-first/006"))
                .count(),
            5,
            "each deterministic native path gets its own site: {hints:?}"
        );
        for builtin in ["date", "hash"] {
            assert!(
                nika_catalog::find_builtin(builtin).is_some(),
                "a hint target must resolve in the real builtin catalog: nika:{builtin}"
            );
        }
        assert!(hints[0].advice.contains("nika:date"), "{:?}", hints[0]);
        assert!(
            hints[1..].iter().all(|h| h.advice.contains("nika:hash")),
            "{hints:?}"
        );
    }

    #[test]
    fn comments_end_the_physical_line_without_spawning_phantom_commands() {
        let hints = hints_of(&exec_wf("\"echo ok # ignored ; jq '.'; date\""));
        assert!(
            hints.iter().all(|h| h.kind != "native-first"),
            "comment text is never executable syntax: {hints:?}"
        );
    }

    #[test]
    fn compound_shell_prefixes_reveal_commands_without_guessing_keywords() {
        let cases = [
            ("\"{ jq '.'; date; }\"", 2),
            ("\"if true; then jq '.'; fi\"", 1),
            ("\"env MODE=x jq '.'\"", 1),
            ("\"MODE=x env jq '.'\"", 1),
            ("\">/tmp/out jq '.'\"", 1),
            ("\"printf ok\\njq '.'\"", 1),
        ];
        for (command, expected) in cases {
            let hints = hints_of(&exec_wf(command));
            assert_eq!(
                hints.iter().filter(|h| h.kind == "native-first").count(),
                expected,
                "{command}: {hints:?}"
            );
        }
    }

    #[test]
    fn conditional_test_commands_and_each_branch_head_are_judged() {
        let cases = [
            ("\"if jq '.' input.json; then date; fi\"", 2),
            (
                "\"if true; then cat a; elif sed s/a/b/ a; else awk '{print}' a; fi\"",
                3,
            ),
            ("\"while jq -e '.more' state.json; do date; done\"", 2),
            ("\"until jq -e '.done' state.json; do date; done\"", 2),
        ];
        for (command, expected) in cases {
            let hints = hints_of(&exec_wf(command));
            assert_eq!(
                hints.iter().filter(|h| h.kind == "native-first").count(),
                expected,
                "{command}: {hints:?}"
            );
        }
    }

    #[test]
    fn prefix_expansion_keeps_comment_heredoc_and_crlf_boundaries() {
        let comment = hints_of(&exec_wf("\"if true; then echo ok # jq '.'\\ndate; fi\""));
        assert_eq!(
            comment.iter().filter(|h| h.kind == "native-first").count(),
            1,
            "comment payload stays inert while the next line is judged: {comment:?}"
        );

        let heredoc = hints_of(&exec_wf("\"if cat <<EOF\\njq '.'\\nEOF\\nthen date\\nfi\""));
        assert!(
            heredoc.iter().all(|h| h.kind != "native-first"),
            "the conservative heredoc backoff still owns the whole shell: {heredoc:?}"
        );

        let crlf = hints_of(&exec_wf("\"if jq '.' input.json; then\\r\\ndate\\r\\nfi\""));
        assert_eq!(
            crlf.iter().filter(|h| h.kind == "native-first").count(),
            2,
            "CRLF keeps the same command boundaries: {crlf:?}"
        );
    }

    #[test]
    fn env_and_conditional_prefix_matrix_finds_all_eight_native_sites() {
        let cases = [
            ("\"env -i MODE=x jq '.'; date\"", 2),
            ("\"env -- MODE=x jq '.'; date\"", 2),
            ("\"if ! jq '.' input.json; then date; fi\"", 2),
            ("\">|/tmp/out MODE=x jq '.'\"", 1),
            ("\"2>&1 MODE=x env OTHER=y jq '.'\"", 1),
        ];
        let mut total = 0;
        for (command, expected) in cases {
            let hints = hints_of(&exec_wf(command));
            let count = hints.iter().filter(|h| h.kind == "native-first").count();
            assert_eq!(count, expected, "{command}: {hints:?}");
            total += count;
        }
        assert_eq!(total, 8, "the hostile prefix matrix keeps all eight sites");
    }

    #[test]
    fn direct_and_pathed_heads_keep_the_precision_boundary() {
        let direct = hints_of(&exec_wf("\"jq /tmp/input\""));
        assert_eq!(
            direct
                .iter()
                .filter(|h| h.code == Some("native-first/003"))
                .count(),
            1
        );
        let pathed = hints_of(&exec_wf("\"/usr/bin/jq /tmp/input\""));
        assert!(
            pathed.iter().all(|h| h.kind != "native-first"),
            "a pathed head remains author-owned: {pathed:?}"
        );
    }

    #[test]
    fn heredoc_bodies_are_not_reinterpreted_as_command_lines() {
        let hints = hints_of(&exec_wf("\"cat <<EOF\\njq '.'\\nEOF\""));
        assert!(
            hints.iter().all(|h| h.code != Some("native-first/003")),
            "the deliberately small segmenter backs off on heredocs: {hints:?}"
        );
    }
}
