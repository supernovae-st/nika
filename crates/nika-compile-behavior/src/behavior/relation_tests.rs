// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The requested relation over synthetic records: filters under the number law and its stated
//! policies, a top N as the sorted prefix with its cutoff ties left open, exact aggregates and
//! their stated (or open) empty case, groups, duplicates, projection and ordered steps. Every
//! case the request leaves open is unverified, never filled with a guess.

use nika_compile_reader::aggregate::AggOp;
use nika_compile_reader::rules::{Comparator, Junction, NumberPolicy};

use super::Operation;
use super::evaluate::{Expected, Halt, Order, Undefined, evaluate};
use super::formats::{Format, records};
use super::numbers::Decimal;
use super::pipeline::{Aggregate, Filter, OnEmpty, Operand, Pipeline, Sort, Stages, Step, Test};
use super::values::{Datum, Row, shown, value_at};

fn json(text: &str) -> Vec<Row> {
    records(Format::Json, text).expect("synthetic JSON records")
}

fn csv(text: &str) -> Vec<Row> {
    records(Format::Csv, text).expect("synthetic CSV records")
}

fn n(text: &str) -> Decimal {
    Decimal::from_law(text).expect("a number the law reads")
}

fn keep(tests: Vec<Test>) -> Filter {
    Filter::new(tests, Junction::And)
}

fn one(filter: Filter, stages: Stages) -> Pipeline {
    Pipeline::new(vec![Step::new(filter, stages)])
}

fn above(field: &str, bound: &str) -> Test {
    Test::new(field, Comparator::Gt, Operand::Number(n(bound)))
}

fn sorted(field: &str, descending: bool, stable: bool, limit: Option<u32>) -> Stages {
    Stages {
        sort: Some(Sort::new(field, descending, stable)),
        limit,
        ..Stages::default()
    }
}

/// The values of `column`, block by block.
fn blocks(expected: &Expected, column: &str) -> Vec<Vec<String>> {
    expected
        .blocks
        .iter()
        .map(|block| {
            block
                .rows
                .iter()
                .map(|row| shown(value_at(row, column)))
                .collect()
        })
        .collect()
}

/// Whether the relation is left unverified: never a guessed result, never a defect.
fn unverified(result: Result<Expected, Undefined>) -> bool {
    result.is_err_and(|undefined| matches!(undefined, Undefined::Unverified(_)))
}

#[test]
fn a_numeric_filter_reads_csv_text_by_value() {
    let rows = csv("id,amount\n1,20\n2,400\n3,50.0\n4,5\n5,10\n");
    let expected = evaluate(
        &one(keep(vec![above("amount", "10")]), Stages::default()),
        rows,
    )
    .expect("defined");
    assert_eq!(blocks(&expected, "id"), [["\"1\"", "\"2\"", "\"3\""]]);
    assert_eq!(expected.order, Order::File);
    assert!(!expected.totals);
}

#[test]
fn a_value_outside_the_domain_is_an_invalid_fixture_and_a_policy_decides_it() {
    let rows = || csv("id,amount\n1,20\n2,n/a\n");
    let mut pipeline = one(keep(vec![above("amount", "10")]), Stages::default());
    assert!(matches!(
        evaluate(&pipeline, rows()),
        Err(Undefined::OutOfDomain(_))
    ));
    pipeline
        .policies
        .insert("amount".to_owned(), NumberPolicy::Skip);
    let kept = evaluate(&pipeline, rows()).expect("skip leaves the record out");
    assert_eq!(blocks(&kept, "id"), [["\"1\""]]);
    pipeline
        .policies
        .insert("amount".to_owned(), NumberPolicy::Fail);
    // The stop names its occurrence: the filter's test on amount, and the value it may name.
    let Err(Undefined::Stops(stop)) = evaluate(&pipeline, rows()) else {
        panic!("a stated FAIL policy stops the run");
    };
    assert_eq!(
        (stop.operation, stop.field.as_str()),
        (Operation::Test, "amount")
    );
    assert_eq!(
        stop.halt,
        Halt::NotANumber(vec![Datum::Text("n/a".to_owned())])
    );
}

