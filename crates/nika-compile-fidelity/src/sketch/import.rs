// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A base no semantic record binds, read from its complete parsed document (slice F, pure): the
//! paths it writes, the parsed slots that carry one written destination, the destination a typed
//! link replaces and the one it puts in its place, and the proof that a revised document is the
//! base with exactly those slots changed. No bytes and no parser here: the compile core parses
//! both documents with its literal projection and owns every byte it emits.
//!
//! [`slots`]: a destination is carried by each `nika:write` whose `args.path` states it (a
//! literal, or a bare constant no other field reads) and by each `permits.fs.write` entry naming
//! it. A write whose path is neither is refused: its destination cannot be proven unchanged.
//!
//! [`proven`]: the revised document is the base with each slot set to its new value and every
//! other field equal — every task, verb, tool, edge, option, calculation, output and permit.
//!
//! [`replaced`] and [`replacing`]: the link's original clause states the destination it
//! replaces (a path the base writes that no other clause of the change states again), and its
//! change clause states exactly one path the base names nowhere.

/// Pure source revision records and their exact replay bindings.
pub mod record;

use nika_compile_reader::hot::{stated_destinations, stated_sources};
use serde_json::Value;

/// The same path, whatever a leading `./` says.
#[must_use]
pub fn same(a: &str, b: &str) -> bool {
    let bare = |p: &str| p.trim().trim_start_matches("./").to_owned();
    bare(a) == bare(b)
}

/// Every path `text` states, as the reader's path law reads it: its sources, then its
/// destinations, each once.
#[must_use]
pub fn stated(text: &str) -> Vec<String> {
    let mut paths = stated_sources(text);
    for path in stated_destinations(text) {
        if !paths.iter().any(|p| same(p, &path)) {
            paths.push(path);
        }
    }
    paths
}

/// An RFC 6901 pointer segment.
fn segment(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// The constant a value reads when it is exactly `${{ const.NAME }}`.
fn bare_constant(value: &str) -> Option<&str> {
    let inner = value.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
    let name = inner.strip_prefix("const.")?;
    name.bytes()
        .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        .then_some(name)
}

/// One write of the base: the pointer of its `args.path`, and where its destination is stated
/// (that pointer itself, or the constant it reads) with the destination.
struct Write {
    path_at: String,
    slot: String,
    destination: String,
}

/// Every `nika:write` of `doc`, in task order.
///
/// # Errors
/// A write whose path is neither a literal nor a bare constant the document defines.
fn writes(doc: &Value) -> Result<Vec<Write>, String> {
    let mut found = Vec::new();
    for (id, task) in doc["tasks"].as_object().into_iter().flatten() {
        if task.pointer("/invoke/tool").and_then(Value::as_str) != Some("nika:write") {
            continue;
        }
        let path_at = format!("/tasks/{}/invoke/args/path", segment(id));
        let stated = doc.pointer(&path_at).and_then(Value::as_str);
        let resolved = match stated {
            Some(value) if !value.contains("${{") => Some((path_at.clone(), value.to_owned())),
            Some(value) => bare_constant(value).and_then(|name| {
                let at = format!("/const/{}", segment(name));
                doc.pointer(&at)
                    .and_then(Value::as_str)
                    .map(|v| (at.clone(), v.to_owned()))
            }),
            None => None,
        };
        let Some((slot, destination)) = resolved else {
            return Err(format!(
                "the write `{id}` states its destination neither as a literal nor as a constant: it is not revised in place"
            ));
        };
        found.push(Write {
            path_at,
            slot,
            destination,
        });
    }
    Ok(found)
}

/// Every destination `doc` writes, once, in task order.
///
/// # Errors
/// A write whose destination cannot be read (`writes`).
pub fn written(doc: &Value) -> Result<Vec<String>, String> {
    let mut paths: Vec<String> = Vec::new();
    for write in writes(doc)? {
        if !paths.iter().any(|p| same(p, &write.destination)) {
            paths.push(write.destination);
        }
    }
    Ok(paths)
}

/// Every string leaf of `value` with its pointer.
fn leaves(value: &Value, at: &str, out: &mut Vec<(String, String)>) {
    match value {
        Value::String(text) => out.push((at.to_owned(), text.clone())),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                leaves(item, &format!("{at}/{i}"), out);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                leaves(item, &format!("{at}/{}", segment(key)), out);
            }
        }
        _ => {}
    }
}

/// Whether any field of `doc` names `path` as its whole value.
#[must_use]
pub fn names(doc: &Value, path: &str) -> bool {
    let mut all = Vec::new();
    leaves(doc, "", &mut all);
    all.iter().any(|(_, text)| same(text, path))
}

