// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! C2 « the E-split » codemod — `vars:` → `inputs:` / `const:` (the
//! machine-applicable half of `NIKA-VALUES-001`).
//!
//! A line-based, structure-aware port of the spec's one official codemod
//! (`nika-spec/scripts/codemod-esplit.py`): comments, blank lines, anchors
//! and source order are preserved byte-for-byte outside the two transformed
//! shapes, and the transform is IDEMPOTENT (a post-C2 document returns
//! [`EsplitOutcome::Clean`]). It is the ONE repair `check --fix` applies
//! when the parser refuses a `vars:` envelope field — the dead form is
//! repairable, never executable (there is no legacy parser path).
//!
//! Per file, in ONE pass:
//! 1. SPLIT the top-level `vars:` block into `inputs:` and/or `const:`
//!    blocks per the ratified classification (below). A single-class file
//!    gets its header renamed — the block body rides byte-for-byte; a
//!    mixed file is regrouped `inputs:` then `const:` (caller contract
//!    first), each entry keeping its exact lines.
//! 2. REWRITE every `${{ ... vars.<name> ... }}` reference CLASS-AWARE in
//!    the same pass — the name→class map comes from the file's own `vars:`
//!    block, never a blind rename. Undeclared names and comment refs are
//!    LEFT ALONE (the re-check teaches them); refs inside `|` / `>` block
//!    scalars ARE rewritten (live template expressions, not comments).
//!
//! The classification (converges with the reference classifier
//! `c2-esplit-classifier-n2.py`):
//!
//! - bare literal (or bare mapping)          → const   (D-2026-07-19-N1)
//! - `{…, required: true}` without `value:`  → inputs  (D-2026-07-19-N1)
//! - `{type, default}` without `required:`   → const   (D-2026-07-19-N2 ·
//!   with the `default:`→`value:` completion — a typed var's default IS
//!   the typed constant's value, and `{type, default}` left standing in
//!   `const:` would read back as a bare literal MAP, silently changing
//!   what `${{ const.X }}` resolves to — equivalence-or-stop)
//! - `{type, value}` without `required:`     → const   (D-2026-07-19-N2)
//! - a credential-shaped name                → STOP    (law 15)
//! - a typed-only decl, or any other shape   → STOP    (law 15)
//!
//! Engine strengthenings over the Python original (equivalence-or-stop ·
//! the corpus the Python ran on never contained these shapes, so no
//! divergence binds): a flow-style `vars: {…}` HEADER STOPS (the Python
//! rewrote the refs and left the dead block standing) · a `required: true`
//! entry WITHOUT `type:` STOPS (the post-C2 `inputs:` authority is typed
//! by law — the codemod never emits a file its own parser refuses) · an
//! EMPTY `vars:` block STOPS (nothing to classify — drop it by hand) ·
//! intra-block trailing comments RIDE ALONG in the mixed regroup (the
//! Python dropped them). Anything that STOPs is surfaced and the file is
//! left untouched — atomic-or-nothing, the codemod never guesses.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// The E-split verdict (the w2 shape · changed-or-stop, plus the
/// idempotence class).
#[derive(Debug)]
#[non_exhaustive]
pub enum EsplitOutcome {
    /// Mechanically migrated (classify-not-rename · every entry's class
    /// came from the ratified rules) — the new text PLUS the surfaced
    /// notes: every `${{ vars.X }}` read LEFT ALONE (an undeclared name ·
    /// a comment ref), one note per site, mirroring the spec codemod's
    /// report (the re-check teaches the undeclared · prose is the
    /// authoring pass's job).
    Changed(String, Vec<String>),
    /// Nothing to migrate (no top-level `vars:` block · idempotence by
    /// contract — a migrated document passes through unchanged).
    Clean,
    /// At least one entry falls outside the ratified rules — each
    /// diagnostic names the entry and its reason; the file is left
    /// untouched (never guess · atomic-or-nothing).
    Stop(Vec<String>),
}

/// One entry's authority class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Inputs,
    Const,
}

impl Class {
    /// The block header (`inputs:` · `const:`).
    fn header(self) -> &'static str {
        match self {
            Self::Inputs => "inputs:",
            Self::Const => "const:",
        }
    }

    /// The `${{ }}` namespace root (`inputs` · `const`).
    fn namespace(self) -> &'static str {
        match self {
            Self::Inputs => "inputs",
            Self::Const => "const",
        }
    }
}

/// The declaration keys of the typed form (the pre-C2 `vars:` vocabulary —
/// a mapping carrying ANY of these is a declaration, not a bare literal).
const DECL_KEYS: [&str; 5] = ["type", "required", "default", "value", "description"];

/// The mixed regroup order (caller contract first).
const BLOCK_ORDER: [Class; 2] = [Class::Inputs, Class::Const];

