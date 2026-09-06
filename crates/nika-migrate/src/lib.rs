// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! W1 « the map » migration — the machine-applicable repair for the dead
//! envelope forms (`NIKA-PARSE-020..023`).
//!
//! Line-based and structure-aware: comments, blank lines and source order
//! are preserved byte-for-byte outside the three transformed shapes. The
//! transform is IDEMPOTENT (a migrated document returns `None`), and it is
//! the ONE repair `check --fix` applies when the parser refuses an old-map
//! document — the old form is repairable, never executable (there is no
//! legacy parser path).
//!
//! Transforms (top-level only — nothing else is touched):
//!
//! The ENVELOPE migrations — `workflow: <scalar>` → the object, and the
//! top-level `description:` hoist — are RETIRED. The envelope nuke of
//! 2026-08-12 killed their target: `workflow: { id, description }` is
//! exactly what the parser refuses now, so emitting it would be a repair
//! whose output its own checker rejects. The identity move onto `nika:`
//! is a codemod of its own (its `description:` prose must be demoted,
//! never silently dropped).
//!
//! 1. `tasks:` sequence → map — `  - id: X` becomes the key `  X:` (the
//!    two-space list marker becomes the key's indent, so task bodies
//!    never re-indent); a single-line FLOW item `  - { id: X, … }`
//!    expands to `  X:` + one block line per remaining entry. The list
//!    migrates ATOMICALLY: every item must have its mechanical rewrite
//!    or the whole sequence stays a sequence — a half-mapped collection
//!    is invalid YAML no later pass can repair (issue #645).
//!
//! A task whose `id:` is NOT the item's first entry is deliberately not
//! handled — the parser's teaching names the file and a human decides
//! (never guess; the conformance suite pins the refusal).
//!
//! The crate carries four waves — `w1` (the map), `w2` (equivalence-or-stop
//! flow migration), [`esplit()`](fn@esplit) (the C2 four-authority
//! flag-day) and [`predicates()`](fn@predicates) (the R5 outcome-class
//! respelling). Split out
//! of `nika-cli` per the size-cap discipline
//! (D-2026-07-09-N1 · one architectural unit, N workspace members · the
//! `nika-source` / `nika-vocab` precedent): the CLI's `fix` verb calls
//! `nika_migrate::w1` / `w2` / `esplit` / `predicates` unchanged. The
//! migrations are
//! pure transforms; the `repair` module carries the byte-level mechanics
//! of applying them (whole-word splice · atomic publish · `std` only).
//! Zero deps.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::unreachable,
        clippy::panic
    )
)]

mod d1;
mod esplit;
mod identity;
mod lot3;
mod predicates;
pub mod repair;

pub use d1::{D1Outcome, d1};
pub use esplit::{EsplitOutcome, esplit};
pub use identity::{IdentityOutcome, identity};
pub use lot3::{Lot3Outcome, lot3};
pub use predicates::predicates;

/// Apply the W1 migration. `Some(new)` when the document changed,
/// `None` when it is already in the new form (idempotence by contract).
#[must_use]
pub fn w1(source: &str) -> Option<String> {
    let lines: Vec<&str> = source.split('\n').collect();

    // The ENVELOPE half of W1 (`workflow: <scalar>` → the object · the
    // top-level `description:` hoist) is RETIRED with the envelope nuke
    // (2026-08-12). Both migrated INTO `workflow: { id, description }`,
    // and that object is exactly what the parser refuses now — a repair
    // that produces a document its own checker rejects is worse than no
    // repair. The identity migration (`workflow:`/`nika: v1` → a single
    // `nika: <id>`) needs a codemod of its own, with the `description:`
    // prose demoted rather than silently dropped; it is NOT this one.
    //
    // What survives here is the TASKS half, whose target is untouched:
    // the sequence → map rewrite and the `- id:` field removal.
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 2);
    let mut in_tasks = false;
    let mut changed = false;
    // The tasks list migrates ATOMICALLY (issue #645): every `  - ` item
    // must have its mechanical rewrite, or the WHOLE sequence stays a
    // sequence — a half-mapped collection is invalid YAML, and writing it
    // would strand the repair loop on an intermediate no pass can parse.
    let mut tasks_convertible = true;
    {
        let mut in_t = false;
        for l in &lines {
            if !l.starts_with(' ') && !l.starts_with('#') && l.contains(':') {
                in_t = l.starts_with("tasks:");
                continue;
            }
            if in_t
                && l.starts_with("  - ")
                && migrate_task_item(l.trim_end_matches('\r')).is_none()
            {
                tasks_convertible = false;
                break;
            }
        }
    }
    for l in &lines {
        // track which top-level section we are in (col-0 keys)
        if !l.starts_with(' ') && !l.starts_with('#') && l.contains(':') {
            in_tasks = l.starts_with("tasks:");
        }
        // A CRLF file stays a CRLF file: the item is rewritten without its
        // `\r` and every emitted line gets it back (measured 2026-08-18:
        // the migrated `  a:` lines were LF in a CRLF document — valid
        // YAML, mixed endings, a diff nobody asked for).
        let eol = if l.ends_with('\r') { "\r" } else { "" };
        if in_tasks
            && tasks_convertible
            && let Some(rewritten) = migrate_task_item(l.trim_end_matches('\r'))
        {
            out.extend(rewritten.into_iter().map(|line| format!("{line}{eol}")));
            changed = true;
            continue;
        }
        out.push((*l).to_owned());
    }
    changed.then(|| out.join("\n"))
}

