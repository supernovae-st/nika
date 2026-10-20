// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::panic)]

use super::*;

#[test]
fn cached_verification_belongs_to_every_offer_and_the_question_identity() {
    let root = tempfile::tempdir().unwrap();
    let source = "nika: hello\nsteps:\n  - id: hi\n    exec: printf hello\n";
    let stated = "print hello";
    let answers = BTreeMap::<String, String>::new();
    let original: Option<&str> = None;
    let presentation = json!({"kind":"proposal", "selected":null, "pending_choices":[{
        "id":"question-one", "key":"format", "question":"Choose the format", "state":"open",
        "options":[{"key":"md", "label":"Markdown", "recommended":true, "values":[]},
                   {"key":"html", "label":"Web page", "recommended":false, "values":[]}]}]});
    let at = landing(root.path(), source, (None, &BTreeMap::new()), None).unwrap();
    let identity = json!([source, stated, [], [], answers, original, at, presentation]);
    let key = blake3::hash(identity.to_string().as_bytes())
        .to_hex()
        .to_string();
    let mut out = nika_compile::initial();
    out.status = CompileStatus::Ready;
    out.provenance.decision = Some(json!({"semantic_verification": [{
        "candidate_sha256": nika_compile::surface::sha256(source), "request": stated,
        "context_sha256": "c".repeat(64), "consumed": 1, "settled": true
    }]}));
    let judged = Judged::ready("cached scope".into(), source, &out);
    let mut desk = SessionDesk {
        ceiling: super::Ceiling::default(),
        root: root.path().to_owned(),
        saved: None,
        probes: vec![],
        knowledge: None,
        verifier: Verifier {
            seat: AuthoringSeat::Deterministic { why: None },
            context: AuthoringContext::default(),
            account: None,
            kept: Arc::new(Mutex::new(Verified {
                ready: Some((key, judged.clone(), Some("cached doubt".into()))),
                ..Verified::default()
            })),
            trial: (
                None,
                Arc::new(|_| panic!("the cache test cannot observe a remote source")),
            ),
        },
        admitted: None,
        judged: None,
    };
    assert_eq!(
        desk.verify(
            source,
            (stated, &answers, original),
            (&[], &[]),
            &presentation,
            None
        ),
        Ok(judged.cached())
    );
    assert_eq!(
        kept(&desk.verifier).unwrap().doubt.as_deref(),
        Some("cached doubt")
    );
    let mut other_label = presentation.clone();
    other_label["pending_choices"][0]["options"][1]["label"] = json!("HTML document");
    let mut other_value = presentation.clone();
    other_value["pending_choices"][0]["options"][1]["values"] =
        json!([{"role":"output_path", "value":"summary.html"}]);
    let mut other_question = presentation.clone();
    other_question["pending_choices"][0]["id"] = json!("question-two");
    for changed in [other_label, other_value, other_question] {
        let result = desk.verify(
            source,
            (stated, &answers, original),
            (&[], &[]),
            &changed,
            None,
        );
        assert!(
            result.is_err(),
            "a changed presentation reused the old verdict: {result:?}"
        );
    }
}

/// The person's words stating the bound ceiling (QA gauntlet q09): directives side by side are
/// quoted together with their aside's brackets, each line read as the Session reads it, and a
/// line stating another amount, or none, quotes nothing.
#[test]
fn the_words_beside_a_ceiling_are_the_persons_directives_stating_it() {
    let q09 = "Résume ./ventes.csv dans rapport.md. L'étape IA doit utiliser Claude Haiku (le moins cher) et chaque exécution doit coûter au maximum 0,10 $ (budget max 0,10 $ par run).";
    let words = ceiling_words(q09, 0.1).unwrap();
    assert_eq!(
        words,
        json!({"excerpt": "chaque exécution doit coûter au maximum 0,10 $ (budget max 0,10 $ par run)", "usd": 0.1})
    );
    let lines =
        "Résume ./notes.md, budget 2 $\nmerci\nLe plafond de 2 $, je le mettrai au lancement";
    assert_eq!(
        ceiling_words(lines, 2.0).unwrap()["excerpt"],
        "budget 2 $ · Le plafond de 2 $"
    );
    let aside = "Écris un rapport dans rapport.md (budget max 0,10 $ par run).";
    assert_eq!(
        ceiling_words(aside, 0.1).unwrap()["excerpt"],
        "(budget max 0,10 $ par run)"
    );
    assert_eq!(ceiling_words(q09, 0.25), None, "another amount");
    assert_eq!(ceiling_words("Résume ./notes.md", 0.1), None, "no words");
    // The last ceiling stated supersedes the earlier ones, which ride beside it; a ceiling
    // stated after the bound one does not (live gauntlet r09).
    let changed = "Résume ./ventes.csv, budget max 0,10 $ par run\nHaiku 5.5 pour l'étape IA, plafond 0,001 $ par run, relance";
    assert_eq!(
        ceiling_words(changed, 0.001).unwrap()["excerpt"],
        "budget max 0,10 $ par run · plafond 0,001 $ par run"
    );
    assert_eq!(
        ceiling_words(changed, 0.1).unwrap()["excerpt"],
        "budget max 0,10 $ par run",
        "the later 0,001 $ is not beside a bound 0,10 $"
    );
}

