// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

/// Capture declared attribution once at the production composition boundary.
/// Environment labels are informational, never authenticated identities.
/// Missing host labels remain unknown; no platform-specific syscall is needed.
#[allow(clippy::disallowed_methods)]
pub(super) fn identity() -> String {
    let env = |name: &str| std::env::var(name).ok();
    crate::approval::compose_operator(
        env("NIKA_OPERATOR"),
        env("USER").or_else(|| env("USERNAME")),
        env("HOSTNAME").or_else(|| env("COMPUTERNAME")),
    )
}