/// Apply the E-split codemod.
#[must_use]
pub fn esplit(source: &str) -> EsplitOutcome {
    let lines: Vec<&str> = source.split('\n').collect();

    // The header: block-form is migratable · flow-form STOPs (engine
    // strengthening — the Python left the dead block standing) · absent
    // is Clean (idempotence).
    let mut saw_vars_key = false;
    for l in &lines {
        if l.starts_with("vars:") {
            if vars_header(l).is_none() {
                return EsplitOutcome::Stop(vec![
                    "[E1] flow-style `vars:` header — expand the block by hand \
                     (the codemod never guesses)"
                        .to_owned(),
                ]);
            }
            saw_vars_key = true;
            break;
        }
    }
    if !saw_vars_key {
        return EsplitOutcome::Clean;
    }

    // One segmentation drives both the classes and the surgery.
    let Some((hdr, end)) = find_vars_span(&lines) else {
        return EsplitOutcome::Clean; // unreachable past the header check
    };
    let (entries, _tail) = parse_block_entries(&lines[hdr + 1..end]);
    if entries.is_empty() {
        return EsplitOutcome::Stop(vec![
            "[E2] empty `vars:` block — nothing to classify · drop the dead \
             block by hand"
                .to_owned(),
        ]);
    }
    let mut name2class: BTreeMap<String, Class> = BTreeMap::new();
    let mut completions: BTreeSet<String> = BTreeSet::new();
    let mut stops: Vec<String> = Vec::new();
    for entry in &entries {
        match classify_entry(&entry.name, &entry.lines) {
            Ok((class, complete)) => {
                name2class.insert(entry.name.clone(), class);
                if complete {
                    completions.insert(entry.name.clone());
                }
            }
            Err(reason) => stops.push(format!("vars.{} · {reason}", entry.name)),
        }
    }
    if !stops.is_empty() {
        return EsplitOutcome::Stop(stops);
    }

    // Surgery: split the block · rewrite the refs (class-aware · the
    // left-alone refs surface as notes, mirroring the codemod report).
    let out = split_block(&lines, &name2class, &completions);
    let mut notes: Vec<String> = Vec::new();
    let out = rewrite_refs(&out, &name2class, &mut notes);
    let migrated = out.join("\n");
    if migrated == source {
        return EsplitOutcome::Clean; // defensive idempotence
    }
    EsplitOutcome::Changed(migrated, notes)
}

// ─────────────────────────── classification ───────────────────────────

/// A credential-shaped name (law 15) — the Python `SECRET_NAME` regex
/// (`secret|api[_-]?key|token|password|passwd|credential|private[_-]?key`,
/// case-insensitive · a search, never anchored).
fn is_credential_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // the `[_-]?` alternations squash away: api_key / api-key / apikey.
    let squashed: String = lower.chars().filter(|c| *c != '_' && *c != '-').collect();
    lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("credential")
        || squashed.contains("apikey")
        || squashed.contains("privatekey")
}

/// The declaration-shape facts of one entry, read line-based off the
/// entry's OWN lines (comment-stripped) — the flow form's top-level keys
/// · the block form's 4-indent keys.
struct Shape {
    /// The value is a mapping at all (vs a scalar / sequence literal).
    is_mapping: bool,
    /// The mapping's direct keys.
    keys: Vec<String>,
    /// A `required:` key parses YAML-true (`true` · `True` · `TRUE` — the
    /// bool spellings the engine's own parser accepts).
    required_true: bool,
}

/// Classify one `vars:` entry per the ratified rules — the class PLUS
/// whether the entry needs the D-N2 `default:`→`value:` completion.
/// `Err(reason)` is a STOP (atomic-or-nothing · the file is left
/// untouched).
fn classify_entry(name: &str, lines: &[String]) -> Result<(Class, bool), String> {
    if is_credential_name(name) {
        return Err("name signals a credential (belongs in secrets: with source:)".to_owned());
    }
    let Some(shape) = entry_shape(lines) else {
        return Err("unparseable entry shape — rewrite by hand".to_owned());
    };
    if !shape.is_mapping {
        return Ok((Class::Const, false)); // bare literal default (D-N1)
    }
    if !shape.keys.iter().any(|k| DECL_KEYS.contains(&k.as_str())) {
        return Ok((Class::Const, false)); // bare mapping literal (D-N1)
    }
    let req = shape.required_true;
    let has_value = shape.keys.iter().any(|k| k == "value");
    let has_default = shape.keys.iter().any(|k| k == "default");
    let has_type = shape.keys.iter().any(|k| k == "type");
    if has_value && !req {
        return Ok((Class::Const, false)); // typed + value: fixed (author authority)
    }
    if req && !has_value {
        // Engine strengthening: the post-C2 `inputs:` authority is typed
        // by law — a type-less required input has no mechanical home.
        return if has_type {
            Ok((Class::Inputs, false)) // required:true caller-provided (D-N1)
        } else {
            Err(
                "required:true without a declared `type:` — the post-C2 inputs: \
                 authority is typed by law (add type: by hand)"
                    .to_owned(),
            )
        };
    }
    if has_default && !req {
        // typed + default: no required (D-N2) → const. A TYPED entry's
        // `default:` IS the typed constant's `value:` (the key set is
        // closed at {type, value}) — the codemod completes the key, or
        // the entry would land as a bare literal MAP and silently change
        // what `${{ const.X }}` resolves to (equivalence-or-stop).
        if has_type && !has_value {
            if shape.keys.iter().all(|k| k == "type" || k == "default") {
                return Ok((Class::Const, true)); // the D-N2 default:→value: completion
            }
            return Err(
                "the typed constant carries {type, value} exactly — a `description:` \
                 or `required:` key has no home there (comment the description · drop \
                 the no-op required · by hand)"
                    .to_owned(),
            );
        }
        return Ok((Class::Const, false)); // a literal mapping — same value both sides
    }
    if has_type && !(req || has_value || has_default) {
        return Err("typed declaration with no required/default/value (law 15)".to_owned());
    }
    Err(format!(
        "shape not decided by the ratified rules (keys={:?})",
        shape.keys
    ))
}

