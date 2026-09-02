// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika guard` — the hook's judge (P0-7 + P0-15 · audit UX 2026-07-30).
//!
//! The regex/split `guard-run.sh` failed open 21 documented ways (the
//! absolute-path miss · the `--resume` substring allow · the unquoted
//! split that judged an empty file token). The judgement now lives here,
//! in the binary: real shell tokenisation (quotes honoured · expansions
//! MARKED, never guessed), basename detection (an absolute path is still
//! nika), wrapper unwinding (`sh -c` · `env` · `cd` tracking), and the
//! in-process oracle (`nika_check` + the catalog pricing fold, the
//! welcome.rs `run_gate` pattern) on the EXACT file the run names. The
//! shim is a thin pipe. The run belongs to the human: guard JUDGES, it
//! never executes.

use std::path::{Path, PathBuf};

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, exit};

/// The judge's four answers. `NotOurs` and `Allow` share the hook wire
/// (the no-opinion shape); they differ only in the human reading.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    /// No `nika run` in the command — the guard has no opinion.
    NotOurs,
    /// A run was judged and may proceed (clean · unpriced or capped).
    Allow(String),
    /// A run must not proceed — the reason teaches the repair.
    Deny(String),
    /// The guard COULD NOT judge — visible degradation, never a silent
    /// claim that the check passed (the P0-15 fail-open class).
    Unavailable(String),
}

/// The hook dialect, sniffed from the payload: a top-level
/// `hook_event_name` string is Claude Code's (Codex emits it
/// verbatim), absent from Cursor's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialect {
    Claude,
    Generic,
}

/// One judgement's input: the command line, the directory it runs in
/// (None = unknown — a relative file then cannot resolve), the wire.
#[derive(Debug)]
struct Input {
    line: String,
    cwd: Option<PathBuf>,
    dialect: Dialect,
}

/// The shell operators the lexer splits on — `Redirect` stays inside a
/// segment (its target is never the workflow), the rest separate the
/// simple commands of a `;`/`&&`/`||`/`|` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    And,
    Or,
    Seq,
    Pipe,
    Redirect,
}

/// One lexed token. `dynamic` marks an UNEXPANDED dynamic form (a
/// variable · a command substitution · a glob outside quotes) — the
/// guard judges what a run names, it never guesses an expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tok {
    text: String,
    dynamic: bool,
    op: Option<Op>,
}

/// One simple command between operators, with the operators that open
/// and close it (a `cd` inside a pipeline never reaches the parent
/// shell's cwd).
#[derive(Debug)]
struct Seg {
    toks: Vec<Tok>,
    before: Option<Op>,
    after: Option<Op>,
}

/// The deny reason's size budget (the old shim's 2000-byte protocol law).
const PROTOCOL_BUDGET: usize = 2000;

/// The hook payload's size budget (audit 2026-07-31): `guard --stdin`
/// reads at most this many bytes — a hostile or broken host cannot
/// hang or OOM the judge; over the cap the answer is a deterministic
/// `guard_unavailable`, deny-shaped in both dialects.
const MAX_PAYLOAD: u64 = 4 * 1024 * 1024;

/// Read the hook payload from `reader`, capped at [`MAX_PAYLOAD`].
/// `Err` carries the partial bytes (the dialect sniff still reads
/// them) plus the reason — the io failure and the oversize both
/// degrade to the same visible `guard_unavailable`.
fn read_payload(reader: &mut impl std::io::Read) -> Result<String, (String, String)> {
    use std::io::Read as _;
    let mut raw = String::new();
    let mut limited = reader.take(MAX_PAYLOAD + 1);
    if let Err(e) = limited.read_to_string(&mut raw) {
        return Err((raw, format!("cannot read the hook payload from stdin: {e}")));
    }
    if raw.len() as u64 > MAX_PAYLOAD {
        return Err((
            raw,
            "payload over 4 MiB — the guard refuses to judge what it cannot hold".to_owned(),
        ));
    }
    Ok(raw)
}

/// The shell tokenizer — quotes honoured, expansions MARKED never
/// performed (the exact class the regex hook's whitespace split got
/// wrong: a quoted path with spaces is ONE token; `$WF` is unknown,
/// not empty).
fn lex(line: &str) -> Vec<Tok> {
    let chars: Vec<char> = line.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // `#` at token start opens a comment to end-of-line — a
        // commented `nika run` is text, not an execution.
        if c == '#' {
            break;
        }
        if let Some((op, len)) = operator_at(&chars, i) {
            toks.push(Tok {
                text: chars[i..i + len].iter().collect(),
                dynamic: false,
                op: Some(op),
            });
            i += len;
            continue;
        }
        let (tok, next) = lex_word(&chars, i);
        toks.push(tok);
        i = next;
    }
    toks
}

/// An operator starting at `i`, with its length — `>&2`/`2>>`-style
/// redirect clusters included.
fn operator_at(chars: &[char], i: usize) -> Option<(Op, usize)> {
    let c = chars[i];
    let two = chars.get(i + 1);
    match c {
        '&' if two == Some(&'&') => Some((Op::And, 2)),
        // A background `&` sequences just like `;` for the guard.
        '&' | ';' => Some((Op::Seq, 1)),
        '|' if two == Some(&'|') => Some((Op::Or, 2)),
        '|' => Some((Op::Pipe, 1)),
        '>' | '<' => {
            let mut j = i;
            while j < chars.len() && matches!(chars[j], '>' | '<' | '&' | '0'..='9') {
                j += 1;
            }
            Some((Op::Redirect, j - i))
        }
        _ => None,
    }
}