/// `  - id: name` (optional trailing comment) → `  name:` (+ comment).
/// The body lines that follow at indent 4 stay untouched: the list marker
/// column becomes the key's indent, so alignment is preserved.
fn task_item_to_key(line: &str) -> Option<String> {
    let rest = line.strip_prefix("  - id: ")?;
    let rest = rest.trim_end();
    let token = match rest.find('#') {
        Some(idx) => rest[..idx].trim_end(),
        None => rest,
    };
    // everything after the token (its original spacing + comment) rides along
    let comment = &rest[token.len()..];
    let ok = !token.is_empty()
        && token.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && token
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    ok.then(|| format!("  {token}:{comment}"))
}

/// One `  - ` sequence item → its map-entry lines, when the rewrite is
/// mechanical: the block form (`  - id: name` → the bare key, body lines
/// untouched) or a single-line flow form (`  - { id: name, … }` → the
/// key + one block line per remaining entry). `None` = a human decides
/// (never guess).
fn migrate_task_item(line: &str) -> Option<Vec<String>> {
    if let Some(key) = task_item_to_key(line) {
        return Some(vec![key]);
    }
    flow_item_to_block(line)
}

/// `  - { id: name, verb: … }` (a single-line flow item) → `  name:` plus
/// one `    key: value` line per remaining entry, each entry's original
/// flow text preserved. The id must LEAD the entries — a buried id is
/// the block form's refusal one line down (never guess).
fn flow_item_to_block(line: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix("  - ")?.trim_start();
    if !rest.starts_with('{') {
        return None;
    }
    let (body, comment) = split_flow_comment(rest);
    let inner = body.trim_end().strip_suffix('}')?.strip_prefix('{')?;
    let scan = flow_scan(inner);
    if !scan.balanced {
        return None; // unterminated quote / brace — a human untangles
    }
    let mut entries: Vec<&str> = Vec::with_capacity(scan.commas.len() + 1);
    let mut start = 0;
    for &c in &scan.commas {
        entries.push(&inner[start..c]);
        start = c + 1;
    }
    entries.push(&inner[start..]);
    let id = entries.first()?.trim().strip_prefix("id:")?.trim();
    let ok = !id.is_empty()
        && id.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !ok {
        return None;
    }
    let mut out = vec![format!("  {id}:{comment}")];
    for e in &entries[1..] {
        let e = e.trim();
        if !e.is_empty() {
            out.push(format!("    {e}"));
        }
    }
    Some(out)
}

/// The quote/depth-aware scan of one flow line.
struct FlowScan {
    /// Byte offsets of every depth-0 `,` (the entry separators).
    commas: Vec<usize>,
    /// Byte offset of the first depth-0 ` #` comment opener.
    comment: Option<usize>,
    /// Quotes and braces all closed by end of line.
    balanced: bool,
}

/// Scan `text` (one line of flow YAML) for its depth-0 structure. A
/// quote byte only OPENS a quoted scalar where YAML lets a value start —
/// after `{` `[` `,` `:` or at the head; anywhere else it is plain-scalar
/// content (`it's` never opens a quote). Inside single quotes `''` is an
/// escaped quote; inside double quotes `\x` escapes `x`.
fn flow_scan(text: &str) -> FlowScan {
    let mut scan = FlowScan {
        commas: Vec::new(),
        comment: None,
        balanced: false,
    };
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    // last non-space byte seen OUTSIDE a quoted scalar
    let mut prev: Option<u8> = None;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == q {
                if q == b'\'' && bytes.get(i + 1) == Some(&b'\'') {
                    i += 1; // the doubled '' is one escaped quote
                } else {
                    quote = None;
                }
            } else if q == b'"' && b == b'\\' {
                i += 1; // the escaped byte is content
            }
        } else {
            match b {
                b'"' | b'\'' if matches!(prev, None | Some(b'{' | b'[' | b',' | b':')) => {
                    quote = Some(b);
                }
                b'{' | b'[' => depth += 1,
                b'}' | b']' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => scan.commas.push(i),
                b'#' if depth == 0 && scan.comment.is_none() && i > 0 && bytes[i - 1] == b' ' => {
                    scan.comment = Some(i - 1); // keep the separating space
                }
                _ => {}
            }
            if !matches!(b, b' ' | b'\t') {
                prev = Some(b);
            }
        }
        i += 1;
    }
    scan.balanced = quote.is_none() && depth == 0;
    scan
}

/// Split the trailing `# comment` off a flow item's text: the first
/// depth-0 ` #` outside quoted scalars (one INSIDE the flow would have
/// swallowed the closing brace — such an item then fails the `}` check
/// and stays for a human). The comment keeps its leading spaces so the
/// rewritten key line reads `  name: # note` like the block form's.
fn split_flow_comment(text: &str) -> (&str, &str) {
    match flow_scan(text).comment {
        Some(at) => (&text[..at], &text[at..]),
        None => (text, ""),
    }
}

// ─────────────────────────── W2 « the flow » ───────────────────────────

//   The machine-applicable half of the `depends_on` migration
//   (`NIKA-PARSE-024` · `NIKA-VAR-021`) — EQUIVALENCE-OR-STOP (spec 03
//   §depends_on): a rewrite applies ONLY when the observable behavior
//   {edges · waves · outputs · outcomes} is provably unchanged; every
//   ambiguous case produces a diagnostic naming the candidate rewrites
//   and their semantic deltas, and the file is left untouched. Never
//   guess.
//
//   GO rules ·
//   R1  a body/for_each whole-island `${{ tasks.X… }}` reference hoists
//       into `with:` (the binding IS the edge) and X leaves the deps —
//       bare projections never error at eval (defined-null); a DEEPER
//       path moves its eval outside `on_error:` armor → GO only without
//       armor.
//   R2  a bare (unreferenced) dep whose producer provably CANNOT skip
//       (no when: · no on_error.skip · no for_each) → `after: {d: success}`.
//   R3  a dep already read through `with:` (value-role) simply leaves.
//
//   STOP classes: S1 skippable producer on a bare dep · S2 `when:`
//   references tasks.* · S3 status-family-only backing · S4 composite
//   island / deep ref under armor · S5 on_finally non-parent read ·
//   S6 flow-style with: needing a merge · S7 unparseable shape.

