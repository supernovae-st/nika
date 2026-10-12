// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The r3 Foundry release at the door: a family recalls the pack it recommends and the blocks that
//! realize the pack's patterns, for a digest and for a threshold report; the audit's French seed,
//! which the lexical pack need not hold, reaches the feeds block through the whole catalogue; the
//! feeds block expands into a person's digest document and checks.

use nika_compile_seats::foundry::witness::witness;
use nika_compile_seats::foundry::{
    Binding, ComponentCatalog, ComponentRef, expand, instantiate, qualified_with,
};
use serde_json::json;

use crate::{Snapshot, bundled};
use nika_compile::{AuthoringKnowledge, CompileRequest};
use nika_compile_seats::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat};

/// The qualified feeds block: the items a feed published on the person's local day.
const FEEDS: &str = "block:feeds-items-on-local-date";
/// The qualified JSON threshold report block.
const THRESHOLD: &str = "block:json-threshold-report";
/// The audit's French news seed.
const FRENCH_SEED: &str = "Récupère les dernières actualités tech de Y Combinator et TechCrunch, fais-moi un résumé et sauvegarde-le en Markdown dans un dossier.";
/// An English digest request.
const ENGLISH_DIGEST: &str = "Every morning, read the news feeds of two sites, keep the items of the day in my time zone and write a digest of them";
/// An English threshold report request.
const THRESHOLD_REQUEST: &str = "Read the orders JSON file, keep the records whose amount is above a threshold, count and total them, and write a report";

fn current() -> Snapshot {
    bundled::admit(Some(&bundled::identity().unwrap())).unwrap()
}

/// The pack's references, by kind and id.
fn held(pack: &AuthoringKnowledge) -> Vec<(String, String)> {
    (pack.references.iter())
        .map(|r| (r.kind.clone(), r.id.clone()))
        .collect()
}

/// The families the pack recalled, by id.
fn families(pack: &AuthoringKnowledge) -> Vec<String> {
    (pack.selection["families"].as_array().into_iter().flatten())
        .filter_map(|family| family["id"].as_str().map(str::to_owned))
        .collect()
}

/// A request reaches `block` through `family`: the family is recalled, the block is held, and
/// the pack offers a pattern through the family's pack that the block realizes, whichever list
/// admitted the block (a request naming the block's own words also finds it by them first).
fn reaches(request: &str, family: &str, block: &str) {
    let snapshot = current();
    let pack = snapshot.pack(request, None).unwrap();
    assert!(
        families(&pack).iter().any(|id| id == family),
        "{:#}",
        pack.selection["families"]
    );
    assert!(
        held(&pack).contains(&("block".to_owned(), block.to_owned())),
        "{:?}",
        held(&pack)
    );
    let via = format!("{family} via ");
    let walked = (pack.selection["patterns"].as_array().into_iter().flatten())
        .filter(|row| row["why"].as_str().is_some_and(|why| why.starts_with(&via)))
        .filter_map(|row| row["id"].as_str())
        .any(|pattern| snapshot.sources(pattern, "REALIZES").contains(&block));
    assert!(walked, "{:#}", pack.selection["patterns"]);
}

#[test]
fn an_english_digest_request_reaches_the_feeds_block_through_its_family_and_pack() {
    reaches(ENGLISH_DIGEST, "family:scheduled-digest", FEEDS);
}

#[test]
fn a_threshold_report_request_reaches_its_block_through_its_family_and_pack() {
    reaches(
        THRESHOLD_REQUEST,
        "family:collection-filter-aggregate",
        THRESHOLD,
    );
}

/// A seat that finds only the feeds block applicable, and every other entry unrelated or
/// undecided.
struct OnlyTheFeeds;

impl DecisionSeat for OnlyTheFeeds {
    fn name(&self) -> &'static str {
        "test/only-the-feeds"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        let id = question.state["reference"]["id"]
            .as_str()
            .unwrap_or_default();
        let choice = match id {
            FEEDS => "applies",
            id if id.starts_with("block:") => "unrelated",
            _ => "none",
        };
        Box::pin(async move { Ok(ChoiceAnswer::new(choice, "test/only-the-feeds")) })
    }
}

#[tokio::test]
async fn the_french_seed_reaches_the_feeds_block_through_the_whole_catalogue() {
    let snapshot = current();
    let pack = snapshot.pack(FRENCH_SEED, None).unwrap();
    let request = CompileRequest::create(FRENCH_SEED).with_authoring_knowledge(pack);
    let (shown, record) =
        qualified_with(FRENCH_SEED, &request, Some(&OnlyTheFeeds), Some(&snapshot))
            .await
            .unwrap();
    assert_eq!(
        record["coverage"]["complete"], true,
        "{:#}",
        record["coverage"]
    );
    let references = &shown.authoring_knowledge.as_ref().unwrap().references;
    let feeds = references.iter().find(|r| r.id == FEEDS).unwrap();
    assert!(
        feeds.text.contains("feeds_today:"),
        "resolved in full:\n{}",
        feeds.text
    );
    let row = (record["references"].as_array().unwrap().iter())
        .find(|r| r["id"] == FEEDS)
        .unwrap();
    assert_eq!(row["shown"], "full", "{row:#}");
}