/// Whether `text` reads the constant `name` (`const.NAME` not glued to a longer name).
fn reads_constant(text: &str, name: &str) -> bool {
    let needle = format!("const.{name}");
    text.match_indices(&needle).any(|(at, _)| {
        let tail = text[at + needle.len()..].chars().next();
        !tail.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// The pointers of every parsed slot that carries `path` as a destination, each once.
///
/// # Errors
/// `path` is no destination of `doc`, a write's destination cannot be read, or a constant that
/// states it is read by another field too.
pub fn slots(doc: &Value, path: &str) -> Result<Vec<String>, String> {
    let writes = writes(doc)?;
    let mine: Vec<&Write> = writes
        .iter()
        .filter(|w| same(&w.destination, path))
        .collect();
    if mine.is_empty() {
        return Err(format!("`{path}` is not a destination the base writes"));
    }
    let mut found: Vec<String> = Vec::new();
    let mut all = Vec::new();
    leaves(doc, "", &mut all);
    for write in &mine {
        if let Some(name) = write.slot.strip_prefix("/const/") {
            let readers: Vec<&String> = (all.iter())
                .filter(|(_, text)| reads_constant(text, name))
                .map(|(at, _)| at)
                .collect();
            let only_writes = readers
                .iter()
                .all(|at| mine.iter().any(|w| w.path_at == **at));
            if !only_writes {
                return Err(format!(
                    "the constant `{name}` states `{path}` and also feeds another field: it is not revised in place"
                ));
            }
        }
        if !found.contains(&write.slot) {
            found.push(write.slot.clone());
        }
    }
    for (i, item) in (doc.pointer("/permits/fs/write").and_then(Value::as_array))
        .into_iter()
        .flatten()
        .enumerate()
    {
        if item.as_str().is_some_and(|text| same(text, path)) {
            found.push(format!("/permits/fs/write/{i}"));
        }
    }
    Ok(found)
}

/// The pointer of every leaf where `a` and `b` differ (a missing or retyped field included).
#[must_use]
pub fn differences(a: &Value, b: &Value) -> Vec<String> {
    let mut out = Vec::new();
    differ(a, b, "", &mut out);
    out
}

fn differ(left: &Value, right: &Value, at: &str, out: &mut Vec<String>) {
    match (left, right) {
        (Value::Object(before), Value::Object(after)) => {
            let added = after.keys().filter(|key| !before.contains_key(*key));
            for key in before.keys().chain(added) {
                let next = format!("{at}/{}", segment(key));
                match (before.get(key), after.get(key)) {
                    (Some(was), Some(is)) => differ(was, is, &next, out),
                    _ => out.push(next),
                }
            }
        }
        (Value::Array(before), Value::Array(after)) if before.len() == after.len() => {
            for (index, (was, is)) in before.iter().zip(after).enumerate() {
                differ(was, is, &format!("{at}/{index}"), out);
            }
        }
        _ if left == right => {}
        _ => out.push(if at.is_empty() {
            "/".to_owned()
        } else {
            at.to_owned()
        }),
    }
}

/// Whether `revised` is `base` with each `(pointer, value)` of `changed` set and nothing else
/// changed.
///
/// # Errors
/// Each pointer where `revised` differs from that expectation.
pub fn proven(
    base: &Value,
    revised: &Value,
    changed: &[(String, String)],
) -> Result<(), Vec<String>> {
    let mut expected = base.clone();
    for (at, value) in changed {
        match expected.pointer_mut(at) {
            Some(slot) => *slot = Value::String(value.clone()),
            None => return Err(vec![format!("`{at}` is not a field of the base")]),
        }
    }
    let beyond = differences(&expected, revised);
    if beyond.is_empty() {
        Ok(())
    } else {
        Err(beyond
            .into_iter()
            .map(|at| format!("the revised document differs from its base at `{at}`"))
            .collect())
    }
}

/// What the facts leave of a choice: one path, or the exact paths a human chooses among.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Settled {
    /// The facts settle it.
    One(String),
    /// Several stated paths remain: the revision asks which, never guesses.
    Several(Vec<String>),
}

impl Settled {
    fn of(left: Vec<String>, none: &str) -> Result<Self, String> {
        match left.len() {
            0 => Err(none.to_owned()),
            1 => Ok(Self::One(left.into_iter().next().unwrap_or_default())),
            _ => Ok(Self::Several(left)),
        }
    }

    /// The path a human `answer` chose among the options, or the one the facts settle.
    ///
    /// # Errors
    /// An answer that is none of the options.
    pub fn chosen(self, answer: Option<&str>) -> Result<Option<String>, String> {
        match (self, answer) {
            (Self::One(path), _) => Ok(Some(path)),
            (Self::Several(options), Some(answer)) => (options.iter())
                .find(|option| same(option, answer))
                .cloned()
                .map(Some)
                .ok_or_else(|| format!("`{answer}` is none of the offered paths")),
            (Self::Several(_), None) => Ok(None),
        }
    }
}

/// The destinations a link may replace: the paths the base writes (`written`) that its original
/// clause states (`in_replaced`), less those another clause of the change states again
/// (`retained`); when more than one remains and the change clause names some of them (`in_by`),
/// those.
///
/// # Errors
/// No such destination.
pub fn replaced_options(
    written: &[String],
    in_replaced: &[String],
    retained: &[String],
    in_by: &[String],
) -> Result<Settled, String> {
    let has = |list: &[String], path: &str| list.iter().any(|p| same(p, path));
    let mut left: Vec<String> = (written.iter())
        .filter(|path| has(in_replaced, path) && !has(retained, path))
        .cloned()
        .collect();
    if left.len() > 1 && left.iter().any(|path| has(in_by, path)) {
        left.retain(|path| has(in_by, path));
    }
    Settled::of(
        left,
        "the replaced clause states no destination the base writes that the change does not keep",
    )
}

/// The destination a link replaces ([`replaced_options`]), when the facts settle it.
///
/// # Errors
/// No such destination, or more than one.
pub fn replaced(
    written: &[String],
    in_replaced: &[String],
    retained: &[String],
    in_by: &[String],
) -> Result<String, String> {
    match replaced_options(written, in_replaced, retained, in_by)? {
        Settled::One(path) => Ok(path),
        Settled::Several(left) => Err(format!(
            "the replaced clause states several destinations the base writes ({}): which one the change replaces is not stated",
            listed(&left)
        )),
    }
}

/// The new paths a change clause may put in place: those it states (`in_by`) that the base
/// names nowhere (`named`).
///
/// # Errors
/// No such path.
pub fn replacing_options(
    in_by: &[String],
    named: impl Fn(&str) -> bool,
) -> Result<Settled, String> {
    let new: Vec<String> = in_by.iter().filter(|path| !named(path)).cloned().collect();
    Settled::of(new, "the change clause states no new destination")
}

/// The new path a change clause puts in place ([`replacing_options`]), when the facts settle it.
///
/// # Errors
/// No such path, or more than one.
pub fn replacing(in_by: &[String], named: impl Fn(&str) -> bool) -> Result<String, String> {
    match replacing_options(in_by, named)? {
        Settled::One(path) => Ok(path),
        Settled::Several(new) => Err(format!(
            "the change clause states several new paths ({}): which one is the destination is not stated",
            listed(&new)
        )),
    }
}

fn listed(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| format!("`{p}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The compile core's parser of a complete workflow document (its literal projection).
pub type Parse<'a> = &'a dyn Fn(&str) -> Option<Value>;

/// The base bytes revised in place, and the parsed slots that changed or were added.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Substituted {
    /// The revised bytes.
    pub source: String,
    /// The pointers of the parsed slots the revision changed or added.
    pub slots: Vec<String>,
}

/// The literal a slot takes: `by`, in the `./` form the slot's own literal used.
fn styled(old: &str, by: &str) -> String {
    let bare = by.trim_start_matches("./");
    if old.starts_with("./") {
        format!("./{bare}")
    } else {
        bare.to_owned()
    }
}

/// Each byte offset in `range` of `source` where `old`, replaced alone by `new`, changes exactly
/// the parsed field `at` to `want` and nothing else.
fn proven_at(
    source: &str,
    document: &Value,
    (old, new): (&str, &str),
    (at, want): (&str, &str),
    range: std::ops::Range<usize>,
    parse: Parse<'_>,
) -> Vec<usize> {
    let mut found = Vec::new();
    for (offset, _) in source.match_indices(old) {
        if !range.contains(&offset) {
            continue;
        }
        let trial = format!(
            "{}{new}{}",
            &source[..offset],
            &source[offset + old.len()..]
        );
        let Some(parsed) = parse(&trial) else {
            continue;
        };
        if differences(document, &parsed).as_slice() == [at.to_owned()]
            && parsed.pointer(at).and_then(Value::as_str) == Some(want)
        {
            found.push(offset);
        }
    }
    found
}

/// `base` with the destination `path` replaced by `by` at its parsed slots, and only there: each
/// occurrence of a slot's literal changed alone and proven by parsing again, every slot covered
/// once, then the complete document proven.
///
/// # Errors
/// The base does not parse; `path` is no destination the base writes in place ([`slots`]); `by`
/// is already named by the base; a slot is not stated by exactly one literal; or the result
/// differs from the base beyond the slots.
pub fn substitute(
    base: &str,
    path: &str,
    by: &str,
    parse: Parse<'_>,
) -> Result<Substituted, String> {
    let document = parse(base).ok_or("the base does not parse as a workflow")?;
    let slots = slots(&document, path)?;
    if names(&document, by) {
        return Err(format!("`{by}` is already named by the base"));
    }
    let mut changed: Vec<(String, String)> = Vec::new();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for slot in &slots {
        let old = document
            .pointer(slot)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("`{slot}` is not a text field of the base"))?;
        let new = styled(old, by);
        match proven_at(
            base,
            &document,
            (old, &new),
            (slot, &new),
            0..base.len(),
            parse,
        )
        .as_slice()
        {
            [at] => edits.push((*at, old.len(), new.clone())),
            [] => {
                return Err(format!(
                    "`{path}` at `{slot}` is not written as a literal the compiler can replace in place"
                ));
            }
            _ => {
                return Err(format!(
                    "`{path}` at `{slot}` is stated by several literals: which one to replace is not proven"
                ));
            }
        }
        changed.push((slot.clone(), new));
    }
    edits.sort_by_key(|&(at, _, _)| std::cmp::Reverse(at));
    let mut source = base.to_owned();
    for (at, len, new) in &edits {
        source.replace_range(*at..*at + *len, new);
    }
    let revised = parse(&source).ok_or("the revised document does not parse")?;
    proven(&document, &revised, &changed).map_err(|why| why.join("; "))?;
    Ok(Substituted { source, slots })
}

/// The byte span of task `id`'s block in `source`: its key line, at the indentation of the
/// `tasks:` entries, to the next line indented as deep or less.
fn task_block(source: &str, id: &str) -> Option<(usize, usize, usize)> {
    let mut offset = 0;
    let mut start: Option<(usize, usize)> = None;
    for line in source.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        let text = line.trim();
        match start {
            None if !text.is_empty() && text == format!("{id}:") && indent > 0 => {
                start = Some((offset, indent));
            }
            Some((begin, depth)) if !text.is_empty() && indent <= depth => {
                return Some((begin, offset, depth));
            }
            _ => {}
        }
        offset += line.len();
    }
    start.map(|(begin, depth)| (begin, source.len(), depth))
}

/// `source` with `path` appended to its `permits.fs.write` list, flow (`[…, "a"]`) or block
/// (`- a`) as the list is written, proven by parsing again.
fn permit_appended(source: &str, document: &Value, path: &str, parse: Parse<'_>) -> Option<String> {
    let permitted = document.pointer("/permits/fs/write")?.as_array()?;
    let last = permitted.last()?.as_str()?;
    let mut expected = document.clone();
    expected
        .pointer_mut("/permits/fs/write")?
        .as_array_mut()?
        .push(Value::String(path.to_owned()));
    let mut found = Vec::new();
    for (offset, _) in source.match_indices(last) {
        let after = offset + last.len();
        let quote = source[after..]
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'');
        let close = after + quote.map_or(0, char::len_utf8);
        let flow = format!("{}, \"{path}\"{}", &source[..close], &source[close..]);
        let line_start = source[..offset].rfind('\n').map_or(0, |n| n + 1);
        let line_end = source[after..]
            .find('\n')
            .map_or(source.len(), |n| after + n + 1);
        let prefix = &source[line_start..offset];
        let block = (prefix.trim_end().ends_with('-')).then(|| {
            let dash = &prefix[..prefix.rfind('-').unwrap_or(0)];
            format!(
                "{}{dash}- \"{path}\"\n{}",
                &source[..line_end],
                &source[line_end..]
            )
        });
        for trial in std::iter::once(flow).chain(block) {
            if parse(&trial).as_ref() == Some(&expected) {
                found.push(trial);
            }
        }
    }
    (found.len() == 1).then(|| found.remove(0))
}

