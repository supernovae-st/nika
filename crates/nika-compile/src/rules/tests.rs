// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The `rules` batteries, a child module of [`super`] so they run under `--lib`; in their
//! own file because the file-LOC gate measures `wc -l` and does not subtract `#[cfg(test)]`.
use super::*;

fn cols(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

fn jq(text: &str) -> Option<String> {
    synthesize(text, &[]).map(|r| r.jq())
}

fn jq_with(text: &str, columns: &[&str]) -> Option<String> {
    synthesize(text, &cols(columns)).map(|r| r.jq())
}

#[test]
fn a_trailing_count_or_total_request_is_the_summary_stage() {
    let hint = cols(&["order_id", "customer", "amount", "status"]);
    let folded = "keep only the rows whose amount is strictly greater than 100 and how many rows were kept and the total of their amounts";
    let rule = synthesize(folded, &hint).expect("a rule");
    assert!(rule.summary());
    assert_eq!(
        rule.jq(),
        "[.records[] | select((.amount | tonumber) > 100)]"
    );
    assert_eq!(rule.to_json()["summary"], true);
    let french = "garde les lignes dont le montant est plus grand que 100 et le nombre de lignes gardées et le total de leurs montants";
    assert!(synthesize(french, &[]).expect("a rule").summary());
    let sentence = "amount > 100. Count how many rows were kept";
    assert!(synthesize(sentence, &[]).expect("a rule").summary());
    // The live seat's paraphrase beside the promoted constraint: one clause, summary.
    let paraphrase = "filter rows with amount strictly greater than 100 and compute count plus total amount ; keep only the rows whose amount is strictly greater than 100";
    let rule = synthesize(paraphrase, &hint).expect("a rule");
    assert!(rule.summary());
    assert_eq!(
        rule.jq(),
        "[.records[] | select((.amount | tonumber) > 100)]"
    );
    assert_eq!(rule.to_json()["clauses"].as_array().map(Vec::len), Some(1));
    assert!(!synthesize("amount > 100", &[]).expect("a rule").summary());
    for none in [
        "how many rows were kept and the total of their amounts",
        "amount > 100 or how many rows were kept",
        "amount > 100 and how many rows were kept and whose status is open",
        "amount > 100 and the total per country",
        "keep only the rows whose status is \"shipped\" and whose total_eur is above 120. Write the count of those orders per country as JSON",
    ] {
        assert_eq!(synthesize(none, &hint), None, "{none}");
    }
}

#[test]
fn a_numeric_rule_in_six_languages_becomes_one_select() {
    let gt = Some("[.records[] | select((.amount | tonumber) > 100)]".to_owned());
    assert_eq!(
        jq("keep only the rows whose amount is strictly greater than 100"),
        gt
    );
    assert_eq!(jq("whose amount is above 100"), gt);
    assert_eq!(jq("amount > 100"), gt);
    assert_eq!(jq("rows where amount > 100"), gt);
    assert_eq!(jq("amount>100"), gt);
    assert_eq!(
        jq("dont le montant est plus grand que 200"),
        Some("[.records[] | select((.montant | tonumber) > 200)]".to_owned())
    );
    assert_eq!(
        jq("les lignes dont le montant est strictement supérieur à 200 €"),
        Some("[.records[] | select((.montant | tonumber) > 200)]".to_owned())
    );
    assert_eq!(
        jq("cuya cantidad es menor que 10"),
        Some("[.records[] | select((.cantidad | tonumber) < 10)]".to_owned())
    );
    assert_eq!(
        jq("las filas cuyo total es mayor o igual a 50"),
        Some("[.records[] | select((.total | tonumber) >= 50)]".to_owned())
    );
    assert_eq!(
        jq("le righe la cui quantità è minore di 5"),
        Some("[.records[] | select((.[\"quantità\"] | tonumber) < 5)]".to_owned())
    );
    assert_eq!(
        jq("as linhas cuja quantidade é menor que 10"),
        Some("[.records[] | select((.quantidade | tonumber) < 10)]".to_owned())
    );
    assert_eq!(
        jq("die Zeilen, deren Betrag größer als 100 ist"),
        Some("[.records[] | select((.Betrag | tonumber) > 100)]".to_owned())
    );
    assert_eq!(
        jq("whose total_eur is at least 120 EUR"),
        Some("[.records[] | select((.total_eur | tonumber) >= 120)]".to_owned())
    );
    assert_eq!(
        jq("whose total_eur is at most 12.5"),
        Some("[.records[] | select((.total_eur | tonumber) <= 12.5)]".to_owned())
    );
    assert_eq!(
        jq("montant ≥ 1,5"),
        Some("[.records[] | select((.montant | tonumber) >= 1.5)]".to_owned())
    );
    assert_eq!(
        jq("total_eur >= 1,000"),
        Some("[.records[] | select((.total_eur | tonumber) >= 1000)]".to_owned())
    );
    assert_eq!(
        jq("whose qty is 0"),
        Some("[.records[] | select((.qty | tonumber) == 0)]".to_owned())
    );
    assert_eq!(
        jq("whose qty is not 0"),
        Some("[.records[] | select((.qty | tonumber) != 0)]".to_owned())
    );
}

#[test]
fn a_columns_hint_names_the_field_in_its_own_spelling() {
    let hint = ["order_id", "customer", "Amount", "status", "unit price"];
    assert_eq!(
        jq_with("keep the rows with amount above 100", &hint),
        Some("[.records[] | select((.Amount | tonumber) > 100)]".to_owned())
    );
    assert_eq!(
        jq_with("whose unit price is below 3", &hint),
        Some("[.records[] | select((.[\"unit price\"] | tonumber) < 3)]".to_owned())
    );
    assert_eq!(
        jq_with("whose unit_price is below 3", &hint),
        Some("[.records[] | select((.[\"unit price\"] | tonumber) < 3)]".to_owned())
    );
    assert_eq!(
        jq_with("products with fewer than 10 units", &["sku", "units"]),
        Some("[.records[] | select((.units | tonumber) < 10)]".to_owned())
    );
    // A word outside the hint is not a column: the human is asked.
    assert_eq!(jq_with("whose total is above 100", &hint), None);
    assert_eq!(jq_with("whose amount_eur is above 100", &hint), None);
}

#[test]
fn an_equality_rule_keeps_the_exact_case_of_its_value() {
    assert_eq!(
        jq("whose status is refunded"),
        Some("[.records[] | select(.status == \"refunded\")]".to_owned())
    );
    assert_eq!(
        jq("whose status is \"Refunded\""),
        Some("[.records[] | select(.status == \"Refunded\")]".to_owned())
    );
    assert_eq!(
        jq("dont le statut est « expédié »"),
        Some("[.records[] | select(.statut == \"expédié\")]".to_owned())
    );
    assert_eq!(
        jq("cuyo estado no es reembolsado"),
        Some("[.records[] | select(.estado != \"reembolsado\")]".to_owned())
    );
    assert_eq!(
        jq("whose status is not 'shipped'"),
        Some("[.records[] | select(.status != \"shipped\")]".to_owned())
    );
    assert_eq!(
        jq("whose status equals shipped"),
        Some("[.records[] | select(.status == \"shipped\")]".to_owned())
    );
    assert_eq!(
        jq("status = shipped"),
        Some("[.records[] | select(.status == \"shipped\")]".to_owned())
    );
    assert_eq!(
        jq("whose country is different from FR"),
        Some("[.records[] | select(.country != \"FR\")]".to_owned())
    );
    assert_eq!(
        jq("deren Status ungleich offen"),
        Some("[.records[] | select(.Status != \"offen\")]".to_owned())
    );
}

#[test]
fn two_columns_compare_when_both_name_columns() {
    assert_eq!(
        jq("la cui quantita e inferiore alla soglia_minima"),
        Some(
            "[.records[] | select((.quantita | tonumber) < (.soglia_minima | tonumber))]"
                .to_owned()
        )
    );
    assert_eq!(
        jq_with(
            "cuja quantidade é menor que minimo",
            &["sku", "nome", "quantidade", "minimo"]
        ),
        Some("[.records[] | select((.quantidade | tonumber) < (.minimo | tonumber))]".to_owned())
    );
    assert_eq!(
        jq("whose stock_qty is below reorder_level"),
        Some(
            "[.records[] | select((.stock_qty | tonumber) < (.reorder_level | tonumber))]"
                .to_owned()
        )
    );
    // A bare word right of a numeric comparison is not a column.
    assert_eq!(jq("whose quantity is below minimum"), None);
    assert_eq!(jq("whose stock_qty is below stock_qty"), None);
}

#[test]
fn clauses_join_through_one_conjunction() {
    assert_eq!(
        jq("keep only the rows whose status is \"shipped\" and whose total_eur is above 120"),
        Some(
            "[.records[] | select(.status == \"shipped\" and (.total_eur | tonumber) > 120)]"
                .to_owned()
        )
    );
    assert_eq!(
        jq("amount > 100 or amount < 10"),
        Some(
            "[.records[] | select((.amount | tonumber) > 100 or (.amount | tonumber) < 10)]"
                .to_owned()
        )
    );
    assert_eq!(
        jq("dont le montant est plus grand que 100 et dont le statut est ouvert"),
        Some(
            "[.records[] | select((.montant | tonumber) > 100 and .statut == \"ouvert\")]"
                .to_owned()
        )
    );
    assert_eq!(
        jq("cuya cantidad es menor que 10 y cuyo estado es activo"),
        Some(
            "[.records[] | select((.cantidad | tonumber) < 10 and .estado == \"activo\")]"
                .to_owned()
        )
    );
    // Two promoted rules joined by the plan's separator.
    assert_eq!(
        jq("amount > 100 ; whose status is open"),
        Some(
            "[.records[] | select((.amount | tonumber) > 100 and .status == \"open\")]".to_owned()
        )
    );
    // Mixed junctions have no fixed precedence in prose: the human is asked.
    assert_eq!(jq("amount > 100 and amount < 10 or qty > 3"), None);
    assert_eq!(jq("amount > 100 or qty > 3 ; status = open"), None);
    assert_eq!(jq("amount > 100 and"), None);
}

#[test]
fn what_the_grammar_does_not_cover_is_none() {
    for text in [
        "a brief of under 150 words",
        "au plus 12 lignes",
        "at most 3 attempts",
        "Process at most 2 products at a time",
        "products with fewer than 10 units",
        "quantité inférieure à 10",
        "au moins 3 articles en stock",
        "the top 3 countries",
        "in a warm tone",
        "keep only the rows whose status is \"shipped\" and whose total_eur is above 120. Write the count of those orders per country as JSON",
        "whose amount is above 100 per country",
        "whose status is refunded today",
        "the 3 rows whose amount > 100",
        "whose status is shipped or refunded",
        "whose amount is between 10 and 20",
        "whose amount is not above 100",
        "whose amount > \"high\"",
        "",
    ] {
        assert_eq!(synthesize(text, &[]), None, "{text}");
    }
}

#[test]
fn the_guard_names_every_column_and_the_record_is_observational() {
    let rule = synthesize(
        "whose status is \"shipped\" and whose total_eur is above stock_min",
        &[],
    )
    .expect("a rule");
    assert_eq!(rule.fields(), ["status", "total_eur", "stock_min"]);
    assert_eq!(
        rule.guard(),
        "(.records | type) == \"array\" and ((.records | length) == 0 or (.records[0] | type == \"object\" and has(\"status\") and has(\"total_eur\") and has(\"stock_min\")))"
    );
    assert!(
        rule.guard_message()
            .contains("`status`, `total_eur`, `stock_min`")
    );
    let record = rule.to_json();
    assert_eq!(record["synthesized"], true);
    assert_eq!(record["junction"], "and");
    assert_eq!(record["clauses"][1]["value_kind"], "column");
    assert!(record.get("field").is_none());
    let one = synthesize("whose amount is above 100", &[]).expect("a rule");
    let record = one.to_json();
    assert_eq!(record["field"], "amount");
    assert_eq!(record["comparator"], ">");
    assert_eq!(record["value"], "100");
    assert_eq!(record["text"], "whose amount is above 100");
    assert_eq!(key("unit price"), ".[\"unit price\"]");
    assert_eq!(key("_a1"), "._a1");
}

#[test]
fn numbers_fold_currency_and_separators() {
    assert_eq!(number("100").as_deref(), Some("100"));
    assert_eq!(number("€100").as_deref(), Some("100"));
    assert_eq!(number("100€").as_deref(), Some("100"));
    assert_eq!(number("15%").as_deref(), Some("15"));
    assert_eq!(number("-3").as_deref(), Some("-3"));
    assert_eq!(number("1,5").as_deref(), Some("1.5"));
    assert_eq!(number("1,000").as_deref(), Some("1000"));
    assert_eq!(number("1,000,000").as_deref(), Some("1000000"));
    assert_eq!(number("12.50").as_deref(), Some("12.50"));
    for not in ["abc", "1.2.3", "1,2,3", "T-4471", "", "1.000,50"] {
        assert_eq!(number(not), None, "{not}");
    }
}

// wave28 v2-02: the seat compared a boolean column to the string "false" and jq kept no row;
// the run was green on an empty file.
#[test]
fn a_truth_value_matches_the_boolean_and_its_spelling() {
    let computation = serde_json::from_value::<crate::predicate::ProposedComputation>(json!({
        "present": true, "polarity": "keep", "join": "and",
        "clauses": [{"field": "explicito", "op": "==", "value": "false", "value_field": ""}],
        "group_by": "", "aggregations": [], "sort_by": "duracao_s", "order": "desc",
        "columns": ["id", "titulo"], "derived": []
    }))
    .unwrap();
    let intent = "Lê ./musica/faixas.json (id, titulo, artista, duracao_s, explicito), guarda as faixas com explicito == false ordenadas por duracao_s decrescente e escreve id e titulo em ./out/limpa.json";
    let rule = crate::predicate::typed_rule(
        intent,
        "guarda as faixas com explicito == false",
        &computation,
    )
    .expect("a rule");
    assert!(
        rule.jq()
            .contains("select((.explicito == false or .explicito == \"false\"))"),
        "{}",
        rule.jq()
    );
    let record = rule.to_json();
    assert_eq!(record["clauses"][0]["value_kind"], "bool");
    assert_eq!(record["clauses"][0]["value"], "false");
    let back = Rule::from_json(&record).expect("round trip");
    assert_eq!(back.jq(), rule.jq());
    // « != vero » excludes the truth in both encodings; another comparator stays literal.
    let clause = Clause {
        field: "attivo".to_owned(),
        comparator: Comparator::Ne,
        value: Operand::Bool(true),
    };
    assert_eq!(clause.jq(), "(.attivo != true and .attivo != \"true\")");
}
