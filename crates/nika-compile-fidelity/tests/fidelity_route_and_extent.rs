// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A request that names an endpoint's route and a payload file in prose, its words verbatim:
//! `/notifications/stock`, sent a POST on the local sink the request states, is that endpoint's
//! route, realized by a send to exactly that URL, never a local directory to grant. A local path
//! the request reads stays a path, even after a sentence that writes. An unquoted spaced name
//! (`un payload out/notification.json`) is settled by a folder another path of the request names
//! (`out/report.json`), or by the person's answer; otherwise it keeps its exact extent, and a
//! quoted name owns its whole extent.
use nika_compile_fidelity::fidelity::{Diagnostic, asked_names, laws_observed};
use nika_compile_reader::lexicon;
use serde_json::{Map, Value, json};

const STOCK: &str = "Parcours toutes les pages du stock depuis le curseur initial de world/source.json. Les pages sont des fichiers JSON locaux identifiés par leur cursor ; un next_cursor null prouve la fin. Chaque stock et seuil est en pièces. Déduplique les occurrences strictement identiques d'un même id, mais conserve des id distincts même si leur sku est égal. Signale seulement les variantes dont stock < threshold, strictement. Un threshold absent est inconnu, jamais zéro ou une valeur globale inventée.\n\nÉcris out/report.json avec source_complete, evaluation_complete, status, pages_read (ordre de lecture), rows_seen (occurrences lues), unique_item_count, alerts, unknown_items et missing_cursors. Les alertes conservent id, sku, stock, threshold, unit dans l'ordre de première rencontre. Les inconnus ont id, sku et reason=missing_threshold. Si toutes les pages ont été lues, status vaut evaluated_with_unknowns s'il reste un seuil inconnu, sinon evaluated ; evaluation_complete est vrai seulement sans inconnus. Si une page référencée manque, status=incomplete_source, source_complete=false et evaluation_complete=false ; conserve les compteurs effectivement connus, missing_cursors, mais alerts et unknown_items sont null : n'annonce pas un bilan complet ou zéro alerte.\n\nUne fois la source complète, si des alertes certaines existent, prépare exactement un payload out/notification.json contenant channel=\"stock\" et item_ids dans leur ordre, puis effectue exactement un POST /notifications/stock vers le sink local fourni par le futur pilote. S'il n'y a aucune alerte certaine ou si la source est incomplète, aucun fichier notification et aucun POST. Aucun autre effet ni autre fichier dans out. Ne modifie pas les sources.\n\nLe sink est une abstraction de test, pas Slack. Ces données ne décrivent ni une API Shopify ni un déclencheur quotidien. Pour cette exécution réelle, le pilote a démarré le sink local http://127.0.0.1:57468 ; effectue le POST prévu, uniquement dans les conditions ci-dessus.\n";

/// The route's whole URL: the sink the request states, then the route.
const ROUTE_URL: &str = "http://127.0.0.1:57468/notifications/stock";

/// The request's open spaced name, owed exactly.
const PAYLOAD: &str = "payload out/notification.json";

/// A candidate reading `reads`, writing each of `writes` (each permitted exactly), and, when
/// `url` is given, sending a POST to it, with `consts` declared.
fn candidate(reads: &[&str], writes: &[&str], url: Option<&str>, consts: &Value) -> Value {
    let mut tasks = Map::new();
    for (k, path) in reads.iter().enumerate() {
        let read = json!({"invoke": {"tool": "nika:read", "args": {"path": path}}});
        tasks.insert(format!("read_{k}"), read);
    }
    for (k, path) in writes.iter().enumerate() {
        let args = json!({"path": path, "content": "{}"});
        tasks.insert(
            format!("write_{k}"),
            json!({"invoke": {"tool": "nika:write", "args": args}}),
        );
    }
    if let Some(url) = url {
        let args = json!({"url": url, "method": "POST", "body": "{}"});
        tasks.insert(
            "post".to_owned(),
            json!({"invoke": {"tool": "nika:fetch", "args": args}}),
        );
    }
    json!({"nika": "stock-alerts", "const": consts, "tasks": tasks,
        "permits": {"fs": {"read": reads, "write": writes}, "net": {"http": ["127.0.0.1:57468"]},
            "tools": ["nika:read", "nika:write", "nika:fetch"]}})
}