/// A single-quoted span from the quote at `i` — literal, never dynamic.
fn lex_single_quote(chars: &[char], mut i: usize, text: &mut String) -> usize {
    i += 1;
    while i < chars.len() && chars[i] != '\'' {
        text.push(chars[i]);
        i += 1;
    }
    (i + 1).min(chars.len())
}

/// A double-quoted span — `\` honoured, `$`/backtick still expand in a
/// real shell so the word is marked dynamic.
fn lex_double_quote(chars: &[char], mut i: usize, text: &mut String) -> (usize, bool) {
    let mut dynamic = false;
    i += 1;
    while i < chars.len() && chars[i] != '"' {
        if chars[i] == '\\' && i + 1 < chars.len() {
            text.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if matches!(chars[i], '$' | '`') {
            dynamic = true;
        }
        text.push(chars[i]);
        i += 1;
    }
    ((i + 1).min(chars.len()), dynamic)
}

/// A `$( … )` command substitution — one dynamic span, spaces included.
fn lex_dollar(chars: &[char], mut i: usize, text: &mut String) -> usize {
    if chars.get(i + 1) != Some(&'(') {
        text.push('$');
        return i + 1;
    }
    let mut depth = 0i32;
    while i < chars.len() {
        match chars[i] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            ch => text.push(ch),
        }
        i += 1;
    }
    i
}

/// A backtick command substitution — one dynamic span.
fn lex_backtick(chars: &[char], mut i: usize, text: &mut String) -> usize {
    i += 1;
    while i < chars.len() && chars[i] != '`' {
        text.push(chars[i]);
        i += 1;
    }
    (i + 1).min(chars.len())
}

/// One word from `start` — quotes delegated to the span helpers, unquoted
/// `$`/`*?[`/backticks marked dynamic. A trailing digit-run before
/// `>`/`<` folds into the redirect token (`2>/dev/null`).
fn lex_word(chars: &[char], start: usize) -> (Tok, usize) {
    let mut text = String::new();
    let mut dynamic = false;
    let mut i = start;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || matches!(c, '&' | '|' | ';' | '#') {
            break;
        }
        if c == '>' || c == '<' {
            if !text.is_empty() && text.chars().all(|d| d.is_ascii_digit()) {
                // `2>` — the fd prefix belongs to the redirect token.
                let (op, len) = operator_at(chars, i).unwrap_or((Op::Redirect, 1));
                text.push_str(&chars[i..i + len].iter().collect::<String>());
                return (
                    Tok {
                        text,
                        dynamic,
                        op: Some(op),
                    },
                    i + len,
                );
            }
            break;
        }
        match c {
            '\'' => i = lex_single_quote(chars, i, &mut text),
            '"' => {
                let (next, span_dynamic) = lex_double_quote(chars, i, &mut text);
                dynamic |= span_dynamic;
                i = next;
            }
            '\\' if i + 1 < chars.len() => {
                text.push(chars[i + 1]);
                i += 2;
            }
            '$' => {
                dynamic = true;
                i = lex_dollar(chars, i, &mut text);
            }
            '`' => {
                dynamic = true;
                i = lex_backtick(chars, i, &mut text);
            }
            '*' | '?' | '[' => {
                dynamic = true;
                text.push(c);
                i += 1;
            }
            _ => {
                text.push(c);
                i += 1;
            }
        }
    }
    (
        Tok {
            text,
            dynamic,
            op: None,
        },
        i,
    )
}

/// Split the token stream into simple commands, remembering the joining
/// operators (a `cd` only reaches segments the pipeline law allows).
fn segments(toks: &[Tok]) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut before: Option<Op> = None;
    for t in toks {
        match t.op {
            Some(op) if op != Op::Redirect => {
                out.push(Seg {
                    toks: std::mem::take(&mut cur),
                    before,
                    after: Some(op),
                });
                before = Some(op);
            }
            _ => cur.push(t.clone()),
        }
    }
    out.push(Seg {
        toks: cur,
        before,
        after: None,
    });
    out
}

/// Redirects (`> log` · `2>/dev/null`) carry no command word — drop the
/// operator and its target.
fn strip_redirects(toks: &[Tok]) -> Vec<&Tok> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < toks.len() {
        if toks[k].op == Some(Op::Redirect) {
            k += 1;
            if k < toks.len() && toks[k].op.is_none() {
                k += 1; // the target word
            }
            continue;
        }
        out.push(&toks[k]);
        k += 1;
    }
    out
}

