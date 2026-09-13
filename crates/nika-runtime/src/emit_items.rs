// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Page large fan-out evidence before its terminal, keeping small tables
//! inline. The bound accounts for JSON text escaped inside the event JSON.

use crate::{EventKind, EventSink, FieldValue, Stamper, emit, i, s};

// Implementation threshold, well below the journal's 1 MiB line limit.
// The event envelope and task id need room too. Never raise the verifier's
// bound to accommodate a fan-out.
const PAGE_BYTES: usize = 64 * 1024;

pub(crate) fn push(
    fields: &mut Vec<(&'static str, FieldValue)>,
    task: &str,
    json: &str,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let Some((pages, counts)) = pages(json) else {
        fields.push(("items", s(json)));
        return;
    };
    for (page, items) in pages.iter().enumerate() {
        emit(
            stamper,
            sink,
            EventKind::TaskItems,
            &[
                ("task", s(task)),
                ("page", count(page)),
                ("items", s(items)),
            ],
        );
    }
    fields.push(("items_pages", count(pages.len())));
    for (key, value) in [
        ("items_total", counts.iter().sum()),
        ("items_ok", counts[0] + counts[1]),
        ("items_recovered", counts[1]),
        ("items_failed", counts[2]),
        ("items_never_started", counts[3]),
    ] {
        fields.push((key, count(value)));
    }
}

fn count(value: usize) -> FieldValue {
    i(i64::try_from(value).unwrap_or(i64::MAX))
}

/// `None` preserves the original inline representation: either it fits, or
/// a single row cannot be paged. The writer still refuses oversized scalar
/// payloads, outputs and whole journals; no row is truncated or discarded.
fn pages(json: &str) -> Option<(Vec<String>, [usize; 4])> {
    if serde_json::to_string(json).ok()?.len() <= PAGE_BYTES {
        return None;
    }
    let rows: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    let mut counts = [0; 4];
    let mut pages = Vec::new();
    let mut page = String::from("[");
    let mut bytes = 4; // the array brackets, inside a JSON string
    for row in rows {
        let status = match row.get("status")?.as_str()? {
            "ok" => 0,
            "recovered" => 1,
            "failed" => 2,
            "never_started" => 3,
            _ => return None,
        };
        counts[status] += 1;
        let text = serde_json::to_string(&row).ok()?;
        let size = serde_json::to_string(&text).ok()?.len() - 2;
        if size + 4 > PAGE_BYTES {
            return None;
        }
        if bytes + size + usize::from(page.len() > 1) > PAGE_BYTES {
            page.push(']');
            pages.push(page);
            page = String::from("[");
            bytes = 4;
        }
        if page.len() > 1 {
            page.push(',');
            bytes += 1;
        }
        page.push_str(&text);
        bytes += size;
    }
    page.push(']');
    pages.push(page);
    Some((pages, counts))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn large_tables_page_losslessly_and_bound_the_double_encoded_bytes() {
        let rows: Vec<_> = (0..17000).map(|index| serde_json::json!({
                "index": index, "item": "🦋\n\"\\", "status": (["ok", "recovered", "failed", "never_started"][index % 4]),
        })).collect();
        let json = serde_json::to_string(&rows).expect("rows");
        let (pages, counts) = pages(&json).expect("large table pages");
        assert!(pages.len() > 1);
        assert_eq!(counts, [4250; 4]);
        let mut restored = Vec::new();
        for page in pages {
            assert!(serde_json::to_string(&page).expect("encoded page").len() <= PAGE_BYTES);
            restored
                .extend(serde_json::from_str::<Vec<serde_json::Value>>(&page).expect("whole rows"));
        }
        assert_eq!(restored, rows);
    }

    #[test]
    fn small_tables_and_unpageable_rows_preserve_every_original_byte() {
        for json in [
            "[]".to_owned(),
            "[{\"status\":\"ok\"}]".to_owned(),
            serde_json::json!([{"status": "failed", "message": "x".repeat(PAGE_BYTES)}])
                .to_string(),
        ] {
            let mut fields = Vec::new();
            let mut sink = crate::VecSink::new();
            push(
                &mut fields,
                "fan",
                &json,
                &mut crate::DeterministicStamper::new(),
                &mut sink,
            );
            assert_eq!(fields, vec![("items", s(&json))]);
            assert!(sink.events().is_empty());
        }
    }
}
