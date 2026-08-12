// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The F-O1 PR-2 runtime re-gate (NEP-0004 law 2 · « the
//! permit-parameterization taint »): an UNTRUSTED value that reaches a
//! permitted verb's ARGUMENT is matched against the step's `permits:` on
//! its RESOLVED, canonical form — the category grant says nothing about
//! the resolved value. Refusal rides `NIKA-SEC-004` (the runtime
//! boundary's one voice); the check-time twin is `NIKA-AUTH-008` (PR-3).
//!
//! The seams covered here (ENGINE.md « le point de re-gate »):
//!
//! - **exec argv[1..]** — rendered per element (no shell), so each
//!   element's RAW template labels against [`ValueTaint`], and a tainted
//!   element's RENDERED value is canonicalized then matched: (a) a `-`-
//!   prefixed token whose static template was NOT itself an option is
//!   option-injection (the re-entry class — `--exec` · `-c` ·
//!   `--checkpoint-action=…` — is never covered, NEP-0004's exec
//!   canonical form); (b) a URL-shaped value re-gates on its host; (c) a
//!   path-shaped value re-gates on the lexical canonical fold
//!   ([`lexically_normalize`]) against `fs.read`. `argv[0]` is the
//!   program — already matched on its RESOLVED value by
//!   `check_exec_permits`. `cwd` re-gates as a path.
//! - **`mcp:*` invoke args** — the tool id is static (gated before
//!   rendering); the rendered args were never re-gated: the grant of the
//!   tool IS the boundary, and a server-side path/host slipped through.
//!   Every tainted string leaf re-gates like an argv element, except the
//!   fs direction is unknowable at the border, so it must be covered by
//!   `fs.read` OR `fs.write`.
//!
//! What is deliberately NOT here:
//!
//! - **first-party fs/net builtins** (`nika:read` · `nika:write` ·
//!   `nika:fetch` · …) — already re-gated on the resolved value at their
//!   own boundary (`boundary.enforce`, canonicalize-then-confine · the
//!   one-hop net enforce). Cited, never duplicated (ENGINE.md step 6).
//! - **the shell command form** — a pipeline has no per-token canonical
//!   form (that is exactly why the argv form exists); under a program
//!   allowlist the shell form is already refused by FORM, and under
//!   `exec: true` the OS jail (`spec_of`, the SAME declared boundary)
//!   confines its fs at open. v1 declares this; F-O2 owns the jail.
//! - **a tainted island rendering to a non-string JSON leaf** (a
//!   structured payload under an mcp arg) — no path/host/option shape to
//!   canonicalize; v1 passes it (the structured-payload channel is a
//!   declared residual, beside the file channel of ENGINE.md risk (d)).
//! - **`env:`** — lane F-O4. **In-boundary exfiltration** (the permit
//!   COVERS the resolved value — the confidentiality axis) — the
//!   trifecta / F-O10 terrain: a re-gate matches, it cannot judge.
//!
//! Over-refusal is accepted v1 (fail-closed · ENGINE.md risk (c)): a
//! tainted negative number (`-5`) reads as an option token, a tainted
//! output path reads under `fs.read`. The honest pivot holds: a value
//! canonicalizing INSIDE the permit runs — the re-gate is not a blind
//! deny. The `declassify:` door (NEP-0004 law 5) lands with PR-3.

use std::collections::BTreeMap;

use nika_cap::{Integrity, lexically_normalize};
use nika_schema::Spanned;
use nika_schema::types::Permits;
use serde_json::Value;

use super::Dispatched;
use crate::integrity::ValueTaint;
use crate::record::TaskRecord;
use crate::witness::PermitWitness;

/// The exec argv re-gate — every `argv[1..]` element whose RAW template
/// reads untrusted content is matched on its RENDERED value against the
/// step's permit. `elements` are the authored templates, `argv` the
/// rendered vector (parallel by construction: one render per element).
pub(super) fn regate_exec_argv(
    permits: &Permits,
    note: &str,
    elements: &[Spanned<String>],
    argv: &[String],
    taint: &ValueTaint<'_>,
    records: &BTreeMap<String, TaskRecord>,
    witness: &PermitWitness,
) -> Option<Dispatched> {
    debug_assert_eq!(
        elements.len(),
        argv.len(),
        "the argv form renders one element per template"
    );
    for (i, (template, rendered)) in elements.iter().zip(argv).enumerate().skip(1) {
        let Integrity::Untrusted { source } = taint.label(&template.value, records) else {
            continue;
        };
        let sink = format!("exec.argv[{i}]");
        if let Some(denial) = regate_value(
            permits,
            note,
            &source,
            &sink,
            &template.value,
            rendered,
            Plane::Exec,
        ) {
            witness.record(
                "regate",
                &sink,
                "deny",
                format!("tainted by {source} · escaped"),
            );
            return Some(denial);
        }
        witness.record(
            "regate",
            &sink,
            "allow",
            format!("tainted by {source} · covered"),
        );
    }
    None
}

