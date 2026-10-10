// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The captures a replay trial is lent: each bound to its exact address, its bytes and its time,
//! and bounded one by one and together before a trial holds them.

use nika_event::source_id::sha256_hex;

use super::{Capture, CaptureRefused, Captures};

fn page(url: &str, bytes: usize) -> Capture {
    Capture::new(
        url,
        200,
        Some("text/plain".to_owned()),
        vec![b'x'; bytes],
        1_000,
    )
}

#[test]
fn a_capture_answers_only_its_exact_address() {
    let mut captures = Captures::new();
    captures
        .insert(page("https://feed.example/items", 3))
        .unwrap();
    assert!(captures.get("https://feed.example/items").is_some());
    for other in [
        "https://feed.example/items/",
        "HTTPS://feed.example/items",
        "https://FEED.example/items",
        "https://feed.example/items?page=1",
        "https://feed.example/items#top",
        "http://feed.example/items",
    ] {
        assert!(captures.get(other).is_none(), "{other}");
    }
}

#[test]
fn a_capture_is_bound_to_its_bytes_and_its_time() {
    let capture = page("https://feed.example/items", 3);
    assert_eq!(capture.sha256(), sha256_hex(b"xxx"));
    assert_eq!((capture.status(), capture.captured_at_ms()), (200, 1_000));
    assert_eq!(capture.content_type(), Some("text/plain"));
    assert_eq!(capture.body(), b"xxx");
    assert!(
        !format!("{capture:?}").contains("xxx"),
        "its body is never printed"
    );
}

#[test]
fn captures_are_bounded_one_by_one() {
    let mut captures = Captures::new();
    let refused = captures.insert(page("ftp://feed.example/items", 1));
    assert_eq!(refused, Err(CaptureRefused::Address));
    assert_eq!(
        captures.insert(page("https://", 1)),
        Err(CaptureRefused::Address)
    );
    let big = Captures::MAX_CAPTURE_BYTES + 1;
    let refused = captures.insert(page("https://feed.example/big", big));
    assert_eq!(refused, Err(CaptureRefused::Body { bytes: big }));
    for k in 0..Captures::MAX_CAPTURES {
        captures
            .insert(page(&format!("https://feed.example/{k}"), 1))
            .unwrap();
    }
    let refused = captures.insert(page("https://feed.example/more", 1));
    assert_eq!(refused, Err(CaptureRefused::Count));
    // An address already held is taken again, never counted twice.
    captures.insert(page("https://feed.example/0", 2)).unwrap();
    assert_eq!(captures.len(), Captures::MAX_CAPTURES);
    assert_eq!(
        captures.get("https://feed.example/0").map(Capture::body),
        Some(&b"xx"[..])
    );
}

#[test]
fn captures_are_bounded_together() {
    let mut captures = Captures::new();
    let each = Captures::MAX_CAPTURE_BYTES;
    for k in 0..Captures::MAX_TOTAL_BYTES / each {
        captures
            .insert(page(&format!("https://feed.example/{k}"), each))
            .unwrap();
    }
    let refused = captures.insert(page("https://feed.example/last", 1));
    let bytes = Captures::MAX_TOTAL_BYTES + 1;
    assert_eq!(refused, Err(CaptureRefused::Total { bytes }));
    assert!(!captures.is_empty());
}
