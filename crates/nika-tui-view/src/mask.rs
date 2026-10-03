// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Masking a protected object along its structure. One walker
//! ([`Walker`]) decides which values a protected object keeps off the
//! screen: every value inside a member whose key names a credential,
//! however deep, and every value that wears a credential's shape or holds
//! one (`secret` judges keys and shapes). The JSON document, the values of
//! a JSON Patch and the edits of a merge patch walk their tokens and trees
//! through it; the line-oriented views walk what a tolerant reading of
//! each line shows ([`Lines`]: keys, brackets, quotes, list marks) and
//! what spans lines: a bracket left open, a block scalar or a mapping
//! nested under a key, a multi-line string, a TOML table. Only values are
//! masked: keys, separators, brackets, quotes, block indicators and fences
//! stay, so a view that highlights a masked line still reads its shape.

use super::secret;

/// The one masking walker, fed a value's structure in document order: a
/// member's key, a container opening and closing, a scalar, the end of a
/// member's value.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Walker {
    /// The object is protected.
    on: bool,
    /// A string is judged by its shape alone, never read as a line.
    flat: bool,
    /// The value being read belongs to a key that names a credential.
    next: bool,
    /// Every scalar deeper than this depth belongs to such a key.
    from: Option<usize>,
    /// Values masked so far.
    masked: usize,
}

impl Walker {
    /// A walker for an object protected (`on`) or not.
    pub(crate) fn new(on: bool) -> Self {
        Self {
            on,
            ..Self::default()
        }
    }

    /// Whether the object is protected.
    pub(crate) const fn on(&self) -> bool {
        self.on
    }

    /// A member's key was read: its value follows.
    pub(crate) fn key(&mut self, key: &str) {
        self.next = self.next || (self.on && secret::secret_key(key));
    }

    /// A container opens at `depth` (the containers around it): all it
    /// holds belongs to the key whose value it is.
    pub(crate) fn open(&mut self, depth: usize) {
        if self.next {
            self.from.get_or_insert(depth);
        }
        self.next = false;
    }

    /// The container opened at `depth` closes.
    pub(crate) fn close(&mut self, depth: usize) {
        if self.from.is_some_and(|from| from >= depth) {
            self.from = None;
        }
    }

    /// The value of the last member ended.
    pub(crate) fn end(&mut self) {
        self.next = false;
    }

    /// Whether a scalar read at `depth` belongs to a key naming a credential.
    fn owned(&self, depth: usize) -> bool {
        self.next || self.from.is_some_and(|from| depth > from)
    }

    /// Whether the scalar `text`, read at `depth`, is masked: it belongs to
    /// a key naming a credential or wears a credential's shape, or it is a
    /// `string` that holds one. A template reference names a secret
    /// without being one: it shows.
    pub(crate) fn hides(&self, depth: usize, text: &str, string: bool) -> bool {
        self.on
            && !secret::reference(text)
            && (self.owned(depth)
                || secret::secret_value(text)
                || (string && !self.flat && holds(text)))
    }

    /// [`Self::hides`], counted when it masks.
    pub(crate) fn hide(&mut self, depth: usize, text: &str, string: bool) -> bool {
        let hidden = self.hides(depth, text, string);
        self.masked += usize::from(hidden);
        hidden
    }

    /// Values masked so far.
    pub(crate) const fn masked(&self) -> usize {
        self.masked
    }
}

/// Whether `text`, read as a line, holds a credential (`PASSWORD=x`,
/// `Authorization: Bearer x`): a string that carries one is masked whole.
fn holds(text: &str) -> bool {
    let mut lines = Lines::default();
    lines.walker.flat = true;
    lines.line(text, true).1 > 0
}

/// One line masked on its own, and how many values were.
pub(crate) fn text(line: &str, ascii: bool) -> (String, usize) {
    Lines::default().line(line, ascii)
}

/// What a tolerant reading of a line sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Spaces.
    Space,
    /// A list mark where the line starts (`- `, `* `, `+ `).
    Mark,
    /// `{`, `[` or `(`.
    Open,
    /// `}`, `]` or `)`.
    Close,
    /// `,` or `;`: a member's value ends.
    Comma,
    /// What follows a key: `:`, `=`, `:=`, `=>`, `==`.
    Colon,
    /// A bare word.
    Word,
    /// A quoted string, its quotes included.
    Quoted,
    /// A template reference, `${{ … }}`.
    Template,
    /// A triple quote that opens a string going on past the line, and the
    /// rest of the line.
    Fence,
}