/// The exec `cwd` re-gate — a directory is always path-shaped, so a
/// tainted cwd is matched on its canonical form against `fs.read`
/// (ENGINE.md step 5 · « `cwd` idem (path) »).
pub(super) fn regate_exec_cwd(
    permits: &Permits,
    note: &str,
    template: &str,
    rendered: &str,
    taint: &ValueTaint<'_>,
    records: &BTreeMap<String, TaskRecord>,
    witness: &PermitWitness,
) -> Option<Dispatched> {
    let Integrity::Untrusted { source } = taint.label(template, records) else {
        return None;
    };
    if permits.allows_path(rendered, false) {
        witness.record(
            "regate",
            "exec.cwd",
            "allow",
            format!("tainted by {source} · covered"),
        );
        return None;
    }
    witness.record(
        "regate",
        "exec.cwd",
        "deny",
        format!("tainted by {source} · escaped"),
    );
    Some(path_refusal(
        note,
        &source,
        "exec.cwd",
        rendered,
        &fs_globs(permits, false),
    ))
}

/// The `mcp:*` invoke-args re-gate — walks the RAW args JSON in parallel
/// with the RENDERED one (same keys, same array lengths — `render_json`
/// preserves shape; only string leaves can change type). Every tainted
/// string leaf re-gates on its rendered value.
#[allow(clippy::too_many_arguments)] // REASON: the border's own state (permit · note · tool · two JSON views · oracle · records · witness) — each a distinct read.
pub(super) fn regate_mcp_args(
    permits: &Permits,
    note: &str,
    tool: &str,
    raw_args: &Value,
    rendered_args: &Value,
    taint: &ValueTaint<'_>,
    records: &BTreeMap<String, TaskRecord>,
    witness: &PermitWitness,
) -> Option<Dispatched> {
    let denial = regate_json(
        permits,
        note,
        tool,
        raw_args,
        rendered_args,
        "",
        taint,
        records,
    );
    // The mcp border's decision, one frame per call: per-leaf verdicts
    // stay internal to the walk (the denial names the leaf) — the
    // witness records the BORDER outcome (NEP-0007 · bounded volume).
    match &denial {
        Some(_) => witness.record("regate", tool, "deny", "a tainted arg leaf escaped"),
        None => witness.record("regate", tool, "allow", "rendered args within the boundary"),
    }
    denial
}

/// The recursive half of [`regate_mcp_args`] — `path` is the JSON
/// accessor chain (`.key[0]`) for the refusal's sink name.
#[allow(clippy::too_many_arguments)] // REASON: the walk's own state (permit · note · tool · two JSON views · path · oracle · records) — each one a distinct read.
fn regate_json(
    permits: &Permits,
    note: &str,
    tool: &str,
    raw: &Value,
    rendered: &Value,
    path: &str,
    taint: &ValueTaint<'_>,
    records: &BTreeMap<String, TaskRecord>,
) -> Option<Dispatched> {
    match raw {
        Value::String(template) => {
            let Integrity::Untrusted { source } = taint.label(template, records) else {
                return None;
            };
            let Value::String(value) = rendered else {
                // A single island resolving to a non-string leaf (number ·
                // structured payload): no canonical form to match — the
                // declared v1 residual (module docs).
                return None;
            };
            let sink = format!("{tool}.args{path}");
            regate_value(permits, note, &source, &sink, template, value, Plane::Mcp)
        }
        Value::Array(items) => {
            let rendered_items = rendered.as_array();
            for (i, item) in items.iter().enumerate() {
                let Some(rendered_item) = rendered_items.and_then(|a| a.get(i)) else {
                    debug_assert!(false, "render_json preserves array shape");
                    continue;
                };
                if let Some(denial) = regate_json(
                    permits,
                    note,
                    tool,
                    item,
                    rendered_item,
                    &format!("{path}[{i}]"),
                    taint,
                    records,
                ) {
                    return Some(denial);
                }
            }
            None
        }
        Value::Object(map) => {
            for (key, value) in map {
                let Some(rendered_value) = rendered.get(key) else {
                    debug_assert!(false, "render_json preserves object keys");
                    continue;
                };
                if let Some(denial) = regate_json(
                    permits,
                    note,
                    tool,
                    value,
                    rendered_value,
                    &format!("{path}.{key}"),
                    taint,
                    records,
                ) {
                    return Some(denial);
                }
            }
            None
        }
        // Non-string raw leaves carry no template — nothing to re-gate.
        _ => None,
    }
}

