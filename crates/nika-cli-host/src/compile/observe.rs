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
use std::path::{Component, Path, PathBuf};

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
    let observed: Vec<Value> = stated
        .iter()
        .flat_map(|path| {
            let mut rows = Vec::new();
            if let Some(row) = observe(root, path) {
                rows.push(row);
            } else {
                rows.extend(observe_folder(root, path));
            }
            rows
        })
        .collect();
    (!observed.is_empty()).then(|| json!({ "observed": observed }))
}

/// The files of a stated folder, each observed as if stated: `./reports/juillet.csv`.
fn observe_folder(root: &Path, stated: &str) -> Vec<Value> {
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

/// One stated path: under the project root (really), a regular file, a tabular or JSON format.
fn observe(root: &Path, stated: &str) -> Option<Value> {
    let full = match locate(root, stated)? {
        Located::Inside(real) => real,
        Located::Outside => {
            return Some(json!({"path": stated, "state": "outside_project", "complete": false}));
        }
        Located::Unresolved(kind) => {
            return Some(json!({"path": stated, "state":
            if kind == std::io::ErrorKind::NotFound { "absent" } else { "unreadable" }, "complete": false}));
        }
    };
    let meta = match std::fs::metadata(&full) {
        Ok(meta) => meta,
        Err(error) => {
            return Some(json!({"path": stated, "state":
            if error.kind() == std::io::ErrorKind::NotFound { "absent" } else { "unreadable" }, "complete": false}));
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
        return Some(json!({"path": stated, "state": "unreadable", "complete": false}));
    };
    let mut common = None;
    let complete = false;
    let (kind, columns, delimiter, values) = match ext.as_str() {
        "csv" | "tsv" => {
            let (columns, delimiter) = header_columns(&head, ext == "tsv");
            let values = csv_values(&head, delimiter, &columns);
            // A header describes names, not the shape of all data rows.
            ("csv", columns, Some(delimiter), values)
        }
        "json" => {
            let rows = json_rows(&full, &head, meta.len());
            common = Some(common_keys(&rows));
            // Conservatively partial: json_rows can fall back to a bounded head.
            ("json", keys_of_rows(&rows), None, categorical(&rows))
        }
        "jsonl" | "ndjson" => {
            let rows = jsonl_rows(&head);
            common = Some(common_keys(&rows));
            ("jsonl", keys_of_rows(&rows), None, categorical(&rows))
        }
        _ => return None,
    };
    if columns.is_empty() {
        let empty = head.trim().is_empty() || matches!(head.trim(), "[]" | "{}");
        return Some(json!({"path": stated, "kind": kind,
            "state": if empty { "empty" } else { "unknown" }, "complete": false}));
    }
    let mut row = json!({
        "path": stated,
        "state": "observed",
        "complete": complete,
        "kind": kind,
        "columns": columns,
        "bytes": meta.len(),
        "peek_sha256": sha256_hex(head.as_bytes()),
    });
    if let Some(common) = common {
        row["common_columns"] = json!(common);
    }
    if let Some(delimiter) = delimiter {
        row["delimiter"] = Value::String(delimiter.to_string());
    }
    if !values.is_empty() {
        row["values"] = Value::Object(values.into_iter().collect());
    }
    Some(row)
}

/// The most rows a value set is read from, and the most distinct values a column may hold
/// to count as categorical (a status · a kind · a currency — never free text).
const SAMPLE_ROWS: usize = 200;
const CATEGORICAL_MAX: usize = 8;
const VALUE_MAX_CHARS: usize = 32;

/// The distinct values of every categorical column of a CSV head (naive cut: a row whose
/// field count differs from the header's is skipped).
fn csv_values(head: &str, delimiter: char, columns: &[String]) -> Vec<(String, Value)> {
    let mut sets: Vec<Vec<String>> = vec![Vec::new(); columns.len()];
    let mut rows = 0_usize;
    for line in head.lines().skip(1).take(SAMPLE_ROWS) {
        let fields: Vec<&str> = line.split(delimiter).collect();
        if fields.len() != columns.len() {
            continue;
        }
        rows += 1;
        for (set, field) in sets.iter_mut().zip(fields) {
            let value = field.trim().trim_matches('"').trim();
            if !value.is_empty() && !set.iter().any(|v| v == value) {
                set.push(value.to_owned());
            }
        }
    }
    if rows < 2 {
        return Vec::new();
    }
    columns
        .iter()
        .zip(sets)
        .filter(|(_, set)| categorical_set(set, rows))
        .map(|(column, set)| {
            (
                column.clone(),
                Value::Array(set.into_iter().map(Value::String).collect()),
            )
        })
        .collect()
}

/// A value set is categorical when it is small, shorter than the rows it came from, and every
/// value is short.
fn categorical_set(set: &[String], rows: usize) -> bool {
    !set.is_empty()
        && set.len() <= CATEGORICAL_MAX
        && set.len() < rows
        && set.iter().all(|v| v.chars().count() <= VALUE_MAX_CHARS)
}

/// The distinct string values of every categorical key across the sampled objects.
fn categorical(rows: &[Value]) -> Vec<(String, Value)> {
    if rows.len() < 2 {
        return Vec::new();
    }
    keys_of_rows(rows)
        .into_iter()
        .filter_map(|key| {
            let mut set: Vec<String> = Vec::new();
            let mut present = 0_usize;
            for row in rows.iter().take(SAMPLE_ROWS) {
                let Some(value) = row.get(&key).and_then(Value::as_str) else {
                    continue;
                };
                present += 1;
                if !value.is_empty() && !set.iter().any(|v| v == value) {
                    set.push(value.to_owned());
                }
            }
            (present >= 2 && categorical_set(&set, present)).then(|| {
                (
                    key,
                    Value::Array(set.into_iter().map(Value::String).collect()),
                )
            })
        })
        .collect()
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

/// The header line's columns and the delimiter that cut it (the most frequent of `,` `;`
/// `\t` `|` on the first line; a TSV is tab-cut).
fn header_columns(head: &str, tsv: bool) -> (Vec<String>, char) {
    let first = head
        .lines()
        .next()
        .unwrap_or("")
        .trim_start_matches('\u{feff}');
    let delimiter = if tsv {
        '\t'
    } else {
        [',', ';', '\t', '|']
            .into_iter()
            .max_by_key(|d| first.matches(*d).count())
            .filter(|d| first.contains(*d))
            .unwrap_or(',')
    };
    let columns = first
        .split(delimiter)
        .map(|c| {
            c.trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim()
                .to_owned()
        })
        .filter(|c| !c.is_empty())
        .collect();
    (columns, delimiter)
}

/// A JSON file's rows: a top-level array, or the one top-level object. A file past
/// the whole-read bound is judged on its head when that head parses.
fn json_rows(path: &Path, head: &str, len: u64) -> Vec<Value> {
    let text = if len <= WHOLE_JSON_BYTES {
        std::fs::read_to_string(path).unwrap_or_else(|_| head.to_owned())
    } else {
        head.to_owned()
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Array(items)) => items,
        Ok(object @ Value::Object(_)) => vec![object],
        _ => Vec::new(),
    }
}

/// The parsed lines of a JSONL head (a cut last line is skipped).
fn jsonl_rows(head: &str) -> Vec<Value> {
    head.lines()
        .filter(|l| !l.trim().is_empty())
        .take(SAMPLE_ROWS)
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect()
}

/// Every key positively observed; neither this union nor the sample is a schema.
fn keys_of_rows(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .filter_map(Value::as_object)
        .flat_map(|o| o.keys().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn common_keys(rows: &[Value]) -> Vec<String> {
    keys_of_rows(rows)
        .into_iter()
        .filter(|key| rows.iter().all(|row| row.get(key).is_some()))
        .collect()
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
}

#[cfg(test)]
mod observation_coverage_tests {
    use super::*;
    #[test]
    fn mixed_records_are_positive_keys_with_a_separate_common_set() {
        let rows = vec![json!({"id":1}), json!({"id":2, "status":"open"})];
        assert_eq!(keys_of_rows(&rows), ["id", "status"]);
        assert_eq!(common_keys(&rows), ["id"]);
        assert!(common_keys(&[json!({"id":1}), json!(null)]).is_empty());
        assert!(keys_of_rows(&[]).is_empty());
    }
    #[test]
    fn an_empty_or_partial_sample_does_not_claim_a_complete_schema() {
        // Parsing a partial JSON document yields no records, never an empty schema.
        assert!(jsonl_rows("{\"id\":1}\n{\"status\":").len() == 1);
        assert!(common_keys(&jsonl_rows("\n")).is_empty());
    }
}
