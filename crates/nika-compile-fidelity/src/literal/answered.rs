// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure permit projection for an answered literal endpoint or path. The compile core still
//! selects the answer, proves its literal replacement, emits source and runs Check; no effect
//! occurs here.

use serde_json::{Value, json};

/// Complete the one empty entry (`""`) a seat left on one side of `permits.fs` (`read` ·
/// `write`) while a path was asked, in place: the capability inference's `introduced` paths on
/// that side, each the answer of one of the questions `keys` (its constant), in the keys' order.
/// Every other entry is kept. No empty entry or two, or an introduced path no question answered,
/// grants nothing.
pub fn grant_paths(after: &mut Value, side: &str, keys: &[&str], introduced: &[String]) -> bool {
    let mut answered: Vec<String> = Vec::new();
    for slug in keys.iter().filter_map(|key| key.strip_prefix("const.")) {
        let path = after["const"][slug]
            .as_str()
            .filter(|path| introduced.iter().any(|p| p == path));
        if let Some(path) = path.filter(|path| !answered.iter().any(|p| p == path)) {
            answered.push(path.to_owned());
        }
    }
    let list = after.pointer_mut(&format!("/permits/fs/{side}"));
    let whole = !answered.is_empty() && answered.len() == introduced.len();
    let Some(list) = list.and_then(Value::as_array_mut).filter(|_| whole) else {
        return false;
    };
    let mut blanks = (list.iter().enumerate()).filter(|(_, entry)| entry.as_str() == Some(""));
    let (Some((at, _)), None) = (blanks.next(), blanks.next()) else {
        return false;
    };
    let rest = list.split_off(at);
    list.extend(answered.into_iter().map(Value::String));
    list.extend(rest.into_iter().skip(1));
    true
}

/// Add the host of an answered URL to `permits.net.http` when a `nika:fetch` url or a
/// `nika:notify` target reads exactly that answer (`${{ const.<slug> }}`): an absent list and its
/// absent parents are created, an empty or stated one is extended; an answer no such argument
/// reads, a wildcard, another scheme or an ancestor of another type grants nothing.
pub fn grant_host(after: &mut Value, slug: &str, value: &Value) -> bool {
    let Some(host) = value
        .as_str()
        .and_then(host_of)
        .filter(|h| !h.contains('*'))
    else {
        return false;
    };
    let whole = format!("${{{{ const.{slug} }}}}");
    let reads = |task: &Value| match task["invoke"]["tool"].as_str() {
        Some("nika:fetch") => task["invoke"]["args"]["url"] == whole.as_str(),
        Some("nika:notify") => task["invoke"]["args"]["target"] == whole.as_str(),
        _ => false,
    };
    if !after["tasks"]
        .as_object()
        .is_some_and(|tasks| tasks.values().any(reads))
    {
        return false;
    }
    let list = member(after, "permits", json!({}))
        .and_then(|permits| member(permits, "net", json!({})))
        .and_then(|net| member(net, "http", json!([])))
        .and_then(Value::as_array_mut);
    let Some(list) = list else {
        return false;
    };
    if list.iter().any(|h| h.as_str() == Some(host)) {
        return false;
    }
    list.push(Value::String(host.to_owned()));
    true
}

/// The member `key` of an object, created as `empty` when absent; none when `value` is no object.
fn member<'a>(value: &'a mut Value, key: &str, empty: Value) -> Option<&'a mut Value> {
    Some(value.as_object_mut()?.entry(key).or_insert(empty))
}