/// A leading `NAME=value` environment assignment (`FOO=bar nika run …`
/// runs nika all the same).
fn is_assignment(t: &str) -> bool {
    let Some((name, _)) = t.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The executable's basename — `/usr/local/bin/nika run …` IS a nika
/// invocation (the absolute-path bypass, P0-15).
fn basename(t: &str) -> &str {
    t.rsplit('/').next().unwrap_or(t)
}

/// Judge one whole command line: every effective `nika run` it contains
/// must pass — the worst verdict wins (a deny anywhere denies; an
/// unjudgeable run is visible; a judged-clean run flows).
fn judge_line(line: &str, cwd: Option<&Path>) -> Verdict {
    let segs = segments(&lex(line));
    let mut eff = cwd.map(Path::to_path_buf);
    let mut verdicts = Vec::new();
    for seg in &segs {
        if let Some(v) = analyze_segment(seg, &mut eff) {
            verdicts.push(v);
        }
    }
    fold(verdicts)
}

/// The worst-of fold: first deny > first unavailable > first allow >
/// no opinion.
fn fold(verdicts: Vec<Verdict>) -> Verdict {
    let mut unavailable = None;
    let mut allow = None;
    for v in verdicts {
        match v {
            Verdict::Deny(_) => return v,
            Verdict::Unavailable(_) if unavailable.is_none() => unavailable = Some(v),
            Verdict::Allow(_) if allow.is_none() => allow = Some(v),
            _ => {}
        }
    }
    unavailable.or(allow).unwrap_or(Verdict::NotOurs)
}

/// What joins a segment to its neighbours — the facts the dispatch
/// needs beyond the words: a pipe ANYWHERE (the `cd` subshell law), a
/// pipe INTO the segment (the shell's stdin is another command's
/// output), and a script FEED (the shell's commands ride bytes the
/// line does not show).
#[derive(Debug, Clone, Copy)]
struct SegCtx {
    piped: bool,
    fed_by_pipe: bool,
    feed: Option<Feed>,
}

/// The unseeable-bytes feed a segment carries, if any: a heredoc body
/// or an input redirect (`<`, fd-prefixed forms included) — the two
/// shapes whose script content never appears in the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Feed {
    Heredoc,
    Redirect,
}

/// One simple command: strip redirects and leading assignments, then
/// dispatch on the basename. Returns `None` when the segment is no
/// affair of the guard's (a non-nika command is never judged — the
/// echo/comment false-denial class, P0-15).
fn analyze_segment(seg: &Seg, cwd: &mut Option<PathBuf>) -> Option<Verdict> {
    let is_redirect = |t: &Tok| t.op == Some(Op::Redirect);
    let ctx = SegCtx {
        piped: seg.before == Some(Op::Pipe) || seg.after == Some(Op::Pipe),
        fed_by_pipe: seg.before == Some(Op::Pipe),
        feed: if seg
            .toks
            .iter()
            .any(|t| is_redirect(t) && t.text.starts_with("<<"))
        {
            Some(Feed::Heredoc)
        } else if seg
            .toks
            .iter()
            .any(|t| is_redirect(t) && t.text.contains('<') && !t.text.contains("<<"))
        {
            Some(Feed::Redirect)
        } else {
            None
        },
    };
    let words = strip_redirects(&seg.toks);
    analyze_command(&words, ctx, cwd)
}

/// The command dispatch, wrappers unwound: `env` prefixes, `sh -c` /
/// `bash -lc` script strings (judged recursively), `cd` (tracked, never
/// judged), `nika` (the gate). The fail-closed posture (audit
/// 2026-07-31): a dynamic command word, a shell control-flow body, and
/// the stdin/expression executors are UNJUDGEABLE — they degrade to
/// `Unavailable`, never to the silent `NotOurs`.
fn analyze_command(words: &[&Tok], ctx: SegCtx, cwd: &mut Option<PathBuf>) -> Option<Verdict> {
    let mut idx = 0;
    while idx < words.len() && is_assignment(&words[idx].text) {
        idx += 1;
    }
    let words = &words[idx..];
    let first = words.first()?;
    // A dynamic command word (`$(echo nika) run x` · `$N run x`) is
    // UNKNOWABLE — the guard never guesses an expansion into an allow.
    if first.dynamic {
        return Some(Verdict::Unavailable(format!(
            "the command word rides an expansion (`{}`) — the guard judges the command a line names, it never guesses an expansion",
            first.text
        )));
    }
    // The basename compare is case-insensitive: APFS runs `NIKA` as
    // nika, so the match must see it (a literal `NIKA` binary on a
    // case-sensitive fs is a negligible false positive — it fails
    // toward judgement).
    let name = basename(&first.text).to_ascii_lowercase();
    // Group/body openers (`( nika run x )` · `then …` · `! …`) carry no
    // command of their own — strip and re-dispatch the rest.
    if matches!(
        name.as_str(),
        "(" | "{" | "then" | "do" | "else" | "elif" | "!"
    ) {
        return analyze_command(&words[1..], ctx, cwd);
    }
    match name.as_str() {
        "cd" => {
            // A cd inside a pipeline runs in a subshell — the parent's
            // cwd (what later segments see) never changes.
            if !ctx.piped {
                apply_cd(words.get(1).copied(), cwd);
            }
            None
        }
        "sh" | "bash" | "zsh" | "dash" => shell_script(words, ctx, cwd),
        "env" => env_command(words, ctx, cwd),
        // `nika-cli` is the same binary under its cargo target name —
        // a dev-shop agent invoking the debug build rode straight past
        // the guard as `{}` no-opinion (gauntlet 08-01, Marc: P0-15's
        // uncapped-priced-run deny never fired, and nothing engine-side
        // caps an advisory-profile run). Same judge, both spellings.
        "nika" | "nika-cli" => nika_command(words, cwd),
        // The value-free wrappers — the first non-option word is the
        // real command (the `env` unwrap's sibling).
        "nice" | "nohup" | "sudo" | "time" | "command" | "exec" | "stdbuf" | "setsid" => {
            wrapper_command(words, ctx, cwd)
        }
        "eval" => eval_command(words, cwd),
        // Shell keywords with bodies — the guard judges commands, it
        // cannot see inside control flow: VISIBLE, never NotOurs.
        "if" | "while" | "for" | "until" | "case" | "select" => {
            Some(Verdict::Unavailable(format!(
                "the `{name}` body is shell control flow — the guard judges commands, it cannot see inside it"
            )))
        }
        // Executors whose argv rides stdin or a find expression.
        "xargs" => Some(Verdict::Unavailable(
            "xargs builds the argv from stdin — the run's file is unknowable".to_owned(),
        )),
        "find" => Some(Verdict::Unavailable(
            "`find -exec` can carry a run — the guard cannot parse a find expression".to_owned(),
        )),
        _ => None,
    }
}