/// The id a copy of task `id` takes: `<id>_<n>`, the first `n` no task holds.
fn copy_id(tasks: &serde_json::Map<String, Value>, id: &str) -> String {
    (2..=tasks.len() + 2)
        .map(|n| format!("{id}_{n}"))
        .find(|candidate| !tasks.contains_key(candidate))
        .unwrap_or_else(|| format!("{id}_copy"))
}

/// `source` (holding the copied task `new_id` in `range`) with that copy's destination set to
/// `path`: its literal path, or the constant expression it reads, replaced in the copy only. The
/// new destination's literal is returned beside.
fn copy_destination(
    mut source: String,
    new_id: &str,
    path: &str,
    range: std::ops::Range<usize>,
    parse: Parse<'_>,
) -> Result<(String, String), String> {
    let cloned = parse(&source).ok_or("the copied task does not parse")?;
    let at = format!("/tasks/{}/invoke/args/path", segment(new_id));
    let stated = cloned
        .pointer(&at)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let new = if stated.contains("${{") {
        path.to_owned()
    } else {
        styled(&stated, path)
    };
    for text in [new.clone(), format!("\"{new}\"")] {
        let found = proven_at(
            &source,
            &cloned,
            (&stated, &text),
            (&at, &new),
            range.clone(),
            parse,
        );
        if let [offset] = found.as_slice() {
            source.replace_range(*offset..*offset + stated.len(), &text);
            return Ok((source, new));
        }
    }
    Err(format!(
        "the copied write's destination is not stated by one literal the compiler can replace: `{stated}`"
    ))
}