/// One token: its kind and its bytes.
type Token = (Kind, usize, usize);

/// The characters that wrap a word without being it.
const WRAP: [char; 6] = ['`', '<', '>', '"', '\'', '*'];

/// A quoted string `rest` starts with (`quote`): to its closing quote, or
/// to the end of the line; a triple quote not closed on the line opens a
/// fence.
fn quoted(rest: &str, quote: char) -> (Kind, usize) {
    let triple = if quote == '"' { "\"\"\"" } else { "'''" };
    if let Some(after) = rest.strip_prefix(triple) {
        return match after.find(triple) {
            Some(end) => (Kind::Quoted, end + 6),
            None => (Kind::Fence, rest.len()),
        };
    }
    let mut chars = rest.char_indices().skip(1);
    while let Some((at, c)) = chars.next() {
        let escaped = c == '\\' && quote == '"';
        let doubled = c == quote && quote == '\'' && rest[at + 1..].starts_with('\'');
        if escaped || doubled {
            chars.next();
        } else if c == quote {
            return (Kind::Quoted, at + 1);
        }
    }
    (Kind::Quoted, rest.len())
}

/// The length of the bare word `rest` starts with: it runs to a space, a
/// bracket, a separator or a template, and past a colon that follows no
/// key (`http://x`, `12:30`, `nika:read`, `a::b`).
fn word(rest: &str) -> usize {
    for (at, c) in rest.char_indices().skip(1) {
        let colon = c == ':'
            && rest[at + 1..]
                .chars()
                .next()
                .is_none_or(|n| n.is_whitespace() || matches!(n, '"' | '\'' | '='));
        let stops = colon
            || c.is_whitespace()
            || matches!(c, '{' | '}' | '[' | ']' | '(' | ')' | ',' | ';' | '=')
            || rest[at..].starts_with("${{");
        if stops {
            return at;
        }
    }
    rest.len()
}

/// The kind and the length of the token `rest` starts with (`before`: the
/// character before it on the line).
fn token(rest: &str, before: Option<char>) -> (Kind, usize) {
    let Some(c) = rest.chars().next() else {
        return (Kind::Space, rest.len());
    };
    if c.is_whitespace() {
        let end = rest.find(|x: char| !x.is_whitespace());
        return (Kind::Space, end.unwrap_or(rest.len()));
    }
    if rest.starts_with("${{") {
        let end = rest.find("}}").map_or(rest.len(), |at| at + 2);
        return (Kind::Template, end);
    }
    let boundary = before
        .is_none_or(|b| b.is_whitespace() || matches!(b, ':' | '[' | '{' | '(' | ',' | '=' | ';'));
    if matches!(c, '"' | '\'') && boundary {
        return quoted(rest, c);
    }
    let two = |second: &[char]| if rest[1..].starts_with(second) { 2 } else { 1 };
    match c {
        '{' | '[' | '(' => (Kind::Open, 1),
        '}' | ']' | ')' => (Kind::Close, 1),
        ',' | ';' => (Kind::Comma, 1),
        '=' => (Kind::Colon, two(&['>', '='])),
        ':' if !rest[1..].starts_with(':') => (Kind::Colon, two(&['='])),
        _ => (Kind::Word, word(rest)),
    }
}

/// The tokens of one line.
fn lex(line: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut at = line.len() - line.trim_start_matches(' ').len();
    if at > 0 {
        out.push((Kind::Space, 0, at));
    }
    while let Some(rest) = line[at..]
        .strip_prefix(['-', '*', '+'])
        .filter(|rest| rest.starts_with(' '))
    {
        let spaces = rest.len() - rest.trim_start_matches(' ').len();
        out.push((Kind::Mark, at, at + 1));
        out.push((Kind::Space, at + 1, at + 1 + spaces));
        at += 1 + spaces;
    }
    while at < line.len() {
        let (kind, len) = token(&line[at..], line[..at].chars().next_back());
        out.push((kind, at, at + len));
        at += len;
    }
    out
}

/// How a line ends, for what it leaves open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tail {
    /// A key and its separator: the value is on the lines below.
    Key,
    /// A block scalar indicator (`|`, `>-`, `|2`) after a key or a mark.
    Block,
    /// A multi-line string opened by this fence.
    Fence(&'static str),
    /// Anything else.
    Other,
}