/// `cd <dir>` — absolute replaces, relative joins, a dynamic or missing
/// target makes the cwd UNKNOWN (a relative workflow then resolves to
/// `guard_unavailable`, never to a guess).
fn apply_cd(target: Option<&Tok>, cwd: &mut Option<PathBuf>) {
    match target {
        Some(t) if !t.dynamic => {
            let p = Path::new(t.text.as_str());
            if p.is_absolute() {
                *cwd = Some(p.to_path_buf());
            } else if let Some(base) = cwd {
                *base = base.join(p);
            }
        }
        // `$SOMEWHERE` · a glob · bare `cd` (HOME unknown to the judge).
        _ => *cwd = None,
    }
}

/// A shell wrapper: only the `-c` script string is judgeable (`bash
/// foo.sh` hides its commands in a file the guard cannot see — no
/// verdict, like any non-nika command). A heredoc, pipe- or
/// redirect-fed script is bytes the line does not show — VISIBLE
/// degradation, never the silent pass (audit 2026-07-31).
fn shell_script(words: &[&Tok], ctx: SegCtx, cwd: &mut Option<PathBuf>) -> Option<Verdict> {
    // The heredoc has no lexer model — its body words ride the segment
    // like argv (a crafted body could even fake a `-c`), so a shell
    // segment carrying `<<` is unjudgeable, full stop.
    if matches!(ctx.feed, Some(Feed::Heredoc)) {
        return Some(Verdict::Unavailable(
            "a heredoc script feeds the shell — the guard cannot see the bytes it would run"
                .to_owned(),
        ));
    }
    let mut k = 1;
    while k < words.len() {
        let text = words[k].text.as_str();
        if text == "-c" {
            let script = words.get(k + 1)?;
            return Some(judge_line(&script.text, cwd.as_deref()));
        }
        if text.starts_with('-') && !text.starts_with("--") {
            let cluster = &text[1..];
            if let Some(pos) = cluster.find('c') {
                let attached = &cluster[pos + 1..];
                if !attached.is_empty() {
                    // The attached form — `-cSCRIPT` · `bash -xcSCRIPT`
                    // (real getopt semantics: the rest of the cluster
                    // after `c` IS the script).
                    return Some(judge_line(attached, cwd.as_deref()));
                }
                let script = words.get(k + 1)?;
                return Some(judge_line(&script.text, cwd.as_deref()));
            }
        }
        // The two long options that swallow the NEXT word as their file.
        k += if matches!(text, "--rcfile" | "--init-file") {
            2
        } else {
            1
        };
    }
    // No `-c` script: a shell whose stdin rides a pipe or an input
    // redirect (`sh < run.sh`) reads its commands from bytes the line
    // does not show — unseeable, so VISIBLE. (A NAMED script file —
    // `bash foo.sh` — at least declares its intent; the redirect shows
    // nothing.)
    if ctx.fed_by_pipe {
        return Some(Verdict::Unavailable(
            "a script rides the pipe into the shell — the guard cannot see the bytes it would run"
                .to_owned(),
        ));
    }
    if matches!(ctx.feed, Some(Feed::Redirect)) {
        return Some(Verdict::Unavailable(
            "a script rides the redirect into the shell — the guard cannot see the bytes it would run"
                .to_owned(),
        ));
    }
    None
}

/// `env [opts] [VAR=x …] cmd …` — the real command sits after the
/// environment prefix. `-S`/`--split-string` splits its argument into
/// the argv itself, so that word's content is judged recursively
/// (macOS ships BSD env — the split is real). A short-flag CLUSTER
/// (`-iS'cmd'`) hides the same split: real getopt rides the letters in
/// one word, so `S` anywhere in the cluster opens it.
fn env_command(words: &[&Tok], ctx: SegCtx, cwd: &mut Option<PathBuf>) -> Option<Verdict> {
    let mut k = 1;
    while k < words.len() {
        let t = words[k].text.as_str();
        if t == "-u" || t == "--unset" {
            k += 2;
        } else if t == "-S" || t == "--split-string" {
            // The NEXT word is the split string — judge its content.
            let script = words.get(k + 1)?;
            return Some(judge_line(&script.text, cwd.as_deref()));
        } else if let Some(split) = t.strip_prefix("--split-string=") {
            return Some(judge_line(split, cwd.as_deref()));
        } else if t.starts_with('-')
            && !t.starts_with("--")
            && let Some(pos) = t[1..].find('S')
        {
            // A short-flag cluster hiding `S` (real getopt semantics:
            // the letters ride one word) — the rest of the cluster
            // after `S` IS the split string (`-iS'cmd'` · the lexer
            // already merged any quotes), or the NEXT word when `S`
            // closes the cluster (`-iS 'cmd'`).
            let attached = &t[1..][pos + 1..];
            if !attached.is_empty() {
                return Some(judge_line(attached, cwd.as_deref()));
            }
            let script = words.get(k + 1)?;
            return Some(judge_line(&script.text, cwd.as_deref()));
        } else if t.starts_with('-') || is_assignment(t) {
            k += 1;
        } else {
            break;
        }
    }
    analyze_command(&words[k..], ctx, cwd)
}