/// Whether `revised` is `document` plus exactly task `new_id` (a copy of `id` writing `new`) and
/// `new` appended to the write permits; the slots it added.
fn addition_proven(
    document: &Value,
    revised: &Value,
    (id, new_id, new): (&str, &str, &str),
) -> Result<Vec<String>, String> {
    let mut expected = document.clone();
    let mut task = document["tasks"][id].clone();
    if let Some(slot) = task.pointer_mut("/invoke/args/path") {
        *slot = Value::String(new.to_owned());
    }
    if let Some(map) = expected["tasks"].as_object_mut() {
        map.insert(new_id.to_owned(), task);
    }
    let permits = expected
        .pointer_mut("/permits/fs/write")
        .and_then(Value::as_array_mut)
        .ok_or("the base has no write permits to extend")?;
    permits.push(Value::String(new.to_owned()));
    let last = permits.len() - 1;
    let beyond = differences(&expected, revised);
    if beyond.is_empty() {
        Ok(vec![
            format!("/tasks/{}", segment(new_id)),
            format!("/permits/fs/write/{last}"),
        ])
    } else {
        Err(format!(
            "the added write changes the base beyond itself at {}",
            beyond.join(", ")
        ))
    }
}

/// `base` with a write of `path` added beside the existing write of `like`: that write task's
/// block copied under a new id, its destination set to `path` in the copy only, and `path`
/// appended to the write permits; the complete document proven to be the base plus exactly that
/// task and that permit.
///
/// # Errors
/// The base does not parse; `like` is not the destination of exactly one write; `path` is already
/// named; the task is not a block the compiler can copy; or the result is not proven.
pub fn add_destination(
    base: &str,
    like: &str,
    path: &str,
    parse: Parse<'_>,
) -> Result<Substituted, String> {
    let document = parse(base).ok_or("the base does not parse as a workflow")?;
    if names(&document, path) {
        return Err(format!("`{path}` is already named by the base"));
    }
    let writes = writes(&document)?;
    let mine: Vec<&Write> = writes
        .iter()
        .filter(|w| same(&w.destination, like))
        .collect();
    let [write] = mine.as_slice() else {
        return Err(format!(
            "`{like}` is not the destination of exactly one write"
        ));
    };
    let id = (write.path_at.strip_prefix("/tasks/"))
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default()
        .replace("~1", "/")
        .replace("~0", "~");
    let tasks = document["tasks"]
        .as_object()
        .ok_or("the base has no task map")?;
    let new_id = copy_id(tasks, &id);
    let (begin, end, _) = task_block(base, &id).ok_or_else(|| {
        format!("the write `{id}` is not a task block the compiler can copy in place")
    })?;
    let renamed = base[begin..end].replacen(&format!("{id}:"), &format!("{new_id}:"), 1);
    let copied = format!("{}{renamed}{}", &base[..end], &base[end..]);
    let parsed = parse(&copied).ok_or("the copied task does not parse")?;
    if parsed["tasks"][&new_id] != document["tasks"][&id] {
        return Err(format!("the write `{id}` does not copy as one task"));
    }
    let range = end..end + renamed.len();
    let (source, new) = copy_destination(copied, &new_id, path, range, parse)?;
    let with_path = parse(&source).ok_or("the copied task does not parse")?;
    let source = permit_appended(&source, &with_path, &new, parse)
        .ok_or("the write permits are not a list the compiler can extend in place")?;
    let revised = parse(&source).ok_or("the revised document does not parse")?;
    let slots = addition_proven(&document, &revised, (&id, &new_id, &new))?;
    Ok(Substituted { source, slots })
}

