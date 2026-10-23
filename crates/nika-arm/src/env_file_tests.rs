// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIK-198 · the env file a scheduled run loads, read as a dotenv file is
//! written: plain `KEY=value` lines first (the case a sourced file lost).

use super::{EnvFileError, parse, read};

fn pairs(text: &str) -> Vec<(String, String)> {
    let parsed = parse(text).map_err(|(line, why)| format!("line {line}: {why}"));
    parsed.expect("the text parses")
}

fn pair(key: &str, value: &str) -> (String, String) {
    (key.to_owned(), value.to_owned())
}

#[test]
fn a_dotenv_file_reads_as_a_shell_would_assign_it() {
    let text = "\u{feff}# provider keys\n\
                \n\
                DEEPSEEK_API_KEY=sk-plain\r\n\
                export ANTHROPIC_API_KEY=sk-exported\n\
                \texport\tOPENAI_API_KEY = ignored-not\n";
    let refused = parse(text).expect_err("a blank before `=` is not an assignment");
    assert_eq!(
        refused,
        (
            5,
            "not a `KEY=value` line (a key is letters, digits and `_`)"
        )
    );
    let text = "\u{feff}# provider keys\n\
                \n\
                DEEPSEEK_API_KEY=sk-plain\r\n\
                export ANTHROPIC_API_KEY=sk-exported\n\
                \t  MISTRAL_API_KEY=sk-indented   \n\
                EMPTY=\n\
                _UNDER_9=x\n";
    assert_eq!(
        pairs(text),
        vec![
            pair("DEEPSEEK_API_KEY", "sk-plain"),
            pair("ANTHROPIC_API_KEY", "sk-exported"),
            pair("MISTRAL_API_KEY", "sk-indented"),
            pair("EMPTY", ""),
            pair("_UNDER_9", "x"),
        ]
    );
}

#[test]
fn quotes_comments_and_repeats_follow_the_shell() {
    let text = "SINGLE='a \"b\" $HOME \\n # not a comment' # trailing\n\
                DOUBLE=\"say \\\"hi\\\" \\\\ \\$HOME \\` \\n\"\n\
                BARE=abc#def # a comment\n\
                BLANK_BEFORE=   spaced value  \n\
                HASH_ONLY= # nothing\n\
                REPEAT=first\n\
                REPEAT=second\n";
    assert_eq!(
        pairs(text),
        vec![
            pair("SINGLE", "a \"b\" $HOME \\n # not a comment"),
            pair("DOUBLE", "say \"hi\" \\ $HOME ` \\n"),
            pair("BARE", "abc#def"),
            pair("BLANK_BEFORE", "spaced value"),
            pair("HASH_ONLY", ""),
            pair("REPEAT", "second"),
        ],
        "a single quote is literal, a double quote unescapes \\\" \\\\ \\$ \\` only, the last \
         assignment wins"
    );
}

#[test]
fn a_line_that_is_not_an_assignment_is_named_never_its_value() {
    for (text, line, why) in [
        (
            "OK=1\nsk-ant-api03-pasted-alone\n",
            2,
            "not a `KEY=value` line (no `=`)",
        ),
        (
            "9KEY=x\n",
            1,
            "not a `KEY=value` line (a key is letters, digits and `_`)",
        ),
        ("export\n", 1, "not a `KEY=value` line (no `=`)"),
        ("A='open\n", 1, "a single-quoted value is never closed"),
        ("A=\"open\n", 1, "a double-quoted value is never closed"),
        ("A='x' y\n", 1, "text follows a closing quote"),
        ("A=\"x\"y\n", 1, "text follows a closing quote"),
        (
            "A=x\0y\n",
            1,
            "a value holds a NUL byte, which no environment can carry",
        ),
    ] {
        assert_eq!(parse(text), Err((line, why)), "{text:?}");
    }
}

#[test]
fn an_unreadable_file_is_refused_with_its_path_and_no_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let gone = dir.path().join("gone.env");
    let error = read(&gone).expect_err("an absent file refuses");
    assert_eq!(error.path(), gone.as_path());
    assert_eq!(error.line(), None);
    assert!(
        error
            .to_string()
            .starts_with(&format!("env file {}: cannot be read (", gone.display()))
    );
    let not_utf8 = dir.path().join("bytes.env");
    std::fs::write(&not_utf8, b"KEY=\xff\xfe\n").expect("bytes");
    assert_eq!(
        read(&not_utf8).map_err(|e| e.to_string()),
        Err(format!(
            "env file {}: is not UTF-8 text",
            not_utf8.display()
        ))
    );
    let malformed = dir.path().join("malformed.env");
    std::fs::write(&malformed, "OK=1\nthe-secret-value-sentinel\n").expect("malformed");
    let error: EnvFileError = read(&malformed).expect_err("a bare word refuses");
    assert_eq!(error.line(), Some(2));
    assert!(
        !error.to_string().contains("sentinel"),
        "never a value: {error}"
    );
    let good = dir.path().join("keys.env");
    std::fs::write(&good, "NIKA_FAKE_KEY=from-the-file\n").expect("keys");
    assert_eq!(
        read(&good),
        Ok(vec![pair("NIKA_FAKE_KEY", "from-the-file")])
    );
}
