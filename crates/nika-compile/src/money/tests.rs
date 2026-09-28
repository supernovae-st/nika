// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{INVALID, directives, parse};

const WORK: &str =
    "Read ./tickets.csv, keep only the rows whose status is open and write them to ./open.csv";

/// The directive texts a request states, with their amount.
fn read(request: &str) -> (Vec<&str>, Option<f64>) {
    let found = directives(request).unwrap();
    let texts = found
        .found
        .iter()
        .map(|d| &request[d.span.clone()])
        .collect();
    (texts, found.money.amount)
}

#[test]
fn equivalent_spellings_state_one_ceiling_in_exact_spans() {
    for (suffix, text) in [
        (". Budget: $0.", "Budget: $0"),
        (", budget=0", "budget=0"),
        (", budget: 0", "budget: 0"),
        (". Budget 0 USD.", "Budget 0 USD"),
        (", 0 USD", "0 USD"),
        (". Plafond de 0 dollars.", "Plafond de 0 dollars"),
        (" --max-cost-usd 0", "--max-cost-usd 0"),
        (" --max-cost-usd=0", "--max-cost-usd=0"),
        (" with a budget of 0 USD", "with a budget of 0 USD"),
        (", budget:0,00", "budget:0,00"),
        (" budget 0 USD", "budget 0 USD"),
    ] {
        let request = format!("{WORK}{suffix}");
        assert_eq!(read(&request), (vec![text], Some(0.0)), "{request}");
    }
    let positive = format!("{WORK}. Budget: $5.");
    assert_eq!(read(&positive), (vec!["Budget: $5"], Some(5.0)));
    let found = directives(&positive).unwrap();
    assert!(found.found[0].currency);
    assert_eq!(found.found[0].anchor.as_deref(), Some("budget"));
    let compact = directives(&format!("{WORK}, budget=0")).unwrap();
    assert!(!compact.found[0].currency);
    assert_eq!(compact.found[0].anchor.as_deref(), Some("budget"));
}

#[test]
fn business_words_quotes_and_paths_are_data() {
    for request in [
        "Read ./t.csv, keep only the rows whose budget is above 15 and write them to ./b.csv",
        "Read ./t.csv, keep only the rows whose budget is 15 and write them to ./b.csv",
        "Read ./t.csv, keep only the rows whose price is under $5 and write them to ./b.csv",
        "Read ./t.csv and keep the rows whose price is under $5",
        "Read the file budget 2026.csv and write it to ./out.csv",
        "Read ./budget=0.csv and write it to ./out.csv",
        "Write \"Budget: $0\" to ./x.txt",
        "Write « budget=0 » to ./x.txt",
        "Budget: see the attached file and write it to ./x.txt",
        "Read ./t.csv and write it to ./b.csv with a budget of 5",
        "Read ./t.csv and keep the rows whose budget is 15",
    ] {
        assert_eq!(read(request), (vec![], None), "{request}");
    }
    // The whole-line reading is unchanged: it still refuses what it always refused.
    assert_eq!(
        parse("Read the file budget 2026.csv and write it to ./out.csv").err(),
        Some(INVALID)
    );
}

#[test]
fn malformed_and_conflicting_directives_refuse() {
    for suffix in [
        ". Budget: $abc.",
        ", budget=abc",
        ". Budget: -1.",
        ". Budget: NaN.",
        ". Budget: inf.",
        ", budget=1e999",
    ] {
        let request = format!("{WORK}{suffix}");
        assert_eq!(directives(&request).err(), Some(INVALID), "{request}");
    }
    let conflict = format!("{WORK}. Budget: $1. Cap: $2.");
    assert!(directives(&conflict).is_err_and(|e| e.contains("different monetary ceilings")));
    let agree = format!("{WORK}. Budget: $1. Cap: 1 USD.");
    assert_eq!(read(&agree), (vec!["Budget: $1", "Cap: 1 USD"], Some(1.0)));
}