/// The wrapper options that swallow the NEXT word as their value
/// (`sudo -u root …` · `nice -n 10 …` · `stdbuf -o L …`). Attached and
/// `--long=value` forms need no table: the value rides the flag's own
/// word and is skipped with it.
const WRAPPER_VALUE_FLAGS: &[&str] = &[
    "-n",           // nice (adjustment)
    "--adjustment", // nice
    "-u",
    "--user", // sudo
    "-g",
    "--group", // sudo
    "-h",
    "--host", // sudo
    "-p",
    "--prompt", // sudo
    "-C",
    "-T",
    "-t",
    "-r", // sudo (close-from · timeout · SELinux)
    "-o",
    "-e",
    "-i", // stdbuf
    "-a", // exec -a NAME
    "-f", // GNU time --format (short)
    "--output",
    "--format", // GNU time
];

/// The value-free wrappers (`sudo` · `nice` · `time` · `command` …):
/// the first non-option word is the real command — re-dispatch from
/// there, mirroring the `env` unwrap.
fn wrapper_command(words: &[&Tok], ctx: SegCtx, cwd: &mut Option<PathBuf>) -> Option<Verdict> {
    let mut k = 1;
    while k < words.len() {
        let t = words[k].text.as_str();
        if WRAPPER_VALUE_FLAGS.contains(&t) {
            k += 2; // the flag swallows the next word (`sudo -u root`)
        } else if t.starts_with('-') {
            k += 1; // boolean and attached forms ride one word (`-n10` · `--user=root`)
        } else {
            break;
        }
    }
    analyze_command(&words[k..], ctx, cwd)
}

/// `eval` with static text judges the text itself — the same bytes the
/// shell would re-parse (the arguments join on spaces, as eval does). A
/// dynamic or missing string is unknowable, VISIBLE.
fn eval_command(words: &[&Tok], cwd: &mut Option<PathBuf>) -> Option<Verdict> {
    let rest = &words[1..];
    if rest.is_empty() {
        return None; // a bare `eval` runs nothing
    }
    if rest.iter().any(|w| w.dynamic) {
        return Some(Verdict::Unavailable(
            "eval rides an expansion — the guard judges static text, it never guesses".to_owned(),
        ));
    }
    let joined = rest
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Some(judge_line(&joined, cwd.as_deref()))
}

/// A `nika …` invocation: skip the global flags (`--plain` · `--color
/// never` — the options-before-`run` bypass), then only `run` is gated —
/// every other verb flows (the guard's whole jurisdiction is the run).
fn nika_command(words: &[&Tok], cwd: &mut Option<PathBuf>) -> Option<Verdict> {
    let mut idx = 1;
    while idx < words.len() {
        let t = words[idx].text.as_str();
        if t == "--color" || t == "--hyperlink" {
            idx += 2;
        } else if t.starts_with('-') {
            idx += 1;
        } else {
            break;
        }
    }
    if words.get(idx).is_none_or(|w| w.text != "run") {
        return None;
    }
    Some(parse_run(&words[idx + 1..], cwd))
}

/// The run's own flags that swallow the NEXT word as their value
/// (every other word that is not a flag is a positional — the first is
/// the workflow file; clap owns arity beyond that).
const VALUE_FLAGS: &[&str] = &[
    "--model",
    "--var",
    "--resume",
    "--resume-compat",
    "--from",
    "--answer",
    "--task",
    "--output",
    "--max-cost-usd",
];

/// What the run parser saw: the file token (None = the bare lazy door),
/// the `--model` override (dynamic values are never applied — a
/// variable is not a model id), the `--max-cost-usd` cap's presence.
struct RunSeen<'a> {
    file: Option<&'a Tok>,
    model: Option<String>,
    model_dynamic: bool,
    capped: bool,
}

/// Parse the words after `run` — `--flag value` and `--flag=value`
/// both honoured (a value flag's target is NEVER mistaken for the
/// workflow, the second split-bug class of the regex hook).
fn parse_run(args: &[&Tok], cwd: &mut Option<PathBuf>) -> Verdict {
    match scan_run(args) {
        Ok(seen) => judge_seen(&seen, cwd),
        Err(v) => v,
    }
}

