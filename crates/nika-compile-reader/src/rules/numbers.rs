// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One number law (R4 A5 · C3/C4): a value a rule reads as a number is a finite JSON number or a
//! text `crate::text::NUMBER_TEXT` accepts, parsed; anything else follows the field's policy.
//! FAIL, the default, stops the run naming the field and the value; SKIP leaves the record out of
//! the comparison, ranking or total, and a ranking or total left with no number stops the run. No
//! jq total order, no invented zero. A text equality's bounded canonical-spelling expansion lives
//! here too: exact spellings the compiler grounded in observed values, never an NFC at run.

use super::{AggOp, Clause, Comparator, Operand, Rule};
use crate::text::NUMBER_TEXT;
use serde_json::json;

/// What a rule does with a record whose field is not a number the law reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumberPolicy {
    /// The run stops, naming the field and the value.
    Fail,
    /// The record is left out: the comparison is false, it leaves the ranking or the total.
    Skip,
}

impl NumberPolicy {
    /// The policy's word, as a question offers it and a record states it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }
}

/// The policies a rule states, by field.
pub(crate) type Numbers = std::collections::BTreeMap<String, NumberPolicy>;

/// The jq test of the law on the value in `.`.
fn law() -> String {
    let text = json!(NUMBER_TEXT);
    format!(
        "(type == \"number\" and (isinfinite or isnan | not)) or (type == \"string\" and test({text}))"
    )
}

/// `true` where the value at `key` is a number the law reads.
pub(crate) fn is_number(key: &str) -> String {
    format!("({key} | {})", law())
}

/// The number at `key` under the law; any other value stops the run, naming `field`.
pub(crate) fn number(key: &str, field: &str) -> String {
    let (law, head) = (law(), json!(format!("`{field}` is ")));
    format!(
        "({key} | if {law} then tonumber else error({head} + (if . == null then \"null or missing\" else tojson end) + \", not a number\") end)"
    )
}

/// `test`, held under SKIP only by a record whose value at `key` is a number.
pub(crate) fn guarded(skip: bool, key: &str, test: String) -> String {
    if skip {
        format!("({} and {test})", is_number(key))
    } else {
        test
    }
}

/// Under SKIP, the records whose value at `key` is a number; none left stops the run saying what
/// `field` could not state, from an empty input too where `none_is_no_value` (an average, a
/// minimum or a maximum over nothing).
pub(crate) fn numbered(key: &str, field: &str, what: &str, none_is_no_value: bool) -> String {
    let none = if none_is_no_value { "true" } else { "$all > 0" };
    let (test, why) = (
        is_number(key),
        json!(format!("no `{field}` is a number: {what}")),
    );
    format!(
        "(length as $all | map(select({test})) | if length == 0 and {none} then error({why}) else . end)"
    )
}

/// A text equality over a column.
fn equality(clause: &Clause) -> bool {
    clause.field != "." && matches!(clause.comparator, Comparator::Eq | Comparator::Ne)
}

impl Rule {
    /// The source fields the rule reads as numbers, first use first: numeric comparisons, sums,
    /// averages, minima, maxima and the key of a ranking that keeps a count.
    #[must_use]
    pub fn number_fields(&self) -> Vec<String> {
        let shape = &self.shape;
        let compared = self.clauses.iter().filter(|c| {
            c.comparator.textual().is_none()
                && (matches!(c.value, Operand::Number(_)) || c.comparator.numeric())
        });
        let columns = compared.flat_map(|c| {
            let other = if let Operand::Column(o) = &c.value {
                Some(o)
            } else {
                None
            };
            std::iter::once(&c.field).chain(other)
        });
        let totals = shape.aggregations.iter().filter(|a| a.op != AggOp::Count);
        let ranked = shape
            .sort_by
            .iter()
            .map(|(f, _)| f)
            .filter(|f| shape.limit.is_some() && !shape.produced().contains(&f.as_str()));
        let mut out: Vec<String> = Vec::new();
        for name in columns
            .chain(totals.filter_map(|a| a.field.as_ref()))
            .chain(ranked)
        {
            if self.program.is_none() && name != "." && !out.contains(name) {
                out.push(name.clone());
            }
        }
        out
    }

    /// The source key of a sort that keeps no count: numeric only where the compiler states a
    /// policy for it from what it observed.
    #[must_use]
    pub fn plain_sort(&self) -> Option<&str> {
        let (field, _) = self.shape.sort_by.as_ref()?;
        let plain = self.program.is_none() && self.shape.limit.is_none();
        (plain && !self.shape.produced().contains(&field.as_str())).then_some(field.as_str())
    }

