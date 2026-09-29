// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the host observes about the files a request names, for the authoring seat: a CSV's
//! header columns, a JSON file's top-level keys, a JSONL line's keys — read once, bounded,
//! under the project root only, never a row's value. The observation rides the request
//! as knowledge (`observed_world` in the seat's opening message) so a candidate names the
//! columns the file spells, asks for none of them and invents none. The live run of
//! nika-076a2a91 (2026-09-22) measured the seat asking `const.status_column` ·
//! `const.paid_value` · `const.amount_column` for a CSV the request named but never described.
//!
//! Containment is judged on the REAL location: a stated path is resolved (every symlink
//! followed) and observed only when it lands under the project root's own real location. A
//! linked file or folder that leads outside the project is reported `outside_project` and never
//! read. What is observed is data (names, keys, short categorical values), never an instruction.
//! `nika compile` observes under its working directory; the Session door under its project root.
//! The pure half (columns, categorical values, the raw kinds of the sampled values) is the compile
//! unit's law, `nika_onboard::compile::observation` (R4 A5); this adapter keeps the I/O.
use std::path::{Component, Path, PathBuf};

use nika_onboard::compile::observation;
use serde_json::{Value, json};

/// The most bytes peeked from one file.
const PEEK_BYTES: usize = 64 * 1024;
/// A file larger than this is described by its head only (never read whole).
const WHOLE_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// The most files observed inside one stated folder.
const FOLDER_FILES: usize = 8;

/// Positive observations and explicit unavailable states for stated paths under `root` (the
/// project root: `nika compile`'s working directory, a Session's project). A stated folder
/// (« les trois fichiers de ventes dans ./reports/ ») contributes its tabular and JSON files,
/// sorted by name, the first eight. Sources and destinations alike: the reader hears « dans
/// ./reports/ » as a destination connector, and a destination that already exists has a shape
/// worth stating too. Bounded: 64 KiB peeked per file, eight files per folder, headers, keys
/// and short categorical values only.
#[must_use]
pub fn world(root: &Path, intent: &str) -> Option<Value> {
    let mut stated = nika_onboard::compile::stated_sources(intent);
    for path in nika_onboard::compile::stated_destinations(intent) {
        if !stated.contains(&path) {
            stated.push(path);
        }
    }
    let seen: Vec<(Value, Option<Value>)> = stated
        .iter()
        .flat_map(|path| match observe(root, path) {
            Some(seen) => vec![seen],
            None => observe_folder(root, path),
        })
        .collect();
    if seen.is_empty() {
        return None;
    }
    // The kinds ride beside the rows, keyed by path, never inside one: a recorded row stays the
    // identity a plan or a verified transform was bound to (R4 A5).
    let kinds: serde_json::Map<String, Value> = seen
        .iter()
        .filter_map(|(row, kinds)| Some((row["path"].as_str()?.to_owned(), kinds.clone()?)))
        .collect();
    let mut world = json!({ "observed": seen.into_iter().map(|(row, _)| row).collect::<Vec<_>>() });
    if !kinds.is_empty() {
        world["kinds"] = Value::Object(kinds);
    }
    Some(world)
}

/// The files of a stated folder, each observed as if stated: `./reports/juillet.csv`.
fn observe_folder(root: &Path, stated: &str) -> Vec<(Value, Option<Value>)> {
    let Some(Located::Inside(full)) = locate(root, stated) else {
        return Vec::new();
    };
    if !full.is_dir() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&full) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|name| {
            Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| {
                    matches!(
                        e.to_ascii_lowercase().as_str(),
                        "csv" | "tsv" | "json" | "jsonl"
                    )
                })
        })
        .collect();
    names.sort();
    let folder = stated.trim_end_matches('/');
    let folder = folder.strip_prefix("./").unwrap_or(folder);
    names
        .iter()
        .take(FOLDER_FILES)
        .filter_map(|name| observe(root, &format!("./{folder}/{name}")))
        .collect()
}

/// The stated path joined under the root, or None for an absolute path or one that climbs out
/// by its words (`..`).
fn under_root(root: &Path, stated: &str) -> Option<PathBuf> {
    let relative = Path::new(stated.strip_prefix("./").unwrap_or(stated));
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return None;
    }
    Some(root.join(relative))
}