/// The host of an `http(s)://` URL, without its port: the form `permits.net.http` lists (the
/// assembler grants `url.host_str()`; the loopback declassification compares exact hosts).
fn host_of(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = if authority.starts_with('[') {
        authority
            .find(']')
            .map_or(authority, |close| &authority[..=close])
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A candidate whose asked `const.payload_path` was answered `out/notification.json` and whose
    /// `const.note` was answered `./secret.txt`, its write side `write`, its read side stated.
    fn answered(write: &Value) -> Value {
        json!({"const": {"payload_path": "out/notification.json", "note": "./secret.txt"},
            "permits": {"fs": {"read": ["world/source.json"], "write": write}}})
    }

    /// The inferred path an answered question supplies completes its side's one empty entry in
    /// place, every other entry kept, the other side untouched. No empty entry or two, no path
    /// introduced, or an introduced path no question answered grants nothing and changes nothing.
    #[test]
    fn an_answered_path_completes_its_sides_one_empty_entry_in_place() {
        let keys = ["const.payload_path", "const.note"];
        let introduced = ["out/notification.json".to_owned()];
        for (before, after) in [
            (
                json!(["out/report.json", ""]),
                json!(["out/report.json", "out/notification.json"]),
            ),
            (
                json!(["", "out/report.json"]),
                json!(["out/notification.json", "out/report.json"]),
            ),
            (json!([""]), json!(["out/notification.json"])),
        ] {
            let mut doc = answered(&before);
            assert!(
                grant_paths(&mut doc, "write", &keys, &introduced),
                "{before}"
            );
            assert_eq!(doc["permits"]["fs"]["write"], after, "{before}");
            assert_eq!(doc["permits"]["fs"]["read"], json!(["world/source.json"]));
        }
        let mut doc = answered(&json!(["out/report.json", ""]));
        assert!(!grant_paths(&mut doc, "read", &keys, &introduced));
        let unanswered = [
            "out/notification.json".to_owned(),
            "out/other.json".to_owned(),
        ];
        for (write, introduced) in [
            (json!(["", ""]), &introduced[..]),
            (json!(["out/report.json"]), &introduced[..]),
            (json!(["out/report.json", ""]), &[][..]),
            (json!(["out/report.json", ""]), &unanswered[..]),
        ] {
            let mut doc = answered(&write);
            assert!(
                !grant_paths(&mut doc, "write", &keys, introduced),
                "{write}"
            );
            assert_eq!(doc["permits"]["fs"]["write"], write, "{write}");
        }
        // Content answered by another question supplies nothing.
        let mut doc = answered(&json!(["out/report.json", ""]));
        assert!(!grant_paths(
            &mut doc,
            "write",
            &["const.note"],
            &introduced
        ));
        // Two questions answering paths on one side complete its one empty entry, in their order.
        let both = ["out/a.json".to_owned(), "out/b.json".to_owned()];
        let mut doc = json!({"const": {"b_path": "out/b.json", "a_path": "out/a.json"},
            "permits": {"fs": {"write": ["out/report.json", ""]}}});
        let order = ["const.b_path", "const.a_path"];
        assert!(grant_paths(&mut doc, "write", &order, &both));
        let completed = json!(["out/report.json", "out/b.json", "out/a.json"]);
        assert_eq!(doc["permits"]["fs"]["write"], completed);
    }

    #[test]
    fn an_answered_url_grants_its_host_without_its_port() {
        assert_eq!(
            host_of("https://hooks.example.invalid/recap"),
            Some("hooks.example.invalid")
        );
        assert_eq!(host_of("http://127.0.0.1:8793/hook"), Some("127.0.0.1"));
        assert_eq!(host_of("http://[::1]:8080/x"), Some("[::1]"));
        assert_eq!(host_of("./out/report.md"), None);
        let notify = json!({"send": {"invoke": {"tool": "nika:notify",
            "args": {"target": "${{ const.endpoint }}"}}}});
        let fetch = json!({"get": {"invoke": {"tool": "nika:fetch",
            "args": {"url": "${{ const.endpoint }}"}}}});
        let recap = json!("https://hooks.example.invalid/recap");
        let empty = json!({"net": {"http": []}});
        // A notify target or a fetch url read whole grants into the stated list or into one created
        // with its absent parents; a host already listed stays, once.
        for permits in [
            empty.clone(),
            json!({}),
            json!({"fs": {}}),
            json!({"net": {}}),
        ] {
            for tasks in [&notify, &fetch] {
                let mut doc = json!({"permits": permits.clone(), "tasks": tasks});
                assert!(grant_host(&mut doc, "endpoint", &recap), "{doc}");
                let again = json!("https://hooks.example.invalid/again");
                assert!(!grant_host(&mut doc, "endpoint", &again));
                assert_eq!(
                    doc["permits"]["net"]["http"],
                    json!(["hooks.example.invalid"])
                );
            }
        }
        let mut bare = json!({"tasks": notify});
        assert!(grant_host(&mut bare, "endpoint", &recap));
        assert_eq!(
            bare["permits"]["net"]["http"],
            json!(["hooks.example.invalid"])
        );
        let mut kept =
            json!({"permits": {"net": {"http": ["api.example.invalid"]}}, "tasks": notify});
        assert!(grant_host(&mut kept, "endpoint", &recap));
        let both = json!(["api.example.invalid", "hooks.example.invalid"]);
        assert_eq!(kept["permits"]["net"]["http"], both);
        // Nothing reads the answer whole as a net argument: nothing is granted or created.
        let partial = json!({"get": {"invoke": {"tool": "nika:fetch",
            "args": {"url": "${{ const.endpoint }}/contacts"}}}});
        let other = json!({"send": {"invoke": {"tool": "nika:notify",
            "args": {"target": "${{ const.other }}"}}}});
        let message = json!({"send": {"invoke": {"tool": "nika:notify",
            "args": {"target": "https://fixed.invalid/", "message": "${{ const.endpoint }}"}}}});
        for tasks in [json!({}), partial, other, message] {
            for permits in [empty.clone(), json!({})] {
                let mut doc = json!({"permits": permits.clone(), "tasks": tasks});
                assert!(!grant_host(&mut doc, "endpoint", &recap), "{doc}");
                assert_eq!(doc["permits"], permits, "{doc}");
            }
        }
        // A wildcard host or another scheme grants nothing.
        for answer in [
            "https://*.example.invalid/x",
            "ftp://hooks.example.invalid/x",
        ] {
            let mut doc = json!({"permits": {}, "tasks": notify});
            assert!(
                !grant_host(&mut doc, "endpoint", &json!(answer)),
                "{answer}"
            );
            assert_eq!(doc["permits"], json!({}), "{answer}");
        }
        // An ancestor of another type fails closed: no panic, no grant, nothing replaced.
        for permits in [
            json!("x"),
            json!({"net": []}),
            json!({"net": "y"}),
            json!({"net": {"http": {}}}),
            json!({"net": {"http": "z"}}),
        ] {
            let mut doc = json!({"permits": permits.clone(), "tasks": notify});
            assert!(!grant_host(&mut doc, "endpoint", &recap), "{permits}");
            assert_eq!(doc["permits"], permits, "{permits}");
        }
    }
}