/// The person's own digest document: their name, their boundary over the two feeds' hosts.
const DIGEST_PARENT: &str = r#"nika: tech-news-digest
permits:
  tools: ["nika:fetch"]
  net: { http: ["news.ycombinator.com", "techcrunch.com"] }
tasks: {}
"#;

#[test]
fn the_feeds_block_expands_into_a_persons_digest_and_checks() {
    let snapshot = current();
    let component = snapshot.resolve(&ComponentRef::new(FEEDS)).unwrap();
    let bindings = [
        Binding::new(
            "const.feeds",
            json!([
                "https://news.ycombinator.com/rss",
                "https://techcrunch.com/feed/"
            ]),
        ),
        Binding::new("const.tz", json!("Europe/Paris")),
        Binding::new("const.max_per_feed", json!(10)),
    ];
    let expansion = expand(DIGEST_PARENT, &instantiate(&component, &bindings).unwrap()).unwrap();
    assert!(expansion.ready, "{:#}", expansion.receipt["check"]);
    for probe in [
        "example.com",
        "example.org",
        "p59-feeds-items-on-local-date",
    ] {
        assert!(
            !expansion.candidate.contains(probe),
            "{probe} inherited:\n{}",
            expansion.candidate
        );
    }
    let receipt = &expansion.receipt;
    assert_eq!(receipt["component"]["id"], FEEDS);
    assert_eq!(
        receipt["component"]["release"]["snapshot_sha256"],
        snapshot.manifest_sha256()
    );
    assert_eq!(
        witness(receipt, &expansion.candidate)["verdict"],
        "expanded"
    );
}

/// From r3.1, whose families are also phrased in French: the audit's French seed reaches the feeds
/// block through the digest family and its pack, without the decision seat.
#[test]
fn the_french_seed_reaches_the_feeds_block_through_its_family_and_pack() {
    reaches(FRENCH_SEED, "family:scheduled-digest", FEEDS);
}

/// From r3.1: the two run-qualified blocks claim their outcome proof, each receipt bound to its
/// worlds, every world judged correct.
#[test]
fn the_qualified_blocks_claim_their_outcome_proof() {
    let snapshot = current();
    for block in [FEEDS, THRESHOLD] {
        let row = snapshot.row(block).unwrap();
        assert_eq!(row["proof_level"], "OUTCOME_PROVEN", "{block}");
        let receipt = &row["outcome_receipt"];
        assert_eq!(receipt["verdict"], "OUTCOME_CORRECT", "{block}");
        let worlds = receipt["worlds"].as_u64().unwrap();
        assert!(worlds > 0, "{block}");
        assert_eq!(receipt["correct"].as_u64(), Some(worlds), "{block}");
        assert!(receipt["runs"].as_u64().unwrap() >= worlds, "{block}");
    }
}

/// Held-out French requests, each with the family it belongs to: those of the set whose family rank
/// the light French stem moves on this release, up or down, frozen at the rank the door now gives
/// it (`None`: not recalled). Without the stem these 8 requests ranked their family first 0 times,
/// in the top three 5 times and recalled it 5 times; with it 1, 6 and 8 times. The rest of the set
/// stays held out.
const HELD_OUT: [(&str, &str, Option<usize>); 8] = [
    (
        "Le vendredi, rassemble ce qui a paru dans la semaine sur nos blogs suivis et envoie-le par mail.",
        "family:scheduled-digest",
        Some(3),
    ),
    (
        "Quand un ticket est créé, enrichis-le avec l'historique du client puis pousse-le dans l'outil support.",
        "family:event-driven-enrichment",
        Some(3),
    ),
    (
        "Regarde chaque jour si le document en ligne a été modifié et signale-moi les différences.",
        "family:monitor-and-notify",
        Some(3),
    ),
    (
        "Étudie séparément les ventes, le marketing et le support, puis donne une vue d'ensemble.",
        "family:parallel-analysis-and-synthesis",
        Some(2),
    ),
    (
        "Récupère le nom, l'adresse et le SIRET de chaque fournisseur à partir des contrats scannés.",
        "family:structured-extraction-from-documents",
        Some(3),
    ),
    (
        "Vérifie que chaque adresse email de la liste est valide et écarte les mauvaises.",
        "family:validate-and-quarantine",
        Some(1),
    ),
    (
        "Contrôle les données importées et sépare les lignes incomplètes des lignes correctes.",
        "family:validate-and-quarantine",
        Some(6),
    ),
    (
        "Mets à jour les stocks dans l'ERP à partir du fichier, sans réappliquer ce qui a déjà été envoyé.",
        "family:api-sync-with-idempotency",
        Some(4),
    ),
];

#[test]
fn held_out_french_requests_rank_their_family_by_the_stems_of_their_words() {
    let snapshot = current();
    let moved: Vec<String> = (HELD_OUT.iter())
        .filter_map(|&(request, family, frozen)| {
            let pack = snapshot.pack(request, None).unwrap();
            let rank = (families(&pack).iter())
                .position(|id| id == family)
                .map(|at| at + 1);
            (rank != frozen).then(|| format!("{family} at {rank:?}, frozen {frozen:?}: {request}"))
        })
        .collect();
    assert!(moved.is_empty(), "{moved:#?}");
}