/// The verb plane a tainted value crosses: the exec argv form adds the
/// option-injection class; an mcp arg's fs direction is unknowable at
/// the border, so it must be covered by `fs.read` OR `fs.write`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plane {
    Exec,
    Mcp,
}

/// One tainted resolved value's verdict: canonicalize FIRST, then match
/// the step's permit. `None` = covered or shapeless (the value runs).
fn regate_value(
    permits: &Permits,
    note: &str,
    source: &str,
    sink: &str,
    template: &str,
    rendered: &str,
    plane: Plane,
) -> Option<Dispatched> {
    // (a) Option-injection (exec only): the AUTHOR's slot was data (the
    //     template does not start with `-`) yet the untrusted value
    //     arrived as a `-`-prefixed token. The re-entry class
    //     (`--exec` · `-c` · `--checkpoint-action=…`) is never covered
    //     unless the permit names it — and a `permits.exec` allowlist
    //     names PROGRAMS, never options, so the refusal is total
    //     (NEP-0004 · the exec canonical form).
    if plane == Plane::Exec && rendered.starts_with('-') && !template.starts_with('-') {
        return Some(Dispatched::security_err(
            note,
            format!(
                "tainted argument under permit · taint path: {source} -> {sink} · \
                 resolved {rendered:?} — an untrusted value resolved to an option token \
                 where the author wrote data (the re-entry class is never covered · \
                 NEP-0004 law 2)"
            ),
        ));
    }
    // (b) URL-shaped → the HOST axis. WHATWG extraction (the same parser
    //     the static twin and the http effect read — a string split
    //     disagrees on `\`/userinfo/case, and that gap is a bypass).
    if let Some(host) = url_host(rendered) {
        if permits.allows_host(&host) {
            return None;
        }
        return Some(Dispatched::security_err(
            note,
            format!(
                "tainted argument under permit · taint path: {source} -> {sink} · \
                 resolved {rendered:?} · host {host:?} ∉ net.http {}",
                net_globs(permits)
            ),
        ));
    }
    // (c) Path-shaped → the FS axis on the lexical canonical fold (the
    //     SAME normalization `allows_path` matches against — message and
    //     verdict cannot disagree).
    if looks_like_path(rendered) {
        let covered = permits.allows_path(rendered, false)
            || (plane == Plane::Mcp && permits.allows_path(rendered, true));
        if covered {
            return None;
        }
        let globs = match plane {
            Plane::Exec => fs_globs(permits, false),
            Plane::Mcp => format!(
                "{} ∪ fs.write {}",
                fs_globs(permits, false),
                fs_globs(permits, true)
            ),
        };
        return Some(path_refusal(note, source, sink, rendered, &globs));
    }
    // (d) Plain data — no permit axis carries a shape to match.
    None
}

/// The path-escape refusal (`NIKA-SEC-004`): taint path source-first,
/// then the resolved value, its canonical form, and the bound it escaped
/// (NEP-0004 law 3 — the check-time `NIKA-AUTH-008` speaks the same
/// shape).
fn path_refusal(note: &str, source: &str, sink: &str, rendered: &str, globs: &str) -> Dispatched {
    Dispatched::security_err(
        note,
        format!(
            "tainted argument under permit · taint path: {source} -> {sink} · \
             resolved {rendered:?} · canonical {:?} ∉ fs.read {globs}",
            lexically_normalize(rendered)
        ),
    )
}

/// Does a resolved value carry path SHAPE? A separator (`/` · `\`), a
/// bare dot segment (`.` · `..`), or a home-rooted `~/…`. A plain word
/// (`report.csv`) is DATA — class (d) — never a path; `..foo` is a real
/// segment name, never a traversal.
fn looks_like_path(value: &str) -> bool {
    value.contains('/') || value.contains('\\') || matches!(value, "." | "..")
}