/// A schedule `nika.yaml` declares for the workflow a revision lands on rides beside it, with the
/// person's words that stated it quoted as typed, and nothing else of their line (live gauntlet
/// r02 B16).
#[test]
fn a_declared_schedule_rides_beside_a_revision_of_its_workflow() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("nika.yaml"),
        "nika: proj\narm:\n  - workflow: watch.nika\n    cadence: \"TZ=Indian/Mauritius 0 7 * * *\"\n    plafond: 0.10\n    manqué: rattraper-une-fois\n",
    )
    .unwrap();
    let stated = "Vérifie python.org Tous les matins à 7h et écris la version dans version.txt\ncorrige le cas sans changement";
    let beside = super::declared_beside(dir.path(), "watch.nika", stated).unwrap();
    assert_eq!(beside["message"], "nika.yaml");
    assert_eq!(beside["zone"], "Indian/Mauritius");
    assert_eq!(beside["excerpt"], "Tous les matins à 7h", "{beside}");
    assert!(super::declared_beside(dir.path(), "other.nika", stated).is_none());
    assert!(super::declared_beside(dir.path(), "watch.nika", "corrige-le").is_none());
}

/// The ceiling a proposal binds: the one the work's words admitted wherever it lands, else the
/// one saved with this conversation's workflow, only where that workflow lands.
#[test]
fn the_bound_ceiling_is_the_stated_one_or_the_saved_one_at_its_file() {
    let saved = Some(("news.nika".to_owned(), 0.1));
    let kept = Ceiling {
        stated: None,
        saved: saved.clone(),
    };
    assert_eq!(kept.at("news.nika"), Some(0.1));
    assert_eq!(kept.at("other.nika"), None);
    let stated = Ceiling {
        stated: Some(0.5),
        saved,
    };
    assert_eq!(stated.at("news.nika"), Some(0.5));
    assert_eq!(Ceiling::default().at("news.nika"), None);
}

/// A desk on `root` that reads, with nothing verified.
fn reading_desk(root: &std::path::Path) -> SessionDesk {
    SessionDesk {
        ceiling: super::Ceiling::default(),
        root: root.to_owned(),
        saved: None,
        probes: vec![],
        knowledge: None,
        verifier: Verifier {
            seat: AuthoringSeat::Deterministic { why: None },
            context: AuthoringContext::default(),
            account: None,
            kept: Arc::new(Mutex::new(Verified::default())),
            trial: (
                None,
                Arc::new(|_| panic!("reading observes no remote source")),
            ),
        },
        admitted: None,
        judged: None,
    }
}

/// A folder is listed, never refused as a file that is not regular, and a file that is not there
/// names what its folder holds (live gauntlet r01, 2026-10-16: fifteen names guessed in
/// `releves/`). The root lists too; hidden entries never show.
#[test]
fn a_folder_lists_and_a_missing_file_names_what_its_folder_holds() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("releves/archives")).unwrap();
    std::fs::write(root.path().join("releves/banque-oct.csv"), "a,b\n").unwrap();
    std::fs::write(root.path().join("releves/carte-oct.csv"), "a,b\n").unwrap();
    std::fs::write(root.path().join("releves/.cache"), "x").unwrap();
    std::fs::create_dir_all(root.path().join(".nika")).unwrap();
    let mut desk = reading_desk(root.path());
    let listed: Value = serde_json::from_str(&desk.read("./releves", None, None).unwrap()).unwrap();
    assert_eq!(listed["directory"], true);
    assert_eq!(
        listed["entries"],
        json!(["archives/", "banque-oct.csv", "carte-oct.csv"])
    );
    let missing = desk.read("./releves/octobre.csv", None, None).unwrap_err();
    assert_eq!(
        missing,
        "`./releves/octobre.csv` does not exist in the project; `./releves` holds: archives/, banque-oct.csv, carte-oct.csv"
    );
    let rooted: Value = serde_json::from_str(&desk.read(".", None, None).unwrap()).unwrap();
    assert_eq!(rooted["entries"], json!(["releves/"]));
    let read: Value =
        serde_json::from_str(&desk.read("releves/banque-oct.csv", None, None).unwrap()).unwrap();
    assert_eq!(read["text"], "a,b");
}