/// The word walk — pure token classification, no judgement.
fn scan_run<'a>(args: &[&'a Tok]) -> Result<RunSeen<'a>, Verdict> {
    let mut seen = RunSeen {
        file: None,
        model: None,
        model_dynamic: false,
        capped: false,
    };
    let mut k = 0;
    while k < args.len() {
        let text = args[k].text.as_str();
        // `nika run --help` (and `-h` · `--version`) prints and exits: no
        // workflow runs, so there is nothing to judge and everything to
        // allow. Three gauntlet personas (2026-09-02) hit the bare-run
        // refusal here while trying to READ the verb's flags — the guard
        // was standing between a user and the help text.
        if matches!(text, "--help" | "-h" | "--version") {
            return Err(Verdict::Allow(format!(
                "`nika run {text}` prints and exits — no workflow runs"
            )));
        }
        if let Some(long) = text.strip_prefix("--") {
            let (name, inline) = long
                .split_once('=')
                .map_or((long, None), |(n, v)| (n, Some(v)));
            if VALUE_FLAGS.contains(&format!("--{name}").as_str()) {
                let (value, dynamic) = if let Some(v) = inline {
                    (v.to_owned(), args[k].dynamic)
                } else {
                    k += 1;
                    match args.get(k) {
                        Some(v) => (v.text.clone(), v.dynamic),
                        None => {
                            return Err(Verdict::Unavailable(format!(
                                "`--{name}` names no value — the run cannot be parsed"
                            )));
                        }
                    }
                };
                match name {
                    "model" => {
                        seen.model = Some(value);
                        seen.model_dynamic = dynamic;
                    }
                    "max-cost-usd" => seen.capped = true,
                    _ => {}
                }
            }
            // Unknown long flags are treated as boolean (clap owns the
            // refusal when they are not); a `--flag=value` form never
            // leaks its value into the positionals.
            k += 1;
            continue;
        }
        if text.starts_with('-') && text.len() > 1 {
            k += 1; // short flags — none on `run` takes a value
            continue;
        }
        if seen.file.is_none() {
            seen.file = Some(args[k]);
        }
        k += 1;
    }
    Ok(seen)
}

/// From the parsed run to the verdict: dynamic file/model forms are
/// UNKNOWABLE (visible), the bare form mirrors the lazy door, and a
/// resolvable file meets the in-process oracle.
fn judge_seen(seen: &RunSeen<'_>, cwd: &mut Option<PathBuf>) -> Verdict {
    let Some(file) = seen.file else {
        return bare_run(cwd.as_deref());
    };
    if file.dynamic {
        let what = if file.text.contains(['*', '?', '[']) {
            "a glob"
        } else {
            "an unexpanded variable"
        };
        return Verdict::Unavailable(format!(
            "the workflow path rides {what} (`{}`) — the guard judges the file a run names, it never guesses an expansion",
            file.text
        ));
    }
    if seen.model_dynamic && !seen.capped {
        return Verdict::Unavailable(
            "the --model value rides an unexpanded variable — priced or not, the guard cannot know"
                .to_owned(),
        );
    }
    let model = if seen.model_dynamic {
        None // a variable is not a model id — never applied (the cap already carries the law)
    } else {
        seen.model.as_deref()
    };
    let path = match resolve_path(&file.text, cwd.as_deref()) {
        Ok(p) => p,
        Err(v) => return v,
    };
    judge_file(&path, &file.text, model, seen.capped)
}

/// Relative paths join the EFFECTIVE cwd (the payload's, as `cd`
/// segments rewrote it); an unknown cwd resolves nothing.
fn resolve_path(file: &str, cwd: Option<&Path>) -> Result<PathBuf, Verdict> {
    let p = Path::new(file);
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }
    match cwd {
        Some(base) => Ok(base.join(p)),
        None => Err(Verdict::Unavailable(
            "the cwd is unknown (a `cd` the guard cannot follow) — a relative workflow path resolves to nothing"
                .to_owned(),
        )),
    }
}

/// Bare `nika run` mirrors the lazy door: exactly one workflow in the
/// workspace is the target; zero or several is unknowable (the run
/// itself refuses those — the guard says so VISIBLY, never waves one
/// through on a guess).
fn bare_run(cwd: Option<&Path>) -> Verdict {
    let Some(dir) = cwd else {
        return Verdict::Unavailable(
            "bare `nika run` with an unknown cwd — the lazy door cannot be mirrored".to_owned(),
        );
    };
    let mut budget = 4000usize;
    let mut paths = Vec::new();
    crate::verbs::probe::collect_workflow_paths(dir, dir, 4, &mut budget, &mut paths);
    paths.sort();
    match paths.len() {
        1 => {
            // collect_workflow_paths returns ROOT-RELATIVE paths — join
            // before judging (the welcome.rs run_gate law).
            let path = dir.join(&paths[0]);
            let display = path.display().to_string();
            judge_file(&path, &display, None, false)
        }
        0 => Verdict::Unavailable(
            "bare `nika run` names no file and no workflow lives here — nothing to judge"
                .to_owned(),
        ),
        n => Verdict::Unavailable(format!(
            "bare `nika run` names no file and several workflows live here ({n}) — the guard cannot know which would run"
        )),
    }
}

/// The in-process oracle on the EXACT file (the welcome.rs `run_gate`
/// fold, one gate up): unreadable = `guard_unavailable` (infrastructure,
/// visible) · unparseable = findings (deny) · red = deny with the
/// findings · priced without the cap = deny (P0-7) · anything else
/// flows. The guard JUDGES — it never executes the run.
fn judge_file(path: &Path, display: &str, model: Option<&str>, capped: bool) -> Verdict {
    let yaml = match std::fs::read_to_string(path) {
        Ok(y) => y,
        Err(e) => {
            return Verdict::Unavailable(format!(
                "cannot read {display}: {e} — the guard never claims a check on bytes it cannot see"
            ));
        }
    };
    let wf = match nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) {
        Ok(wf) => wf,
        Err(e) => {
            return Verdict::Deny(format!(
                "{display} does not parse — PARSE ✗ [{}] {e}\nrepair, then re-check with the SAME oracle this gate used: nika check {display}",
                e.spec_code()
            ));
        }
    };
    let wf = match model {
        Some(m) => crate::verbs::with_model_override(&wf, m),
        None => wf,
    };
    let report = nika_check::check(&wf);
    if !report.is_clean() {
        return Verdict::Deny(red_reason(display, &report));
    }
    let priced = report
        .cost
        .tasks
        .iter()
        .filter_map(|t| t.model.as_deref())
        .find(|m| nika_catalog::find_pricing_for(m).is_some());
    match priced {
        Some(m) if !capped => Verdict::Deny(format!(
            "{display} runs on {m}, a METERED model (catalog list price), and the command carries no spend cap — an uncapped priced run never leaves the hook (P0-7)\nfix: nika run {display} --max-cost-usd <usd>"
        )),
        Some(m) => Verdict::Allow(format!("{display}: clean · priced ({m}) · capped")),
        None => Verdict::Allow(format!("{display}: clean · unpriced")),
    }
}