/// The W2 migration verdict.
pub enum W2Outcome {
    /// Mechanically migrated (equivalence preserved by rule).
    Changed(String),
    /// Ambiguous — each diagnostic names the case and its candidates.
    Stop(Vec<String>),
}

/// One whole-island `${{ tasks.<id><path> }}` reference.
struct IslandRef {
    task: String,
    /// The path after the id (`.output` · `.status` · `.output.title` …).
    path: String,
}

/// Scan the `${{ … }}` islands of one line. Returns (whole-island refs ·
/// carries-a-tasks-island-that-is-NOT-a-whole-ref).
fn scan_islands(line: &str) -> (Vec<IslandRef>, bool) {
    let mut refs = Vec::new();
    let mut composite = false;
    let mut rest = line;
    while let Some(open) = rest.find("${{") {
        let after = &rest[open + 3..];
        let Some(close) = after.find("}}") else {
            break;
        };
        let inner = after[..close].trim();
        if let Some(stripped) = inner.strip_prefix("tasks.") {
            if let Some((task, path)) = split_task_path(stripped) {
                refs.push(IslandRef { task, path });
            } else if inner.contains("tasks.") {
                composite = true;
            }
        } else if inner.contains("tasks.") {
            composite = true;
        }
        rest = &after[close + 2..];
    }
    (refs, composite)
}

/// `<id><path>` where the WHOLE text is one reference — id then a chain
/// of `.seg` / `[…]` steps and nothing else.
fn split_task_path(s: &str) -> Option<(String, String)> {
    let id_end = s
        .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .unwrap_or(s.len());
    if id_end == 0 {
        return None;
    }
    let (id, mut path) = s.split_at(id_end);
    let full_path = path;
    while !path.is_empty() {
        if let Some(rest) = path.strip_prefix('.') {
            let seg = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            if seg == 0 {
                return None;
            }
            path = &rest[seg..];
        } else if let Some(rest) = path.strip_prefix('[') {
            let close = rest.find(']')?;
            path = &rest[close + 1..];
        } else {
            return None;
        }
    }
    Some((id.to_owned(), full_path.to_owned()))
}

