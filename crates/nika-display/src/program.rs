// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The name of the running program, for the commands a hint gives.

/// The program the person runs Nika as (`nika-dev`, `nika`): the name a hint gives, so a command
/// copied from it reaches this same binary (live gauntlet, 2026-10-16: the hints said `nika`,
/// which on that machine was an older release that refused the workflow). Any other name (a test
/// binary, a renamed copy) reads as `nika`.
/// The Session reads the same name (`nika-session`, `runtime::schedule::program`).
pub(crate) fn program() -> String {
    named(std::env::args_os().next())
}

/// The program name `first`, the process's first argument, says: its file name when it is
/// `nika` or a `nika-` channel name, `nika` otherwise.
fn named(first: Option<std::ffi::OsString>) -> String {
    first
        .and_then(|first| {
            std::path::Path::new(&first)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .filter(|name| {
            name == "nika"
                || (name.starts_with("nika-")
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.')))
        })
        .unwrap_or_else(|| "nika".to_owned())
}

#[cfg(test)]
mod tests {
    use super::named;

    #[test]
    fn the_running_channel_names_the_hints_and_anything_else_reads_as_nika() {
        for (first, name) in [
            ("/Users/someone/.local/bin/nika-dev", "nika-dev"),
            ("nika", "nika"),
            ("/opt/nika/bin/nika-0.123.0", "nika-0.123.0"),
            ("/tmp/target/debug/deps/nika_display-1a2b3c", "nika"),
            ("nika-dev;rm", "nika"),
            ("bash", "nika"),
        ] {
            assert_eq!(named(Some(first.into())), name, "{first}");
        }
        assert_eq!(named(None), "nika");
    }
}