/// Whether a word is a block scalar indicator (`|`, `>`, `|-`, `>+2`).
fn indicator(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some('|' | '>'))
        && text.len() <= 3
        && chars.all(|c| c.is_ascii_digit() || matches!(c, '+' | '-'))
}

/// Where a trailing comment starts: the first word opening with `#`, else
/// the end of the tokens. A comment carries no structure: its brackets
/// open and close nothing, its keys still mask what follows them.
fn comment(line: &str, tokens: &[Token]) -> usize {
    tokens
        .iter()
        .position(|&(kind, start, _)| kind == Kind::Word && line[start..].starts_with('#'))
        .unwrap_or(tokens.len())
}

/// How a line ends before its comment (`notes`: where it starts) and the
/// index of its last token that is not a space.
fn tail(line: &str, tokens: &[Token], notes: usize) -> (Tail, usize) {
    let shown: Vec<usize> = (0..notes).filter(|&i| tokens[i].0 != Kind::Space).collect();
    let Some(&last) = shown.last() else {
        return (Tail::Other, usize::MAX);
    };
    let (kind, start, end) = tokens[last];
    let tail = match kind {
        Kind::Colon => Tail::Key,
        Kind::Fence if line[start..].starts_with('\'') => Tail::Fence("'''"),
        Kind::Fence => Tail::Fence("\"\"\""),
        Kind::Word if indicator(&line[start..end]) => {
            let introduced = shown.iter().rev().skip(1).find_map(|&i| {
                let (kind, start, _) = tokens[i];
                match kind {
                    Kind::Colon | Kind::Mark => Some(true),
                    Kind::Word if line[start..].starts_with(['!', '&']) => None,
                    _ => Some(false),
                }
            });
            if introduced == Some(true) {
                Tail::Block
            } else {
                Tail::Other
            }
        }
        _ => Tail::Other,
    };
    (tail, last)
}

/// The quotes around a string token: how many bytes open it and how many
/// close it (none when the line ends first).
fn quotes(kind: Kind, text: &str) -> (usize, usize) {
    let triple = |t: &str| text.len() >= 6 && text.starts_with(t) && text.ends_with(t);
    match kind {
        Kind::Fence => (3, 0),
        Kind::Quoted if triple("\"\"\"") || triple("'''") => (3, 3),
        Kind::Quoted if text.len() >= 2 && text.ends_with(&text[..1]) => (1, 1),
        Kind::Quoted => (1, 0),
        _ => (0, 0),
    }
}

/// What a token says: a string without its quotes, a word without its
/// wrappers.
fn content(kind: Kind, text: &str) -> &str {
    match kind {
        Kind::Word => text.trim_matches(WRAP),
        Kind::Quoted | Kind::Fence => {
            let (open, close) = quotes(kind, text);
            &text[open..text.len() - close]
        }
        _ => text,
    }
}

/// A hidden token as it shows: a string keeps its quotes, a fence its
/// triple quote, a word its wrappers around the mask.
fn veil(kind: Kind, text: &str, ascii: bool) -> String {
    let mask = secret::mask(ascii);
    match kind {
        Kind::Quoted | Kind::Fence => {
            let (open, close) = quotes(kind, text);
            format!("{}{mask}{}", &text[..open], &text[text.len() - close..])
        }
        Kind::Word if !content(kind, text).is_empty() => {
            text.replacen(content(kind, text), mask, 1)
        }
        _ => mask.to_owned(),
    }
}

/// Whether a word names an authorization scheme: what follows it is the
/// credential (`Bearer x`, `Authorization:Basic x`).
fn scheme(text: &str) -> bool {
    text.trim_matches(WRAP)
        .rsplit(|c: char| !c.is_alphanumeric())
        .next()
        .is_some_and(|w| w.eq_ignore_ascii_case("bearer") || w.eq_ignore_ascii_case("basic"))
}

/// What a line leaves open for the lines after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    /// A multi-line string: its fence, and whether it belongs to a key
    /// naming a credential.
    Fence(&'static str, bool),
    /// A block scalar under a line indented this much, and whether it
    /// belongs to a key naming a credential.
    Block(usize, bool),
}

