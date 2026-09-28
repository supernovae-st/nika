// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Fold paged item evidence only when its terminal closes a complete set.
//! Missing, duplicate, reordered or malformed pages never become a table,
//! nor do unknown statuses or counts that disagree with the rows (spec 17).

use nika_event::Event;
use serde_json::Value;

use crate::state::{int_field, str_field};

/// Row counts by status: `ok` · `recovered` · `failed` · `never_started` ·
/// `cancelled`.
#[derive(Debug, Default)]
pub(crate) struct Pages {
    next: i64,
    rows: Vec<Value>,
    counts: [i64; 5],
    invalid: bool,
}

impl Pages {
    pub(crate) fn push(&mut self, event: &Event) {
        if self.invalid {
            return;
        }
        if !self.append(event) {
            self.invalid = true;
            self.rows.clear();
        }
    }

    fn append(&mut self, event: &Event) -> bool {
        if int_field(event, "page") != Some(self.next) {
            return false;
        }
        let Some(rows) = str_field(event, "items")
            .and_then(|text| serde_json::from_str::<Vec<Value>>(text).ok())
        else {
            return false;
        };
        if rows.is_empty() {
            return false;
        }
        for row in rows {
            if row.get("index").and_then(Value::as_u64) != u64::try_from(self.rows.len()).ok()
                || !row.get("item").is_some_and(Value::is_string)
            {
                return false;
            }
            let class = match row.get("status").and_then(Value::as_str) {
                Some("ok") => 0,
                Some("recovered") => 1,
                Some("failed") => 2,
                Some("never_started") => 3,
                Some("cancelled") => 4,
                _ => return false,
            };
            self.counts[class] += 1;
            self.rows.push(row);
        }
        self.next += 1;
        true
    }

    pub(crate) fn finish(self, event: &Event) -> Option<String> {
        if self.invalid || self.next == 0 {
            return None;
        }
        // A terminal written before `cancelled` existed carries no count:
        // its absence holds only while no collected row is cancelled. A
        // present count must be the integer the rows give.
        let historical = !event.fields.iter().any(|f| f.key == "items_cancelled");
        let cancelled = if historical {
            Some(0)
        } else {
            int_field(event, "items_cancelled")
        };
        if cancelled != Some(self.counts[4]) {
            return None;
        }
        for (key, expected) in [
            ("items_pages", self.next),
            ("items_total", self.counts.iter().sum()),
            ("items_ok", self.counts[0] + self.counts[1]),
            ("items_recovered", self.counts[1]),
            ("items_failed", self.counts[2]),
            ("items_never_started", self.counts[3]),
        ] {
            if int_field(event, key) != Some(expected) {
                return None;
            }
        }
        serde_json::to_string(&self.rows).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value as FieldValue};

    fn page(index: i64, row: usize) -> Event {
        crate::demo::bare_event(EventKind::TaskItems, 0)
            .with_field(KeyValue::new("page", FieldValue::Int(index)))
            .with_field(KeyValue::new("task", FieldValue::String("fan".into())))
            .with_field(KeyValue::new(
                "items",
                FieldValue::String(
                    serde_json::json!([
                        {"index": row, "item": format!("🦋-{row}"), "status": "ok"}
                    ])
                    .to_string(),
                ),
            ))
    }

    fn terminal() -> Event {
        let mut event = crate::demo::bare_event(EventKind::TaskCompleted, 0)
            .with_field(KeyValue::new("task", FieldValue::String("fan".into())));
        for (key, value) in [
            ("items_pages", 2),
            ("items_total", 2),
            ("items_ok", 2),
            ("items_recovered", 0),
            ("items_failed", 0),
            ("items_never_started", 0),
        ] {
            event = event.with_field(KeyValue::new(key, FieldValue::Int(value)));
        }
        event
    }