/// The answer key of the bounded question that chooses the replaced destination.
pub const ASK_DESTINATION: &str = "revision.destination";
/// The answer key of the bounded question that chooses the new path.
pub const ASK_PATH: &str = "revision.path";
/// The answer key of the bounded question that chooses the destination an added write copies.
pub const ASK_LIKE: &str = "revision.like";

/// One structural edit of a base no semantic record binds.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Edit {
    /// The destination `old` replaced by `new` at its parsed slots.
    Replace {
        /// The destination the base writes that the change replaces.
        old: String,
        /// The path the change puts in its place.
        new: String,
    },
    /// A write of `new` added beside the write of `like`, both kept.
    Add {
        /// The destination whose write the new one copies.
        like: String,
        /// The destination the change adds.
        new: String,
    },
}

/// What a typed revision decides: its edit, or the bounded questions the facts leave open (each
/// key with the exact paths it offers), with the change's clauses the resolved request consumes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Decided {
    /// The facts and the answers settle one edit.
    Edit(Edit),
    /// A destination or a path stays open: the human chooses among these paths.
    Ask(Vec<(&'static str, Vec<String>)>),
}

/// The original and change clause of the one typed link, held to the link laws: the original
/// clause stated once in `original` and no prohibition, the change clause one the change states.
fn link<'a>(
    original: &str,
    (original_ledger, change_ledger): (&Value, &Value),
    link: &'a Value,
) -> Result<(&'a str, &'a str), String> {
    let (Some(replaces), Some(by)) = (link["replaces"].as_str(), link["by"].as_str()) else {
        return Err("the link names no original clause or no change clause".to_owned());
    };
    let duties = |ledger: &Value| ledger.as_array().cloned().unwrap_or_default();
    let of = |ledger: &Value, clause: &str| duties(ledger).iter().any(|d| d["evidence"] == clause);
    if !of(original_ledger, replaces) || original.matches(replaces).count() != 1 {
        return Err(format!(
            "« {replaces} » is not a clause the original request states once"
        ));
    }
    if (duties(original_ledger).iter())
        .any(|d| d["evidence"] == replaces && d["state"] == "refused")
    {
        return Err(format!(
            "« {replaces} » is a prohibition: a revision never supersedes it"
        ));
    }
    if !of(change_ledger, by) {
        return Err(format!("« {by} » is not a clause the change states"));
    }
    Ok((replaces, by))
}