/// Where a stated path really lands.
enum Located {
    /// Its real location (every symlink resolved) lies under the root's real location.
    Inside(PathBuf),
    /// A symlink on the way leads outside the project: never read.
    Outside,
    /// It cannot be resolved (absent, or unreadable), with the reason's kind.
    Unresolved(std::io::ErrorKind),
}

/// Resolve a stated path the way the filesystem will: the containment check is made on the
/// real location, so a link inside the project that points outside it is refused.
fn locate(root: &Path, stated: &str) -> Option<Located> {
    let joined = under_root(root, stated)?;
    let real_root = root.canonicalize().ok()?;
    Some(match joined.canonicalize() {
        Ok(real) if real.starts_with(&real_root) => Located::Inside(real),
        Ok(_) => Located::Outside,
        Err(error) => Located::Unresolved(error.kind()),
    })
}

/// One stated path: under the project root (really), a regular file, a tabular or JSON format;
/// its row, and the raw kinds of its sampled values when it was read.
fn observe(root: &Path, stated: &str) -> Option<(Value, Option<Value>)> {
    let full = match locate(root, stated)? {
        Located::Inside(real) => real,
        Located::Outside => {
            return Some((
                json!({"path": stated, "state": "outside_project", "complete": false}),
                None,
            ));
        }
        Located::Unresolved(kind) => {
            return Some((
                json!({"path": stated, "state":
            if kind == std::io::ErrorKind::NotFound { "absent" } else { "unreadable" }, "complete": false}),
                None,
            ));
        }
    };
    let meta = match std::fs::metadata(&full) {
        Ok(meta) => meta,
        Err(error) => {
            return Some((
                json!({"path": stated, "state":
            if error.kind() == std::io::ErrorKind::NotFound { "absent" } else { "unreadable" }, "complete": false}),
                None,
            ));
        }
    };
    if !meta.is_file() {
        return None;
    }
    let ext = full
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)?;
    let Some(head) = peek(&full) else {
        return Some((
            json!({"path": stated, "state": "unreadable", "complete": false}),
            None,
        ));
    };
    let (kind, sample, complete) = match ext.as_str() {
        // A header describes names, not the shape of all data rows.
        "csv" | "tsv" => ("csv", observation::csv(&head, ext == "tsv"), false),
        "json" => {
            let (rows, complete) = json_rows(&full, &head, meta.len());
            ("json", observation::records(&rows), complete)
        }
        "jsonl" | "ndjson" => (
            "jsonl",
            observation::records(&observation::jsonl(&head)),
            false,
        ),
        _ => return None,
    };
    if sample.columns.is_empty() && !complete {
        let empty = head.trim().is_empty() || matches!(head.trim(), "[]" | "{}");
        return Some((
            json!({"path": stated, "kind": kind,
            "state": if empty { "empty" } else { "unknown" }, "complete": false}),
            None,
        ));
    }
    let mut row = json!({
        "path": stated,
        "state": "observed",
        "complete": complete,
        "kind": kind,
        "columns": sample.columns,
        "bytes": meta.len(),
        "peek_sha256": sha256_hex(head.as_bytes()),
    });
    if let Some(common) = sample.common {
        row["common_columns"] = json!(common);
    }
    if let Some(delimiter) = sample.delimiter {
        row["delimiter"] = Value::String(delimiter.to_string());
    }
    if !sample.values.is_empty() {
        row["values"] = Value::Object(sample.values.into_iter().collect());
    }
    Some((row, Some(sample.kinds)))
}

/// The first bytes of the file, as text (invalid UTF-8 cut at the last valid boundary).
fn peek(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0_u8; PEEK_BYTES];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    Some(match String::from_utf8(buf) {
        Ok(text) => text,
        Err(e) => {
            let valid = e.utf8_error().valid_up_to();
            let mut bytes = e.into_bytes();
            bytes.truncate(valid);
            String::from_utf8(bytes).unwrap_or_default()
        }
    })
}

