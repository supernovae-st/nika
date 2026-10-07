// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Exact transport grain, independent of the smaller persisted-journal grain.
use super::*;

#[tokio::test]
async fn wire_limit_is_inclusive_with_no_silent_truncation() {
    for (len, accepted) in [
        (MAX_LINE_BYTES - 1, true),
        (MAX_LINE_BYTES, true),
        (MAX_LINE_BYTES + 1, false),
    ] {
        let mut wire = vec![b'a'; len];
        wire.push(b'\n');
        let mut reader = BufReader::new(wire.as_slice());
        let mut pending = Vec::new();
        let result =
            read_bounded_line(&mut reader, &mut pending, std::time::Duration::from_secs(1)).await;
        if accepted {
            assert_eq!(result.unwrap().len(), len);
        } else {
            assert!(result.unwrap_err().to_string().contains("bound"));
            assert!(pending.len() <= MAX_LINE_BYTES + 1);
        }
    }
}
