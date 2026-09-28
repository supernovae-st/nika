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
fn a_stated_aggregate_is_the_shape_after_the_filter() {
    assert_eq!(
        jq("the total of the amount column"),
        Some(".records | {\"total\": (map(.amount | tonumber) | add // 0)}".to_owned())
    );
    assert_eq!(
        jq("the total of the amount column ; whose client is acme"),
        Some(
            "[.records[] | select(.client == \"acme\")] | {\"total\": (map(.amount | tonumber) | add // 0)}"
                .to_owned()
        )
    );
    let rule = synthesize("la moyenne de la colonne montant", &[]).expect("a rule");
    assert_eq!(rule.totals_names(), ["moyenne"]);
    assert_eq!(rule.fields(), ["montant"]);
    assert!(!rule.summary());
    // A grouping is a stage of the shape: one row per client with its total.
    let grouped = synthesize("the total of the amount column per client", &[]).expect("a rule");
    assert_eq!(
        grouped.jq(),
        ".records | group_by(.client) | map({\"client\": (.[0] | .client), \"total\": (map(.amount | tonumber) | add // 0)})"
    );
    assert_eq!(grouped.fields(), ["client", "amount"]);
    assert_eq!(
        grouped.output_columns(),
        Some(vec!["client".to_owned(), "total".to_owned()])
    );
    assert!(grouped.totals_names().is_empty());
}

#[test]
fn a_join_a_top_n_a_projection_and_a_dedup_lower_after_the_filter() {
    let join = synthesize("merge them on the id column", &[]).expect("a join");
    assert!(join.joins());
    assert_eq!(join.fields(), ["id"]);
    assert_eq!(
        join.jq(),
        ".records | reduce .[1:][] as $right (.[0]; [.[] as $a | $right[] | select(.id == ($a | .id)) | $a + .])"
    );
    assert_eq!(
        join.guard(),
        "(.records | type) == \"array\" and (.records | length) >= 2 and all(.records[]; type == \"array\" and (length == 0 or (.[0] | type == \"object\" and has(\"id\"))))"
    );
    assert!(join.guard_message().contains("joins the sources on `id`"));
    // A filter after the join selects over the joined rows.
    assert_eq!(
        jq("merge them on the id column ; whose amount is above 100"),
        Some(".records | reduce .[1:][] as $right (.[0]; [.[] as $a | $right[] | select(.id == ($a | .id)) | $a + .]) | [.[] | select((.amount | tonumber) > 100)]".to_owned())
    );
    assert_eq!(
        jq("keep the 2 rows with the highest amount"),
        Some(".records | sort_by(.amount | tonumber? // .) | reverse | .[:2]".to_owned())
    );
    assert_eq!(
        jq("whose client is acme ; keep the 2 rows with the highest amount"),
        Some("[.records[] | select(.client == \"acme\")] | sort_by(.amount | tonumber? // .) | reverse | .[:2]".to_owned())
    );
    let slim = synthesize("keep only the id and title of each ticket", &[]).expect("a projection");
    assert_eq!(
        slim.jq(),
        ".records | map({\"id\": .id, \"title\": .title})"
    );
    assert_eq!(slim.fields(), ["id", "title"]);
    assert_eq!(
        slim.output_columns(),
        Some(vec!["id".to_owned(), "title".to_owned()])
    );
    // A removal of duplicates over a text source runs over its lines and writes lines.
    let distinct = synthesize("remove the duplicate lines", &[]).expect("a dedup");
    assert!(!distinct.lines());
    let lines = distinct.over_lines().expect("over lines");
    assert!(lines.lines());
    assert_eq!(
        lines.jq(),
        ".records | reduce .[] as $r ([]; if any(.[]; . == $r) then . else . + [$r] end) | join(\"\\n\") | if length > 0 then . + \"\\n\" else . end"
    );
    assert_eq!(
        lines.guard(),
        "(.records | type) == \"array\" and all(.records[]; type == \"string\")"
    );
    assert_eq!(Rule::from_json(&lines.to_json()), Some(lines));
    // A filter, a sort or a projection has no meaning over lines: asked, never guessed.
    for text in [
        "whose status is open",
        "sort the rows by amount",
        "keep only the id and title of each ticket",
        "count the rows per client",
    ] {
        assert_eq!(
            synthesize(text, &[]).and_then(|r| r.over_lines()),
            None,
            "{text}"
        );
    }
    // Two stages of the same kind, or a dedup beside a top-N, are not one computation.
    assert_eq!(
        jq("sort the rows by amount ; sort the rows by client"),
        None
    );
    assert_eq!(
        jq("remove the duplicate lines ; keep the 2 rows with the highest amount"),
        None
    );
}