    /// The policy stated for `field`, if any (a field with none reads as a recorded plan read it).
    #[must_use]
    pub fn number_policy(&self, field: &str) -> Option<NumberPolicy> {
        self.numbers.get(field).copied()
    }

    /// The same rule with `policy` stated for `field`, a field it reads as a number or its plain
    /// sort's key; `None` for any other field and for a verified program.
    #[must_use]
    pub fn with_number_policy(&self, field: &str, policy: NumberPolicy) -> Option<Self> {
        let read = self.number_fields().iter().any(|f| f == field);
        let mut rule = self.clone();
        rule.numbers.insert(field.to_owned(), policy);
        (read || self.plain_sort() == Some(field)).then_some(rule)
    }

    /// The text equalities the rule states, as (field, literal), in clause order.
    #[must_use]
    pub fn text_equalities(&self) -> Vec<(String, String)> {
        let stated = |c: &Clause| match &c.value {
            Operand::Text(text) if equality(c) => Some((c.field.clone(), text.clone())),
            _ => None,
        };
        self.clauses.iter().filter_map(stated).collect()
    }

    /// The same rule where the text equality of `field` with `literal` also matches exactly each
    /// of `spellings`, the bounded canonical-spelling expansion; `None` without that equality.
    #[must_use]
    pub fn with_spellings(&self, field: &str, literal: &str, spellings: &[String]) -> Option<Self> {
        let text = Operand::Text(literal.to_owned());
        let mut rule = self.clone();
        let mut found = false;
        for clause in &mut rule.clauses {
            if equality(clause) && clause.field == field && clause.value == text {
                found = true;
                for spelling in spellings.iter().filter(|s| *s != literal) {
                    if !clause.spellings.contains(spelling) {
                        clause.spellings.push(spelling.clone());
                    }
                }
            }
        }
        (found && self.program.is_none()).then_some(rule)
    }
}