/// The edit a typed revision states on `document` — one link replacing a destination, or one
/// added clause adding one — settled by the facts or by `answer` (keyed [`ASK_DESTINATION`],
/// [`ASK_PATH`], [`ASK_LIKE`]), else the bounded questions; with the clauses the resolved request
/// consumes as additions.
///
/// # Errors
/// The accounting or link laws, a gate added, a destination neither replaced nor added, more than
/// one structural edit, an answer none of the offered paths, or facts that settle nothing.
pub fn decide(
    document: &Value,
    original: &str,
    ledgers: (&Value, &Value),
    stated: &Value,
    answer: &dyn Fn(&str) -> Option<String>,
) -> Result<(Decided, Vec<String>), String> {
    let accounted = super::revision::accounted(ledgers.1, stated).map_err(|w| w.join("; "))?;
    if let Some((clause, _)) = accounted.iter().find(|(_, kind)| *kind == Some("gate")) {
        return Err(format!(
            "the change adds « {clause} », a gate the program's structure carries: not revised in place"
        ));
    }
    // An added destination: a clause the reader reads as an effect, or one the seat types as a
    // destination by naming the write it copies (`like`) that states a path the base names
    // nowhere — the reader's kind is not the only witness of a write.
    let typed_like = stated["like"].is_string();
    let effects: Vec<&String> = (accounted.iter())
        .filter(|(clause, kind)| {
            kind.is_some()
                || (typed_like && stated_paths(clause).iter().any(|p| !names(document, p)))
        })
        .map(|(clause, _)| clause)
        .collect();
    let adds: Vec<String> = accounted.iter().map(|(clause, _)| clause.clone()).collect();
    let links = stated["supersedes"].as_array().cloned().unwrap_or_default();
    let written = written(document)?;
    let named = |path: &str| names(document, path);
    let pick =
        |key: &'static str, settled: Settled, open: &mut Vec<(&'static str, Vec<String>)>| {
            let options = match &settled {
                Settled::Several(options) => options.clone(),
                Settled::One(_) => Vec::new(),
            };
            let chosen = settled.chosen(answer(key).as_deref())?;
            if chosen.is_none() {
                open.push((key, options));
            }
            Ok::<_, String>(chosen)
        };
    let mut open = Vec::new();
    let edit = match (links.as_slice(), effects.as_slice()) {
        ([one], []) => {
            let (replaces, by) = link(original, ledgers, one)?;
            let retained: Vec<String> = (super::revision::clauses(ledgers.1).iter())
                .filter_map(Value::as_str)
                .filter(|clause| *clause != by)
                .flat_map(stated_paths)
                .collect();
            let in_by = stated_paths(by);
            let old_options = replaced_options(&written, &stated_paths(replaces), &retained, &in_by)?;
            let old = pick(ASK_DESTINATION, old_options, &mut open)?;
            // The link targets a written destination but its change clause states no new path:
            // the human states it (a bounded path question), never the seat or a guess.
            let new = match replacing_options(&in_by, named) {
                Ok(settled) => pick(ASK_PATH, settled, &mut open)?,
                Err(_) => {
                    if let Some(path) = answer(ASK_PATH) {
                        Some(new_path(&path, named)?)
                    } else {
                        open.push((ASK_PATH, Vec::new()));
                        None
                    }
                }
            };
            old.zip(new).map(|(old, new)| Edit::Replace { old, new })
        }
        ([], [clause]) => {
            let new = pick(ASK_PATH, replacing_options(&stated_paths(clause), named)?, &mut open)?;
            let likes = match stated["like"].as_str() {
                Some(like) => written
                    .iter()
                    .find(|w| same(w, like))
                    .map(|w| Settled::One(w.clone()))
                    .ok_or_else(|| format!("`{like}` is not a destination the base writes"))?,
                None => Settled::of(written.clone(), "the base writes no destination to copy")?,
            };
            let like = pick(ASK_LIKE, likes, &mut open)?;
            like.zip(new).map(|(like, new)| Edit::Add { like, new })
        }
        ([], []) => {
            return Err("the change replaces and adds no destination: a base no semantic record binds is revised only by replacing or adding one destination it writes".to_owned());
        }
        _ => return Err("the change states several structural edits: one destination is replaced or added at a time".to_owned()),
    };
    Ok((edit.map_or(Decided::Ask(open), Decided::Edit), adds))
}