#[test]
fn a_negation_among_the_lead_words_is_read_as_nothing_never_inverted() {
    for text in [
        "do not keep the tickets whose status is closed",
        "never keep the tickets whose status is closed",
        "Read ./tickets.json, do not keep the tickets whose status is closed",
        "ne garde pas les lignes dont amount dépasse 200",
        "ne garde jamais les lignes dont amount dépasse 200",
        "don't keep rows whose amount is above 100",
        // An exclusion names what leaves: never read as a keep of those rows.
        "exclude the rows whose amount is below 100 or whose status is refunded",
        "drop the rows whose status is closed",
        "filter out the rows whose amount is above 100",
        "remove the rows whose status is closed",
        "supprime les lignes dont le montant est plus grand que 100",
    ] {
        assert_eq!(synthesize(text, &[]), None, "{text}");
    }
    // The French restriction is "only": a filter, read as stated.
    assert_eq!(
        jq("ne garde que les lignes dont amount dépasse 200"),
        Some("[.records[] | select((.amount | tonumber) > 200)]".to_owned())
    );
    // A negation after the copula is the clause's own polarity, still read.
    assert_eq!(
        jq("whose status is not closed"),
        Some("[.records[] | select(.status != \"closed\")]".to_owned())
    );
}

/// The exclusion leads before they became data (rules.rs at 4bddf8a14, sha256 15e84e0e…):
/// the asset holds exactly these words, in this order.
#[test]
fn the_exclusion_leads_are_the_frozen_list() {
    let frozen = "exclude excludes excluding drop drops remove removes delete deletes discard discards omit omits skip skips ignore ignores strip out exclus exclure excluez supprime supprimez supprimer retire retirez retirer enleve enlevez enlever elimine eliminez eliminer ignorez ecarte ecartez elimina quita descarta excluye omite rimuovi escludi scarta entferne losche verwerfe";
    let read: Vec<&str> = EXCLUSION_LEADS.lines().collect();
    assert_eq!(read, frozen.split(' ').collect::<Vec<_>>());
}

/// A stage the words before the relative clause state runs after the clause (R4 F1): « count
/// the rows where status is paid » is a filter AND a count, never the filter alone.
#[test]
fn a_counted_lead_keeps_its_filter_and_its_count() {
    let paid = "[.records[] | select(.status == \"paid\")]";
    for (text, expected) in [
        (
            "count the rows where status is paid",
            format!("{paid} | {{\"count\": length}}"),
        ),
        (
            "count the orders where status is paid",
            format!("{paid} | {{\"count\": length}}"),
        ),
        (
            "count the rows where status is paid and amount_usd is over 10",
            "[.records[] | select(.status == \"paid\" and (.amount_usd | tonumber) > 10)] | {\"count\": length}".to_owned(),
        ),
        (
            "count the rows where status is paid or where status is open",
            "[.records[] | select(.status == \"paid\" or .status == \"open\")] | {\"count\": length}".to_owned(),
        ),
        (
            "compte les lignes dont le statut est payé",
            "[.records[] | select(.statut == \"payé\")] | {\"count\": length}".to_owned(),
        ),
        (
            "the number of rows whose status is paid",
            format!("{paid} | {{\"number\": length}}"),
        ),
        (
            "count the rows per client where status is paid",
            format!(
                "{paid} | group_by(.client) | map({{\"client\": (.[0] | .client), \"count\": length}})"
            ),
        ),
        (
            "the total of the amount column where status is paid",
            format!("{paid} | {{\"total\": (map(.amount | tonumber) | add // 0)}}"),
        ),
    ] {
        assert_eq!(jq(text), Some(expected), "{text}");
    }
    let rule = synthesize("count the rows where status is paid", &[]).expect("a rule");
    assert!(rule.filters() && rule.shaped());
    assert_eq!(rule.fields(), ["status"]);
    assert_eq!(rule.totals_names(), ["count"]);
}

/// Words before the relative clause the grammar cannot account for leave the clause unread
/// (R4 F1): a stage word it does not read whole over the rows, or a modifier after a determiner,
/// is never dropped while the filter alone reads READY.
#[test]
fn a_lead_the_grammar_cannot_account_for_is_never_dropped() {
    for text in [
        "count the paid rows where amount_usd is over 10",
        "how many rows where status is paid",
        "sort the rows where status is paid",
        "sum the amounts where status is paid",
        "the total amount of the rows where status is paid",
        "the paid rows where amount_usd > 10",
        "keep the open tickets whose priority is high",
        "keep the rows with the highest amount whose status is paid",
        "les lignes payées dont le montant est plus grand que 10",
        "count total_eur > 100",
        // A stage stated after the counted clause, or before it, has no order this rule keeps.
        "keep the 2 rows with the highest amount ; count the rows where status is paid",
        "count the rows where status is paid ; keep the rows where amount > 10",
        "keep the rows where amount > 10 and count the rows where status is paid",
    ] {
        assert_eq!(synthesize(text, &[]), None, "{text}");
    }
}

/// The clause's own verb, the grammar's words and the records' noun carry nothing a filter
/// drops: the leads read before keep reading the same filter, byte for byte.
#[test]
fn a_plain_lead_reads_the_same_filter() {
    let paid = Some("[.records[] | select(.status == \"paid\")]".to_owned());
    for text in [
        "keep the rows where status is paid",
        "filter the rows where status is paid",
        "list the orders whose status is paid",
        "show me the orders whose status is paid",
        "give me rows where status is paid",
        "select the tickets whose status is paid",
        "les lignes dont le status est paid",
        "ne garde que les lignes dont le status est paid",
    ] {
        assert_eq!(jq(text), paid, "{text}");
    }
}