/// A line inside a multi-line value: masked whole, its indent kept, when
/// the value belongs to a key naming a credential; read on its own else.
fn inside(line: &str, hidden: bool, ascii: bool) -> (String, usize) {
    let body = line.trim_start();
    if body.is_empty() {
        return (line.to_owned(), 0);
    }
    if hidden {
        let indent = &line[..line.len() - body.len()];
        return (format!("{indent}{}", secret::mask(ascii)), 1);
    }
    text(line, ascii)
}

/// The name of a TOML or INI table header (`[auth]`, `[[users]]`,
/// `[db.conn]`) when the line is one.
fn table(line: &str) -> Option<&str> {
    let body = line.split(" #").next().unwrap_or_default().trim();
    let inner = body.strip_prefix('[')?.strip_suffix(']')?;
    let inner = inner
        .strip_prefix('[')
        .and_then(|i| i.strip_suffix(']'))
        .unwrap_or(inner)
        .trim();
    let named = inner.chars().any(char::is_alphanumeric)
        && inner
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '));
    named.then_some(inner)
}

/// The lines of a protected text, masked along the structure they share.
#[derive(Debug)]
pub(crate) struct Lines {
    walker: Walker,
    /// Brackets the lines before left open.
    depth: usize,
    /// A multi-line string or a block scalar in progress.
    open: Option<Open>,
    /// Lines indented deeper than this sit under a key naming a credential.
    under: Option<usize>,
    /// The current TOML or INI table names a credential.
    table: bool,
}

impl Default for Lines {
    fn default() -> Self {
        Self {
            walker: Walker::new(true),
            depth: 0,
            open: None,
            under: None,
            table: false,
        }
    }
}

impl Lines {
    /// One line (its control characters already made visible) with its
    /// secret-looking values masked, and how many were.
    pub(crate) fn line(&mut self, line: &str, ascii: bool) -> (String, usize) {
        let indent = line.len() - line.trim_start_matches(' ').len();
        let body = line.trim_start();
        match self.open {
            Some(Open::Fence(fence, hidden)) => return self.fenced(line, fence, hidden, ascii),
            Some(Open::Block(at, hidden)) if body.is_empty() || indent > at => {
                return inside(line, hidden, ascii);
            }
            Some(Open::Block(..)) => self.open = None,
            None => {}
        }
        let goes_on = body.starts_with("- ") || body.starts_with(['{', '[']);
        if !body.is_empty()
            && self
                .under
                .is_some_and(|at| indent < at || (indent == at && !goes_on))
        {
            self.under = None;
        }
        if self.depth == 0
            && self.under.is_none()
            && indent == 0
            && let Some(name) = table(line)
            && !secret::secret_value(name)
        {
            self.table = secret::secret_key(name);
            return (line.to_owned(), 0);
        }
        let forced = self.table || self.under.is_some();
        let (out, masked, tail) = self.scan(line, forced, ascii);
        let owned = forced || self.walker.owned(self.depth);
        match tail {
            Tail::Key if self.walker.next && !forced => self.under = Some(indent),
            Tail::Block => self.open = Some(Open::Block(indent, owned)),
            Tail::Fence(fence) => self.open = Some(Open::Fence(fence, owned)),
            Tail::Key | Tail::Other => {}
        }
        self.walker.end();
        (out, masked)
    }

    /// A line of a multi-line string, up to the fence that closes it; what
    /// follows the fence is read as the rest of a line.
    fn fenced(
        &mut self,
        line: &str,
        fence: &'static str,
        hidden: bool,
        ascii: bool,
    ) -> (String, usize) {
        let Some(at) = line.find(fence) else {
            return inside(line, hidden, ascii);
        };
        self.open = None;
        let (mut out, mut masked) = inside(&line[..at], hidden, ascii);
        out.push_str(fence);
        let forced = self.table || self.under.is_some();
        let (rest, more, _) = self.scan(&line[at + fence.len()..], forced, ascii);
        self.walker.end();
        out.push_str(&rest);
        masked += more;
        (out, masked)
    }

    /// Whether the token at `index` is a key: a separator follows it, the
    /// value before it is over, and it does not wear a credential's shape
    /// (`https://user:pw@host?q=1` is a value, not the key of `1`).
    fn keyed(&self, tokens: &[Token], index: usize, text: &str) -> bool {
        let (kind, ..) = tokens[index];
        kind != Kind::Fence
            && !self.walker.next
            && tokens[index + 1..]
                .iter()
                .find(|t| t.0 != Kind::Space)
                .is_some_and(|t| t.0 == Kind::Colon)
            && !secret::secret_value(content(kind, text))
    }

