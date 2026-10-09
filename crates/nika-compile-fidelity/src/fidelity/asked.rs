// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Law 1 and the typed answer of an open name ([`paths::open_names`]: the reader leaves the
//! first word of `un payload out/notification.json` open). A question asks it when its label or
//! why states it as one whole literal (the reader's own spans: `« payload out/notification.json »`,
//! never a longer `….json.backup`) and no other open name, its constant is declared blank and a
//! task reads that constant whole as its `path`. Law 1 then leaves the name to that answer, and the
//! question takes exactly the name's readings, the only answers the compiler bakes. A name no
//! question or two questions name, or a key asked twice, binds nothing: the name stays owed
//! exactly, as an exact path does.

use nika_compile_reader::paths::{self, PathShape};
use serde_json::{Value, json};

/// The remedy an unrealized open name adds to Law 1's finding.
pub(super) const REMEDY: &str = " Its unquoted words leave where the file's name starts open: write this whole text only when it is the file's name. When the words name a shorter file, the human settles it: declare `<slug>: \"\"` under `const:`, read or write `${{ const.<slug> }}` whole as that task's path, keep one `\"\"` entry on that side of `permits.fs`, and ask `const.<slug>` in `questions` with this exact name quoted in its label; the compiler offers the name's readings as the only answers. Never shorten it yourself.";

/// The `(key, open name)` pairs the asked questions (`key` · `label` · `why`) bind, one to one:
/// a key asked twice, or a name two questions name, binds nothing.
fn bound(intent: &str, doc: &Value, asked: &[Value]) -> Vec<(String, String)> {
    let open = paths::open_names(intent);
    let pairs: Vec<(String, String)> = (asked.iter())
        .filter_map(|question| {
            let key = question["key"].as_str()?;
            let twice = asked.iter().filter(|other| other["key"] == key).count() > 1;
            let slug = key.strip_prefix("const.").filter(|_| !twice)?;
            let said = [&question["label"], &question["why"]].map(|t| t.as_str().unwrap_or(""));
            let spans: Vec<PathShape> = said.into_iter().flat_map(paths::literals).collect();
            let stated = |name: &&String| {
                spans.iter().any(|span| match span {
                    PathShape::File(text) | PathShape::Placeholder(text) => text == *name,
                    _ => false,
                })
            };
            let mut quoted = open.iter().filter(stated);
            let (Some(name), None) = (quoted.next(), quoted.next()) else {
                return None;
            };
            let whole = format!("const.{slug}");
            let path = |task: &Value| {
                let arg = task["invoke"]["args"]["path"]
                    .as_str()
                    .unwrap_or_default()
                    .trim();
                let inner = (arg.strip_prefix("${{")).and_then(|rest| rest.strip_suffix("}}"));
                inner.map(str::trim) == Some(whole.as_str())
            };
            let read = (doc["tasks"].as_object().into_iter().flatten()).any(|(_, task)| path(task));
            (doc["const"][slug] == "" && read).then(|| (key.to_owned(), name.clone()))
        })
        .collect();
    let once = |name: &String| pairs.iter().filter(|(_, other)| other == name).count() == 1;
    (pairs.iter())
        .filter(|(_, name)| once(name))
        .cloned()
        .collect()
}

/// The open names of `intent` Law 1 leaves to the typed answers of the questions that ask them.
#[must_use]
pub fn asked_names(intent: &str, doc: &Value, asked: &[Value]) -> Vec<String> {
    (bound(intent, doc, asked).into_iter())
        .map(|(_, name)| name)
        .collect()
}

