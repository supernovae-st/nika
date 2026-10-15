// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The guard's shell lexer — quotes honoured, expansions MARKED never
//! performed. A newline ends a command as `;` does, a comment ends with
//! its line, a heredoc body is its command's data, and every command
//! substitution (`$( … )` · backticks · `<( … )`) rides its word as the
//! text the shell would run, so the judge sees each command a multi-line
//! call carries. Split out of `guard.rs` when the lexer stopped reading
//! only a first line that the rest rode unjudged (2026-10-15).

/// The shell operators the lexer emits. `Redirect` and `Paren` stay
/// inside a segment (a redirect's target is never the workflow; a
/// subshell paren is a word-like marker); the rest separate the simple
/// commands of a `;`/newline/`&&`/`||`/`|` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Op {
    And,
    Or,
    Seq,
    Pipe,
    Redirect,
    /// `(` or `)` — the metacharacters: `(nika run x)` is a subshell.
    Paren,
}

/// One lexed token. `dynamic` marks an UNEXPANDED dynamic form (a
/// variable · a command substitution · a glob outside quotes) — the
/// guard judges what a run names, it never guesses an expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Tok {
    pub(super) text: String,
    pub(super) dynamic: bool,
    pub(super) op: Option<Op>,
    /// The command substitutions the word carries, as the text the shell
    /// would run — each is judged like a line of its own
    /// (`X=$(nika run wf.nika)` runs nika before `X` is set).
    pub(super) subs: Vec<String>,
}

impl Tok {
    fn word(text: String, dynamic: bool, subs: Vec<String>) -> Self {
        Self {
            text,
            dynamic,
            op: None,
            subs,
        }
    }

    fn op(text: String, op: Op) -> Self {
        Self {
            text,
            dynamic: false,
            op: Some(op),
            subs: Vec::new(),
        }
    }
}

/// One simple command between operators, with the operators that open
/// and close it (a `cd` inside a pipeline never reaches the parent
/// shell's cwd).
#[derive(Debug)]
pub(super) struct Seg {
    pub(super) toks: Vec<Tok>,
    pub(super) before: Option<Op>,
    pub(super) after: Option<Op>,
}

/// A heredoc whose body starts after the current line: its delimiter,
/// the `<<-` tab strip, whether the body expands (an unquoted
/// delimiter), and the delimiter token that carries the body's
/// substitutions.
struct Pending {
    delimiter: String,
    strip_tabs: bool,
    expands: bool,
    tok: usize,
}

