// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `mode: sitemap` — sitemap.xml `<urlset>` AND `<sitemapindex>` →
//! array of `{loc, lastmod?, changefreq?, priority?}` via a streaming
//! `quick-xml` pass (namespace-agnostic local names · no DOM build).
//!
//! Field text ACCUMULATES across events: quick-xml 0.40 emits
//! `&amp;`-class entities as separate [`Event::GeneralRef`] events
//! BETWEEN `Text` events (and CDATA as `Event::CData`) — an
//! assign-per-event handler would keep only the last fragment, mangling
//! every sitemap URL with a query string (review lens 2 · P1 · the
//! sitemaps.org spec MANDATES entity-escaping).

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::ExtractError;

/// The per-entry fields captured (the spec's `<url>`/`<sitemap>` children).
const FIELDS: [(&[u8], &str); 4] = [
    (b"loc", "loc"),
    (b"lastmod", "lastmod"),
    (b"changefreq", "changefreq"),
    (b"priority", "priority"),
];

fn field_name(local: &[u8]) -> Option<&'static str> {
    FIELDS
        .iter()
        .find(|(tag, _)| *tag == local)
        .map(|(_, name)| *name)
}

/// One sitemap error with a what-failed prefix.
fn sitemap_err(what: &str, e: &dyn std::fmt::Display) -> ExtractError {
    ExtractError::Sitemap {
        reason: format!("{what}: {e}"),
    }
}

/// Append one resolved general reference to the field buffer: numeric
/// char-refs (`&#47;` · `&#x2F;`) resolve to their char, the five
/// predefined entities to their text, unknown custom entities are
/// preserved VERBATIM (sitemaps carry no DTD — faithful over guessy).
fn append_general_ref(
    buf: &mut String,
    r: &quick_xml::events::BytesRef<'_>,
) -> Result<(), ExtractError> {
    if let Some(ch) = r
        .resolve_char_ref()
        .map_err(|e| sitemap_err("char reference", &e))?
    {
        buf.push(ch);
        return Ok(());
    }
    let name = r.decode().map_err(|e| sitemap_err("entity decode", &e))?;
    if let Some(resolved) = quick_xml::escape::resolve_predefined_entity(&name) {
        buf.push_str(resolved); // amp/lt/gt/quot/apos
    } else {
        buf.push('&');
        buf.push_str(&name);
        buf.push(';');
    }
    Ok(())
}

/// Streaming parser state for the sitemap event loop.
#[derive(Default)]
struct SitemapParser {
    entries: Vec<serde_json::Value>,
    saw_root: bool,
    in_entry: bool,
    field: Option<&'static str>,
    buf: String,
    current: serde_json::Map<String, serde_json::Value>,
}

impl SitemapParser {
    fn on_start(&mut self, el: &quick_xml::events::BytesStart<'_>) {
        match el.local_name().as_ref() {
            b"urlset" | b"sitemapindex" => self.saw_root = true,
            b"url" | b"sitemap" if self.saw_root => {
                self.in_entry = true;
                self.current.clear();
            }
            // MUTATION (equivalent · the `in_entry` guard here is
            // defensive depth, not output-load-bearing): a stray
            // field outside an entry fills `buf`, but the value is
            // only ever FLUSHED into an entry by the
            // `</url>`/`</sitemap>` End arm — which IS `in_entry`-
            // gated and tested. Dropping these guards changes no
            // output; they stay for malformed-XML robustness.
            local if self.in_entry && field_name(local).is_some() => {
                self.field = field_name(local);
                self.buf.clear();
            }
            _ => {}
        }
    }

    fn on_end(&mut self, el: &quick_xml::events::BytesEnd<'_>) {
        let local = el.local_name();
        match local.as_ref() {
            b"url" | b"sitemap" if self.in_entry => {
                // An entry without a <loc> is dropped (the URL IS
                // the entry); extras ride along when present.
                if self.current.contains_key("loc") {
                    self.entries
                        .push(serde_json::Value::Object(std::mem::take(&mut self.current)));
                }
                // Malformed `<url><loc>x</url>` hygiene: a field
                // left open dies with its entry.
                self.in_entry = false;
                self.field = None;
                self.buf.clear();
            }
            local_name => {
                // Flush ONLY the matching close — a stray nested
                // element inside a field must not steal the flush
                // (`<loc>a<b/>c</loc>` keeps accumulating).
                if self.field.is_some() && field_name(local_name) == self.field {
                    if let Some(name) = self.field.take() {
                        self.current.insert(name.to_owned(), self.buf.trim().into());
                    }
                    self.buf.clear();
                }
            }
        }
    }
}

pub(crate) fn sitemap(body: &str) -> Result<serde_json::Value, ExtractError> {
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);
    let mut p = SitemapParser::default();

    loop {
        match reader.read_event() {
            Ok(Event::Start(el)) => p.on_start(&el),
            // The three content-event kinds a field's text arrives as —
            // ALL append (never assign).
            Ok(Event::Text(t)) => {
                if p.field.is_some() {
                    p.buf
                        .push_str(&t.decode().map_err(|e| sitemap_err("text decode", &e))?);
                }
            }
            Ok(Event::CData(c)) => {
                if p.field.is_some() {
                    let bytes = c.into_inner();
                    let text =
                        std::str::from_utf8(&bytes).map_err(|e| sitemap_err("CDATA decode", &e))?;
                    p.buf.push_str(text);
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if p.field.is_some() {
                    append_general_ref(&mut p.buf, &r)?;
                }
            }
            Ok(Event::End(el)) => p.on_end(&el),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(ExtractError::Sitemap {
                    reason: e.to_string(),
                });
            }
        }
    }

    if !p.saw_root {
        return Err(ExtractError::Sitemap {
            reason: "no <urlset> or <sitemapindex> root element".to_owned(),
        });
    }
    Ok(serde_json::Value::Array(p.entries))
}