/// The host of a resolved URL value, via the `url` crate — the SAME
/// WHATWG normalization the static twin (`permits_fit::url_host`) and
/// the http effect (`nika-http`'s `host_of`) read, trailing dot stripped
/// on all three sides. `None` when the value is not an authority-carrying
/// URL (a relative path · an opaque `scheme:…` — those fall through to
/// the path axis or to data, never a string-split guess).
fn url_host(raw: &str) -> Option<String> {
    match url::Url::parse(raw).ok()?.host()? {
        url::Host::Domain(d) => Some(d.trim_end_matches('.').to_owned()),
        url::Host::Ipv4(a) => Some(a.to_string()),
        url::Host::Ipv6(a) => Some(a.to_string()),
    }
}

/// The declared `net.http` globs for a refusal message (`[]` stated
/// plainly when the block is absent — default-deny).
fn net_globs(permits: &Permits) -> String {
    match &permits.net {
        Some(net) => format!("{:?}", net.http),
        None => "[] (no `net:` block declared · default-deny)".to_owned(),
    }
}

/// The declared `fs.{read,write}` globs for a refusal message (same
/// absent-block honesty).
fn fs_globs(permits: &Permits, write: bool) -> String {
    match &permits.fs {
        Some(fs) => {
            let globs = if write { &fs.write } else { &fs.read };
            format!("{globs:?}")
        }
        None => "[] (no `fs:` block declared · default-deny)".to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn permits_of(yaml: &str) -> Permits {
        let wf = nika_schema::parse(
            &format!("nika: w\n{yaml}\ntasks:\n  t:\n    exec: {{ command: [\"true\"] }}\n"),
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        wf.permits.expect("permits declared").value
    }

    fn tainted(template: &str) -> (ValueTaint<'static>, BTreeMap<String, TaskRecord>) {
        // One settled untrusted record `dl`; every template below reads it.
        let mut rec = TaskRecord::unran(
            crate::record::TaskStatus::Success,
            crate::record::TerminalCause::Normal,
        );
        rec.integrity = Integrity::untrusted("dl");
        let mut records = BTreeMap::new();
        records.insert("dl".to_owned(), rec);
        let _ = template;
        (ValueTaint::bare(), records)
    }

    #[test]
    fn option_injection_is_refused_only_when_the_slot_was_data() {
        let permits = permits_of("permits: { exec: [\"tar\"] }");
        let (taint, records) = tainted("");
        let elements = vec![
            Spanned::new("tar".to_owned(), nika_schema::Span::default()),
            Spanned::new("-xf".to_owned(), nika_schema::Span::default()),
            Spanned::new(
                "${{ tasks.dl.output }}".to_owned(),
                nika_schema::Span::default(),
            ),
        ];
        // The re-entry token arrives through the data slot → refused.
        let argv = vec![
            "tar".to_owned(),
            "-xf".to_owned(),
            "--checkpoint-action=exec=sh id".to_owned(),
        ];
        let denial = regate_exec_argv(
            &permits,
            "exec · tar",
            &elements,
            &argv,
            &taint,
            &records,
            &PermitWitness::new(),
        );
        let denial = denial.expect("the option token is refused");
        let err = denial.result.err().expect("a refusal");
        assert_eq!(err.record.code, "NIKA-SEC-004");
        assert!(
            err.record.message.contains("option token")
                && err.record.message.contains("dl -> exec.argv[2]")
                && err
                    .record
                    .message
                    .contains("--checkpoint-action=exec=sh id"),
            "taint path source-first + the resolved value: {}",
            err.record.message
        );
        // The AUTHOR wrote the option (`--output=` prefix) with data inside
        // → NOT option-injection (the path axis still watches the value).
        let authored = vec![
            Spanned::new("tar".to_owned(), nika_schema::Span::default()),
            Spanned::new(
                "--file=${{ tasks.dl.output }}".to_owned(),
                nika_schema::Span::default(),
            ),
        ];
        let argv = vec!["tar".to_owned(), "--file=report.csv".to_owned()];
        assert!(
            regate_exec_argv(
                &permits,
                "exec · tar",
                &authored,
                &argv,
                &taint,
                &records,
                &PermitWitness::new()
            )
            .is_none(),
            "an authored option with a plain-data value runs"
        );
    }

    #[test]
    fn traversal_under_fs_read_is_refused_but_the_pivot_runs() {
        let permits = permits_of("permits: { exec: [\"tar\"], fs: { read: [\"datasets/**\"] } }");
        let (taint, records) = tainted("");
        let elements = vec![
            Spanned::new("tar".to_owned(), nika_schema::Span::default()),
            Spanned::new("-xf".to_owned(), nika_schema::Span::default()),
            Spanned::new(
                "${{ tasks.dl.output }}".to_owned(),
                nika_schema::Span::default(),
            ),
        ];
        // The ledger row: `datasets/../../../etc/passwd` escapes → refused.
        let escaping = vec![
            "tar".to_owned(),
            "-xf".to_owned(),
            "datasets/../../../etc/passwd".to_owned(),
        ];
        let denial = regate_exec_argv(
            &permits,
            "exec · tar",
            &elements,
            &escaping,
            &taint,
            &records,
            &PermitWitness::new(),
        )
        .expect("the traversal is refused");
        let err = denial.result.err().expect("a refusal");
        assert_eq!(err.record.code, "NIKA-SEC-004");
        assert!(
            err.record
                .message
                .contains("canonical \"../../etc/passwd\" ∉ fs.read [\"datasets/**\"]"),
            "the canonical fold + the escaped bound: {}",
            err.record.message
        );
        // The pivot: the same slot canonicalizes INSIDE the permit → runs.
        let inside = vec![
            "tar".to_owned(),
            "-xf".to_owned(),
            "datasets/2026/report.csv".to_owned(),
        ];
        assert!(
            regate_exec_argv(
                &permits,
                "exec · tar",
                &elements,
                &inside,
                &taint,
                &records,
                &PermitWitness::new()
            )
            .is_none(),
            "a value covered by the permit is not a blind deny"
        );
        // A back-spelled in-boundary path (`datasets/../datasets/q3.csv`)
        // canonicalizes inside → runs (NEP-0004 fixture 013's runtime twin).
        let folded = vec![
            "tar".to_owned(),
            "-xf".to_owned(),
            "datasets/../datasets/q3.csv".to_owned(),
        ];
        assert!(
            regate_exec_argv(
                &permits,
                "exec · tar",
                &elements,
                &folded,
                &taint,
                &records,
                &PermitWitness::new()
            )
            .is_none(),
            "the canonical form, never a raw prefix"
        );
    }

    #[test]
    fn a_tainted_url_regates_on_its_host() {
        let permits =
            permits_of("permits: { exec: [\"curl\"], net: { http: [\"api.example.com\"] } }");
        let (taint, records) = tainted("");
        let elements = vec![
            Spanned::new("curl".to_owned(), nika_schema::Span::default()),
            Spanned::new(
                "${{ tasks.dl.output }}".to_owned(),
                nika_schema::Span::default(),
            ),
        ];
        let evil = vec!["curl".to_owned(), "https://evil.example/x".to_owned()];
        let denial = regate_exec_argv(
            &permits,
            "exec · curl",
            &elements,
            &evil,
            &taint,
            &records,
            &PermitWitness::new(),
        )
        .expect("the escaped host is refused");
        let err = denial.result.err().expect("a refusal");
        assert!(
            err.record
                .message
                .contains("host \"evil.example\" ∉ net.http [\"api.example.com\"]"),
            "the host axis speaks: {}",
            err.record.message
        );
        let ok = vec!["curl".to_owned(), "https://api.example.com/v1".to_owned()];
        assert!(
            regate_exec_argv(
                &permits,
                "exec · curl",
                &elements,
                &ok,
                &taint,
                &records,
                &PermitWitness::new()
            )
            .is_none(),
            "an in-permit host runs"
        );
    }

    #[test]
    fn a_tainted_cwd_regates_as_a_path() {
        let permits = permits_of("permits: { exec: [\"make\"], fs: { read: [\"src/**\"] } }");
        let (taint, records) = tainted("");
        assert!(
            regate_exec_cwd(
                &permits,
                "exec · make",
                "${{ tasks.dl.output }}",
                "src/lib",
                &taint,
                &records,
                &PermitWitness::new()
            )
            .is_none()
        );
        let denial = regate_exec_cwd(
            &permits,
            "exec · make",
            "${{ tasks.dl.output }}",
            "src/../../etc",
            &taint,
            &records,
            &PermitWitness::new(),
        )
        .expect("the escaping cwd is refused");
        assert_eq!(
            denial.result.err().expect("a refusal").record.code,
            "NIKA-SEC-004"
        );
    }

    #[test]
    fn mcp_args_regate_per_leaf_with_union_direction() {
        let permits = permits_of(
            "permits: { tools: [\"mcp:fs/read\"], fs: { read: [\"datasets/**\"], write: [\"out/**\"] }, net: { http: [\"api.example.com\"] } }",
        );
        let (taint, records) = tainted("");
        // A nested leaf escaping fs in BOTH directions → refused; a
        // write-only covered leaf runs (the direction union); a plain
        // literal leaf is never labeled.
        let raw = serde_json::json!({
            "paths": ["literal", "${{ tasks.dl.output }}"],
            "note": "authored",
        });
        let escaping = serde_json::json!({
            "paths": ["literal", "out/../../etc/passwd"],
            "note": "authored",
        });
        let denial = regate_mcp_args(
            &permits,
            "invoke · mcp:fs/read",
            "mcp:fs/read",
            &raw,
            &escaping,
            &taint,
            &records,
            &PermitWitness::new(),
        )
        .expect("the escaping leaf is refused");
        let err = denial.result.err().expect("a refusal");
        assert!(
            err.record
                .message
                .contains("dl -> mcp:fs/read.args.paths[1]"),
            "the sink names the JSON accessor chain: {}",
            err.record.message
        );
        let covered = serde_json::json!({
            "paths": ["literal", "out/report.csv"],
            "note": "authored",
        });
        assert!(
            regate_mcp_args(
                &permits,
                "invoke · mcp:fs/read",
                "mcp:fs/read",
                &raw,
                &covered,
                &taint,
                &records,
                &PermitWitness::new()
            )
            .is_none(),
            "fs.write alone covers an mcp leaf (direction unknowable)"
        );
        // A single island resolving to a NON-STRING leaf passes v1 (the
        // declared structured-payload residual).
        let raw_island = serde_json::json!({ "payload": "${{ tasks.dl.output }}" });
        let structured = serde_json::json!({ "payload": { "url": "https://evil.example/x" } });
        assert!(
            regate_mcp_args(
                &permits,
                "invoke · mcp:fs/read",
                "mcp:fs/read",
                &raw_island,
                &structured,
                &taint,
                &records,
                &PermitWitness::new()
            )
            .is_none(),
            "the structured-payload channel is the declared v1 residual"
        );
    }

    #[test]
    fn plain_data_is_never_a_boundary_concern() {
        let permits = permits_of("permits: { exec: [\"echo\"] }");
        let (taint, records) = tainted("");
        let elements = vec![
            Spanned::new("echo".to_owned(), nika_schema::Span::default()),
            Spanned::new(
                "${{ tasks.dl.output }}".to_owned(),
                nika_schema::Span::default(),
            ),
        ];
        let argv = vec!["echo".to_owned(), "hello world".to_owned()];
        assert!(
            regate_exec_argv(
                &permits,
                "exec · echo",
                &elements,
                &argv,
                &taint,
                &records,
                &PermitWitness::new()
            )
            .is_none(),
            "a tainted plain word carries no path/host/option shape"
        );
    }

    #[test]
    fn url_host_matches_the_shared_parity_vectors() {
        // The re-gate's extractor MUST agree with the check's
        // (`permits_fit::url_host`) and nika-http's `host_of` — the
        // no-drift table both of those pin.
        let cases: &[(&str, Option<&str>)] = &[
            ("https://api.github.com/x", Some("api.github.com")),
            ("https://API.github.com/x", Some("api.github.com")),
            ("https://api.github.com./x", Some("api.github.com")),
            ("https://user@api.github.com/x", Some("api.github.com")),
            ("http://127.0.0.1:8080/x", Some("127.0.0.1")),
            ("https://[::1]/x", Some("::1")),
            ("datasets/report.csv", None),
            ("pwned: opaque", None),
            ("not a url at all", None),
        ];
        for (raw, want) in cases {
            assert_eq!(url_host(raw).as_deref(), *want, "url_host({raw:?})");
        }
    }

    #[test]
    fn path_shape_is_a_separator_or_a_dot_segment() {
        assert!(looks_like_path("datasets/report.csv"));
        assert!(looks_like_path(".."));
        assert!(looks_like_path("."));
        assert!(looks_like_path("a\\b"));
        assert!(!looks_like_path("report.csv"));
        assert!(!looks_like_path("..foo"));
        assert!(!looks_like_path("hello world"));
    }
}