/// The closed choice of the question `key` when it asks an open name: that name's readings,
/// longest first, as `{key, label}`; none for any other question or without a document.
#[must_use]
pub fn asked_readings(intent: &str, doc: Option<&Value>, asked: &[Value], key: &str) -> Vec<Value> {
    let Some(doc) = doc else {
        return Vec::new();
    };
    (bound(intent, doc, asked).into_iter())
        .filter(|(asking, _)| asking == key)
        .flat_map(|(_, name)| paths::readings(&name))
        .map(|reading| json!({"key": reading, "label": format!("the file `{reading}`")}))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{asked_names, asked_readings};
    use serde_json::{Value, json};

    /// The stock words: a report named exactly, a payload whose first word stays open.
    const STOCK: &str = "Écris out/report.json avec les alertes. Si des alertes existent, prépare exactement un payload out/notification.json contenant les ids.";
    const PAYLOAD: &str = "payload out/notification.json";

    fn question(key: &str, label: &str) -> Value {
        json!({"key": key, "label": label, "why": "La requête le laisse ouvert."})
    }

    /// A candidate writing the report and, through `${{ const.payload_path }}` (declared as
    /// `blank`), the payload; one empty write entry beside the report's grant.
    fn asking(blank: &str) -> Value {
        json!({"const": {"payload_path": blank},
        "permits": {"fs": {"write": ["out/report.json", ""]}},
        "tasks": {
            "report": {"invoke": {"tool": "nika:write", "args": {"path": "out/report.json"}}},
            "payload": {"invoke": {"tool": "nika:write",
                "args": {"path": "${{ const.payload_path }}", "content": "{}"}}}
        }})
    }

    fn keys(offers: &[Value]) -> Vec<&str> {
        offers
            .iter()
            .filter_map(|offer| offer["key"].as_str())
            .collect()
    }

    /// The question naming the open name binds it: Law 1 leaves it to that answer and the
    /// question offers exactly its readings; another question, or no document, offers none.
    #[test]
    fn the_question_naming_an_open_name_takes_its_readings() {
        let asked = [question(
            "const.payload_path",
            &format!("Quel fichier est « {PAYLOAD} » ?"),
        )];
        let doc = asking("");
        assert_eq!(asked_names(STOCK, &doc, &asked), [PAYLOAD]);
        let offers = asked_readings(STOCK, Some(&doc), &asked, "const.payload_path");
        assert_eq!(keys(&offers), [PAYLOAD, "out/notification.json"]);
        assert_eq!(offers[1]["label"], "the file `out/notification.json`");
        assert!(asked_readings(STOCK, Some(&doc), &asked, "const.other").is_empty());
        assert!(asked_readings(STOCK, None, &asked, "const.payload_path").is_empty());
        let why = [
            json!({"key": "const.payload_path", "label": "Quel fichier ?",
            "why": format!("Les mots « {PAYLOAD} » laissent ouvert son début.")}),
        ];
        assert_eq!(
            asked_names(STOCK, &doc, &why),
            [PAYLOAD],
            "named in its why"
        );
    }

    /// Anything short of the whole ask binds nothing: a label naming only the shorter file or no
    /// file, a constant already answered, a path composed around it, a constant read as content,
    /// a quoted name (no open name at all) or two questions naming the same name.
    #[test]
    fn anything_short_of_the_whole_ask_binds_nothing() {
        let label = format!("Quel fichier est « {PAYLOAD} » ?");
        let named = [question("const.payload_path", &label)];
        for unnamed in [
            "Quel fichier est out/notification.json ?",
            "Où va le payload ?",
        ] {
            let asked = [question("const.payload_path", unnamed)];
            assert!(
                asked_names(STOCK, &asking(""), &asked).is_empty(),
                "{unnamed}"
            );
        }
        assert!(asked_names(STOCK, &asking("out/notification.json"), &named).is_empty());
        let mut composed = asking("");
        composed["tasks"]["payload"]["invoke"]["args"]["path"] =
            json!("out/${{ const.payload_path }}");
        assert!(asked_names(STOCK, &composed, &named).is_empty());
        let mut content = asking("");
        content["tasks"]["payload"]["invoke"]["args"] =
            json!({"path": "out/x.json", "content": "${{ const.payload_path }}"});
        assert!(asked_names(STOCK, &content, &named).is_empty());
        let quoted = STOCK.replace(
            "un payload out/notification.json",
            "un « payload out/notification.json »",
        );
        assert!(asked_names(&quoted, &asking(""), &named).is_empty());
        let mut doc = asking("");
        doc["const"]["copy_path"] = json!("");
        doc["tasks"]["copy"] =
            json!({"invoke": {"tool": "nika:write", "args": {"path": "${{ const.copy_path }}"}}});
        let twice = [named[0].clone(), question("const.copy_path", &label)];
        assert!(asked_names(STOCK, &doc, &twice).is_empty());
        assert!(asked_readings(STOCK, Some(&doc), &twice, "const.copy_path").is_empty());
    }

    /// Two open names, each named by its own question: each binds its own whatever the order of
    /// the questions; a question naming both binds neither, and a name no question names stays
    /// owed while the other is left to its answer.
    #[test]
    fn each_open_name_binds_only_the_question_naming_it() {
        let two = "Lis un journal in/a.json. Puis prépare un payload out/c.json.";
        let doc = json!({"const": {"source_path": "", "payload_path": ""},
        "permits": {"fs": {"read": [""], "write": [""]}},
        "tasks": {
            "load": {"invoke": {"tool": "nika:read",
                "args": {"path": "${{ const.source_path }}"}}},
            "save": {"invoke": {"tool": "nika:write",
                "args": {"path": "${{ const.payload_path }}"}}}
        }});
        let source = question(
            "const.source_path",
            "Quel fichier est « journal in/a.json » ?",
        );
        let payload = question(
            "const.payload_path",
            "Quel fichier est « payload out/c.json » ?",
        );
        for asked in [
            [source.clone(), payload.clone()],
            [payload.clone(), source.clone()],
        ] {
            let mut names = asked_names(two, &doc, &asked);
            names.sort();
            assert_eq!(names, ["journal in/a.json", "payload out/c.json"]);
            let read = asked_readings(two, Some(&doc), &asked, "const.source_path");
            assert_eq!(keys(&read), ["journal in/a.json", "in/a.json"]);
            let write = asked_readings(two, Some(&doc), &asked, "const.payload_path");
            assert_eq!(keys(&write), ["payload out/c.json", "out/c.json"]);
        }
        let both = question(
            "const.source_path",
            "« journal in/a.json » ou « payload out/c.json » ?",
        );
        let asked = [both, payload.clone()];
        assert_eq!(asked_names(two, &doc, &asked), ["payload out/c.json"]);
        assert!(asked_readings(two, Some(&doc), &asked, "const.source_path").is_empty());
        assert_eq!(asked_names(two, &doc, &[payload]), ["payload out/c.json"]);
    }

    /// The label or why must state the open name as one whole literal, as the reader spans it: a
    /// longer file that starts or ends with its words names another file and binds nothing,
    /// quoted or not; the exact name, quoted, unquoted after a function word or in backticks,
    /// binds it.
    #[test]
    fn only_the_whole_open_name_binds_never_a_longer_file() {
        let doc = asking("");
        for longer in [
            format!("Quel fichier est « {PAYLOAD}.backup » ?"),
            format!("Quel fichier est {PAYLOAD}.backup ?"),
            format!("Quel fichier est « notes {PAYLOAD} » ?"),
            format!("Quel fichier est « {PAYLOAD}/x.json » ?"),
        ] {
            let asked = [question("const.payload_path", &longer)];
            assert!(asked_names(STOCK, &doc, &asked).is_empty(), "{longer}");
            let offers = asked_readings(STOCK, Some(&doc), &asked, "const.payload_path");
            assert!(offers.is_empty(), "{longer}: {offers:?}");
        }
        for exact in [
            format!("Quel fichier est « {PAYLOAD} » ?"),
            format!("Quel fichier est {PAYLOAD} ?"),
            format!("Which file is `{PAYLOAD}`?"),
        ] {
            let asked = [question("const.payload_path", &exact)];
            assert_eq!(asked_names(STOCK, &doc, &asked), [PAYLOAD], "{exact}");
        }
    }

    /// One key asked twice never claims a name, whichever names its questions quote: one
    /// constant read and written whole, asked once per open name, waives nothing and offers no
    /// reading, so both names stay owed; asked once, it binds the name it quotes.
    #[test]
    fn a_key_asked_twice_binds_no_name() {
        let two = "Lis un journal in/a.json. Puis prépare un payload out/c.json.";
        let doc = json!({"const": {"location_path": ""},
        "permits": {"fs": {"read": [""], "write": [""]}},
        "tasks": {
            "load": {"invoke": {"tool": "nika:read",
                "args": {"path": "${{ const.location_path }}"}}},
            "save": {"invoke": {"tool": "nika:write",
                "args": {"path": "${{ const.location_path }}"}}}
        }});
        let journal = question(
            "const.location_path",
            "Quel fichier est « journal in/a.json » ?",
        );
        let payload = question(
            "const.location_path",
            "Quel fichier est « payload out/c.json » ?",
        );
        for asked in [
            [journal.clone(), payload.clone()],
            [payload.clone(), journal.clone()],
            [payload.clone(), payload.clone()],
        ] {
            assert!(asked_names(two, &doc, &asked).is_empty(), "{asked:?}");
            let offers = asked_readings(two, Some(&doc), &asked, "const.location_path");
            assert!(offers.is_empty(), "{offers:?}");
        }
        assert_eq!(asked_names(two, &doc, &[journal]), ["journal in/a.json"]);
    }

    /// A genuine spaced folder stays choosable whole: its readings start with the whole name.
    #[test]
    fn a_genuine_spaced_name_is_offered_whole_first() {
        let intent = "Read ./in/a.json, then write project notes/summary.json.";
        let doc = json!({"const": {"summary_path": ""},
            "permits": {"fs": {"read": ["./in/a.json"], "write": [""]}},
            "tasks": {"save": {"invoke": {"tool": "nika:write",
                "args": {"path": "${{ const.summary_path }}"}}}}});
        let asked = [question(
            "const.summary_path",
            "Is it « project notes/summary.json »?",
        )];
        let offers = asked_readings(intent, Some(&doc), &asked, "const.summary_path");
        assert_eq!(
            keys(&offers),
            ["project notes/summary.json", "notes/summary.json"]
        );
    }
}
