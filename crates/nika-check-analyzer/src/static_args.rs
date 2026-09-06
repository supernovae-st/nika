// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The arguments a static check may JUDGE: a literal, or a bare
//! `${{ const.<name> }}` resolved through the file's own `const:` table —
//! and the host of a literal or authority-literal URL through the SAME
//! WHATWG parser the runtime dials with.
//!
//! Carved out of `nika-check::permits_fit` at the parent's 15k wall
//! (2026-09-06 · the documentation-host advisory was the line that crossed
//! it): three rungs (PERMITS · the hints · `--infer-permits`) read these
//! through one seam, and the seam reads the AST alone.

use nika_schema::raw::{RawInvokeAction, RawWorkflow};

/// The `const:` string values, resolved once per scan.
///
/// `const:` is the ONE authority a run cannot move. Measured 2026-07-28,
/// `nika run <file> --var p=X` on a file whose `p` is a const answers with
/// the refusal « this workflow declares no inputs », because `--var`
/// satisfies `inputs:` and nothing else. So a bare `${{ const.<name> }}` has
/// a value that is known at check time and CANNOT differ at run time.
///
/// `inputs:` and `config:` are deliberately NOT resolved here even when they
/// carry a default — the run can supply another value, and a boundary verdict
/// computed against a value the run may replace is exactly the kind of claim
/// this checker must not make.
///
/// A pair list rather than a map: a `const:` block holds a handful of entries,
/// so a linear scan costs nothing and spares the crate a dependency (the house
/// lint forbids `std::collections::HashMap` in favour of `rustc_hash`, which
/// `nika-check` does not otherwise pull in).
#[derive(Debug, Default)]
pub struct ConstStrings(Vec<(String, String)>);

impl ConstStrings {
    #[must_use]
    pub fn of(wf: &RawWorkflow) -> Self {
        use nika_schema::types::VarDecl;
        Self(
            wf.consts
                .iter()
                .filter_map(|(k, decl)| {
                    let v = match decl {
                        VarDecl::Untyped(v)
                        | VarDecl::Typed {
                            default: Some(v), ..
                        } => v,
                        VarDecl::Typed { default: None, .. } => return None,
                    };
                    Some((k.value.clone(), v.as_str()?.to_owned()))
                })
                .collect(),
        )
    }

    /// A bare `${{ const.<name> }}` resolved to its declared string.
    ///
    /// BARE only: further navigation (`.field` · `[0]`), operators, or any
    /// surrounding text make the result something other than the const, and
    /// this returns `None` so the value stays the runtime's concern. Same
    /// discipline as `cost::static_vars_array_len`, which resolves this
    /// exact shape to bound a `for_each` count — one rung already treats a
    /// const-backed expression as static, and the asymmetry between them is
    /// what let a boundary escape through.
    #[must_use]
    pub fn resolve(&self, expr: &str) -> Option<&str> {
        let inner = expr.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
        let name = inner.strip_prefix("const.")?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        self.0
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// `args.<key>` as a value this scan can judge — a literal, OR a bare
/// `${{ const.<name> }}` resolved through the const table.
///
/// The security reason this exists (F8, measured 2026-07-28): reading
/// `const:`-backed paths as dynamic made under-declaring both free and
/// profitable. Two files with a byte-identical `tasks:` block, one line of
/// `permits:` apart — the honest one was refused `NIKA-SEC-009`, and the
/// one whose `fs.read` grant was DELETED passed `--native-strict` with
/// `0 hints` and a green audited card, then died at run on its first task
/// with `NIKA-SEC-004`. The gate blocked what worked and passed what could
/// not run, because TRIFECTA reads its private-read leg off `permits:`
/// while the effect stayed in the body. Resolving the const closes it at
/// the source: the under-declared file now fails PERMITS and never reaches
/// TRIFECTA with a short leg.
pub fn judgeable_arg(consts: &ConstStrings, a: &RawInvokeAction, key: &str) -> Option<String> {
    let s = a.args.as_ref()?.value.get(key)?.as_str()?;
    if s.contains("${{") {
        return consts.resolve(s).map(str::to_owned);
    }
    Some(s.to_owned())
}

/// The host of a literal URL (`https://api.x.com/p` → `api.x.com`), via the
/// `url` crate — the SAME WHATWG normalization the runtime http effect
/// connects with (`nika-http`). A hand-rolled string parser disagrees on
/// `\` (a path separator for http/https), userinfo (`a@b`), case, and C0
/// bytes; that disagreement is a boundary bypass, so check + runtime MUST
/// share the parser. Bracket-free for IPv6 (permits write `::1`, matching
/// `nika_types::net`). `None` when there is no parseable host (a relative /
/// garbage value → not a static-permits concern).
#[must_use]
pub fn url_host(raw: &str) -> Option<String> {
    match url::Url::parse(raw).ok()?.host()? {
        // Strip the absolute-FQDN trailing dot (`allowed.com.` ≡ `allowed.com`)
        // — the runtime extractor (`nika-http`) + the SSRF layer do the same,
        // so check and runtime agree.
        url::Host::Domain(d) => Some(d.trim_end_matches('.').to_owned()),
        url::Host::Ipv4(a) => Some(a.to_string()),
        url::Host::Ipv6(a) => Some(a.to_string()),
    }
}

/// The host of a TEMPLATED URL whose scheme + authority prefix is literal
/// (`https://api.x.com/collect?k=${{ with.k }}` → `api.x.com`). The value
/// rides in the path or the query, where it cannot rewrite the authority —
/// but ONLY once a `/` · `?` · `#` closes the authority inside the literal
/// prefix: `https://api.${{ x }}.com` and `https://host${{ p }}` stay
/// derived (`None`): a `${{ }}` value there can inject `@evil.example/` and
/// move the host. Same WHATWG parser as [`url_host`], so check and runtime
/// agree on what the host is (#1393: a sanctioned secret in `?k=…` reached
/// `attacker.example.com` and the journey said « 0 destinations »).
#[must_use]
pub fn templated_url_host(raw: &str) -> Option<String> {
    let (prefix, _) = raw.split_once("${{")?;
    let parsed = url::Url::parse(prefix).ok()?;
    let after_scheme = prefix.strip_prefix(parsed.scheme())?.strip_prefix("://")?;
    if !after_scheme.contains(['/', '?', '#']) {
        return None;
    }
    url_host(prefix)
}