/// The stock request's candidate: the source read, `writes` written, a POST sent to `url`.
fn stock(writes: &[&str], url: Option<&str>) -> Value {
    candidate(&["world/source.json"], writes, url, &json!({}))
}

/// The paths the path law finds unrealized for `intent` over `doc`, in its order.
fn unrealized(intent: &str, doc: &Value) -> Vec<String> {
    let plan = lexicon::read(intent).plan;
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_observed(intent, &plan, doc, &[], &[], &[], None, &mut out);
    (out.iter())
        .filter(|d| d.kind == "path")
        .filter_map(|d| d.message.split('`').nth(1).map(str::to_owned))
        .collect()
}

/// Both output files, as a candidate may read the request.
const OUTPUTS: [&str; 2] = ["out/report.json", "out/notification.json"];

/// A POST to exactly the stated route (its URL written whole, or the sink through a constant)
/// realizes it; the payload's name, settled by the request's own `out/report.json`, is realized
/// by its write.
#[test]
fn the_stated_route_is_realized_by_a_send_to_exactly_it() {
    assert_eq!(
        unrealized(STOCK, &stock(&OUTPUTS, Some(ROUTE_URL))),
        [""; 0]
    );
    let consts = json!({"sink_url": "http://127.0.0.1:57468"});
    let bound = "${{ const.sink_url }}/notifications/stock";
    let doc = candidate(&["world/source.json"], &OUTPUTS, Some(bound), &consts);
    assert_eq!(unrealized(STOCK, &doc), [""; 0]);
    let whole = stock(&["out/report.json", PAYLOAD], Some(ROUTE_URL));
    assert_eq!(unrealized(STOCK, &whole), [""; 0]);
}

/// The route stays owed: no send, a send to another route, a send to an origin the request
/// never states, and a GET realize nothing.
#[test]
fn only_a_send_to_exactly_the_stated_route_realizes_it() {
    let elsewhere = [
        None,
        Some("http://127.0.0.1:57468/notifications/alerts"),
        Some("http://127.0.0.1:9/notifications/stock"),
    ];
    for url in elsewhere {
        let doc = stock(&OUTPUTS, url);
        assert_eq!(unrealized(STOCK, &doc), ["/notifications/stock"], "{url:?}");
    }
    let mut get = stock(&OUTPUTS, Some(ROUTE_URL));
    get["tasks"]["post"]["invoke"]["args"]["method"] = json!("GET");
    assert_eq!(unrealized(STOCK, &get), ["/notifications/stock"]);
}

/// A name the request quotes owns its whole extent, spaces included: writing its last words
/// writes another file.
#[test]
fn a_quoted_spaced_name_keeps_its_whole_extent() {
    let quoted = STOCK.replace(
        "un payload out/notification.json",
        "un « payload out/notification.json »",
    );
    let doc = stock(&OUTPUTS, Some(ROUTE_URL));
    assert_eq!(unrealized(&quoted, &doc), [PAYLOAD]);
}

