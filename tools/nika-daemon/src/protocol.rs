//! IPC protocol — message types and wire format.
//!
//! Wire format: `[4-byte big-endian length][JSON payload]`
//!
//! All messages are length-prefixed JSON. The length prefix is a u32 in
//! big-endian byte order, giving a maximum message size of ~4 GB (but we
//! cap at 16 MB for safety).

// Placeholder — TDD tests come first in Phase 1.2