/// Read one entry's shape off its lines (first line is the `  name:` line).
/// `None` only for an unbalanced flow map (a STOP, never a guess).
fn entry_shape(lines: &[String]) -> Option<Shape> {
    let first = lines.first()?;
    let code = strip_comment(first);
    let after_indent = code.get(2..)?; // strip the 2-space indent
    let (_name, rest) = after_indent.split_once(':')?;
    let rest = rest.trim();
    if rest.is_empty() {
        // block form — the value's direct keys live at indent exactly 4.
        let mut keys = Vec::new();
        let mut required_true = false;
        for l in &lines[1..] {
            let c = strip_comment(l);
            let indent = c.len() - c.trim_start_matches(' ').len();
            if indent != 4 {
                continue;
            }
            let t = c.trim_start();
            if t.starts_with("- ") || t == "-" {
                // a block sequence literal — never a declaration.
                return Some(Shape {
                    is_mapping: false,
                    keys: Vec::new(),
                    required_true: false,
                });
            }
            if let Some((k, v)) = t.split_once(':') {
                if k.trim() == "required" && matches!(v.trim(), "true" | "True" | "TRUE") {
                    required_true = true;
                }
                keys.push(k.trim().to_owned());
            }
        }
        return Some(Shape {
            is_mapping: !keys.is_empty(),
            keys,
            required_true,
        });
    }
    if rest.starts_with('{') {
        let entries = flow_top_level_entries(rest)?;
        let mut keys = Vec::new();
        let mut required_true = false;
        for (k, v) in entries {
            if k == "required" && matches!(v.trim(), "true" | "True" | "TRUE") {
                required_true = true;
            }
            keys.push(k);
        }
        return Some(Shape {
            is_mapping: true,
            keys,
            required_true,
        });
    }
    // scalar · flow sequence · block scalar — a bare literal.
    Some(Shape {
        is_mapping: false,
        keys: Vec::new(),
        required_true: false,
    })
}

/// The top-level `key: value` entries of a flow map (`rest` starts with
/// `{`), splitting commas at depth 1 only — nested maps / sequences /
/// quotes never split. `None` when the map never closes.
fn flow_top_level_entries(rest: &str) -> Option<Vec<(String, String)>> {
    let b = rest.as_bytes();
    let mut depth: i32 = 0;
    let mut in_s = false;
    let mut in_d = false;
    let mut entries = Vec::new();
    let mut seg_start = 1; // after '{'
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_s {
            if c == b'\'' {
                // a doubled '' is an escaped quote, never the closer
                if i + 1 < b.len() && b[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_s = false;
            }
        } else if in_d {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_d = false;
            }
        } else {
            match c {
                b'\'' => in_s = true,
                b'"' => in_d = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        push_flow_segment(&rest[seg_start..i], &mut entries);
                        return Some(entries);
                    }
                }
                b',' if depth == 1 => {
                    push_flow_segment(&rest[seg_start..i], &mut entries);
                    seg_start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// One flow segment → its `(key, raw-value)` pair (the key unquoted).
fn push_flow_segment(seg: &str, entries: &mut Vec<(String, String)>) {
    let seg = seg.trim();
    if seg.is_empty() {
        return;
    }
    match seg.split_once(':') {
        Some((k, v)) => entries.push((
            k.trim().trim_matches(['"', '\'']).to_owned(),
            v.trim().to_owned(),
        )),
        None => entries.push((seg.to_owned(), String::new())),
    }
}

// ─────────────────────────── line scanners ───────────────────────────

/// What a top-level `vars:` line IS — the block-style header shapes (a
/// flow header `vars: {…}` is NOT one). `Option` is the match: `None`
/// means the line is no block header at all.
enum VarsHeader {
    /// `vars:` alone on the line (an empty rest after trim).
    Bare,
    /// `vars:  # note` — the trailing comment rides (including the `#`).
    Commented(String),
}

/// A top-level block-style `vars:` header (`vars:` · `vars:  # note`).
fn vars_header(line: &str) -> Option<VarsHeader> {
    let rest = line.strip_prefix("vars:")?.trim_end();
    if rest.is_empty() {
        return Some(VarsHeader::Bare);
    }
    let t = rest.trim_start();
    // a non-empty rest is a header ONLY when it is a comment (`vars: x` is
    // the flow form, refused upstream) — the outer Option is the match.
    t.strip_prefix('#')
        .map(|c| VarsHeader::Commented(format!("#{c}")))
}

/// The `  name:` entry key of a vars-block body line (KEY2 ·
/// `[A-Za-z_][A-Za-z0-9_-]*` at exactly 2-space indent).
fn key2(line: &str) -> Option<String> {
    let rest = line.strip_prefix("  ")?;
    if rest.starts_with(' ') {
        return None;
    }
    let b = rest.as_bytes();
    let (&first, _) = b.split_first()?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut i = 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
        i += 1;
    }
    if b.get(i) != Some(&b':') {
        return None;
    }
    Some(rest[..i].to_owned())
}

/// A `key: |`-shaped block-scalar intro (`BLOCK_INTRO` ·
/// `:\s*[|>][+\-]?\d*\s*(#.*)?$` — quote-agnostic like the original).
fn block_intro(line: &str) -> bool {
    let code = naive_comment_cut(line).trim_end();
    let b = code.as_bytes();
    let mut i = b.len();
    // the tail walks backwards: digits? · sign? · `|`/`>` · whitespace · `:`
    while i > 0 && b[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i > 0 && (b[i - 1] == b'+' || b[i - 1] == b'-') {
        i -= 1;
    }
    if i == 0 || (b[i - 1] != b'|' && b[i - 1] != b'>') {
        return false;
    }
    i -= 1;
    while i > 0 && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
        i -= 1;
    }
    i > 0 && b[i - 1] == b':'
}

/// The line cut at its first whitespace-led `#` (the `BLOCK_INTRO`
/// comment rule — NOT quote-aware, matching the original regex).
fn naive_comment_cut(line: &str) -> &str {
    let b = line.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'#' && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') {
            return &line[..i];
        }
    }
    line
}