/// Whether a whole-island path is a BARE projection (never errors at
/// eval · defined-null law): one `.segment`, no deeper step, no index.
fn is_bare_projection(path: &str) -> bool {
    let Some(rest) = path.strip_prefix('.') else {
        return false; // bare envelope — the scan layer rejects VAR-020
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The status-family projections (terminal-observation role · spec 13
/// adds `.cause`, the same pass-set as `.status`).
fn is_status_family(path: &str) -> bool {
    matches!(
        path,
        ".status" | ".cause" | ".duration_ms" | ".started_at" | ".ended_at"
    )
}

/// A synthesized binding name for a hoisted reference (deterministic ·
/// collision-suffixed).
fn binding_name(task: &str, path: &str, taken: &mut std::collections::BTreeSet<String>) -> String {
    let mut segs: Vec<&str> = Vec::new();
    for part in path.split(['.', '[']) {
        let part = part.trim_end_matches(']').trim_matches(['\'', '"']);
        if part.is_empty() || part.chars().all(|c| c.is_ascii_digit()) || part == "output" {
            continue;
        }
        segs.push(part);
    }
    let mut name = if segs.is_empty() {
        task.to_owned()
    } else {
        format!("{task}_{}", segs.join("_"))
    };
    name = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = name.clone();
    let mut n = 2;
    while !taken.insert(name.clone()) {
        name = format!("{base}_{n}");
        n += 1;
    }
    name
}

/// The extracted `depends_on` of one task (dep names · the line span to
/// drop · malformed flag).
struct DepsBlock {
    deps: Vec<String>,
    lines: Vec<usize>,
    malformed: bool,
}

/// Apply the W2 migration (equivalence-or-stop).
pub fn w2(source: &str) -> W2Outcome {
    let lines: Vec<&str> = source.split('\n').collect();
    let task_starts = scan_task_starts(&lines);
    let facts = collect_task_facts(&lines, &task_starts);

    // pass 3 · decisions per task.
    let mut stops: Vec<String> = Vec::new();
    let mut plan = SurgeryPlan::default();
    for (ix, (id, key_line)) in task_starts.iter().enumerate() {
        let (start, end) = task_range(&lines, &task_starts, ix);
        let block = &facts.deps_of[id];
        if block.malformed {
            stops.push(format!(
                "[S7] task `{id}`: malformed depends_on entries — rewrite by hand"
            ));
            continue;
        }
        let with_head = facts.with_head.get(id).copied();
        let cx = TaskCx {
            id,
            key_line: *key_line,
            start,
            end,
            block,
            armor: has_armor(&lines, start, end),
            with_head,
            with_block: with_block_range(&lines, with_head, end),
        };
        let with_refs = collect_with_refs(&lines, cx.with_block);
        let body = scan_task_body(
            &lines,
            &cx,
            facts.with_keys.get(id).map(Vec::as_slice),
            &mut stops,
        );
        let after_entries = decide_deps(&cx, &with_refs, &body, &facts.can_skip, &mut stops);
        if !stops.is_empty() {
            continue; // decisions for THIS task are moot — but keep scanning others
        }
        plan_task_surgery(
            &lines,
            &cx,
            &body.hoists,
            after_entries,
            &mut plan,
            &mut stops,
        );
    }

    if !stops.is_empty() {
        return W2Outcome::Stop(stops);
    }
    if !plan.changed {
        return W2Outcome::Stop(vec![
            "[S7] no mechanical W2 repair found for this document".to_owned(),
        ]);
    }
    W2Outcome::Changed(emit_migrated(&lines, &plan))
}

/// Pass 1 · the 2-space task key lines inside the `tasks:` section.
fn scan_task_starts(lines: &[&str]) -> Vec<(String, usize)> {
    let mut in_tasks = false;
    let mut task_starts: Vec<(String, usize)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if !l.starts_with(' ') && !l.starts_with('#') && l.contains(':') {
            in_tasks = l.starts_with("tasks:");
            continue;
        }
        if in_tasks && let Some(key) = two_space_key(l) {
            task_starts.push((key.to_owned(), i));
        }
    }
    task_starts
}

/// The line range of task `ix` (key line → before the next 2-space key /
/// top-level), trimmed to the `tasks:` section end.
fn task_range(lines: &[&str], task_starts: &[(String, usize)], ix: usize) -> (usize, usize) {
    let start = task_starts[ix].1;
    let end = task_starts
        .get(ix + 1)
        .map_or(lines.len(), |(_, next)| *next);
    // trim to the tasks: section end (first col-0 key after start)
    let mut e = start + 1;
    while e < end {
        let l = lines[e];
        if !l.is_empty() && !l.starts_with(' ') {
            break;
        }
        e += 1;
    }
    (start, e)
}

/// Line-scanned facts for every task (pass 2 · the doc may not parse
/// strict while `depends_on` is present, so everything stays line-based).
struct TaskFacts {
    /// task → its extracted `depends_on` block.
    deps_of: std::collections::BTreeMap<String, DepsBlock>,
    /// task → the producer may SKIP (`when:` · `for_each:` · `on_error` skip).
    can_skip: std::collections::BTreeMap<String, bool>,
    /// task → key line of `with:` · flow?
    with_head: std::collections::BTreeMap<String, (usize, bool)>,
    /// task → the 6-space keys under a block `with:`.
    with_keys: std::collections::BTreeMap<String, Vec<String>>,
}

/// One task's field scan (pass 2 unit).
struct TaskFieldScan {
    block: DepsBlock,
    skippable: bool,
    with_head: Option<(usize, bool)>,
}

/// Pass 2 · per-task facts (deps · `can_skip` · `with:` head + keys).
fn collect_task_facts(lines: &[&str], task_starts: &[(String, usize)]) -> TaskFacts {
    let mut facts = TaskFacts {
        deps_of: std::collections::BTreeMap::new(),
        can_skip: std::collections::BTreeMap::new(),
        with_head: std::collections::BTreeMap::new(),
        with_keys: std::collections::BTreeMap::new(),
    };
    for (ix, (id, _)) in task_starts.iter().enumerate() {
        let (start, end) = task_range(lines, task_starts, ix);
        let scan = scan_task_fields(lines, start, end);
        if let Some(head) = scan.with_head {
            facts.with_head.insert(id.clone(), head);
        }
        // with keys (6-space keys under a block with:)
        if let Some(&(wl, flow)) = facts.with_head.get(id)
            && !flow
        {
            facts
                .with_keys
                .insert(id.clone(), scan_with_keys(lines, wl, end));
        }
        facts.can_skip.insert(id.clone(), scan.skippable);
        facts.deps_of.insert(id.clone(), scan.block);
    }
    facts
}

/// Scan one task's 4-indent fields: `depends_on` entries · skippability
/// markers (`when:` · `for_each:` · `on_error` skip) · the `with:` head.
fn scan_task_fields(lines: &[&str], start: usize, end: usize) -> TaskFieldScan {
    let mut block = DepsBlock {
        deps: Vec::new(),
        lines: Vec::new(),
        malformed: false,
    };
    let mut skippable = false;
    let mut with_head: Option<(usize, bool)> = None;
    for i in start + 1..end {
        let l = lines[i];
        let t = l.trim_start();
        let indent = l.len() - t.len();
        if indent != 4 {
            continue; // only task-level fields drive the decision
        }
        if let Some(rest) = t.strip_prefix("depends_on:") {
            scan_deps_entries(lines, i, end, rest, &mut block);
        }
        if t.starts_with("when:") || t.starts_with("for_each:") {
            skippable = true;
        }
        if let Some(rest) = t.strip_prefix("with:") {
            let flow = rest.trim_start().starts_with('{');
            with_head = Some((i, flow));
        }
        if t.starts_with("on_error:") {
            // block or flow · skip: true anywhere within marks skippable
            if lines[i..end.min(i + 8)]
                .iter()
                .any(|l| l.contains("skip: true"))
            {
                skippable = true;
            }
        }
    }
    TaskFieldScan {
        block,
        skippable,
        with_head,
    }
}

/// The alphabet a W2 splice can write into `after:` — a bare task id
/// (`[a-z0-9_]+`). THE shape predicate: `nika-schema`'s parser mirrors it
/// (`is_bare_task_id`, `parser/tasks.rs`) to decide what the PARSE-024
/// finding may promise, so the finding and this scanner never disagree.
fn is_bare_task_id(entry: &str) -> bool {
    !entry.is_empty()
        && entry
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// One raw entry as the PARSER sees it — YAML dequotes a scalar, so the
/// line scanner strips a matching quote pair before it judges the
/// alphabet. `["a"]` IS `[a]` to everything that parses; the two
/// disagreed until 2026-09-06 (the finding promised `--fix` on the quoted
/// form · this scanner called it malformed · the author read both on one
/// screen). An unbalanced quote survives verbatim and fails the check.
fn dequote(entry: &str) -> &str {
    let mut chars = entry.chars();
    match (chars.next(), chars.next_back()) {
        (Some(open), Some(close)) if open == close && (open == '"' || open == '\'') => {
            chars.as_str()
        }
        _ => entry,
    }
}

/// Parse one `depends_on:` line (inline flow list · block list below) into
/// the task's `DepsBlock`.
fn scan_deps_entries(lines: &[&str], i: usize, end: usize, rest: &str, block: &mut DepsBlock) {
    block.lines.push(i);
    let rest = rest.trim();
    // a trailing `# …` belongs to no entry (the parser never sees it) —
    // reading it as one made `[a] # note` malformed while the finding
    // promised the repair. The block form already strips its comments.
    let rest = match rest.find(" #") {
        Some(cut) if rest.starts_with('[') => rest[..cut].trim_end(),
        _ => rest,
    };
    if let Some(inner) = rest.strip_prefix('[') {
        let Some(inner) = inner.strip_suffix(']') else {
            block.malformed = true;
            return;
        };
        for d in inner.split(',') {
            let d = d.trim();
            if d.is_empty() {
                continue; // `[]` declares no edge — the dead line just drops
            }
            let d = dequote(d);
            if is_bare_task_id(d) {
                block.deps.push(d.to_owned());
            } else {
                block.malformed = true;
            }
        }
    } else if rest.is_empty() || rest.starts_with('#') {
        // block list follows
        let mut j = i + 1;
        while j < end {
            let jl = lines[j].trim_start();
            let jind = lines[j].len() - jl.len();
            if jind < 6 || !jl.starts_with('-') {
                break;
            }
            let d = jl.trim_start_matches('-').trim();
            let d = d.split('#').next().unwrap_or("").trim();
            if is_bare_task_id(dequote(d)) {
                block.deps.push(dequote(d).to_owned());
            } else {
                block.malformed = true;
            }
            block.lines.push(j);
            j += 1;
        }
    } else {
        block.malformed = true;
    }
}

/// The 6-space keys under a block-style `with:` head.
fn scan_with_keys(lines: &[&str], wl: usize, end: usize) -> Vec<String> {
    let mut j = wl + 1;
    let mut keys = Vec::new();
    while j < end {
        let l = lines[j];
        let t = l.trim_start();
        let ind = l.len() - t.len();
        if t.is_empty() || t.starts_with('#') {
            j += 1;
            continue;
        }
        if ind < 6 {
            break;
        }
        if ind == 6
            && let Some((k, _)) = t.split_once(':')
        {
            keys.push(k.trim().to_owned());
        }
        j += 1;
    }
    keys
}

/// Everything pass 3 knows about one task while deciding its rewrite.
struct TaskCx<'a> {
    id: &'a str,
    /// The task-key line (insert anchor).
    key_line: usize,
    start: usize,
    end: usize,
    block: &'a DepsBlock,
    /// The task carries `on_error:` armor.
    armor: bool,
    /// Key line of `with:` · flow?
    with_head: Option<(usize, bool)>,
    /// Line range of the block-style `with:`.
    with_block: Option<(usize, usize)>,
}

/// Whether any task-body line opens `on_error:` armor.
fn has_armor(lines: &[&str], start: usize, end: usize) -> bool {
    lines[start + 1..end]
        .iter()
        .any(|l| l.trim_start().starts_with("on_error:"))
}

/// The line range of a block-style `with:` (head line → first <6-indent key).
fn with_block_range(
    lines: &[&str],
    with_head: Option<(usize, bool)>,
    end: usize,
) -> Option<(usize, usize)> {
    let (wl, flow) = with_head?;
    if flow {
        return None;
    }
    let mut wend = wl + 1;
    while wend < end {
        let l = lines[wend];
        let t = l.trim_start();
        let ind = l.len() - t.len();
        if !t.is_empty() && !t.starts_with('#') && ind < 6 {
            break;
        }
        wend += 1;
    }
    Some((wl, wend))
}

/// Task references inside the `with:` block (value-role set · any-role map).
struct WithRefs {
    /// Tasks referenced in value role.
    value: std::collections::BTreeSet<String>,
    /// task → has-value-role.
    any: std::collections::BTreeMap<String, bool>,
}

/// Referenced tasks (value-role · status-only) from the WITH block.
fn collect_with_refs(lines: &[&str], with_block: Option<(usize, usize)>) -> WithRefs {
    let mut with_refs = WithRefs {
        value: std::collections::BTreeSet::new(),
        any: std::collections::BTreeMap::new(),
    };
    if let Some((wl, wend)) = with_block {
        for line in &lines[wl..wend] {
            let (refs, _composite) = scan_islands(line);
            for r in refs {
                let value_role = !is_status_family(&r.path) && r.path != ".error";
                let e = with_refs.any.entry(r.task.clone()).or_insert(false);
                *e |= value_role;
                if value_role {
                    with_refs.value.insert(r.task);
                }
            }
        }
    }
    with_refs
}

/// References + hoists gathered from one task body (everything EXCEPT the
/// with block + the `depends_on` lines).
struct BodyScan {
    /// (src-island-inner, binding, path) per hoist.
    hoists: Vec<(String, String, String)>,
    /// Tasks read in value role from the body.
    value_refs: std::collections::BTreeSet<String>,
    /// Tasks read only through the status family.
    status_only: std::collections::BTreeSet<String>,
}

/// Body scan: hoist candidates + per-role reference sets + stop classes
/// S2 (`when:`) · S4 (composite / deep-under-armor) · S5 (`on_finally`).
fn scan_task_body(
    lines: &[&str],
    cx: &TaskCx<'_>,
    with_keys: Option<&[String]>,
    stops: &mut Vec<String>,
) -> BodyScan {
    let id = cx.id;
    let mut in_finally = false;
    let mut hoists: Vec<(String, String, String)> = Vec::new();
    let mut taken: std::collections::BTreeSet<String> = with_keys
        .map(|ks| ks.iter().cloned().collect())
        .unwrap_or_default();
    let mut value_refs = std::collections::BTreeSet::new();
    let mut status_only = std::collections::BTreeSet::new();
    for (i, line) in lines.iter().enumerate().take(cx.end).skip(cx.start + 1) {
        if cx.block.lines.contains(&i) {
            continue;
        }
        if let Some((wl, wend)) = cx.with_block
            && i >= wl
            && i < wend
        {
            continue;
        }
        let t = line.trim_start();
        let (refs, composite) = scan_islands(line);
        let is_when = t.starts_with("when:");
        let is_finally_head = t.starts_with("on_finally:");
        if is_finally_head {
            in_finally = true;
        }
        if refs.is_empty() && !composite {
            continue;
        }
        if is_when {
            stops.push(format!(
                "[S2] task `{id}`: when: references tasks.* — pre-W2 it REPLACED \
                 the gate; candidates: after: {{x: success}} (strict) · \
                 after: {{x: terminal}} + a .status binding (always/branch) · \
                 hoist the value into with: — each changes skipped-vs-cancelled \
                 observability differently; a human picks"
            ));
            continue;
        }
        if in_finally {
            for r in &refs {
                if r.task != *id {
                    stops.push(format!(
                        "[S5] task `{id}`: on_finally references tasks.{} — the \
                         parent is the only readable task in a cleanup (the read \
                         would race); hoist the value into the parent's with: or \
                         drop the read",
                        r.task
                    ));
                }
            }
            continue; // parent refs stay legal in place
        }
        if composite {
            stops.push(format!(
                "[S4] task `{id}`: a composite tasks.* island is not a plain \
                 reference — hoist the whole expression into with: by hand \
                 (its evaluation stage moves to the boundary)"
            ));
            continue;
        }
        for r in refs {
            if cx.armor && !is_bare_projection(&r.path) {
                stops.push(format!(
                    "[S4] task `{id}`: deep reference tasks.{}{} sits under \
                     on_error: — hoisting moves its evaluation outside the armor; \
                     split the read or accept the new error path by hand",
                    r.task, r.path
                ));
                continue;
            }
            if is_status_family(&r.path) {
                status_only.insert(r.task.clone());
            } else if r.path != ".error" {
                value_refs.insert(r.task.clone());
            }
            let src = format!("tasks.{}{}", r.task, r.path);
            if !hoists.iter().any(|(s, _, _)| *s == src) {
                let name = binding_name(&r.task, &r.path, &mut taken);
                hoists.push((src, name, r.path));
            }
        }
    }
    BodyScan {
        hoists,
        value_refs,
        status_only,
    }
}

/// Deps decisions: value-backed deps leave (R1/R3) · provably-strict bare
/// deps become `after:` entries (R2) · S1/S3 stop classes otherwise.
fn decide_deps(
    cx: &TaskCx<'_>,
    with_refs: &WithRefs,
    body: &BodyScan,
    can_skip: &std::collections::BTreeMap<String, bool>,
    stops: &mut Vec<String>,
) -> Vec<String> {
    let id = cx.id;
    let mut after_entries: Vec<String> = Vec::new();
    for d in &cx.block.deps {
        let value_backed = with_refs.value.contains(d) || body.value_refs.contains(d);
        let status_only = !value_backed
            && (body.status_only.contains(d)
                || with_refs.any.get(d).is_some_and(|has_value| !has_value));
        if value_backed {
            continue; // R1/R3 · the value edge carries the old pass-set
        }
        if status_only {
            stops.push(format!(
                "[S3] task `{id}`: dep `{d}` is backed only by an observation \
                 reference — the observation edge admits on MORE states than the \
                 old gate; keep tightness via after: {{{d}: success}} or accept \
                 the wider admission by hand"
            ));
            continue;
        }
        if can_skip.get(d).copied().unwrap_or(false) {
            stops.push(format!(
                "[S1] task `{id}`: bare dep `{d}` on a producer that may SKIP — \
                 the old gate ran on skipped; after: {{{d}: success}} cancels \
                 there · after: {{{d}: terminal}} also runs on failure · a value \
                 binding keeps {{success, skipped}} but imports data (W2-Q1)"
            ));
            continue;
        }
        after_entries.push(format!("      {d}: success"));
    }
    after_entries
}

/// The accumulated surgery plan across tasks (pass 3 output · pass 4 input).
#[derive(Default)]
struct SurgeryPlan {
    /// `depends_on` lines to drop.
    drop_lines: std::collections::BTreeSet<usize>,
    /// task-key line → the inserted block lines.
    inserts: std::collections::BTreeMap<usize, Vec<String>>,
    /// (line, from, to) island rewrites.
    rewrites: Vec<(usize, String, String)>,
    changed: bool,
}

/// Surgery for one clean task: drop its deps lines · insert the hoist /
/// `after:` blocks · rewrite its body islands (S6 stops a flow-style merge).
fn plan_task_surgery(
    lines: &[&str],
    cx: &TaskCx<'_>,
    hoists: &[(String, String, String)],
    after_entries: Vec<String>,
    plan: &mut SurgeryPlan,
    stops: &mut Vec<String>,
) {
    let id = cx.id;
    if cx.block.lines.is_empty() && hoists.is_empty() {
        return;
    }
    // surgery plan for this task
    for &l in &cx.block.lines {
        plan.drop_lines.insert(l);
        plan.changed = true;
    }
    let mut ins: Vec<String> = Vec::new();
    if !hoists.is_empty() {
        match cx.with_head {
            Some((_, true)) => {
                stops.push(format!(
                    "[S6] task `{id}`: hoist needed but with: is flow-style — \
                     merge by hand"
                ));
                return;
            }
            Some((wl, false)) => {
                // merge right under the existing with: line
                let merged = plan.inserts.entry(wl).or_default();
                for (src, name, _) in hoists {
                    merged.push(format!("      {name}: ${{{{ {src} }}}}"));
                }
                plan.changed = true;
            }
            None => {
                ins.push("    with:".to_owned());
                for (src, name, _) in hoists {
                    ins.push(format!("      {name}: ${{{{ {src} }}}}"));
                }
                plan.changed = true;
            }
        }
    }
    if !after_entries.is_empty() {
        ins.push("    after:".to_owned());
        ins.extend(after_entries);
        plan.changed = true;
    }
    if !ins.is_empty() {
        plan.inserts.entry(cx.key_line).or_default().extend(ins);
    }
    // island rewrites in the body (outside the with block)
    for (i, line) in lines.iter().enumerate().take(cx.end).skip(cx.start + 1) {
        if let Some((wl, wend)) = cx.with_block
            && i >= wl
            && i < wend
        {
            continue;
        }
        for (src, name, _) in hoists {
            let island_from = format!("${{{{ {src} }}}}");
            // tolerate tight spacing `${{tasks.x.output}}`
            let island_tight = format!("${{{{{src}}}}}");
            if line.contains(&island_from) {
                plan.rewrites
                    .push((i, island_from, format!("${{{{ with.{name} }}}}")));
            } else if line.contains(&island_tight) {
                plan.rewrites
                    .push((i, island_tight, format!("${{{{ with.{name} }}}}")));
            }
        }
    }
}

/// Pass 4 · emit: drop the dead lines · apply rewrites · splice inserts.
fn emit_migrated(lines: &[&str], plan: &SurgeryPlan) -> String {
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 8);
    for (i, l) in lines.iter().enumerate() {
        if plan.drop_lines.contains(&i) {
            continue;
        }
        let mut line = (*l).to_owned();
        for (ri, from, to) in &plan.rewrites {
            if *ri == i {
                line = line.replace(from, to);
            }
        }
        out.push(line);
        if let Some(ins) = plan.inserts.get(&i) {
            out.extend(ins.iter().cloned());
        }
    }
    out.join("\n")
}

/// A 2-space task key (`  name:` · optional trailing comment).
fn two_space_key(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("  ")?;
    if rest.starts_with(' ') || rest.starts_with('#') || rest.starts_with('-') {
        return None;
    }
    let (key, _) = rest.split_once(':')?;
    let ok = !key.is_empty()
        && key.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    ok.then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "# banner\nnika: demo-flow\nmodel: mock/echo\n\ntasks:\n  # fetch first\n  - id: fetch\n    invoke:\n      tool: nika:fetch\n      args: { url: x }\n\n  - id: summarize\n    depends_on: [fetch]\n    with:\n      doc: ${{ tasks.fetch.output }}\n    infer:\n      prompt: go\n";

    #[test]
    fn w1_keeps_crlf_line_endings_whole() {
        // Every line CRLF in, every line CRLF out — the rewritten task
        // lines included (they were LF before this pin: mixed endings).
        let old = "nika: t\r\ntasks:\r\n  - id: a\r\n    exec:\r\n      command: [\"true\"]\r\n  - id: b  # two\r\n    exec:\r\n      command: [\"true\"]\r\n";
        let new = w1(old).expect("changed");
        assert!(new.contains("  a:\r\n    exec:"), "{new:?}");
        assert!(new.contains("  b:  # two\r\n"), "{new:?}");
        for line in new.split_inclusive('\n') {
            assert!(line.ends_with("\r\n"), "an LF crept in: {line:?}");
        }
        // and an LF document stays LF (no `\r` invented)
        let lf = w1("nika: t\ntasks:\n  - id: a\n    exec:\n      command: [\"true\"]\n")
            .expect("changed");
        assert!(!lf.contains('\r'), "{lf:?}");
    }
    #[test]
    fn migrates_the_tasks_shapes_and_preserves_comments() {
        let new = w1(OLD).expect("changes");
        assert!(new.contains("  # fetch first\n  fetch:\n    invoke:"));
        assert!(new.contains("  summarize:\n    depends_on: [fetch]"));
        assert!(new.contains("# banner"), "comments preserved");
        assert!(!new.contains("- id:"));
    }

    #[test]
    fn idempotent_by_contract() {
        let once = w1(OLD).expect("changes");
        assert!(w1(&once).is_none(), "migrated form must return None");
    }

    #[test]
    fn verb_named_ids_survive_the_trap() {
        // the census trap: task ids shadowing verb names — the KEY is the
        // identity, the inner verb key is untouched
        let old = "nika: t\ntasks:\n  - id: invoke\n    exec:\n      command: [\"true\"]\n  - id: agent\n    infer:\n      prompt: hi\n";
        let new = w1(old).expect("changes");
        assert!(new.contains("  invoke:\n    exec:"));
        assert!(new.contains("  agent:\n    infer:"));
    }

    #[test]
    fn trailing_comments_ride_along() {
        let old = "nika: t\ntasks:\n  - id: probe  # the one task\n    exec:\n      command: [\"true\"]\n";
        let new = w1(old).expect("changes");
        assert!(new.contains("  probe:  # the one task"));
    }

    #[test]
    fn description_without_a_workflow_line_stays_put() {
        // The hoist needs its anchor: no `workflow:` line means nowhere
        // to re-emit the key — the line must survive untouched (dropping
        // it would silently lose the author's prose).
        let old = "description: |\n  some prose here\ntasks:\n  - id: probe\n    exec:\n      command: [\"true\"]\n";
        let new = w1(old).expect("tasks still migrate");
        assert!(
            new.contains("description: |\n  some prose here\n"),
            "prose kept at its column: {new}"
        );
        assert!(new.contains("  probe:\n"), "tasks list still maps: {new}");
    }

    #[test]
    fn already_new_form_untouched() {
        let doc = "nika: t\ntasks:\n  probe:\n    exec:\n      command: [\"true\"]\n";
        assert!(w1(doc).is_none());
    }

    #[test]
    fn id_not_first_line_is_left_for_a_human() {
        // deliberate refusal: the id is not the item's first line — the
        // transform does not fire on that item (the parser teaches).
        // Atomicity (issue #645): one non-mechanical item parks the WHOLE
        // list — a half-mapped collection would be invalid YAML.
        let old = "nika: t\ntasks:\n  - depends_on: []\n    id: probe\n";
        assert!(
            w1(old).is_none(),
            "the buried id parks the list, and with the envelope half retired \
             nothing else in the document changes — None IS the refusal"
        );
    }

    #[test]
    fn flow_items_expand_and_the_whole_list_converges() {
        // Issue #645 — the exact repro bytes: the flow item expands to
        // block entries, the block item keeps its body, and the whole
        // list becomes a map in the SAME pass as the envelope (the old
        // code left the flow item a sequence entry → mixed collection →
        // the intermediate document no longer parsed).
        let old = "nika: daily-brief\nmodel: ollama/llama3.2:3b\ntasks:\n  - { id: notes, invoke: { tool: \"nika:read\", args: { path: ./notes/today.md } } }\n  - id: triage\n    depends_on: [notes]\n    with:\n      notes: ${{ tasks.notes.output }}\n    infer: { prompt: \"triage ${{ with.notes }}\" }\n";
        let new = w1(old).expect("changes");
        assert_eq!(
            new,
            "nika: daily-brief\nmodel: ollama/llama3.2:3b\ntasks:\n  notes:\n    invoke: { tool: \"nika:read\", args: { path: ./notes/today.md } }\n  triage:\n    depends_on: [notes]\n    with:\n      notes: ${{ tasks.notes.output }}\n    infer: { prompt: \"triage ${{ with.notes }}\" }\n",
        );
        assert!(w1(&new).is_none(), "still idempotent: {new}");
    }

    #[test]
    fn quoted_commas_nested_flow_and_a_trailing_comment_survive() {
        // The splitter is quote/depth-aware: `,` and braces inside quoted
        // scalars never split an entry, `''` is an escaped quote, and a
        // `#` comment rides the rewritten key line like the block form's.
        let old = "nika: t\ntasks:\n  - { id: say, infer: { prompt: \"a, b } c\", note: 'it''s } fine' } } # the loud one\n";
        let new = w1(old).expect("changes");
        assert_eq!(
            new,
            "nika: t\ntasks:\n  say: # the loud one\n    infer: { prompt: \"a, b } c\", note: 'it''s } fine' }\n",
        );
    }

    #[test]
    fn one_non_mechanical_item_parks_the_whole_list() {
        // Atomicity: the flow item COULD expand, but the second item's
        // buried id is the ratified refusal — so the list stays a list
        // (valid YAML; the parser's teaching names the file) instead of
        // becoming a mixed collection no later pass can parse.
        let old = "nika: t\ntasks:\n  - { id: a, exec: { command: [\"true\"] } }\n  - { exec: { command: [\"true\"] }, id: b }\n";
        assert!(
            w1(old).is_none(),
            "the convertible item stays a sequence entry too — all or nothing"
        );
    }

    #[test]
    fn multi_line_flow_and_scalar_items_stay_for_a_human() {
        // A flow item spanning lines and a scalar item are outside the
        // mechanical reach — the whole list parks (atomicity), the
        // envelope still migrates, and the file remains valid YAML.
        let old = "nika: t\ntasks:\n  - { id: a,\n      exec: { command: [\"true\"] } }\n";
        assert!(w1(old).is_none(), "multi-line flow item parks the list");
        let old_scalar = "nika: t\ntasks:\n  - just_a_string\n";
        assert!(w1(old_scalar).is_none(), "a scalar item parks the list");
    }

    #[test]
    fn w2_emits_the_r5_predicate_spelling() {
        // The codemod never emits a spelling its own parser refuses —
        // post-R5 the provably-strict control edge is `after: {d: success}`.
        let old = "nika: t\ntasks:\n  a:\n    exec:\n      command: [\"true\"]\n  b:\n    depends_on: [a]\n    exec:\n      command: [\"true\"]\n";
        let W2Outcome::Changed(new) = w2(old) else {
            panic!("provably-strict control migrates");
        };
        assert!(new.contains("    after:\n      a: success\n"), "{new}");
        assert!(!new.contains("succeeded"), "{new}");
    }
}
