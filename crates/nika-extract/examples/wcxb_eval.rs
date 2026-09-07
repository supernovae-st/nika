// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Dev-only WCXB harness · scores `mode: article` WITHOUT the engine.
//!
//! Reads a directory of already-decompressed WCXB pages, calls
//! [`nika_extract::extract`] with `ExtractMode::Article` on each, and
//! writes `{file_id: markdown}` to a JSON file. The markdown strip and
//! the scoring stay in the benchmark's own Python (`evaluate.py` is the
//! official scorer and is never touched here).
//!
//! Usage · `wcxb_eval <html_dir> <out.json> <base_url_prefix> [threads]`

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::disallowed_macros)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

use nika_extract::{ExtractMode, ExtractOptions};

type Page = (String, PathBuf);

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .ok_or("usage: wcxb_eval <dir> <out.json> <base_prefix> [threads]")?;
    let out = args.next().ok_or("missing <out.json>")?;
    let prefix = args.next().unwrap_or_default();
    let threads: usize = match args.next() {
        Some(raw) => raw.parse()?,
        None => 8,
    };

    let mut pages: Vec<Page> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "html") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("non-utf8 file name")?
                .to_owned();
            pages.push((stem, path));
        }
    }
    pages.sort();
    let total = pages.len();

    let started = Instant::now();
    let chunk = total.div_ceil(threads.max(1)).max(1);
    let slices: Vec<&[Page]> = pages.chunks(chunk).collect();
    let collected: Vec<Vec<(String, String)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = slices
            .into_iter()
            .map(|slice| scope.spawn(|| run_slice(slice, &prefix)))
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });

    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut empty = 0_usize;
    for part in collected {
        for (id, text) in part {
            if text.is_empty() {
                empty += 1;
            }
            map.insert(id, text);
        }
    }
    if map.len() != total {
        return Err(format!("lost pages: {} of {total} survived", map.len()).into());
    }
    let seconds = started.elapsed().as_secs_f64();
    std::fs::write(&out, serde_json::to_string(&map)?)?;
    println!("pages={total} empty={empty} wall_s={seconds:.2} out={out}");
    Ok(())
}

/// One worker's share · every page yields an entry (an extraction error
/// becomes an EMPTY prediction, exactly as the run-through-the-door
/// scorer counts a null item).
fn run_slice(slice: &[Page], prefix: &str) -> Vec<(String, String)> {
    slice
        .iter()
        .map(|(id, path)| {
            let base = format!("{prefix}{id}.html");
            let text = std::fs::read(path)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default();
            let mut opts = ExtractOptions::new();
            opts.base_url = Some(&base);
            let markdown = match nika_extract::extract(&text, ExtractMode::Article, &opts) {
                Ok(serde_json::Value::String(text)) => text,
                Ok(other) => other.to_string(),
                Err(error) => {
                    eprintln!("ERR {id} {error}");
                    String::new()
                }
            };
            (id.clone(), markdown)
        })
        .collect()
}