/// An unquoted spaced name keeps its exact extent, article or not, when no folder the request
/// names settles it (`./in` and `./data` hold neither `project notes` nor `notes`): only the
/// whole name realizes it.
#[test]
fn a_spaced_name_keeps_its_exact_extent() {
    for intent in [
        "Read ./in/a.json and write the summary to project notes/summary.json.",
        "Read ./data/a.json, then write project notes/summary.json.",
        "Read ./data/a.json, then write a project notes/summary.json.",
    ] {
        let read = intent.split_whitespace().nth(1).unwrap_or_default();
        let read = read.trim_end_matches(',');
        let wrote = |path: &str| candidate(&[read], &[path], None, &json!({}));
        let owed = ["project notes/summary.json"];
        assert_eq!(
            unrealized(intent, &wrote("notes/summary.json")),
            owed,
            "{intent}"
        );
        let whole = wrote("project notes/summary.json");
        assert_eq!(unrealized(intent, &whole), [""; 0], "{intent}");
    }
}

/// A folder the request names settles the start: beside `./notes/a.json`, `project
/// notes/summary.json` is `notes/summary.json`, citing `./notes`. Its write realizes it, and the
/// whole spaced text, another file, no longer does.
#[test]
fn a_folder_the_request_names_settles_a_spaced_name() {
    for intent in [
        "Read ./notes/a.json, then write project notes/summary.json.",
        "Read ./notes/a.json, then write a project notes/summary.json.",
    ] {
        let wrote = |path: &str| candidate(&["./notes/a.json"], &[path], None, &json!({}));
        let settled = wrote("notes/summary.json");
        assert_eq!(unrealized(intent, &settled), [""; 0], "{intent}");
        let whole = wrote("project notes/summary.json");
        assert_eq!(
            unrealized(intent, &whole),
            ["notes/summary.json"],
            "{intent}"
        );
    }
}

/// A source the request states after a destination sentence stays a source: an earlier `to`
/// never makes it a destination a send to the stated sink at that route could realize.
#[test]
fn a_source_after_a_destination_sentence_is_never_realized_by_a_send() {
    let intent =
        "Write to ./out/report.json. Read /srv/stock. Send the report to http://127.0.0.1:57468.";
    let url = Some("http://127.0.0.1:57468/srv/stock");
    let doc = candidate(&[], &["./out/report.json"], url, &json!({}));
    assert_eq!(unrealized(intent, &doc), ["/srv/stock"]);
}

/// The open name is left to the typed answer of the question naming it (its blank constant read
/// whole as the payload's path, one empty write entry beside the report's grant): nothing else
/// is owed. Unnamed, it stays owed exactly, and the finding tells how the human settles it; an
/// exact path's finding never does. The report goes to `reports/` here, a folder no reading of
/// the payload sits in, so the words leave its start open.
#[test]
fn an_open_name_is_left_to_the_question_naming_it_and_owed_exactly_otherwise() {
    let open = STOCK.replace("Écris out/report.json", "Écris reports/report.json");
    let consts = json!({"payload_path": ""});
    let asked_path = "${{ const.payload_path }}";
    let mut doc = candidate(
        &["world/source.json"],
        &["reports/report.json", ""],
        Some(ROUTE_URL),
        &consts,
    );
    doc["tasks"]["write_1"]["invoke"]["args"]["path"] = json!(asked_path);
    let label = format!("Quel fichier désigne « {PAYLOAD} » ?");
    let named = [json!({"key": "const.payload_path", "label": label, "why": ""})];
    let waived = asked_names(&open, &doc, &named);
    assert_eq!(waived, [PAYLOAD]);
    let plan = lexicon::read(&open).plan;
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_observed(&open, &plan, &doc, &[], &waived, &[], None, &mut out);
    assert!(out.iter().all(|d| d.kind != "path"), "{out:#?}");
    let unnamed = [json!({"key": "const.payload_path", "label": "Où va le payload ?", "why": ""})];
    assert!(asked_names(&open, &doc, &unnamed).is_empty());
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_observed(&open, &plan, &doc, &[], &[], &[], None, &mut out);
    let owed: Vec<&str> = (out.iter())
        .filter(|d| d.kind == "path")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(owed.len(), 1, "the open name alone is owed: {out:#?}");
    assert!(owed[0].contains(&format!("`{PAYLOAD}`")), "{}", owed[0]);
    assert!(owed[0].contains("the human settles it"), "{}", owed[0]);
    let route = unrealized(STOCK, &stock(&OUTPUTS, None));
    assert_eq!(route, ["/notifications/stock"]);
    let plan = lexicon::read(STOCK).plan;
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_observed(
        STOCK,
        &plan,
        &stock(&OUTPUTS, None),
        &[],
        &[],
        &[],
        None,
        &mut out,
    );
    let exact = out
        .iter()
        .find(|d| d.message.contains("`/notifications/stock`"));
    assert!(
        !exact.is_some_and(|d| d.message.contains("the human settles it")),
        "{out:#?}"
    );
}