/// The shell tokenizer — quotes honoured, expansions MARKED never
/// performed (the exact class the regex hook's whitespace split got
/// wrong: a quoted path with spaces is ONE token; `$WF` is unknown,
/// not empty).
pub(super) fn lex(line: &str) -> Vec<Tok> {
    let chars: Vec<char> = line.chars().collect();
    let mut toks: Vec<Tok> = Vec::new();
    let mut pending: Vec<Pending> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '\\' && next == Some('\n') {
            i += 2; // a continued line is one line
        } else if c == '\n' {
            // A newline ends a command exactly as `;` does; the bodies
            // of the heredocs its line opened follow it.
            toks.push(Tok::op("\n".to_owned(), Op::Seq));
            i = skip_bodies(&chars, i + 1, &pending, &mut toks);
            pending.clear();
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '#' {
            // A word-start `#` comments out the rest of its LINE — a
            // commented `nika run` is text, the next line a command.
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '('
            && next == Some('(')
            && let Some((tok, after)) = arithmetic(&chars, i)
        {
            toks.push(tok);
            i = after;
        } else if c == '(' || c == ')' {
            toks.push(Tok::op(c.to_string(), Op::Paren));
            i += 1;
        } else if !(matches!(c, '<' | '>') && next == Some('('))
            && let Some((op, len)) = operator_at(&chars, i)
        {
            let text: String = chars[i..i + len].iter().collect();
            let heredoc = opens_heredoc(&text);
            toks.push(Tok::op(text, op));
            i += len;
            if heredoc {
                i = push_heredoc(&chars, i, &mut toks, &mut pending);
            }
        } else {
            let (tok, after) = lex_word(&chars, i);
            let heredoc = tok.op.is_some() && opens_heredoc(&tok.text);
            toks.push(tok);
            i = after;
            if heredoc {
                i = push_heredoc(&chars, i, &mut toks, &mut pending);
            }
        }
    }
    toks
}

/// `<<` (fd-prefixed forms included) opens a heredoc; `<<<` is a
/// here-string whose word is on the line itself.
fn opens_heredoc(text: &str) -> bool {
    text.ends_with("<<") && !text.ends_with("<<<")
}

/// The delimiter after a `<<` that ends at `i`, pushed as the redirect's
/// target word (it carries the body's substitutions) and queued until
/// its line ends. No delimiter, no body: nothing is ever hidden on a
/// guess.
fn push_heredoc(
    chars: &[char],
    i: usize,
    toks: &mut Vec<Tok>,
    pending: &mut Vec<Pending>,
) -> usize {
    let (delimiter, strip_tabs, quoted, after) = heredoc_delimiter(chars, i);
    if !delimiter.is_empty() {
        pending.push(Pending {
            delimiter: delimiter.clone(),
            strip_tabs,
            expands: !quoted,
            tok: toks.len(),
        });
    }
    toks.push(Tok::word(delimiter, false, Vec::new()));
    after
}

/// The delimiter word after a `<<` that ends at `i`: a touching `-` is
/// `<<-` (leading tabs strip), quotes and backslashes fall away and mark
/// the body literal. Returns the delimiter, the tab strip, whether it
/// was quoted, and where the word ends.
fn heredoc_delimiter(chars: &[char], mut i: usize) -> (String, bool, bool, usize) {
    let strip_tabs = chars.get(i) == Some(&'-');
    if strip_tabs {
        i += 1;
    }
    while chars.get(i).is_some_and(|&ch| ch == ' ' || ch == '\t') {
        i += 1;
    }
    let mut delimiter = String::new();
    let mut quoted = false;
    while let Some(&ch) = chars.get(i) {
        match ch {
            '\'' | '"' => {
                quoted = true;
                let close = chars[i + 1..]
                    .iter()
                    .position(|&q| q == ch)
                    .map_or(chars.len(), |n| i + 1 + n);
                delimiter.extend(&chars[i + 1..close]);
                i = close + 1;
            }
            '\\' => {
                quoted = true;
                delimiter.extend(chars.get(i + 1));
                i += 2;
            }
            _ if ch.is_whitespace() || matches!(ch, ';' | '&' | '|' | '(' | ')' | '<' | '>') => {
                break;
            }
            _ => {
                delimiter.push(ch);
                i += 1;
            }
        }
    }
    (delimiter, strip_tabs, quoted, i.min(chars.len()))
}

/// Where a heredoc body starting at `i` ends: the start of the line that
/// is exactly its delimiter (leading tabs stripped for `<<-`) and the
/// index past that line — `None` when no such line comes.
fn heredoc_end(
    chars: &[char],
    i: usize,
    delimiter: &str,
    strip_tabs: bool,
) -> Option<(usize, usize)> {
    let mut line_start = i;
    while line_start < chars.len() {
        let eol = chars[line_start..]
            .iter()
            .position(|&ch| ch == '\n')
            .map_or(chars.len(), |n| line_start + n);
        let line: String = chars[line_start..eol].iter().collect();
        let line: &str = if strip_tabs {
            line.trim_start_matches('\t')
        } else {
            &line
        };
        if line == delimiter {
            return Some((line_start, (eol + 1).min(chars.len())));
        }
        line_start = eol + 1;
    }
    None
}

/// Skip the bodies of the heredocs a line opened, from `i` (just past
/// its newline). A body is DATA, but an unquoted delimiter lets
/// `$( … )` and backticks in it run — those ride the delimiter's token.
/// An unterminated body is never hidden: its lines stay commands (a
/// stray `<<` must not blind the judge).
fn skip_bodies(chars: &[char], mut i: usize, pending: &[Pending], toks: &mut [Tok]) -> usize {
    for p in pending {
        let Some((body_end, after)) = heredoc_end(chars, i, &p.delimiter, p.strip_tabs) else {
            return i;
        };
        if p.expands
            && let Some(tok) = toks.get_mut(p.tok)
        {
            tok.subs.extend(body_subs(&chars[i..body_end]));
        }
        i = after;
    }
    i
}

/// The command substitutions in text the shell expands but never runs
/// as commands — an unquoted heredoc body, an arithmetic, an array
/// list: `$( … )` and backticks there still run.
fn body_subs(chars: &[char]) -> Vec<String> {
    let mut subs = Vec::new();
    let mut scratch = String::new();
    let mut i = 0;
    while i < chars.len() {
        i = match chars[i] {
            '\\' => i + 2,
            '$' if chars.get(i + 1) == Some(&'(') => {
                push_substitution(chars, i + 1, &mut scratch, &mut subs)
            }
            '`' => lex_backtick(chars, i, &mut scratch, &mut subs),
            _ => i + 1,
        };
    }
    subs
}

/// `((…))` is arithmetic when the inner pair closes right before the
/// outer one (bash's own test) — one word whose `$( … )` still runs.
/// Anything else is two subshells, paren by paren. Its `<<` is a shift:
/// read as a heredoc it would hide the lines below.
fn arithmetic(chars: &[char], i: usize) -> Option<(Tok, usize)> {
    let (inner, after) = substitution_body(chars, i + 1);
    if chars.get(after) != Some(&')') {
        return None;
    }
    let subs = body_subs(&inner.chars().collect::<Vec<_>>());
    Some((Tok::word("((".to_owned(), false, subs), after + 1))
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
/// real shell so the word is marked dynamic, and a substitution there
/// still runs (`echo "$(nika run x)"`).
fn lex_double_quote(
    chars: &[char],
    mut i: usize,
    text: &mut String,
    subs: &mut Vec<String>,
) -> (usize, bool) {
    let mut dynamic = false;
    i += 1;
    while i < chars.len() && chars[i] != '"' {
        match chars[i] {
            '\\' if chars.get(i + 1) == Some(&'\n') => i += 2,
            '\\' if i + 1 < chars.len() => {
                text.push(chars[i + 1]);
                i += 2;
            }
            '$' if chars.get(i + 1) == Some(&'(') => {
                dynamic = true;
                i = push_substitution(chars, i + 1, text, subs);
            }
            '`' => {
                dynamic = true;
                i = lex_backtick(chars, i, text, subs);
            }
            ch => {
                dynamic |= ch == '$';
                text.push(ch);
                i += 1;
            }
        }
    }
    ((i + 1).min(chars.len()), dynamic)
}

/// A `$` — a substitution (`$( … )`), a braced or bracketed expansion
/// (`${ … }` · `$[ … ]`: one word, spaces and `#` included, whose own
/// substitutions still run), an ANSI-C string (`$'…'`), or a variable.
fn lex_dollar(chars: &[char], i: usize, text: &mut String, subs: &mut Vec<String>) -> usize {
    match chars.get(i + 1) {
        Some('(') => push_substitution(chars, i + 1, text, subs),
        Some(&open @ ('{' | '[')) => {
            let (body, after) = substitution_body(chars, i + 1);
            subs.extend(body_subs(&body.chars().collect::<Vec<_>>()));
            text.push('$');
            text.push(open);
            text.push_str(&body);
            text.push(if open == '{' { '}' } else { ']' });
            after
        }
        Some('\'') => lex_ansi_c(chars, i + 1, text),
        _ => {
            text.push('$');
            i + 1
        }
    }
}

/// A `$'…'` string from its quote at `i` — literal like `'…'`, except
/// that `\'` escapes a quote instead of closing the string.
fn lex_ansi_c(chars: &[char], mut i: usize, text: &mut String) -> usize {
    i += 1;
    while i < chars.len() && chars[i] != '\'' {
        if chars[i] == '\\' && i + 1 < chars.len() {
            text.push(chars[i + 1]);
            i += 2;
            continue;
        }
        text.push(chars[i]);
        i += 1;
    }
    (i + 1).min(chars.len())
}

/// The parenthesised substitution opening at `open`: one dynamic span
/// of the word's text, and a line of its own in `subs`.
fn push_substitution(
    chars: &[char],
    open: usize,
    text: &mut String,
    subs: &mut Vec<String>,
) -> usize {
    let (body, after) = substitution_body(chars, open);
    text.push_str(&body);
    subs.push(body);
    after
}

/// The body of the bracketed span opening at `open` (its `(`, `{` or
/// `[`): all up to the matching closer — nested pairs, quoted and
/// escaped closers kept — so the body reads as the shell would run it.
/// Inside `( … )` the shell reads commands: a closer in a comment or a
/// heredoc body never closes the span (`$(cat <<'EOF'` with a `1)` in
/// the message is still one substitution).
fn substitution_body(chars: &[char], open: usize) -> (String, usize) {
    let opener = chars[open];
    let closer = match opener {
        '{' => '}',
        '[' => ']',
        _ => ')',
    };
    let commands = opener == '(';
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut heredocs: Vec<(String, bool)> = Vec::new();
    let mut i = open;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '\\' && quote != Some('\'') && next.is_some() {
            i += 2;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if matches!(c, '\'' | '"') {
            quote = Some(c);
        } else if commands && c == '#' && word_start(chars, i) {
            while chars.get(i + 1).is_some_and(|&ch| ch != '\n') {
                i += 1;
            }
        } else if commands && c == '<' && next == Some('<') {
            if chars.get(i + 2) == Some(&'<') {
                i += 3; // a here-string
                continue;
            }
            let (delimiter, strip_tabs, _, after) = heredoc_delimiter(chars, i + 2);
            if !delimiter.is_empty() {
                heredocs.push((delimiter, strip_tabs));
            }
            i = after;
            continue;
        } else if c == '\n' && !heredocs.is_empty() {
            i += 1;
            for (delimiter, strip_tabs) in std::mem::take(&mut heredocs) {
                i = heredoc_end(chars, i, &delimiter, strip_tabs)
                    .map_or(chars.len(), |(_, after)| after);
            }
            continue;
        } else if c == opener {
            depth += 1;
        } else if c == closer {
            depth -= 1;
            if depth == 0 {
                return (chars[open + 1..i].iter().collect(), i + 1);
            }
        }
        i += 1;
    }
    (chars[(open + 1).min(chars.len())..].iter().collect(), i)
}

/// Whether `i` starts a word — where a `#` opens a comment.
fn word_start(chars: &[char], i: usize) -> bool {
    i == 0 || chars[i - 1].is_whitespace() || matches!(chars[i - 1], ';' | '&' | '|' | '(' | ')')
}

/// A backtick command substitution — one dynamic span; its body joins
/// `subs` for judging.
fn lex_backtick(chars: &[char], mut i: usize, text: &mut String, subs: &mut Vec<String>) -> usize {
    i += 1;
    let mut body = String::new();
    while i < chars.len() && chars[i] != '`' {
        body.push(chars[i]);
        i += 1;
    }
    text.push_str(&body);
    subs.push(body);
    (i + 1).min(chars.len())
}

/// One word from `start` — quotes delegated to the span helpers, unquoted
/// `$`/`*?[`/backticks marked dynamic. A trailing digit-run before
/// `>`/`<` folds into the redirect token (`2>/dev/null`). A `#` inside
/// a word is a letter (`a#b` · a URL fragment), never a comment.
fn lex_word(chars: &[char], start: usize) -> (Tok, usize) {
    let mut text = String::new();
    let mut dynamic = false;
    let mut subs = Vec::new();
    let mut i = start;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        // A process substitution (`<( … )` · `>( … )` · zsh's `=( … )`)
        // runs its body like `$( … )` and stands for a path.
        if next == Some('(') && (matches!(c, '<' | '>') || (c == '=' && i == start)) {
            dynamic = true;
            text.push(c);
            i = push_substitution(chars, i + 1, &mut text, &mut subs);
            continue;
        }
        if c.is_whitespace()
            || matches!(c, '&' | '|' | ';' | ')')
            || (c == '(' && !text.ends_with('='))
        {
            break;
        }
        if c == '>' || c == '<' {
            if !text.is_empty() && text.chars().all(|d| d.is_ascii_digit()) {
                // `2>` — the fd prefix belongs to the redirect token.
                let (op, len) = operator_at(chars, i).unwrap_or((Op::Redirect, 1));
                text.push_str(&chars[i..i + len].iter().collect::<String>());
                return (Tok::op(text, op), i + len);
            }
            break;
        }
        match c {
            // `name=( … )` — an array: one word, its substitutions run.
            '(' => {
                let (body, after) = substitution_body(chars, i);
                subs.extend(body_subs(&body.chars().collect::<Vec<_>>()));
                text.push('(');
                text.push_str(&body);
                text.push(')');
                i = after;
            }
            '\'' => i = lex_single_quote(chars, i, &mut text),
            '"' => {
                let (after, span_dynamic) = lex_double_quote(chars, i, &mut text, &mut subs);
                dynamic |= span_dynamic;
                i = after;
            }
            // A backslash-newline inside a word joins the two lines.
            '\\' if next == Some('\n') => i += 2,
            '\\' if next.is_some() => {
                text.push(chars[i + 1]);
                i += 2;
            }
            '$' => {
                dynamic = true;
                i = lex_dollar(chars, i, &mut text, &mut subs);
            }
            '`' => {
                dynamic = true;
                i = lex_backtick(chars, i, &mut text, &mut subs);
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
    // `[` and `[[` alone are the test command, not a glob bracket.
    if matches!(text.as_str(), "[" | "[[") {
        dynamic = false;
    }
    (Tok::word(text, dynamic, subs), i)
}

/// Split the token stream into simple commands, remembering the joining
/// operators (a `cd` only reaches segments the pipeline law allows).
pub(super) fn segments(toks: &[Tok]) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut before: Option<Op> = None;
    for t in toks {
        match t.op {
            Some(op) if !matches!(op, Op::Redirect | Op::Paren) => {
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
/// operator and its target (a duplication like `2>&1` carries its own:
/// the next word is a word again).
pub(super) fn strip_redirects(toks: &[Tok]) -> Vec<&Tok> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < toks.len() {
        if toks[k].op == Some(Op::Redirect) {
            let text = &toks[k].text;
            let attached = text.contains('&') && text.ends_with(|c: char| c.is_ascii_digit());
            k += 1;
            if !attached && k < toks.len() && toks[k].op.is_none() {
                k += 1; // the target word
            }
            continue;
        }
        out.push(&toks[k]);
        k += 1;
    }
    out
}
