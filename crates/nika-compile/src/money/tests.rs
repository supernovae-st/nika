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