/// The request's own `Écris out/report.json` names the folder `out`: of the payload's two readings
/// only `out/notification.json` sits in it, so the words settle the name. A candidate that omits
/// it owes that path, never the whole spaced text.
#[test]
fn a_folder_the_request_names_settles_the_payload() {
    let report_only = stock(&["out/report.json"], Some(ROUTE_URL));
    assert_eq!(unrealized(STOCK, &report_only), ["out/notification.json"]);
}

/// When the words leave the start open (the report goes to `reports/`), the name is owed whole
/// and its finding lists the readings with the path alone as the stated default; the person's
/// answer, one of its readings, settles it, and that reading's write realizes it.
#[test]
fn the_persons_answered_reading_settles_an_open_name() {
    let open = STOCK.replace("Écris out/report.json", "Écris reports/report.json");
    let writes = ["reports/report.json", "out/notification.json"];
    let doc = candidate(&["world/source.json"], &writes, Some(ROUTE_URL), &json!({}));
    let plan = lexicon::read(&open).plan;
    let owed = |allowed: &[String]| {
        let mut out: Vec<Diagnostic> = Vec::new();
        laws_observed(&open, &plan, &doc, allowed, &[], &[], None, &mut out);
        (out.into_iter())
            .filter(|d| d.kind == "path")
            .map(|d| d.message)
            .collect::<Vec<_>>()
    };
    let unanswered = owed(&[]);
    assert_eq!(unanswered.len(), 1, "{unanswered:#?}");
    let finding = &unanswered[0];
    assert!(
        finding.contains(&format!("`{PAYLOAD}` or `out/notification.json`")),
        "{finding}"
    );
    assert!(
        finding.contains("`out/notification.json`, the path alone, as the stated default"),
        "{finding}"
    );
    assert_eq!(
        owed(&["out/notification.json".to_owned()]),
        Vec::<String>::new()
    );
}

/// A request names two outputs and leaves where one's name starts open (« Il faudra générer
/// out/stock.json »): its other output, `out/stock.md`, names the folder `out`, so Law 1 owes
/// `out/stock.json`, and a candidate reading the articles and writing both outputs owes nothing.
#[test]
fn a_second_output_in_the_folder_settles_an_open_name() {
    let request = "Prépare un workflow stock.nika qui lit articles.csv et ne garde que les \
                   articles en rupture. Il faudra générer out/stock.json avec le nombre et la \
                   liste des références, puis un court résumé dans out/stock.md.";
    let writes = ["out/stock.json", "out/stock.md"];
    let doc = candidate(&["articles.csv"], &writes, None, &json!({}));
    assert_eq!(unrealized(request, &doc), [""; 0]);
}

/// A local path the request reads stays a path to read: a send to the stated sink at that route
/// realizes no read of it.
#[test]
fn a_local_path_the_request_reads_is_never_realized_by_a_send() {
    let intent = "Lis les pages de /srv/stock, puis effectue un POST vers http://127.0.0.1:57468 et \
                  écris ./out/report.json.";
    let url = Some("http://127.0.0.1:57468/srv/stock");
    let doc = candidate(&[], &["./out/report.json"], url, &json!({}));
    assert_eq!(unrealized(intent, &doc), ["/srv/stock"]);
}