/// The column where a YAML comment starts on `line`, or `None`. Aware of
/// single / double quotes AND `${{ ... }}` islands (a `#` inside either
/// is literal). A `#` opens a comment only at column 0 or after
/// whitespace. (Port of the Python `comment_col`.)
fn comment_col(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let n = b.len();
    let mut in_s = false;
    let mut in_d = false;
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < n {
        let c = b[i];
        if in_s {
            if c == b'\'' {
                if i + 1 < n && b[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_s = false;
            }
        } else if in_d {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_d = false;
            }
        } else if depth > 0 {
            if c == b'}' && i > 0 && b[i - 1] == b'}' {
                depth -= 1;
            }
        } else {
            match c {
                b'\'' => in_s = true,
                b'"' => in_d = true,
                b'$' if i + 3 <= n && &b[i..i + 3] == b"${{" => {
                    depth += 1;
                    i += 3;
                    continue;
                }
                b'#' if i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t' => return Some(i),
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// The line cut at its comment column (shape scanning is comment-blind
/// but quote/island-aware — a `#` inside a quoted value never cuts).
fn strip_comment(line: &str) -> &str {
    match comment_col(line) {
        None => line,
        Some(c) => &line[..c],
    }
}

/// The 0-based flags of lines inside a `|` / `>` block-scalar body.
/// (Port of the Python `block_scalar_body`.)
fn block_scalar_body(lines: &[String]) -> Vec<bool> {
    let mut body = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if block_intro(&lines[i]) {
            let intro = lines[i].len() - lines[i].trim_start_matches(' ').len();
            let mut j = i + 1;
            while j < lines.len() {
                let lj = &lines[j];
                if lj.trim().is_empty() || (lj.len() - lj.trim_start_matches(' ').len()) > intro {
                    body[j] = true;
                    j += 1;
                    continue;
                }
                break;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    body
}

// ─────────────────────────── block surgery ───────────────────────────

/// `(header_idx, end_exclusive)` of the top-level `vars:` block. `end`
/// excludes trailing blank lines (they belong after the block).
fn find_vars_span(lines: &[&str]) -> Option<(usize, usize)> {
    for (i, line) in lines.iter().enumerate() {
        if vars_header(line).is_some() {
            let mut j = i + 1;
            let mut last = i;
            while j < lines.len() {
                let lj = lines[j];
                if lj.trim().is_empty() {
                    j += 1;
                    continue;
                }
                if lj.starts_with(' ') || lj.starts_with('\t') {
                    last = j;
                    j += 1;
                    continue;
                }
                break;
            }
            return Some((i, last + 1));
        }
    }
    None
}

/// One block entry: its lead lines (the comments / blanks that precede
/// it · they ride WITH the entry in a regroup), its own lines, its key.
struct BlockEntry {
    lead: Vec<String>,
    lines: Vec<String>,
    name: String,
}

/// Parse a vars-block body into its entries + the trailing lead (the
/// comments / blanks after the last entry). (Port of the Python
/// `_parse_block_entries`.)
fn parse_block_entries(body: &[&str]) -> (Vec<BlockEntry>, Vec<String>) {
    let mut entries: Vec<BlockEntry> = Vec::new();
    let mut lead: Vec<String> = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let line = body[i];
        let indent = line.len() - line.trim_start_matches(' ').len();
        let stripped = line.trim_start();
        if stripped.is_empty() || (stripped.starts_with('#') && indent <= 2) {
            lead.push((*line).to_owned());
            i += 1;
            continue;
        }
        if indent == 2
            && let Some(name) = key2(line)
        {
            let mut ent = vec![(*line).to_owned()];
            i += 1;
            while i < body.len() {
                let lj = body[i];
                let jindent = lj.len() - lj.trim_start_matches(' ').len();
                if lj.trim().is_empty() {
                    // a blank inside a value only if a deeper line follows
                    let mut k = i + 1;
                    while k < body.len() && body[k].trim().is_empty() {
                        k += 1;
                    }
                    if k < body.len() && (body[k].len() - body[k].trim_start_matches(' ').len()) > 2
                    {
                        ent.extend(body[i..k].iter().map(|s| (*s).to_owned()));
                        i = k;
                        continue;
                    }
                    break;
                }
                if jindent > 2 {
                    ent.push((*lj).to_owned());
                    i += 1;
                    continue;
                }
                break;
            }
            entries.push(BlockEntry {
                lead: std::mem::take(&mut lead),
                lines: ent,
                name,
            });
        } else {
            lead.push((*line).to_owned());
            i += 1;
        }
    }
    (entries, lead)
}

/// Rename (single-class) or regroup (mixed) the top-level `vars:` block.
/// Every entry rides through ONE per-entry path (lead + lines, then the
/// tail — byte-for-byte when nothing completes); `{type, default}`-exact
/// entries landing in const: get the D-N2 `default:`→`value:` completion.
/// (Port of the Python `_split_block` · the completion + the kept
/// intra-block trailing comments are the engine strengthenings.)
fn split_block(
    lines: &[&str],
    name2class: &BTreeMap<String, Class>,
    completions: &BTreeSet<String>,
) -> Vec<String> {
    let Some((hdr, end)) = find_vars_span(lines) else {
        return lines.iter().map(|s| (*s).to_owned()).collect();
    };
    let mut classes: BTreeSet<Class> = name2class.values().copied().collect();
    let (entries, tail) = parse_block_entries(&lines[hdr + 1..end]);
    let mut out: Vec<String> = lines[..hdr].iter().map(|s| (*s).to_owned()).collect();
    let emit_entry = |entry: &BlockEntry, out: &mut Vec<String>| {
        out.extend(entry.lead.iter().cloned());
        if completions.contains(&entry.name) {
            out.extend(complete_default_to_value(entry));
        } else {
            out.extend(entry.lines.iter().cloned());
        }
    };
    if classes.len() <= 1 {
        // single class: rename the header, keep the body byte-for-byte
        // (the completion aside)
        let only = classes.pop_first().unwrap_or(Class::Const);
        let comment = match vars_header(lines[hdr]) {
            Some(VarsHeader::Commented(c)) => format!(" {c}"),
            _ => String::new(),
        };
        out.push(format!("{}{comment}", only.header()));
        for entry in &entries {
            emit_entry(entry, &mut out);
        }
    } else {
        // mixed: regroup into inputs: then const: (caller contract first)
        for class in BLOCK_ORDER {
            let mut emitted_header = false;
            for entry in entries
                .iter()
                .filter(|e| name2class.get(&e.name) == Some(&class))
            {
                if !emitted_header {
                    out.push(class.header().to_owned());
                    emitted_header = true;
                }
                emit_entry(entry, &mut out);
            }
        }
    }
    out.extend(tail);
    out.extend(lines[end..].iter().map(|s| (*s).to_owned()));
    out
}

/// The D-N2 completion: rewrite the entry's `default:` KEY line to
/// `value:` (block form: the indent-4 key line · flow form: the first
/// whole-word `default:` on the entry's first line). Everything else
/// rides byte-for-byte — a typed var's `default:` IS the typed
/// constant's `value:` (spec 01 §const · the key set is closed).
fn complete_default_to_value(entry: &BlockEntry) -> Vec<String> {
    let mut done = false;
    entry
        .lines
        .iter()
        .map(|l| {
            if done {
                return l.clone();
            }
            if let Some(rewritten) = rename_default_key_line(l) {
                done = true;
                return rewritten;
            }
            l.clone()
        })
        .collect()
}

/// `default:` → `value:` on ONE key line (block: the indent-4 key line ·
/// flow: the first whole-word `default` followed by `:`). `None` when
/// the line carries no `default` key (comment lines never count).
fn rename_default_key_line(line: &str) -> Option<String> {
    if line.trim_start().starts_with('#') {
        return None;
    }
    // block form: `    default:` at the key column (the entry's OWN key —
    // a deeper indent is a nested structure, never the declaration key).
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent == 4 {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("default:") {
            return Some(format!("    value:{rest}"));
        }
        if let Some(rest) = t.strip_prefix("default :") {
            return Some(format!("    value :{rest}"));
        }
    }
    // flow form: the first whole-word `default` followed by `:`.
    let b = line.as_bytes();
    let mut i = 0;
    while i + 7 < b.len() {
        if &b[i..i + 7] == b"default" && (i == 0 || !is_word_byte(b[i - 1])) {
            let mut j = i + 7;
            while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                j += 1;
            }
            if j < b.len() && b[j] == b':' {
                return Some(format!("{}value{}", &line[..i], &line[j..]));
            }
        }
        i += 1;
    }
    None
}

// ─────────────────────────── reference rewrite ───────────────────────────

/// Rewrite every `${{ ... vars.<name> ... }}` reference class-aware —
/// comment refs verbatim · block-scalar refs rewritten · undeclared
/// names verbatim — and SURFACE the left-alone sites as notes (the spec
/// codemod's report shape: the re-check teaches the undeclared · prose
/// is the authoring pass's job). (Port of the Python `_rewrite_refs` +
/// `scan_refs`.)
fn rewrite_refs(
    lines: &[String],
    name2class: &BTreeMap<String, Class>,
    notes: &mut Vec<String>,
) -> Vec<String> {
    let body = block_scalar_body(lines);
    lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let ccol = if body[idx] { None } else { comment_col(line) };
            rewrite_line_refs(line, ccol, idx + 1, name2class, notes)
        })
        .collect()
}

/// One line's island rewrites + left-alone notes. `ccol` is the comment
/// column (`None` inside a block scalar, where every island is a live
/// template expression). `lineno` is 1-based (the note's site).
fn rewrite_line_refs(
    line: &str,
    ccol: Option<usize>,
    lineno: usize,
    name2class: &BTreeMap<String, Class>,
    notes: &mut Vec<String>,
) -> String {
    let b = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut copied = 0;
    let mut i = 0;
    while i + 3 <= b.len() {
        if &b[i..i + 3] == b"${{" {
            let in_comment = ccol.is_some_and(|c| i >= c);
            let Some(rel) = line[i + 3..].find("}}") else {
                break;
            };
            let close = i + 3 + rel;
            let island_body = &line[i + 3..close];
            if in_comment {
                // prose: never rewritten — but a DECLARED name surfaces
                // (the authoring pass owns the comment), an undeclared
                // one surfaces all the same.
                for name in vars_ref_names(island_body) {
                    note_left_alone(notes, lineno, &name, name2class.contains_key(&name), true);
                }
                break; // everything after the comment column is prose
            }
            for name in vars_ref_names(island_body) {
                if !name2class.contains_key(&name) {
                    note_left_alone(notes, lineno, &name, false, false);
                }
            }
            let substituted = substitute_vars_refs(island_body, name2class);
            out.push_str(&line[copied..i]);
            out.push_str("${{");
            out.push_str(&substituted);
            out.push_str("}}");
            copied = close + 2;
            i = close + 2;
            continue;
        }
        i += 1;
    }
    out.push_str(&line[copied..]);
    out
}

/// One left-alone site → its note (declared-in-comment vs undeclared).
fn note_left_alone(
    notes: &mut Vec<String>,
    lineno: usize,
    name: &str,
    declared: bool,
    in_comment: bool,
) {
    let note = if in_comment && declared {
        format!(
            "`${{{{ vars.{name} }}}}` line {lineno} · inside a comment — left as-is \
             (prose is the authoring pass's job)"
        )
    } else {
        format!(
            "`${{{{ vars.{name} }}}}` line {lineno} · `{name}` is not declared in the \
             migrated block — left as-is (the re-check refuses it · NIKA-VALUES-001)"
        )
    };
    if !notes.contains(&note) {
        notes.push(note);
    }
}

/// Every `vars.<name>` name inside one island body (VARREF ·
/// `\bvars\.([a-zA-Z_][a-zA-Z0-9_]*)` — the scan half, shared by the
/// note collector and the substitute).
fn vars_ref_names(body: &str) -> Vec<String> {
    let b = body.as_bytes();
    let mut names = Vec::new();
    let mut i = 0;
    while i + 5 < b.len() {
        if &b[i..i + 5] == b"vars."
            && (i == 0 || !is_word_byte(b[i - 1]))
            && (b[i + 5].is_ascii_alphabetic() || b[i + 5] == b'_')
        {
            let mut end = i + 6;
            while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
                end += 1;
            }
            names.push(body[i + 5..end].to_owned());
            i = end;
            continue;
        }
        i += 1;
    }
    names
}

/// The class-aware `vars.<name>` substitution inside one island body
/// (VARREF · `\bvars\.([a-zA-Z_][a-zA-Z0-9_]*)` — an undeclared name
/// stays verbatim). Byte-walking is ASCII-safe: a match boundary is
/// always an ASCII `v`, so no slice ever splits a UTF-8 char.
fn substitute_vars_refs(body: &str, name2class: &BTreeMap<String, Class>) -> String {
    let b = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut copied = 0;
    let mut i = 0;
    while i + 5 < b.len() {
        if &b[i..i + 5] == b"vars."
            && (i == 0 || !is_word_byte(b[i - 1]))
            && (b[i + 5].is_ascii_alphabetic() || b[i + 5] == b'_')
        {
            let mut end = i + 6;
            while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
                end += 1;
            }
            if let Some(class) = name2class.get(&body[i + 5..end]) {
                out.push_str(&body[copied..i]);
                out.push_str(class.namespace());
                out.push('.');
                out.push_str(&body[i + 5..end]);
                copied = end;
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&body[copied..]);
    out
}

/// The VARREF word-boundary alphabet (`[A-Za-z0-9_]`).
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The migrated text, or panic with the STOP notes.
    fn changed(source: &str) -> String {
        match esplit(source) {
            EsplitOutcome::Changed(text, _notes) => text,
            EsplitOutcome::Clean => panic!("expected Changed, got Clean: {source}"),
            EsplitOutcome::Stop(notes) => panic!("expected Changed, got Stop {notes:?}: {source}"),
        }
    }

    #[test]
    fn single_class_const_renames_header_body_byte_for_byte() {
        let old = "nika: w\nvars:\n  output_dir: \"./output\"\n  retries: 3\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n";
        let new = changed(old);
        assert!(
            new.contains("const:\n  output_dir: \"./output\"\n  retries: 3\ntasks:"),
            "header renamed · body byte-for-byte: {new}"
        );
        assert!(!new.contains("vars:"));
    }

    #[test]
    fn single_class_inputs_renames_header() {
        let old = "nika: w\nvars:\n  topic:\n    type: string\n    required: true\n    description: the subject\ntasks:\n  t:\n    infer: { prompt: go }\n";
        let new = changed(old);
        assert!(
            new.contains("inputs:\n  topic:\n    type: string\n    required: true\n    description: the subject\ntasks:"),
            "{new}"
        );
    }

    #[test]
    fn mixed_file_regroups_inputs_then_const_preserving_lines_and_comments() {
        let old = "nika: w\nvars:\n  # the caller contract\n  topic:\n    type: string\n    required: true\n  output_dir: \"./output\"\n  # a fixed knob\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: go }\n";
        let new = changed(old);
        let expected = "nika: w\ninputs:\n  # the caller contract\n  topic:\n    type: string\n    required: true\nconst:\n  output_dir: \"./output\"\n  # a fixed knob\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: go }\n";
        assert_eq!(
            new, expected,
            "inputs first · entries keep exact lines + lead comments"
        );
    }

    #[test]
    fn refs_rewrite_class_aware_never_blind() {
        let old = "vars:\n  topic:\n    type: string\n    required: true\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: \"${{ vars.topic }} · ${{ vars.retries }}\" }\n";
        let new = changed(old);
        assert!(new.contains("${{ inputs.topic }}"), "{new}");
        assert!(new.contains("${{ const.retries }}"), "{new}");
        assert!(!new.contains("vars.topic") && !new.contains("vars.retries"));
    }

    #[test]
    fn undeclared_refs_are_left_alone() {
        let old = "vars:\n  topic:\n    type: string\n    required: true\ntasks:\n  t:\n    infer: { prompt: \"${{ vars.topik }}\" }\n";
        let EsplitOutcome::Changed(new, notes) = esplit(old) else {
            panic!("expected Changed");
        };
        assert!(
            new.contains("${{ vars.topik }}"),
            "the typo'd ref stays — the re-check teaches it: {new}"
        );
        assert!(new.contains("inputs:\n  topic:"));
        assert!(
            notes
                .iter()
                .any(|n| n.contains("vars.topik") && n.contains("not declared")),
            "the left-alone site is surfaced: {notes:?}"
        );
    }

    #[test]
    fn comment_refs_are_left_alone() {
        let old = "vars:\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: go } # read ${{ vars.retries }} here\n";
        let EsplitOutcome::Changed(new, notes) = esplit(old) else {
            panic!("expected Changed");
        };
        assert!(
            new.contains("# read ${{ vars.retries }} here"),
            "prose is the authoring pass's job: {new}"
        );
        assert!(
            notes
                .iter()
                .any(|n| n.contains("vars.retries") && n.contains("comment")),
            "the comment site is surfaced: {notes:?}"
        );
    }

    #[test]
    fn block_scalar_refs_are_rewritten() {
        let old = "vars:\n  retries: 3\ntasks:\n  t:\n    infer:\n      prompt: |\n        retry up to ${{ vars.retries }} times\n";
        let new = changed(old);
        assert!(
            new.contains("retry up to ${{ const.retries }} times"),
            "a block scalar is a live template expression: {new}"
        );
    }

    #[test]
    fn credential_name_stops_atomically() {
        let old = "vars:\n  topic:\n    type: string\n    required: true\n  api_token: abc123\ntasks:\n  t:\n    infer: { prompt: go }\n";
        match esplit(old) {
            EsplitOutcome::Stop(notes) => {
                assert!(
                    notes
                        .iter()
                        .any(|n| n.contains("vars.api_token") && n.contains("credential")),
                    "{notes:?}"
                );
            }
            other => panic!("expected Stop, got {other:?}"),
        }
    }

    #[test]
    fn typed_only_decl_stops() {
        let old = "vars:\n  region:\n    type: string\ntasks:\n  t:\n    infer: { prompt: go }\n";
        match esplit(old) {
            EsplitOutcome::Stop(notes) => {
                assert!(
                    notes
                        .iter()
                        .any(|n| n.contains("vars.region") && n.contains("law 15")),
                    "{notes:?}"
                );
            }
            other => panic!("expected Stop, got {other:?}"),
        }
    }

    #[test]
    fn typed_with_default_is_const_dn2_with_the_value_completion() {
        // D-N2 · a typed var's `default:` IS the typed constant's
        // `value:` — left standing it would read back as a bare literal
        // MAP and silently change `${{ const.limit }}`.
        let old = "vars:\n  limit:\n    type: integer\n    default: 5\ntasks:\n  t:\n    infer: { prompt: \"${{ vars.limit }}\" }\n";
        let new = changed(old);
        assert!(
            new.contains("const:\n  limit:\n    type: integer\n    value: 5"),
            "{new}"
        );
        assert!(!new.contains("default:"), "the key is completed: {new}");
        assert!(new.contains("${{ const.limit }}"), "{new}");
    }

    #[test]
    fn typed_default_flow_entry_completes_inline() {
        let old = "vars:\n  limit: { type: integer, default: 5 }\ntasks:\n  t:\n    infer: { prompt: go }\n";
        let new = changed(old);
        assert!(
            new.contains("const:\n  limit: { type: integer, value: 5 }"),
            "{new}"
        );
    }

    #[test]
    fn typed_default_block_value_completes_keeping_the_value_lines() {
        // the t2-bookmark-triage shape: a block-sequence default value.
        let old = "vars:\n  bookmarks:\n    type: array\n    default:\n      - \"https://example.com\"\n      - \"https://example.org\" # the second\ntasks:\n  t:\n    for_each: { items: \"${{ vars.bookmarks }}\" }\n    infer: { prompt: go }\n";
        let new = changed(old);
        assert!(
            new.contains("const:\n  bookmarks:\n    type: array\n    value:\n      - \"https://example.com\"\n      - \"https://example.org\" # the second"),
            "{new}"
        );
        assert!(
            new.contains("for_each: { items: \"${{ const.bookmarks }}\" }"),
            "{new}"
        );
    }

    #[test]
    fn typed_default_with_description_stops_atomically() {
        // The description has no home in the closed {type, value} form —
        // never guess (a human comments it by hand).
        let old = "vars:\n  bookmarks:\n    type: array\n    default: []\n    description: the pile\ntasks:\n  t:\n    infer: { prompt: go }\n";
        match esplit(old) {
            EsplitOutcome::Stop(notes) => {
                assert!(
                    notes
                        .iter()
                        .any(|n| n.contains("vars.bookmarks") && n.contains("no home")),
                    "{notes:?}"
                );
            }
            other => panic!("expected Stop, got {other:?}"),
        }
    }

    #[test]
    fn typed_value_is_const() {
        let old = "vars:\n  window:\n    type: integer\n    value: 30\ntasks:\n  t:\n    infer: { prompt: go }\n";
        let new = changed(old);
        assert!(
            new.contains("const:\n  window:\n    type: integer\n    value: 30"),
            "{new}"
        );
    }

    #[test]
    fn required_true_with_default_is_inputs() {
        // req && !has_value fires BEFORE the default rule (rule order).
        let old = "vars:\n  region: { type: string, required: true, default: \"eu\" }\ntasks:\n  t:\n    infer: { prompt: go }\n";
        let new = changed(old);
        assert!(
            new.contains("inputs:\n  region: { type: string, required: true, default: \"eu\" }"),
            "{new}"
        );
    }

    #[test]
    fn flow_entries_classify_and_keep_their_lines() {
        let old = "vars:\n  topic: { type: string, required: true }\n  retries: 3\n  limits: { max: 10, min: 1 }\ntasks:\n  t:\n    infer: { prompt: \"${{ vars.topic }} ${{ vars.limits }}\" }\n";
        let new = changed(old);
        assert!(
            new.contains("inputs:\n  topic: { type: string, required: true }\n"),
            "{new}"
        );
        assert!(
            new.contains("const:\n  retries: 3\n  limits: { max: 10, min: 1 }\n"),
            "{new}"
        );
        assert!(
            new.contains("${{ inputs.topic }}") && new.contains("${{ const.limits }}"),
            "{new}"
        );
    }

    #[test]
    fn flow_header_stops() {
        let old = "vars: { retries: 3 }\ntasks:\n  t:\n    infer: { prompt: go }\n";
        match esplit(old) {
            EsplitOutcome::Stop(notes) => {
                assert!(notes.iter().any(|n| n.contains("[E1]")), "{notes:?}");
            }
            other => panic!("expected Stop, got {other:?}"),
        }
    }

    #[test]
    fn required_true_without_type_stops() {
        // Engine strengthening — the migration must never emit a file its
        // own parser refuses (inputs: is typed by law).
        let old = "vars:\n  topic:\n    required: true\ntasks:\n  t:\n    infer: { prompt: go }\n";
        match esplit(old) {
            EsplitOutcome::Stop(notes) => {
                assert!(
                    notes
                        .iter()
                        .any(|n| n.contains("vars.topic") && n.contains("typed by law")),
                    "{notes:?}"
                );
            }
            other => panic!("expected Stop, got {other:?}"),
        }
    }

    #[test]
    fn empty_vars_block_stops() {
        let old = "vars:\ntasks:\n  t:\n    infer: { prompt: go }\n";
        match esplit(old) {
            EsplitOutcome::Stop(notes) => {
                assert!(notes.iter().any(|n| n.contains("[E2]")), "{notes:?}");
            }
            other => panic!("expected Stop, got {other:?}"),
        }
    }

    #[test]
    fn header_comment_is_preserved() {
        let old =
            "vars:  # SLOT: the values\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: go }\n";
        let new = changed(old);
        assert!(new.contains("const: # SLOT: the values"), "{new}");
    }

    #[test]
    fn idempotent_clean_on_post_c2() {
        let new_form = "nika: w\ninputs:\n  topic: { type: string, required: true }\nconst:\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: \"${{ inputs.topic }} ${{ const.retries }}\" }\n";
        assert!(matches!(esplit(new_form), EsplitOutcome::Clean));
        // and a migrated document passes through unchanged on a second run
        let once = changed("vars:\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: go }\n");
        assert!(matches!(esplit(&once), EsplitOutcome::Clean), "{once}");
    }

    #[test]
    fn block_sequence_literal_is_const() {
        let old = "vars:\n  locales:\n    - fr\n    - es\ntasks:\n  t:\n    for_each: { items: \"${{ vars.locales }}\" }\n    infer: { prompt: go }\n";
        let new = changed(old);
        assert!(
            new.contains("const:\n  locales:\n    - fr\n    - es"),
            "{new}"
        );
        assert!(
            new.contains("for_each: { items: \"${{ const.locales }}\" }"),
            "{new}"
        );
    }

    #[test]
    fn trailing_block_comment_rides_along_in_the_mixed_regroup() {
        // Engine strengthening — the Python dropped the intra-block tail.
        let old = "vars:\n  topic: { type: string, required: true }\n  retries: 3\n  # sweep this up next pass\ntasks:\n  t:\n    infer: { prompt: go }\n";
        let new = changed(old);
        assert!(new.contains("# sweep this up next pass"), "{new}");
    }

    #[test]
    fn word_boundary_never_rewrites_inside_an_identifier() {
        let old =
            "vars:\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: \"${{ avars.retries }}\" }\n";
        let new = changed(old);
        assert!(new.contains("${{ avars.retries }}"), "{new}");
    }
}
