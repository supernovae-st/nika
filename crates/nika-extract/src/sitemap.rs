// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `mode: sitemap` — sitemap.xml `<urlset>` AND `<sitemapindex>` →
//! array of `{loc, lastmod?}` via a streaming `quick-xml` pass
//! (namespace-agnostic local names · no DOM build).

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::ExtractError;

pub(crate) fn sitemap(body: &str) -> Result<serde_json::Value, ExtractError> {
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);

    let mut entries: Vec<serde_json::Value> = Vec::new();
    let mut saw_root = false;
    let mut in_entry = false;
    let mut field: Option<&'static str> = None;
    let mut loc: Option<String> = None;
    let mut lastmod: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(el)) => match el.local_name().as_ref() {
                b"urlset" | b"sitemapindex" => saw_root = true,
                b"url" | b"sitemap" if saw_root => {
                    in_entry = true;
                    loc = None;
                    lastmod = None;
                }
                // MUTATION (equivalent · the `in_entry` guard here is
                // defensive depth, not output-load-bearing): a stray
                // `<loc>`/`<lastmod>` outside an entry sets `field`, but
                // the captured value is only ever FLUSHED by the
                // `</url>`/`</sitemap>` End arm — which IS `in_entry`-
                // gated and tested. So dropping these two guards changes
                // no output; they stay for malformed-XML robustness.
                b"loc" if in_entry => field = Some("loc"),
                b"lastmod" if in_entry => field = Some("lastmod"),
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if let Some(name) = field {
                    // decode = bytes→str per encoding; unescape = XML
                    // entities (`&amp;` is common in sitemap URLs).
                    let decoded = t.decode().map_err(|e| ExtractError::Sitemap {
                        reason: format!("text decode: {e}"),
                    })?;
                    let value = quick_xml::escape::unescape(&decoded)
                        .map_err(|e| ExtractError::Sitemap {
                            reason: format!("entity unescape: {e}"),
                        })?
                        .trim()
                        .to_owned();
                    match name {
                        "loc" => loc = Some(value),
                        _ => lastmod = Some(value),
                    }
                }
            }
            Ok(Event::End(el)) => match el.local_name().as_ref() {
                b"url" | b"sitemap" if in_entry => {
                    if let Some(loc_value) = loc.take() {
                        let mut entry = serde_json::Map::new();
                        entry.insert("loc".to_owned(), loc_value.into());
                        if let Some(lastmod_value) = lastmod.take() {
                            entry.insert("lastmod".to_owned(), lastmod_value.into());
                        }
                        entries.push(serde_json::Value::Object(entry));
                    }
                    in_entry = false;
                }
                b"loc" | b"lastmod" => field = None,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(ExtractError::Sitemap {
                    reason: e.to_string(),
                });
            }
        }
    }

    if !saw_root {
        return Err(ExtractError::Sitemap {
            reason: "no <urlset> or <sitemapindex> root element".to_owned(),
        });
    }
    Ok(serde_json::Value::Array(entries))
}