/// The deny reason for a red file — the conformance codes + the
/// boundary/builtin tally, capped at the protocol budget, ending with
/// the repair loop.
fn red_reason(display: &str, report: &nika_check::CheckReport) -> String {
    use std::fmt::Write as _;
    let mut reason = format!("nika check refuses {display}:");
    for v in report.conformance.iter().take(4) {
        let first = v.message.lines().next().unwrap_or_default();
        let _ = write!(reason, "\n  {} {first}", v.code);
    }
    let extra = report.extra_conformance_codes();
    if !extra.is_empty() {
        let codes: Vec<String> = extra.iter().map(ToString::to_string).collect();
        let _ = write!(
            reason,
            "\n  + {} boundary/builtin finding(s): {}",
            codes.len(),
            codes.join(" · ")
        );
    }
    if report.conformance.len() > 4 {
        let _ = write!(reason, "\n  … and {} more", report.conformance.len() - 4);
    }
    let _ = write!(
        reason,
        "\nrepair, then re-check with the SAME oracle this gate used: nika check {display}"
    );
    let mut end = reason.len().min(PROTOCOL_BUDGET);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason.truncate(end);
    reason
}

/// The dialect sniff: the PARSED JSON decides — a top-level
/// `hook_event_name` string is Claude Code's (Codex emits it
/// verbatim), absent from Cursor's. The raw substring is only the
/// fallback for a payload that is not JSON at all (a MALFORMED Claude
/// payload still degrades into the Claude shape). Sniffing the raw
/// bytes first would let the COMMAND text spoof the envelope (audit
/// 2026-07-31): a Cursor payload whose command embeds the literal
/// marker would answer in a shape the host cannot parse.
fn payload_dialect(raw: &str) -> Dialect {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(v) => {
            if v.get("hook_event_name").and_then(|h| h.as_str()).is_some() {
                Dialect::Claude
            } else {
                Dialect::Generic
            }
        }
        Err(_) if raw.contains("hook_event_name") => Dialect::Claude,
        Err(_) => Dialect::Generic,
    }
}

/// Parse the host hook payload: Cursor's flat `{command, cwd}` or
/// Claude Code's `{tool_input:{command}, cwd}` (Codex emits the Claude
/// dialect verbatim). A payload the guard cannot read is
/// `guard_unavailable` — VISIBLE, never a silent pass.
fn parse_payload(raw: &str) -> Result<Input, Verdict> {
    let v: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        // Same scope law as the missing-command arm below: bytes that
        // never say `nika` cannot hold a run this judge would claim, so
        // an unreadable payload about somebody else's command is not
        // ours to refuse. A host speaking a format we do not parse must
        // not lose its whole shell to us.
        if !raw.to_ascii_lowercase().contains("nika") {
            return Verdict::NotOurs;
        }
        Verdict::Unavailable(format!(
            "the hook payload is not JSON ({e}) — the host sent something the guard cannot read"
        ))
    })?;
    let command = v.get("command").and_then(|c| c.as_str()).or_else(|| {
        v.get("tool_input")
            .and_then(|t| t.get("command"))
            .and_then(|c| c.as_str())
    });
    let Some(command) = command else {
        // SCOPE BEFORE DEGRADATION. A shape we cannot read is only OURS
        // to refuse if it could have carried a run: the judge never
        // claims a command whose word is not `nika` or `nika-cli`, and
        // both contain that substring, so raw bytes without it can hold
        // no run for us to miss.
        //
        // Without this, wiring a host whose payload shape we do not
        // parse denied EVERY command in the session — `ls` came back
        // « nika run blocked » (2026-08-02). That is the shim's fixed
        // bug one layer down, and it bites the moment a new host is
        // wired, which is exactly when nobody is looking for it.
        if !raw.to_ascii_lowercase().contains("nika") {
            return Err(Verdict::NotOurs);
        }
        return Err(Verdict::Unavailable(
            "the hook payload carries no command — nothing to judge".to_owned(),
        ));
    };
    let cwd = v
        .get("cwd")
        .and_then(|c| c.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    Ok(Input {
        line: command.to_owned(),
        cwd,
        dialect: payload_dialect(raw),
    })
}