    /// Whether the value token `text` is masked (`forced`: every value of
    /// the line belongs to a key naming a credential; `after`: it follows
    /// an authorization scheme).
    fn judge(&self, kind: Kind, text: &str, forced: bool, after: bool) -> bool {
        let inner = content(kind, text);
        if secret::reference(inner) || (kind == Kind::Fence && inner.trim().is_empty()) {
            return false;
        }
        let string = matches!(kind, Kind::Quoted | Kind::Fence);
        forced || after || self.walker.hides(self.depth, inner, string)
    }

    /// The tokens of one line through the walker: keys and structure kept,
    /// every value the walker hides masked (a run of hidden words as one
    /// mask), brackets carried to the lines after.
    fn scan(&mut self, line: &str, forced: bool, ascii: bool) -> (String, usize, Tail) {
        let tokens = lex(line);
        let notes = comment(line, &tokens);
        let (tail, last) = tail(line, &tokens, notes);
        let mut out = String::with_capacity(line.len());
        let (mut masked, mut joined, mut after, mut pending) = (0, false, false, "");
        for (index, &(kind, start, end)) in tokens.iter().enumerate() {
            let text = &line[start..end];
            let hidden = match kind {
                Kind::Space => {
                    if joined {
                        pending = text;
                    } else {
                        out.push_str(text);
                    }
                    continue;
                }
                Kind::Open | Kind::Close if index >= notes => false,
                Kind::Open => {
                    self.walker.open(self.depth);
                    self.depth += 1;
                    false
                }
                Kind::Close => {
                    if let Some(depth) = self.depth.checked_sub(1) {
                        self.depth = depth;
                        self.walker.close(depth);
                    }
                    false
                }
                Kind::Comma => {
                    self.walker.end();
                    false
                }
                Kind::Mark | Kind::Colon => false,
                _ if index == last && tail == Tail::Block => false,
                _ if self.keyed(&tokens, index, text) => {
                    self.walker.key(content(kind, text));
                    false
                }
                _ => self.judge(kind, text, forced, after),
            };
            after = kind == Kind::Word && scheme(text);
            if hidden && joined && kind == Kind::Word {
                pending = "";
                continue;
            }
            out.push_str(pending);
            pending = "";
            if hidden {
                out.push_str(&veil(kind, text, ascii));
                masked += 1;
            } else {
                out.push_str(text);
            }
            joined = hidden && kind == Kind::Word;
        }
        out.push_str(pending);
        (out, masked, tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> (Vec<String>, usize) {
        let mut lines = Lines::default();
        let mut total = 0;
        let shown = text
            .lines()
            .map(|line| {
                let (shown, masked) = lines.line(line, true);
                total += masked;
                shown
            })
            .collect();
        (shown, total)
    }

    #[test]
    fn a_protected_line_never_shows_a_secret_in_clear() {
        assert_eq!(
            text("  api_key: sk-live-0123456789", false),
            ("  api_key: ••••••".to_owned(), 1)
        );
        assert_eq!(
            text("\"password\": \"hunter2\",", true),
            ("\"password\": \"******\",".to_owned(), 1)
        );
        let (line, n) = text("curl -H 'Authorization: Bearer abc123' https://x", true);
        assert!(!line.contains("abc123"), "{line}");
        assert_eq!(n, 1);
        assert_eq!(
            text("token: ${{ secrets.token }}", false),
            ("token: ${{ secrets.token }}".to_owned(), 0)
        );
        assert_eq!(
            text("plain words stay", false),
            ("plain words stay".to_owned(), 0)
        );
    }

    #[test]
    fn keys_anywhere_on_a_line_mask_their_values_and_only_them() {
        assert_eq!(
            text("let password = \"hunter2\"; let user = \"ann\";", true).0,
            "let password = \"******\"; let user = \"ann\";"
        );
        assert_eq!(
            text("{ name: \"ann\", password: \"hunter2\" },", true).0,
            "{ name: \"ann\", password: \"******\" },"
        );
        assert_eq!(
            text("db = { conn = { password = \"x\" }, port = 1 }", true).0,
            "db = { conn = { password = \"******\" }, port = 1 }"
        );
        assert_eq!(
            text("password: correct horse battery staple", true),
            ("password: ******".to_owned(), 1),
            "one value, one mask, whatever its words"
        );
        assert_eq!(
            text("const API_KEY: &str = \"hunter2\";", true).0,
            "const API_KEY: ****** = \"******\";"
        );
        assert_eq!(
            text("env: [\"DB_PASSWORD=hunter2\", \"PORT=1\"]", true).0,
            "env: [\"******\", \"PORT=1\"]",
            "a string that holds a credential is masked whole"
        );
        assert_eq!(
            text("url: https://user:pw@example.com/x time: 12:30", true).0,
            "url: ****** time: 12:30"
        );
        assert_eq!(
            text("see https://user:pw@example.com/x?q=1", true).0,
            "see ******=1",
            "a value wearing a credential's shape is never read as a key"
        );
    }

    #[test]
    fn what_spans_lines_stays_masked_to_its_end() {
        let (shown, masked) = lines("password: |\n  line one\n\n  line two\nuser: ann\n");
        assert_eq!(
            shown,
            ["password: |", "  ******", "", "  ******", "user: ann"]
        );
        assert_eq!(masked, 2);
        let (shown, _) = lines("password = \"\"\"\nhunter2\n\"\"\"\nnext = 1\n");
        assert_eq!(shown, ["password = \"\"\"", "******", "\"\"\"", "next = 1"]);
        let (shown, _) = lines("auth:\n  user: ann\n  pass: x\nopen: 1\n");
        assert_eq!(
            shown,
            ["auth:", "  user: ******", "  pass: ******", "open: 1"]
        );
        let (shown, _) = lines("api_keys:\n- one\n- two\nnext: 1\n");
        assert_eq!(shown, ["api_keys:", "- ******", "- ******", "next: 1"]);
        let (shown, _) = lines("[auth]\npass = \"x\"\n[server]\nport = 1\n");
        assert_eq!(
            shown,
            ["[auth]", "pass = \"******\"", "[server]", "port = 1"]
        );
        let (shown, _) = lines("auth = {\n  \"pass\": \"x\",\n}\nport = 1\n");
        assert_eq!(
            shown,
            ["auth = {", "  \"pass\": \"******\",", "}", "port = 1"]
        );
    }

    #[test]
    fn a_comment_opens_no_bracket_and_a_shaped_name_is_no_table() {
        let (shown, _) = lines("password: x # (see the vault\nuser: ann\n# token: abc\n");
        assert_eq!(
            shown,
            ["password: ****** (******", "user: ann", "# token: ******"]
        );
        let (shown, _) = lines("[sk-live-0123456789abcdef]\nport = 1\n");
        assert_eq!(shown, ["[******]", "port = 1"]);
    }

    #[test]
    fn structure_stays_readable_and_references_stay_whole() {
        let (shown, masked) =
            lines("prompt: |\n  Say hello to ${{ inputs.name }}\nkey: ${{ secrets.key }}\n");
        assert_eq!(
            shown,
            [
                "prompt: |",
                "  Say hello to ${{ inputs.name }}",
                "key: ${{ secrets.key }}"
            ]
        );
        assert_eq!(masked, 0);
        let (shown, _) = lines("password: !!binary |\n  aGVsbG8=\n");
        assert_eq!(shown, ["password: ****** |", "  ******"]);
        assert_eq!(
            text("Use `Bearer abc123` here", true).0,
            "Use `Bearer ******` here"
        );
    }

    #[test]
    fn the_walker_masks_what_a_secret_key_holds_however_deep() {
        let mut walker = Walker::new(true);
        walker.key("auth");
        walker.open(0);
        walker.key("user");
        assert!(
            walker.hide(1, "ann", true),
            "under a key naming a credential"
        );
        walker.end();
        walker.close(0);
        walker.key("user");
        assert!(!walker.hide(0, "ann", true));
        assert!(walker.hide(0, "sk-live-0123456789", true), "by its shape");
        assert!(walker.hide(0, "PASSWORD=x", true), "a string holding one");
        assert!(
            !walker.hide(0, "PASSWORD=x", false),
            "a number or literal is not read"
        );
        assert!(!walker.hide(0, "${{ secrets.token }}", true), "a reference");
        assert_eq!(walker.masked(), 3);
        let mut off = Walker::new(false);
        off.key("password");
        assert!(
            !off.hide(0, "hunter2", true),
            "an open object masks nothing"
        );
    }
}