/// JSON keys are complete only after a successful whole read and parse. A bounded head or a
/// failed whole read remains partial, even when the head itself is valid JSON.
fn json_rows(path: &Path, head: &str, len: u64) -> (Vec<Value>, bool) {
    let whole = (len <= WHOLE_JSON_BYTES)
        .then(|| std::fs::read_to_string(path).ok())
        .flatten()
        .filter(|text| text.len() as u64 <= WHOLE_JSON_BYTES);
    let complete = whole.is_some();
    match serde_json::from_str::<Value>(whole.as_deref().unwrap_or(head)) {
        Ok(Value::Array(items)) if !items.is_empty() => (items, complete),
        Ok(object @ Value::Object(_)) => (vec![object], complete),
        _ => (Vec::new(), false),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A whole JSON read records complete key evidence; kinds remain bounded counts beside the
    /// row. This supersedes the historical A4 partial-row pin without changing its other facts.
    #[test]
    fn a_whole_json_observation_records_complete_keys_and_separate_kinds() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let people = r#"[{"id": 1, "address": "a@x.org", "status": "active"}, {"id": 2, "address": "b@x.org", "status": "active"}, {"id": 3, "address": "c@y.org", "status": "inactive"}]"#;
        std::fs::write(dir.path().join("data/people.json"), format!("{people}\n")).unwrap();
        let seen = super::world(dir.path(), "read ./data/people.json").expect("observed");
        assert_eq!(
            seen["observed"][0],
            json!({"bytes": 162, "columns": ["address", "id", "status"],
                "common_columns": ["address", "id", "status"], "complete": true, "kind": "json",
                "path": "./data/people.json",
                "peek_sha256": "4cc35dc1fbdb9665a603d9d6e94efdce7ba1019aae984c61ef91b171a76a6be9",
                "state": "observed", "values": {"status": ["active", "inactive"]}})
        );
        let kinds = &seen["kinds"]["./data/people.json"];
        assert_eq!(kinds["sampled"], 3);
        assert_eq!(kinds["keys"]["id"], json!({"number": 3}));
        assert_eq!(kinds["keys"]["address"], json!({"text": 3}));
        assert!(!kinds.to_string().contains("x.org"), "counts only: {kinds}");
        let absent = super::world(dir.path(), "read ./data/missing.json").unwrap();
        assert!(absent.get("kinds").is_none(), "{absent}");
    }

    #[test]
    fn a_stated_csv_under_the_cwd_is_observed_by_its_header_and_never_by_a_row() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(
            dir.path().join("data/paiements.csv"),
            "id;client;montant;statut\n1;Acme;1480.5;payé\n2;Bolt;135.25;impayé\n",
        )
        .unwrap();
        let seen = super::world(
            dir.path(),
            "prends ce fichier ./data/paiements.csv, garde uniquement les paiements payés",
        )
        .expect("observed");
        let row = &seen["observed"][0];
        assert_eq!(row["path"], "./data/paiements.csv");
        assert_eq!(row["kind"], "csv");
        assert_eq!(row["delimiter"], ";");
        assert_eq!(row["columns"], json!(["id", "client", "montant", "statut"]));
        // Two rows, two distinct values each: nothing is categorical yet; no row value leaks.
        assert!(row.get("values").is_none(), "{row}");
        let text = seen.to_string();
        assert!(!text.contains("Acme") && !text.contains("1480.5"), "{text}");
        std::fs::write(
            dir.path().join("data/paiements.csv"),
            "id;client;montant;statut\n1;Acme;1480.5;payé\n2;Bolt;135.25;impayé\n3;Cora;12;payé\n4;Dune;7;payé\n",
        )
        .unwrap();
        let seen = super::world(dir.path(), "prends ./data/paiements.csv").expect("observed");
        let row = &seen["observed"][0];
        assert_eq!(row["values"]["statut"], json!(["payé", "impayé"]));
        assert!(
            row["values"].get("client").is_none(),
            "four names in four rows are free text"
        );
        assert!(
            row["values"].get("montant").is_none(),
            "four amounts in four rows are free values"
        );
    }

    #[test]
    fn a_stated_folder_contributes_its_tabular_files_sorted_and_bounded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("reports")).unwrap();
        for name in ["septembre.csv", "juillet.csv", "aout.csv"] {
            std::fs::write(
                dir.path().join("reports").join(name),
                "region,ventes\nNord,1200\nSud,800\n",
            )
            .unwrap();
        }
        std::fs::write(dir.path().join("reports/notes.md"), "not a table").unwrap();
        let seen = super::world(
            dir.path(),
            "prends les trois fichiers de ventes mensuelles dans ./reports/ (juillet, août, septembre), additionne les ventes par région",
        )
        .expect("observed");
        let rows = seen["observed"].as_array().unwrap();
        let paths: Vec<&str> = rows.iter().filter_map(|r| r["path"].as_str()).collect();
        assert_eq!(
            paths,
            [
                "./reports/aout.csv",
                "./reports/juillet.csv",
                "./reports/septembre.csv"
            ],
            "{seen}"
        );
        assert!(
            rows.iter()
                .all(|r| r["columns"] == json!(["region", "ventes"])),
            "{seen}"
        );
        for n in 0..12 {
            std::fs::write(
                dir.path().join(format!("reports/x{n:02}.csv")),
                "a,b\n1,2\n",
            )
            .unwrap();
        }
        let seen = super::world(dir.path(), "lis ./reports/").expect("observed");
        assert_eq!(seen["observed"].as_array().unwrap().len(), FOLDER_FILES);
        assert!(super::world(dir.path(), "lis ../reports/").is_none());
    }

    #[test]
    fn json_and_jsonl_sources_are_observed_by_their_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("tickets.json"),
            r#"[{"id": 1, "topic": "login", "status": "open"}, {"id": 2}]"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("events.jsonl"),
            "{\"t\": 1, \"kind\": \"a\"}\n{\"t\": 2}\n",
        )
        .unwrap();
        let seen =
            super::world(dir.path(), "read ./tickets.json and ./events.jsonl").expect("observed");
        let rows = seen["observed"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        // serde_json keeps keys sorted (no preserve_order in the engine): the shape, not the spelling order.
        assert_eq!(rows[0]["columns"], json!(["id", "status", "topic"]));
        assert_eq!(rows[1]["kind"], "jsonl");
        assert_eq!(rows[1]["columns"], json!(["kind", "t"]));
        std::fs::write(
            dir.path().join("tickets.json"),
            r#"[{"id": 1, "topic": "login", "status": "open"}, {"id": 2, "topic": "export", "status": "open"}, {"id": 3, "topic": "vat", "status": "closed"}]"#,
        )
        .unwrap();
        let seen = super::world(dir.path(), "read ./tickets.json").expect("observed");
        assert_eq!(
            seen["observed"][0]["values"]["status"],
            json!(["open", "closed"])
        );
        assert!(seen["observed"][0]["values"].get("topic").is_none());
    }

    /// A link inside the project that leads outside it — a file or a folder — is never read:
    /// its real location is judged, not its words. A link that stays inside is observed.
    #[cfg(unix)]
    #[test]
    fn a_link_leading_outside_the_project_is_refused_and_one_inside_is_observed() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(
            outside.path().join("secret.csv"),
            "secret_col,private_amount\nx,1\n",
        )
        .unwrap();
        std::fs::create_dir_all(outside.path().join("vault")).unwrap();
        std::fs::write(outside.path().join("vault/ledger.csv"), "vault_col\n1\n").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.csv"),
            project.path().join("ventes.csv"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path().join("vault"), project.path().join("reports"))
            .unwrap();
        std::fs::write(project.path().join("real.csv"), "id,statut\n1,paye\n").unwrap();
        std::os::unix::fs::symlink(
            project.path().join("real.csv"),
            project.path().join("alias.csv"),
        )
        .unwrap();
        let seen = super::world(
            project.path(),
            "lis ./ventes.csv et ./alias.csv, puis les fichiers de ./reports/",
        )
        .expect("observed");
        let text = seen.to_string();
        for leaked in ["secret_col", "private_amount", "vault_col", "ledger.csv"] {
            assert!(!text.contains(leaked), "{leaked} leaked: {text}");
        }
        let rows = seen["observed"].as_array().unwrap();
        let state = |path: &str| {
            rows.iter()
                .find(|r| r["path"] == path)
                .map(|r| r["state"].clone())
        };
        assert_eq!(state("./ventes.csv"), Some(json!("outside_project")));
        assert_eq!(state("./reports/"), Some(json!("outside_project")));
        assert_eq!(state("./alias.csv"), Some(json!("observed")));
        let alias = rows.iter().find(|r| r["path"] == "./alias.csv").unwrap();
        assert_eq!(alias["columns"], json!(["id", "statut"]));
    }

    #[test]
    fn an_absent_file_a_parent_escape_or_a_prose_format_observes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.md"), "# hello\n").unwrap();
        assert_eq!(
            super::world(dir.path(), "read ./missing.csv").unwrap()["observed"][0]["state"],
            "absent"
        );
        assert!(super::world(dir.path(), "read ../etc/passwd.csv").is_none());
        assert!(super::world(dir.path(), "read ./notes.md").is_none());
    }

    #[test]
    fn whole_json_keys_can_disprove_an_assertion_without_refusing_present_keys() {
        use nika_onboard::compile::{CompileRequest, CompileStatus, compile};
        let dir = tempfile::tempdir().unwrap();
        let intent = "Read ./orders.json (columns id, status), keep only the rows whose status is open and write them to ./out.json";
        for (data, ready) in [
            (json!([{"id": 1, "state": "open"}]), false),
            (json!([{}]), false),
            (json!({}), false),
            (json!([{"id": 1, "status": "open"}]), true),
        ] {
            std::fs::write(dir.path().join("orders.json"), data.to_string()).unwrap();
            let mut request = CompileRequest::create(intent);
            request.knowledge = world(dir.path(), intent);
            let out = compile(&request).unwrap();
            assert_eq!(out.status == CompileStatus::Ready, ready, "{data}: {out:?}");
        }
    }

    #[test]
    fn a_whole_json_contradiction_moves_the_previously_asserted_basis() {
        use nika_onboard::compile::{Basis, CompileRequest, CompileStatus, basis, compile};
        let dir = tempfile::tempdir().unwrap();
        let intent = "Read ./orders.json (columns id, status), keep only the rows whose status is open and write them to ./out.json";
        let mut request = CompileRequest::create(intent);
        request.knowledge = world(dir.path(), intent);
        let out = compile(&request).unwrap();
        assert_eq!(out.status, CompileStatus::Ready);
        for (data, holds) in [
            (json!([{"id": 1, "state": "open"}]), false),
            (json!([{}]), false),
            (json!([{"id": 1, "status": "open"}]), true),
        ] {
            std::fs::write(dir.path().join("orders.json"), data.to_string()).unwrap();
            let fresh = world(dir.path(), intent);
            let verdict = basis(out.provenance.decision.as_ref(), fresh.as_ref(), intent);
            if holds {
                assert!(matches!(verdict, Basis::Holds(1)), "{verdict:?}");
            } else {
                assert!(matches!(verdict, Basis::Moved(_)), "{verdict:?}");
            }
        }
    }

    #[test]
    fn valid_json_from_a_bounded_or_fallback_head_is_not_complete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.json");
        let head = r#"[{"id":1}]"#;
        for len in [head.len() as u64, WHOLE_JSON_BYTES + 1] {
            let (rows, complete) = json_rows(&path, head, len);
            assert_eq!(rows, vec![json!({"id": 1})]);
            assert!(!complete);
        }
    }

    #[test]
    fn malformed_whole_json_and_bounded_jsonl_cannot_claim_complete_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("broken.json"), "[{\"id\":1}").unwrap();
        std::fs::write(dir.path().join("records.jsonl"), "{\"id\":1}\n").unwrap();
        for source in ["./broken.json", "./records.jsonl"] {
            let seen = world(dir.path(), &format!("read {source}")).unwrap();
            assert_eq!(seen["observed"][0]["complete"], false);
        }
    }

    #[test]
    fn complete_keys_cover_records_beyond_the_bounded_kind_sample() {
        let dir = tempfile::tempdir().unwrap();
        let mut rows = vec![json!({"id": 1}); 200];
        rows.push(json!({"id": 201, "tail_key": true}));
        std::fs::write(dir.path().join("rows.json"), json!(rows).to_string()).unwrap();
        let seen = world(dir.path(), "read ./rows.json").unwrap();
        assert_eq!(seen["observed"][0]["complete"], true);
        assert_eq!(seen["observed"][0]["columns"], json!(["id", "tail_key"]));
        assert_eq!(seen["observed"][0]["common_columns"], json!(["id"]));
        assert_eq!(seen["kinds"]["./rows.json"]["sampled"], 200);
    }
}