#[test]
fn spans_are_byte_exact_after_multibyte_text_and_a_money_only_line_is_named() {
    let request = "Lis « le rapport » et écris ./résumé.txt. Budget: $0.";
    let found = directives(request).unwrap();
    assert_eq!(&request[found.found[0].span.clone()], "Budget: $0");
    assert!(!found.money.money_only);
    let alone = directives("Budget: $0.").unwrap();
    assert!(alone.money.money_only);
    assert!(!directives(WORK).unwrap().money.money_only);
}

#[test]
fn compact_currency_keeps_its_validation_and_trailing_directive_identity() {
    for (request, directive, amount) in [
        (
            "What can you tell me about stars and budget=0.5USD?",
            "budget=0.5USD",
            0.5,
        ),
        (
            "What can you tell me about stars and budget:0,50USD?",
            "budget:0,50USD",
            0.5,
        ),
        ("What can you tell me about stars, 2USD?", "2USD", 2.0),
        ("Copie ./in.txt vers ./out.txt, 0USD.", "0USD", 0.0),
        ("Copy ./in.txt to ./out.txt, budget=$0", "budget=$0", 0.0),
    ] {
        assert_eq!(read(request), (vec![directive], Some(amount)), "{request}");
    }
    for request in [
        "What can you tell me about stars, 0.5oopsUSD?",
        "Copy ./in.txt to ./out.txt, budget 0.5oopsUSD",
        "Copy ./in.txt to ./out.txt and budget=NaN.fooUSD",
    ] {
        assert_eq!(directives(request).err(), Some(INVALID), "{request}");
    }
    for request in [
        "Read ./t.csv and keep the rows whose price is under 2USD",
        "Read ./t.csv and keep rows whose price is 2USD",
        "What can you tell me about stars and 2USD?",
        "What can you tell me about stars and 0.5oopsUSD.txt?",
        "What can you tell me about stars and ./budget=0.5oopsUSD?",
        "Write \"budget=0.5oopsUSD\" and `2USD` to ./x.txt",
    ] {
        assert_eq!(read(request), (vec![], None), "{request}");
    }
}

#[test]
fn a_conjoined_business_range_never_loses_its_upper_bound_to_a_ceiling() {
    for request in [
        "Read ./prices.csv and keep rows whose price is between 1USD and 2USD",
        "Lis ./prix.csv et garde les lignes dont le prix est entre 1USD et 2USD",
    ] {
        assert_eq!(read(request), (vec![], None), "{request}");
    }
}

#[test]
fn a_money_segment_keeps_conjunctions_missing_amounts_and_default_references() {
    for clause in [
        "budget 1 dollar et plafond 2 dollars",
        "budget 0,50 dollar et 2 USD",
        "budget 1 USD and cap 2 USD",
        "budget",
        "budget nope dollars",
        "budget of nope USD",
    ] {
        assert!(
            directives(&format!("{WORK}, {clause}")).is_err(),
            "{clause}"
        );
    }
    let found = directives(&format!(
        "{WORK}, budget 2 dollars, remplace explicitement mon défaut de 1 dollar"
    ))
    .unwrap();
    assert_eq!(found.money.amount, Some(2.0));
    assert_eq!(found.money.replaced_default, Some(1.0));
    assert_eq!(
        read(&format!("{WORK}, budget report.txt dollars")),
        (vec![], None)
    );
}