/// A path a human answered as the new destination: relative, inside the project (no `..`), and
/// named nowhere in the base.
///
/// # Errors
/// An empty, absolute, escaping or already named path.
pub fn new_path(answer: &str, named: impl Fn(&str) -> bool) -> Result<String, String> {
    let path = answer.trim();
    let escapes = path.split(['/', '\\']).any(|part| part == "..");
    if path.is_empty() || path.starts_with(['/', '\\', '~']) || path.contains(':') || escapes {
        return Err(format!(
            "`{path}` is not a relative path inside the project"
        ));
    }
    if path.chars().any(char::is_whitespace) {
        return Err(format!("`{path}` is not a single path"));
    }
    if named(path) {
        return Err(format!("`{path}` is already named by the base"));
    }
    Ok(path.to_owned())
}

/// Every path `text` states ([`stated`]).
fn stated_paths(text: &str) -> Vec<String> {
    stated(text)
}

/// The request answered by an applied destination edit. A path supplied by a bounded human
/// answer is kept beside its change clause when that clause did not name it. The annotation
/// comes only from the decided edit; typed links themselves remain the seat's exact words.
///
/// # Errors
/// The clause replacement laws of [`super::revision::resolved`].
pub fn resolved(
    original: &str,
    links: &Value,
    added: &[String],
    edit: &Edit,
) -> Result<String, Vec<String>> {
    let mut contextual = links.clone();
    if let Edit::Replace { new, .. } = edit
        && let Some(link) = contextual
            .as_array_mut()
            .and_then(|links| links.first_mut())
        && let Some(by) = link["by"].as_str()
        && !stated_paths(by).iter().any(|path| same(path, new))
    {
        link["by"] = Value::String(format!("{by} (destination: {new})"));
    }
    super::revision::resolved(original, &contextual, added)
}