#[cfg(test)]
/// Test shorthand: every read of the law becomes `(key | num)` and every test of it
/// `(key | isnum)`, so a battery pins where the law is read while its exact text is pinned once.
pub(crate) fn short(jq: &str) -> String {
    let law = law();
    let read = format!(" | if {law} then tonumber else error(");
    let close = ", not a number\") end)";
    let mut out = jq.to_owned();
    while let Some(at) = out.find(&read) {
        let Some(start) = out[..at].rfind('(') else {
            break;
        };
        let Some(end) = out[at..].find(close) else {
            break;
        };
        let key = out[start + 1..at].to_owned();
        out = format!(
            "{}({key} | num){}",
            &out[..start],
            &out[at + end + close.len()..]
        );
    }
    let test = format!(" | {law})");
    while let Some(at) = out.find(&test) {
        let Some(start) = out[..at].rfind('(') else {
            break;
        };
        let key = out[start + 1..at].to_owned();
        out = format!(
            "{}({key} | isnum){}",
            &out[..start],
            &out[at + test.len()..]
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{Clause, Comparator, Junction, Operand, Rule, Shape, synthesize};
    use super::{NumberPolicy, number, short};
    use serde_json::json;

    fn rule(text: &str) -> Rule {
        synthesize(text, &[]).expect(text)
    }

    fn typed(text: &str, comparator: Comparator, value: Operand) -> Rule {
        let clause = Clause::new("status", comparator, value);
        Rule::typed(text, vec![clause], Junction::And, Shape::default())
    }

    #[test]
    fn the_law_reads_a_finite_number_or_a_decimal_text_and_names_anything_else() {
        assert_eq!(
            number(".amount", "amount"),
            r#"(.amount | if (type == "number" and (isinfinite or isnan | not)) or (type == "string" and test("^[ \\t]*-?(0|[1-9][0-9]*)([.][0-9]+)?[ \\t]*$")) then tonumber else error("`amount` is " + (if . == null then "null or missing" else tojson end) + ", not a number") end)"#
        );
        // A field name reaches jq as a string, never as program text.
        assert!(number(".[\"a\\\"b\"]", "a\"b").contains(r#"error("`a\"b` is ""#));
    }

    #[test]
    fn fail_reads_the_law_and_skip_guards_every_numeric_read() {
        let above = rule("keep only the rows whose amount is strictly greater than 100");
        assert_eq!(above.number_fields(), ["amount"]);
        // No stated policy reads as a recorded plan always read it; a stated one reads the law.
        assert_eq!(
            short(&above.jq()),
            "[.records[] | select((.amount | tonumber) > 100)]"
        );
        let stated = above
            .with_number_policy("amount", NumberPolicy::Fail)
            .expect("numeric");
        assert_eq!(
            short(&stated.jq()),
            "[.records[] | select((.amount | num) > 100)]"
        );
        let skip = above
            .with_number_policy("amount", NumberPolicy::Skip)
            .expect("numeric");
        assert_eq!(
            short(&skip.jq()),
            "[.records[] | select(((.amount | isnum) and (.amount | num) > 100))]"
        );
        // Two columns: each side reads the law, each skipped side is guarded.
        let columns = Rule::typed(
            "stock_qty below reorder_level",
            vec![Clause::new(
                "stock_qty",
                Comparator::Lt,
                Operand::Column("reorder_level".into()),
            )],
            Junction::And,
            Shape::default(),
        );
        assert_eq!(columns.number_fields(), ["stock_qty", "reorder_level"]);
        let one = columns
            .with_number_policy("reorder_level", NumberPolicy::Skip)
            .expect("numeric");
        assert_eq!(
            short(&one.jq()),
            "[.records[] | select(((.reorder_level | isnum) and (.stock_qty | tonumber) < (.reorder_level | num)))]"
        );
    }

    #[test]
    fn a_ranking_and_a_total_never_fall_back_to_total_order_or_an_invented_value() {
        let top = rule("keep the 2 rows with the highest amount");
        assert_eq!(top.number_fields(), ["amount"]);
        assert_eq!(top.plain_sort(), None);
        assert_eq!(
            short(&top.jq()),
            ".records | sort_by(.amount | tonumber? // .) | reverse | .[:2]"
        );
        let strict = top
            .with_number_policy("amount", NumberPolicy::Fail)
            .expect("numeric");
        assert_eq!(
            short(&strict.jq()),
            ".records | sort_by((.amount | num)) | reverse | .[:2]"
        );
        let skip = top
            .with_number_policy("amount", NumberPolicy::Skip)
            .expect("numeric");
        assert_eq!(
            short(&skip.jq()),
            ".records | (length as $all | map(select((.amount | isnum))) | if length == 0 and $all > 0 then error(\"no `amount` is a number: no row can be ranked\") else . end) | sort_by((.amount | num)) | reverse | .[:2]"
        );
        let total = rule("the total of the amount column");
        assert_eq!(
            short(&total.jq()),
            ".records | {\"total\": (map(.amount | tonumber) | add // 0)}"
        );
        let strict = total
            .with_number_policy("amount", NumberPolicy::Fail)
            .expect("numeric");
        assert_eq!(
            short(&strict.jq()),
            ".records | {\"total\": (map((.amount | num)) | add // 0)}"
        );
        let skip = total
            .with_number_policy("amount", NumberPolicy::Skip)
            .expect("numeric");
        assert_eq!(
            short(&skip.jq()),
            ".records | {\"total\": (((length as $all | map(select((.amount | isnum))) | if length == 0 and $all > 0 then error(\"no `amount` is a number: its total cannot be stated\") else . end) | map((.amount | num))) | add // 0)}"
        );
        // An average, minimum or maximum over no number is no value: it stops, from an empty
        // input too, and an average divides by the numbers it kept, never by every row.
        let average = rule("the average of the amount column");
        let stop = "error(\"no `amount` is a number: its average cannot be stated\")";
        let skip = average
            .with_number_policy("amount", NumberPolicy::Skip)
            .expect("numeric");
        assert_eq!(
            short(&skip.jq()),
            format!(
                ".records | {{\"average\": (((length as $all | map(select((.amount | isnum))) | if length == 0 and true then {stop} else . end) | map((.amount | num))) | if length == 0 then {stop} else add / length end)}}"
            )
        );
        let strict = average
            .with_number_policy("amount", NumberPolicy::Fail)
            .expect("numeric");
        assert_eq!(
            short(&strict.jq()),
            format!(
                ".records | {{\"average\": (map((.amount | num)) | if length == 0 then {stop} else add / length end)}}"
            )
        );
        let least = rule("the minimum of the amount column")
            .with_number_policy("amount", NumberPolicy::Fail)
            .expect("numeric");
        assert_eq!(
            short(&least.jq()),
            ".records | {\"minimum\": (map((.amount | num)) | if length == 0 then error(\"no `amount` is a number: its minimum cannot be stated\") else min end)}"
        );
    }

    #[test]
    fn a_plain_sort_is_numeric_only_where_the_compiler_states_it() {
        let sorted = rule("sort the rows by amount");
        assert!(sorted.number_fields().is_empty());
        assert_eq!(sorted.plain_sort(), Some("amount"));
        assert_eq!(
            short(&sorted.jq()),
            ".records | sort_by(.amount | tonumber? // .)"
        );
        let numeric = sorted
            .with_number_policy("amount", NumberPolicy::Fail)
            .expect("its key");
        assert_eq!(short(&numeric.jq()), ".records | sort_by((.amount | num))");
    }

    #[test]
    fn spellings_widen_one_text_equality_exactly_and_nothing_else() {
        let nfc = "livr\u{e9}";
        let observed = ["livre\u{301}".to_owned(), nfc.to_owned()];
        let equal = typed("status is livré", Comparator::Eq, Operand::Text(nfc.into()));
        assert_eq!(
            equal.text_equalities(),
            [("status".to_owned(), nfc.to_owned())]
        );
        let spelled = equal
            .with_spellings("status", nfc, &observed)
            .expect("stated");
        assert_eq!(
            spelled.jq(),
            "[.records[] | select((.status == \"livr\u{e9}\" or .status == \"livre\u{301}\"))]"
        );
        let differ = typed(
            "status is not livré",
            Comparator::Ne,
            Operand::Text(nfc.into()),
        );
        let spelled_ne = differ
            .with_spellings("status", nfc, &observed)
            .expect("stated");
        assert_eq!(
            spelled_ne.jq(),
            "[.records[] | select((.status != \"livr\u{e9}\" and .status != \"livre\u{301}\"))]"
        );
        assert!(equal.with_spellings("status", "other", &observed).is_none());
        assert!(equal.with_spellings("statut", nfc, &observed).is_none());
        let contains = typed(
            "status contains livré",
            Comparator::Contains,
            Operand::Text(nfc.into()),
        );
        assert!(contains.text_equalities().is_empty());
        assert!(contains.with_spellings("status", nfc, &observed).is_none());
    }

    #[test]
    fn annotations_are_recorded_only_when_stated_and_a_record_carrying_them_is_refused() {
        let above = rule("keep only the rows whose amount is strictly greater than 100");
        let record = above.to_json();
        assert!(record.get("numbers").is_none(), "{record}");
        assert_eq!(Rule::from_json(&record), Some(above.clone()));
        let skip = above
            .with_number_policy("amount", NumberPolicy::Skip)
            .expect("numeric");
        assert_eq!(skip.to_json()["numbers"], json!({"amount": "skip"}));
        assert_eq!(
            Rule::from_json(&skip.to_json()),
            None,
            "a record never replays a policy"
        );
        let mut forged = record.clone();
        forged["numbers"] = json!({"amount": 7});
        assert_eq!(
            Rule::from_json(&forged),
            None,
            "a wrong-typed policy is refused"
        );
        let nfc = "livr\u{e9}";
        let equal = typed("status is livré", Comparator::Eq, Operand::Text(nfc.into()));
        assert!(equal.to_json()["clauses"][0].get("spellings").is_none());
        let spelled = equal
            .with_spellings("status", nfc, &["livre\u{301}".to_owned()])
            .expect("stated");
        assert_eq!(
            spelled.to_json()["clauses"][0]["spellings"],
            json!(["livre\u{301}"])
        );
        assert_eq!(
            Rule::from_json(&spelled.to_json()),
            None,
            "spellings never replay"
        );
        // The guard keeps a field a failure reads and steps aside for one SKIP may leave out.
        assert!(above.guard().contains("has(\"amount\")"));
        let fail = above
            .with_number_policy("amount", NumberPolicy::Fail)
            .expect("numeric");
        assert!(fail.guard().contains("has(\"amount\")"));
        assert!(!skip.guard().contains("has(\"amount\")"));
        // A policy names only a field the rule reads as a number; a program is nobody's.
        assert!(
            above
                .with_number_policy("status", NumberPolicy::Skip)
                .is_none()
        );
        let program = Rule::program("the program", ".records", vec!["amount".into()]);
        assert!(
            program
                .with_number_policy("amount", NumberPolicy::Skip)
                .is_none()
        );
        assert!(program.number_fields().is_empty());
    }
}