/// What a request states about money: every money-shaped word is business data, one directive
/// was stated (its exact text and amount), or the stated money refuses.
#[derive(Clone, Copy, Debug)]
enum Stated {
    Data,
    Directive(&'static str, f64),
    Refused,
}

/// The frozen B15 EN/FR public matrix (R4 · frozen before any run): twelve scenarios, each in
/// English then French, over `./data/input.csv`. Their business outcome is judged end to end
/// (`nika-cli` `compile_money_doors`); here only what the request states about money.
const MATRIX: [(&str, &str, Stated); 24] = [
    (
        "S01-EN",
        "read ./data/input.csv, keep the rows where cost is under 5 USD, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "S01-FR",
        "lis ./data/input.csv, garde les lignes où cost est inférieur à 5 USD, écris-les dans ./out/result.json",
        Stated::Data,
    ),
    (
        "S02-EN",
        "read ./data/input.csv, keep the rows whose budget is 1500 USD, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "S02-FR",
        "lis ./data/input.csv, garde les lignes dont le budget est 1500 USD, écris-les dans ./out/result.json",
        Stated::Data,
    ),
    (
        "S03-EN",
        "read ./data/input.csv, keep the rows whose budget is over 1000 USD, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "S03-FR",
        "lis ./data/input.csv, garde les lignes dont budget dépasse 1000 USD, écris-les dans ./out/result.json",
        Stated::Data,
    ),
    (
        "S04-EN",
        "read ./data/input.csv, keep the rows whose budget is between 1000 USD and 1600 USD, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "S04-FR",
        "lis ./data/input.csv, garde les lignes dont le budget est entre 1000 USD et 1600 USD, écris-les dans ./out/result.json",
        Stated::Data,
    ),
    (
        "S05-EN",
        "read ./data/input.csv, keep the rows where cost is under $5, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "S05-FR",
        "lis ./data/input.csv, garde les lignes où plafond=3USD, écris-les dans ./out/result.json",
        Stated::Data,
    ),
    (
        "S06-EN",
        "read ./data/input.csv, keep the rows where price is under 15 USD, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "S06-FR",
        "lis ./data/input.csv, garde les lignes dont le montant est supérieur à 100 dollars, écris-les dans ./out/result.json",
        Stated::Data,
    ),
    (
        "S07-EN",
        "read ./data/input.csv, keep the rows with a budget of 1500 USD, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "S07-FR",
        "lis ./data/input.csv, garde les lignes avec un budget de 1500 USD, écris-les dans ./out/result.json",
        Stated::Data,
    ),
    (
        "S08-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json, budget 0 USD",
        Stated::Directive("budget 0 USD", 0.0),
    ),
    (
        "S08-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json, plafond de 0 dollar",
        Stated::Directive("plafond de 0 dollar", 0.0),
    ),
    (
        "S09-EN",
        "read ./data/input.csv, keep the rows whose budget is 1500 USD, write them to ./out/result.json. Budget: 0 USD.",
        Stated::Directive("Budget: 0 USD", 0.0),
    ),
    (
        "S09-FR",
        "lis ./data/input.csv, garde les lignes dont le budget est 1500 USD, écris-les dans ./out/result.json. Budget : 0 dollar.",
        Stated::Directive("Budget : 0 dollar", 0.0),
    ),
    (
        "S10-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json with a budget of 2 USD",
        Stated::Directive("with a budget of 2 USD", 2.0),
    ),
    (
        "S10-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json avec un plafond de 2 dollars",
        Stated::Directive("avec un plafond de 2 dollars", 2.0),
    ),
    (
        "S11-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json. Budget: 1 USD. Cap: 2 USD.",
        Stated::Refused,
    ),
    (
        "S11-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json. Budget : 1 dollar. Plafond : 2 dollars.",
        Stated::Refused,
    ),
    (
        "S12-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json. Budget: $abc.",
        Stated::Refused,
    ),
    (
        "S12-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json avec un plafond de $NaN",
        Stated::Refused,
    ),
];

/// The frozen separating cases (R4 B15): quoted and path text, a business money action, a
/// relative clause a conjunction continues, the heads that attach a phrase to the work, the
/// French copula alone and inside a relative clause, `cost` in its own sentence, and (v1.1)
/// negative business values apart from a genuine negative directive (Addon V4) and a consent
/// line's explicit amendment (M7).
const SEPARATING: [(&str, &str, Stated); 16] = [
    (
        "X01-EN quoted",
        "write \"Budget: $0\" to ./out/note.txt",
        Stated::Data,
    ),
    (
        "X02-FR quoted",
        "écris « plafond : 3 dollars » dans ./out/note.txt",
        Stated::Data,
    ),
    (
        "X03 path",
        "read ./budget=0.csv and write it to ./out/copy.csv",
        Stated::Data,
    ),
    (
        "X04-EN business action",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, then refund the cost of 50 USD to each customer",
        Stated::Data,
    ),
    (
        "X05-FR business action",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, puis accorde à chaque client un budget de 50 dollars",
        Stated::Data,
    ),
    (
        "X06 governed conjunction",
        "read ./data/input.csv, keep the rows whose status is open and budget=1500USD, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "X07 greeting head",
        "hello budget 2 USD",
        Stated::Directive("budget 2 USD", 2.0),
    ),
    (
        "X08-FR standalone copula",
        "Copie ./in.txt vers ./out.txt. Le budget est de 2 dollars.",
        Stated::Directive("Le budget est de 2 dollars", 2.0),
    ),
    (
        "X09-FR relative copula",
        "lis ./data/input.csv, garde les lignes dont le budget est de 2 dollars, écris-les dans ./out/result.json",
        Stated::Data,
    ),
    (
        "X10 cost sentence",
        "Summarize ./notes/a.md into ./out/a.md. The cost is 5 USD.",
        Stated::Data,
    ),
    (
        "X11 zero copy",
        "Copy ./data/input.csv to ./out/copy.csv, budget 0 USD",
        Stated::Directive("budget 0 USD", 0.0),
    ),
    (
        "X12-EN negative business value",
        "read ./data/input.csv, keep the rows whose budget is -5 USD, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "X13-FR negative business value",
        "lis ./data/input.csv, garde les lignes dont le budget est inférieur à -10 dollars, écris-les dans ./out/result.json",
        Stated::Data,
    ),
    (
        "X14 negative directive",
        "Copy ./data/input.csv to ./out/copy.csv. Budget: -1 USD.",
        Stated::Refused,
    ),
    (
        "X15 consent amendment",
        "yes but budget 0 dollars",
        Stated::Directive("budget 0 dollars", 0.0),
    ),
    (
        "X16 malformed consent amendment",
        "yes but budget NaN dollars",
        Stated::Refused,
    ),
];

/// Every case whose reading differs from its frozen expectation, with what the law read.
fn misread(cases: &[(&str, &str, Stated)]) -> Vec<String> {
    let mut wrong = Vec::new();
    for &(id, request, expected) in cases {
        let got = directives(request).map(|found| {
            let texts: Vec<String> = found
                .found
                .iter()
                .map(|d| request[d.span.clone()].to_owned())
                .collect();
            (texts, found.money.amount)
        });
        let bits = |amount: &Option<f64>| amount.map(f64::to_bits);
        let right = match (expected, &got) {
            (Stated::Data, Ok((texts, amount))) => texts.is_empty() && amount.is_none(),
            (Stated::Directive(text, value), Ok((texts, amount))) => {
                texts.as_slice() == [text] && bits(amount) == Some(value.to_bits())
            }
            (Stated::Refused, Err(_)) => true,
            _ => false,
        };
        if !right {
            wrong.push(format!("{id}: expected {expected:?}, read {got:?}"));
        }
    }
    wrong
}

#[test]
fn the_frozen_matrix_reads_money_by_the_role_of_its_words() {
    let wrong = misread(&MATRIX);
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn separating_cases_keep_business_money_apart_from_explicit_directives() {
    let wrong = misread(&SEPARATING);
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// A skeleton's name opening its segment is the work named whole: the ceiling beside it is the
/// work's, and the name itself is never an amount (« 01-hello »). Inside a clause the same
/// word is business data (primary review of 73291db3d, hypothesis 1).
const SKELETON_HEADS: [(&str, &str, Stated); 5] = [
    (
        "K1 template name",
        "chain budget 0 USD",
        Stated::Directive("budget 0 USD", 0.0),
    ),
    (
        "K2 numbered skeleton",
        "01-hello budget 0 USD",
        Stated::Directive("budget 0 USD", 0.0),
    ),
    (
        "K3 hyphenated template",
        "agent-loop budget 2 USD",
        Stated::Directive("budget 2 USD", 2.0),
    ),
    (
        "K4 template word inside a clause",
        "read ./data/input.csv, keep the rows of the supply chain budget 1500 USD, write them to ./out/result.json",
        Stated::Data,
    ),
    (
        "K5 template sentence",
        "Copy ./a.txt to ./out/a.txt. fanout budget 2 USD.",
        Stated::Directive("budget 2 USD", 2.0),
    ),
];

#[test]
fn a_skeleton_name_heads_its_ceiling_and_is_never_an_amount() {
    let wrong = misread(&SKELETON_HEADS);
    assert!(wrong.is_empty(), "{wrong:#?}");
}