#[test]
fn a_truth_matches_either_encoding_and_a_text_equality_is_exact() {
    let rows = json(
        r#"[{"id":1,"active":true,"region":"north"},{"id":2,"active":"true","region":"North"},
            {"id":3,"active":false,"region":"north"},{"id":4,"active":"True","region":"nord"}]"#,
    );
    let active = Test::new("active", Comparator::Eq, Operand::Bool(true));
    let expected =
        evaluate(&one(keep(vec![active]), Stages::default()), rows.clone()).expect("defined");
    assert_eq!(blocks(&expected, "id"), [["1", "2"]]);
    let mut north = Test::new("region", Comparator::Eq, Operand::Text("north".to_owned()));
    let exact = evaluate(
        &one(keep(vec![north.clone()]), Stages::default()),
        rows.clone(),
    )
    .expect("defined");
    assert_eq!(blocks(&exact, "id"), [["1", "3"]]);
    north.spellings = vec!["nord".to_owned()];
    let spelled = evaluate(&one(keep(vec![north]), Stages::default()), rows).expect("defined");
    assert_eq!(blocks(&spelled, "id"), [["1", "3", "4"]]);
}

#[test]
fn a_junction_joins_the_tests_as_stated() {
    let rows = || {
        json(
            r#"[{"id":1,"amount":5,"vip":true},{"id":2,"amount":500,"vip":false},{"id":3,"amount":5,"vip":false}]"#,
        )
    };
    let tests = || {
        vec![
            above("amount", "100"),
            Test::new("vip", Comparator::Eq, Operand::Bool(true)),
        ]
    };
    let or = evaluate(
        &one(Filter::new(tests(), Junction::Or), Stages::default()),
        rows(),
    )
    .expect("defined");
    assert_eq!(blocks(&or, "id"), [["1", "2"]]);
    let and = evaluate(&one(keep(tests()), Stages::default()), rows()).expect("defined");
    assert_eq!(and.blocks.iter().map(|b| b.rows.len()).sum::<usize>(), 0);
}