/// `base` with `edit` applied in place ([`substitute`] or [`add_destination`]).
///
/// # Errors
/// Why the edit is not proven on this base.
pub fn apply(base: &str, edit: &Edit, parse: Parse<'_>) -> Result<Substituted, String> {
    match edit {
        Edit::Replace { old, new } => substitute(base, old, new, parse),
        Edit::Add { like, new } => add_destination(base, like, new, parse),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A filter-and-total base: two destinations, one through a constant, a read of a path
    /// sharing its name, and data text naming a destination.
    fn base() -> Value {
        json!({"nika": "orders",
            "const": {"confirmed_path": "confirmed.csv", "total_path": "total.txt"},
            "permits": {"fs": {"read": ["orders.csv", "total.txt.bak"], "write": ["confirmed.csv", "total.txt"]}},
            "tasks": {
                "read_source": {"invoke": {"tool": "nika:read", "args": {"path": "orders.csv"}}},
                "total": {"invoke": {"tool": "nika:jq", "args": {"expression": "add // 0", "note": "total.txt"}}},
                "write_confirmed": {"with": {"content": "${{ tasks.read_source.output }}"},
                    "invoke": {"tool": "nika:write", "args": {"path": "${{ const.confirmed_path }}", "content": "${{ with.content }}", "overwrite": true}}},
                "write_total": {"invoke": {"tool": "nika:write", "args": {"path": "./total.txt", "content": "${{ tasks.total.output }}"}}}}})
    }

    #[test]
    fn a_destination_is_carried_by_its_constant_or_literal_and_its_permits_only() {
        let doc = base();
        assert_eq!(written(&doc).unwrap(), ["confirmed.csv", "./total.txt"]);
        assert_eq!(
            slots(&doc, "confirmed.csv").unwrap(),
            ["/const/confirmed_path", "/permits/fs/write/0"]
        );
        // The literal write and its permit; never the data text or the similarly named read.
        assert_eq!(
            slots(&doc, "total.txt").unwrap(),
            ["/tasks/write_total/invoke/args/path", "/permits/fs/write/1"]
        );
        assert!(
            slots(&doc, "orders.csv")
                .unwrap_err()
                .contains("not a destination")
        );
    }

    #[test]
    fn a_shared_constant_or_an_expression_path_is_not_revised_in_place() {
        let mut doc = base();
        doc["tasks"]["total"]["invoke"]["args"]["label"] = json!("${{ const.confirmed_path }}");
        assert!(
            slots(&doc, "confirmed.csv")
                .unwrap_err()
                .contains("also feeds")
        );
        let mut doc = base();
        doc["tasks"]["write_total"]["invoke"]["args"]["path"] = json!("${{ with.where }}");
        assert!(
            slots(&doc, "total.txt")
                .unwrap_err()
                .contains("neither as a literal")
        );
    }

    #[test]
    fn only_the_slots_may_change_and_every_other_field_is_compared() {
        let doc = base();
        let mut revised = doc.clone();
        revised["const"]["confirmed_path"] = json!("final.csv");
        revised["permits"]["fs"]["write"][0] = json!("final.csv");
        let swap = [
            ("/const/confirmed_path".to_owned(), "final.csv".to_owned()),
            ("/permits/fs/write/0".to_owned(), "final.csv".to_owned()),
        ];
        assert!(proven(&doc, &revised, &swap).is_ok());
        let mut calculation = revised.clone();
        calculation["tasks"]["total"]["invoke"]["args"]["expression"] = json!("length");
        let why = proven(&doc, &calculation, &swap).unwrap_err();
        assert!(
            why[0].contains("/tasks/total/invoke/args/expression"),
            "{why:?}"
        );
        let mut option = revised.clone();
        option["tasks"]["write_confirmed"]["invoke"]["args"]["overwrite"] = json!(false);
        assert!(proven(&doc, &option, &swap).is_err());
        let mut added = revised;
        added["tasks"]["write_more"] = json!({});
        assert!(proven(&doc, &added, &swap).is_err());
    }

    #[test]
    fn the_replaced_destination_is_the_one_the_change_does_not_keep() {
        let written = ["confirmed.csv".to_owned(), "total.txt".to_owned()];
        let clause = written.to_vec();
        let kept = ["total.txt".to_owned()];
        assert_eq!(
            replaced(&written, &clause, &kept, &[]).unwrap(),
            "confirmed.csv"
        );
        assert!(
            replaced(&written, &clause, &[], &[])
                .unwrap_err()
                .contains("several")
        );
        let explicit = ["confirmed.csv".to_owned(), "final.csv".to_owned()];
        assert_eq!(
            replaced(&written, &clause, &[], &explicit).unwrap(),
            "confirmed.csv"
        );
        assert!(replaced(&written, &["orders.csv".to_owned()], &[], &[]).is_err());
        let doc = base();
        let by = ["confirmed.csv".to_owned(), "final.csv".to_owned()];
        assert_eq!(replacing(&by, |p| names(&doc, p)).unwrap(), "final.csv");
        assert!(replacing(&["total.txt".to_owned()], |p| names(&doc, p)).is_err());
    }

    #[test]
    fn a_path_is_the_same_whatever_its_leading_dot_slash() {
        assert!(same("./a.txt", "a.txt") && !same("data/a.txt", "a.txt"));
        assert_eq!(
            stated("Copie entree.txt dans a.txt."),
            ["entree.txt", "a.txt"]
        );
    }
}