    #[test]
    fn the_live_fold_and_replay_keep_the_complete_table() {
        let mut view = crate::state::RunView::new();
        view.apply(&page(0, 0));
        view.apply(&page(1, 1));
        view.apply(&terminal());
        let row = &view.rows()[0];
        let items: Vec<Value> =
            serde_json::from_str(row.items_json.as_deref().expect("table")).expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1]["item"], "🦋-1");
    }

    #[test]
    fn restarted_legs_and_conflicting_terminals_do_not_reuse_item_tables() {
        for conflict in [true, false] {
            let mut view = crate::state::RunView::new();
            view.apply(&page(0, 0));
            view.apply(&page(1, 1));
            if conflict {
                view.apply(
                    &terminal().with_field(KeyValue::new("items", FieldValue::String("[]".into()))),
                );
            } else {
                view.apply(&terminal());
                assert!(view.rows()[0].items_json.is_some());
                view.apply(&page(0, 0));
                view.apply(
                    &crate::demo::bare_event(EventKind::TaskStarted, 1)
                        .with_field(KeyValue::new("task", FieldValue::String("fan".into()))),
                );
                assert!(view.rows()[0].items_json.is_none());
                view.apply(&page(1, 1));
                view.apply(&terminal());
            }
            assert!(view.rows()[0].items_json.is_none());
        }
    }

    #[test]
    fn mismatched_status_counts_never_complete_a_table() {
        let mut pages = Pages::default();
        pages.push(&page(0, 0));
        pages.push(&page(1, 1));
        let mut terminal = terminal();
        for field in &mut terminal.fields {
            if field.key == "items_failed" {
                field.value = FieldValue::Int(1);
            }
        }
        assert!(pages.finish(&terminal).is_none());
    }

    /// One page of rows `first..` with the given statuses.
    fn statuses(index: i64, first: usize, statuses: &[&str]) -> Event {
        let rows: Vec<Value> = statuses
            .iter()
            .enumerate()
            .map(|(offset, status)| {
                serde_json::json!({"index": first + offset, "item": "i", "status": status})
            })
            .collect();
        crate::demo::bare_event(EventKind::TaskItems, 0)
            .with_field(KeyValue::new("page", FieldValue::Int(index)))
            .with_field(KeyValue::new("task", FieldValue::String("fan".into())))
            .with_field(KeyValue::new(
                "items",
                FieldValue::String(Value::Array(rows).to_string()),
            ))
    }

    /// A terminal closing one page of `failed · cancelled · never_started`,
    /// with `items_cancelled` set to `cancelled` (absent when `None`).
    fn stopped(cancelled: Option<i64>) -> Event {
        let mut event = crate::demo::bare_event(EventKind::TaskFailed, 0)
            .with_field(KeyValue::new("task", FieldValue::String("fan".into())));
        for (key, value) in [
            ("items_pages", 1),
            ("items_total", 3),
            ("items_ok", 0),
            ("items_recovered", 0),
            ("items_failed", 1),
            ("items_never_started", 1),
        ] {
            event = event.with_field(KeyValue::new(key, FieldValue::Int(value)));
        }
        match cancelled {
            Some(n) => event.with_field(KeyValue::new("items_cancelled", FieldValue::Int(n))),
            None => event,
        }
    }

    fn fold(page: &Event, terminal: &Event) -> Option<Vec<Value>> {
        let mut pages = Pages::default();
        pages.push(page);
        let text = pages.finish(terminal)?;
        Some(serde_json::from_str(&text).expect("rows"))
    }

    /// Spec 17 (next MINOR after 0.121): a started row abandoned without a
    /// terminal is `cancelled`, counted apart from `never_started`.
    #[test]
    fn a_cancelled_row_completes_only_beside_its_own_count() {
        let rows = statuses(0, 0, &["failed", "cancelled", "never_started"]);
        let table = fold(&rows, &stopped(Some(1))).expect("a complete table");
        let words: Vec<&str> = table.iter().filter_map(|r| r["status"].as_str()).collect();
        assert_eq!(words, ["failed", "cancelled", "never_started"]);
        for wrong in [None, Some(0), Some(2)] {
            assert!(fold(&rows, &stopped(wrong)).is_none(), "{wrong:?}");
        }
    }

    /// A historical terminal has no `items_cancelled`: it stays readable
    /// while no row is cancelled, and a present zero must hold too.
    #[test]
    fn a_historical_count_absence_is_accepted_without_cancelled_rows() {
        let rows = statuses(0, 0, &["failed", "never_started", "never_started"]);
        let mut terminal = stopped(None);
        for field in &mut terminal.fields {
            if field.key == "items_never_started" {
                field.value = FieldValue::Int(2);
            }
        }
        assert!(fold(&rows, &terminal).is_some(), "historical form");
        let zero = terminal
            .clone()
            .with_field(KeyValue::new("items_cancelled", FieldValue::Int(0)));
        assert!(fold(&rows, &zero).is_some(), "new form, zero cancelled");
        let one = terminal
            .clone()
            .with_field(KeyValue::new("items_cancelled", FieldValue::Int(1)));
        assert!(fold(&rows, &one).is_none(), "a count no row supports");
        let text = terminal.with_field(KeyValue::new(
            "items_cancelled",
            FieldValue::String("0".into()),
        ));
        assert!(
            fold(&rows, &text).is_none(),
            "a present count is an integer"
        );
    }

    #[test]
    fn an_unknown_item_status_never_completes_a_table() {
        for unknown in ["abandoned", "Cancelled", "never started", ""] {
            let rows = statuses(0, 0, &["failed", unknown, "never_started"]);
            assert!(fold(&rows, &stopped(Some(1))).is_none(), "{unknown:?}");
            assert!(fold(&rows, &stopped(Some(0))).is_none(), "{unknown:?}");
        }
    }

    #[test]
    fn missing_duplicate_reordered_and_wrong_item_pages_never_claim_completeness() {
        for sequence in [
            vec![],
            vec![(0, 0)],
            vec![(1, 1), (0, 0)],
            vec![(0, 0), (0, 1)],
            vec![(0, 0), (1, 0)],
        ] {
            let mut pages = Pages::default();
            for (index, row) in sequence {
                pages.push(&page(index, row));
            }
            assert!(pages.finish(&terminal()).is_none());
        }
    }
}