#[test]
fn a_top_n_is_the_sorted_prefix_and_a_cut_through_ties_stays_open() {
    let rows = || {
        json(r#"[{"id":1,"amount":9},{"id":2,"amount":8},{"id":3,"amount":8},{"id":4,"amount":1}]"#)
    };
    let open = evaluate(
        &one(Filter::all(), sorted("amount", true, false, Some(2))),
        rows(),
    )
    .expect("defined");
    assert_eq!(blocks(&open, "id"), [vec!["1"], vec!["2", "3"]]);
    assert_eq!(open.size(), 2);
    assert!(open.blocks[1].partial());
    assert_eq!(open.order, Order::Sorted { stable: false });
    assert_eq!(open.tie_stop, None);
    let stable = evaluate(
        &one(Filter::all(), sorted("amount", true, true, Some(2))),
        rows(),
    )
    .expect("defined");
    assert_eq!(blocks(&stable, "id"), [vec!["1"], vec!["2"]]);
    assert!(!stable.blocks[1].partial());
    // Under a stated policy the cut through distinct tied rows has no answer: a stop is admitted.
    let mut bound = one(Filter::all(), sorted("amount", true, false, Some(2)));
    bound
        .policies
        .insert("amount".to_owned(), NumberPolicy::Fail);
    assert_eq!(
        evaluate(&bound, rows())
            .expect("defined")
            .tie_stop
            .as_deref(),
        Some("amount")
    );
}

#[test]
fn a_sort_with_no_stated_policy_orders_numbers_by_value_and_never_guesses_a_reading() {
    let rows = csv("id,amount\n1,900\n2,1000\n3,20\n");
    let expected = evaluate(
        &one(Filter::all(), sorted("amount", false, false, None)),
        rows,
    )
    .expect("defined");
    assert_eq!(blocks(&expected, "id"), [["\"3\""], ["\"1\""], ["\"2\""]]);
    let padded = csv("id,amount\n1,007\n2,10\n");
    assert!(unverified(evaluate(
        &one(Filter::all(), sorted("amount", false, false, None)),
        padded
    )));
}

#[test]
fn date_times_of_different_offsets_are_never_ordered_as_text() {
    let mixed = || {
        json(r#"[{"id":1,"at":"2031-03-01T00:30:00Z"},{"id":2,"at":"2031-03-01T02:10:00+02:00"}]"#)
    };
    assert!(unverified(evaluate(
        &one(Filter::all(), sorted("at", true, false, None)),
        mixed()
    )));
    let window = Test::new(
        "at",
        Comparator::Ge,
        Operand::Text("2031-03-01T00:00:00Z".to_owned()),
    );
    assert!(unverified(evaluate(
        &one(keep(vec![window]), Stages::default()),
        mixed()
    )));
    let same =
        json(r#"[{"id":1,"at":"2031-03-01T00:30:00Z"},{"id":2,"at":"2031-03-01T00:10:00Z"}]"#);
    let expected = evaluate(&one(Filter::all(), sorted("at", false, false, None)), same)
        .expect("one form and one offset: text order is time order");
    assert_eq!(blocks(&expected, "id"), [["2"], ["1"]]);
}

fn total(
    op: AggOp,
    field: Option<&str>,
    name: &str,
    round: Option<u32>,
    on_empty: OnEmpty,
) -> Stages {
    Stages {
        aggregates: vec![Aggregate::new(
            op,
            field.map(str::to_owned),
            name,
            round,
            on_empty,
        )],
        ..Stages::default()
    }
}

fn the_total(expected: &Expected, name: &str) -> String {
    let row = expected
        .blocks
        .first()
        .and_then(|block| block.rows.first())
        .expect("one row of totals");
    shown(value_at(row, name))
}

#[test]
fn aggregates_are_exact_and_their_empty_case_is_stated_or_left_open() {
    let empty = || json("[]");
    let sum = evaluate(
        &one(
            Filter::all(),
            total(AggOp::Sum, Some("amount"), "total", None, OnEmpty::Zero),
        ),
        empty(),
    )
    .expect("a total of nothing is zero");
    assert!(sum.totals);
    assert_eq!(the_total(&sum, "total"), "0");
    let count = evaluate(
        &one(
            Filter::all(),
            total(AggOp::Count, None, "count", None, OnEmpty::Zero),
        ),
        empty(),
    )
    .expect("a count of nothing is zero");
    assert_eq!(the_total(&count, "count"), "0");
    let mean = one(
        Filter::all(),
        total(
            AggOp::Avg,
            Some("amount"),
            "average",
            None,
            OnEmpty::Unstated,
        ),
    );
    assert!(unverified(evaluate(&mean, empty())));
    let least = one(
        Filter::all(),
        total(AggOp::Min, Some("amount"), "least", None, OnEmpty::Stops),
    );
    assert!(matches!(
        evaluate(&least, empty()),
        Err(Undefined::Stops(_))
    ));
    let thirds = || json(r#"[{"amount":1},{"amount":2},{"amount":2}]"#);
    assert!(unverified(evaluate(&mean, thirds())));
    let rounded = one(
        Filter::all(),
        total(
            AggOp::Avg,
            Some("amount"),
            "average",
            Some(2),
            OnEmpty::Unstated,
        ),
    );
    let rounded = evaluate(&rounded, thirds()).expect("rounded as stated");
    assert_eq!(the_total(&rounded, "average"), "1.67");
    let exact = evaluate(
        &one(
            Filter::all(),
            total(AggOp::Sum, Some("amount"), "total", None, OnEmpty::Zero),
        ),
        csv("amount\n0.1\n0.2\n"),
    )
    .expect("defined");
    assert_eq!(the_total(&exact, "total"), "0.3");
}

#[test]
fn groups_aggregate_their_members_and_fix_no_order() {
    let per_customer = Stages {
        group_by: Some("customer".to_owned()),
        ..total(AggOp::Sum, Some("amount"), "total", None, OnEmpty::Zero)
    };
    let expected = evaluate(
        &one(Filter::all(), per_customer.clone()),
        csv("customer,amount\na,10\nb,5\na,2.5\n"),
    )
    .expect("defined");
    assert_eq!(expected.order, Order::Free);
    assert_eq!(blocks(&expected, "customer"), [["\"a\"", "\"b\""]]);
    assert_eq!(blocks(&expected, "total"), [["12.5", "5"]]);
    // `70` and `70.0` are different texts and the same number: one group or two is no fact.
    assert!(unverified(evaluate(
        &one(Filter::all(), per_customer),
        csv("customer,amount\n70,1\n70.0,2\n"),
    )));
    let unnamed = Stages {
        group_by: Some("customer".to_owned()),
        ..Stages::default()
    };
    assert!(unverified(evaluate(
        &one(Filter::all(), unnamed),
        csv("customer,amount\na,1\n")
    )));
}

#[test]
fn duplicates_are_removed_exactly_and_loose_duplicates_stay_open() {
    let emails = Stages {
        columns: vec!["email".to_owned()],
        distinct: true,
        ..Stages::default()
    };
    let expected = evaluate(
        &one(Filter::all(), emails.clone()),
        csv("email,plan\na@x,free\nb@x,pro\na@x,pro\n"),
    )
    .expect("defined");
    assert_eq!(blocks(&expected, "email"), [["\"a@x\"", "\"b@x\""]]);
    assert!(unverified(evaluate(
        &one(
            Filter::all(),
            Stages {
                distinct: true,
                ..Stages::default()
            }
        ),
        csv("amount\n70\n70.0\n"),
    )));
    let by_title = Stages {
        distinct_by: vec!["title".to_owned(), "artist".to_owned()],
        ..Stages::default()
    };
    let first = evaluate(
        &one(Filter::all(), by_title),
        json(r#"[{"title":"t","artist":"a","year":1},{"title":"t","artist":"b","year":2},{"title":"t","artist":"a","year":3}]"#),
    )
    .expect("the first in file order is kept");
    assert_eq!(blocks(&first, "year"), [["1", "2"]]);
}

#[test]
fn a_projection_follows_the_filter_and_reads_number_columns_under_the_law() {
    let active = Test::new("active", Comparator::Eq, Operand::Bool(true));
    let projected = Stages {
        columns: vec!["id".to_owned(), "name".to_owned()],
        renames: vec![("name".to_owned(), "label".to_owned())],
        ..Stages::default()
    };
    let expected = evaluate(
        &one(keep(vec![active]), projected),
        json(r#"[{"id":1,"name":"a","active":true},{"id":2,"name":"b","active":false}]"#),
    )
    .expect("defined");
    let row = &expected.blocks[0].rows[0];
    assert_eq!(row.keys().collect::<Vec<_>>(), ["id", "label"]);
    let numbered = Stages {
        columns: vec!["amount".to_owned()],
        number_columns: vec!["amount".to_owned()],
        ..Stages::default()
    };
    let read = evaluate(
        &one(Filter::all(), numbered.clone()),
        csv("amount\n12.50\n"),
    )
    .expect("a number column");
    assert_eq!(blocks(&read, "amount"), [["12.5"]]);
    assert!(matches!(
        evaluate(&one(Filter::all(), numbered), csv("amount\nn/a\n")),
        Err(Undefined::Stops(_))
    ));
}

#[test]
fn a_later_step_runs_on_what_the_step_before_wrote() {
    let per_customer = Stages {
        group_by: Some("customer".to_owned()),
        ..total(AggOp::Sum, Some("amount"), "total", None, OnEmpty::Zero)
    };
    let pipeline = Pipeline::new(vec![
        Step::new(Filter::all(), per_customer),
        Step::new(keep(vec![above("total", "10")]), Stages::default()),
    ]);
    let expected =
        evaluate(&pipeline, csv("customer,amount\na,10\nb,5\na,2.5\n")).expect("defined");
    assert_eq!(blocks(&expected, "customer"), [["\"a\""]]);
    let after_totals = Pipeline::new(vec![
        Step::new(
            Filter::all(),
            total(AggOp::Sum, Some("amount"), "total", None, OnEmpty::Zero),
        ),
        Step::new(keep(vec![above("total", "1")]), Stages::default()),
    ]);
    assert!(unverified(evaluate(&after_totals, csv("amount\n5\n"))));
    let after_cut = Pipeline::new(vec![
        Step::new(Filter::all(), sorted("amount", true, false, Some(1))),
        Step::new(keep(vec![above("amount", "0")]), Stages::default()),
    ]);
    assert!(unverified(evaluate(
        &after_cut,
        json(r#"[{"id":1,"amount":8},{"id":2,"amount":8}]"#)
    )));
}

#[test]
fn an_order_no_request_fixes_is_never_assumed() {
    let per_customer = Stages {
        group_by: Some("customer".to_owned()),
        limit: Some(1),
        ..total(AggOp::Sum, Some("amount"), "total", None, OnEmpty::Zero)
    };
    assert!(unverified(evaluate(
        &one(Filter::all(), per_customer),
        csv("customer,amount\na,1\nb,2\n"),
    )));
    let regrouped_ties = Pipeline::new(vec![
        Step::new(
            Filter::all(),
            Stages {
                group_by: Some("customer".to_owned()),
                ..total(AggOp::Sum, Some("amount"), "total", None, OnEmpty::Zero)
            },
        ),
        Step::new(Filter::all(), sorted("total", true, true, None)),
    ]);
    assert!(unverified(evaluate(
        &regrouped_ties,
        csv("customer,amount\na,1\nb,1\n"),
    )));
    let first_in_file = evaluate(
        &one(
            Filter::all(),
            Stages {
                limit: Some(1),
                ..Stages::default()
            },
        ),
        csv("id\n1\n2\n"),
    )
    .expect("the first rows of the file");
    assert_eq!(blocks(&first_in_file, "id"), [["\"1\""]]);
}