/// The hook protocol render.
///
/// Two verdicts used to share one arm, and they are not the same claim.
/// `NotOurs` means there is no `nika run` in the command at all — this
/// guard has no opinion and never looked. `Allow` means a run was
/// judged and found clean. Lumping them emitted `{"permission":"allow"}`
/// for `ls`, for `rm -rf`, for every shell command in the session:
/// installing the nika kit changed how the host treats commands that
/// have nothing to do with nika, which is authority the kit has no
/// business taking.
///
/// So `NotOurs` is `{}` on BOTH wires — the bytes that say « behave as
/// if this hook were not installed ». It is already what the Claude
/// lane emitted, for the reason stated right here in the old comment:
/// the hook teaches, it never widens the user's own permission flow.
/// The generic lane simply did not follow its own rule.
///
/// `Allow` keeps its affirmative on the generic wire, because that one
/// is EARNED — a file the ladder just saw clean. Cursor's docs define
/// `"allow"` as « to proceed » and are silent (checked 2026-08-02 at
/// cursor.com/docs/agent/hooks) on whether it skips the user's
/// confirmation, on what a no-opinion answer looks like, and on how
/// several hooks combine. Under that silence the guard claims only what
/// it measured.
///
/// Deny AND `guard_unavailable` are both denial-SHAPED: an unjudged run
/// never gets the guard's allow, and the reason names the degradation.
fn render_hook(verdict: &Verdict, dialect: Dialect) -> String {
    match verdict {
        Verdict::NotOurs => "{}".to_owned(),
        Verdict::Allow(_) => match dialect {
            Dialect::Claude => "{}".to_owned(),
            Dialect::Generic => r#"{"permission":"allow"}"#.to_owned(),
        },
        Verdict::Deny(reason) => deny_json(reason, dialect),
        Verdict::Unavailable(reason) => deny_json(&format!("guard_unavailable: {reason}"), dialect),
    }
}

/// The two denial envelopes — built through `serde_json`, never
/// hand-interpolated (findings carry quotes and newlines).
fn deny_json(reason: &str, dialect: Dialect) -> String {
    match dialect {
        Dialect::Claude => serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            }
        })
        .to_string(),
        Dialect::Generic => serde_json::json!({
            "permission": "deny",
            "agent_message": reason,
            "user_message": "nika run blocked by the nika guard — repair what the reason names, then rerun.",
        })
        .to_string(),
    }
}

/// The operator reading — one painted line per verdict class.
fn render_human(verdict: &Verdict, theme: Theme) -> String {
    match verdict {
        Verdict::NotOurs => format!(
            "{} — not a nika run (the guard has no opinion)\n",
            theme.paint(Role::Dim, "allow")
        ),
        Verdict::Allow(why) => format!("{} — {why}\n", theme.paint(Role::Good, "allow")),
        Verdict::Deny(why) => format!("{} — {why}\n", theme.paint(Role::Bad, "deny")),
        Verdict::Unavailable(why) => {
            format!("{} — {why}\n", theme.paint(Role::Bad, "guard_unavailable"))
        }
    }
}

/// The exit class of a verdict: allow/no-opinion `0` · deny `2` (a FILE
/// finding) · `guard_unavailable` `3` (the environment failed the judge).
fn exit_of(verdict: &Verdict) -> u8 {
    match verdict {
        Verdict::NotOurs | Verdict::Allow(_) => exit::OK,
        Verdict::Deny(_) => exit::FILE,
        Verdict::Unavailable(_) => exit::ENV,
    }
}

/// Render + grade one verdict — the single fold every input path ends in.
fn finish(verdict: &Verdict, dialect: Dialect, human: bool, theme: Theme) -> VerbOutput {
    // W8 metrics (audit UX 2026-07-30): an ALLOW hands the run back to
    // the human — the guard-side half of human_run_handoff (welcome's
    // run CTA is the other). Content-free · off unless NIKA_METRICS=1.
    if matches!(verdict, Verdict::Allow(_)) {
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::HumanRunHandoff,
            crate::metrics::Facts {
                handoff: Some(crate::metrics::Handoff::GuardAllow),
                ..crate::metrics::Facts::none()
            },
        );
    }
    let text = if human {
        render_human(verdict, theme)
    } else {
        render_hook(verdict, dialect)
    };
    VerbOutput {
        text,
        code: exit_of(verdict),
    }
}

/// Judge one already-parsed input.
fn evaluate(input: &Input, human: bool, theme: Theme) -> VerbOutput {
    let verdict = judge_line(&input.line, input.cwd.as_deref());
    finish(&verdict, input.dialect, human, theme)
}

/// The `nika guard` verb. `--stdin` reads the host hook payload (the
/// shim's wire); `--command <line>` judges one shell command line
/// directly. The default output is the hook JSON protocol; `--human` is
/// the operator reading. The run belongs to the human: guard JUDGES, it
/// never executes.
#[must_use]
pub fn run(
    stdin: bool,
    command: Option<&str>,
    cwd: Option<&str>,
    human: bool,
    theme: Theme,
) -> VerbOutput {
    if stdin {
        let raw = match read_payload(&mut std::io::stdin()) {
            Ok(raw) => raw,
            Err((raw, why)) => {
                let v = Verdict::Unavailable(why);
                return finish(&v, payload_dialect(&raw), human, theme);
            }
        };
        return match parse_payload(&raw) {
            Ok(input) => evaluate(&input, human, theme),
            Err(v) => finish(&v, payload_dialect(&raw), human, theme),
        };
    }
    let Some(line) = command else {
        return VerbOutput::env(
            "guard: pass --stdin (the hook wire) or --command <line>".to_owned(),
        );
    };
    let cwd = cwd
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    let input = Input {
        line: line.to_owned(),
        cwd,
        dialect: Dialect::Generic,
    };
    evaluate(&input, human, theme)
}

#[cfg(test)]
mod tests;
