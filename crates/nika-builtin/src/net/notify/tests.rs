// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

/// A webhook credential stand-in, compared by value and never printed.
const TOKEN: &str = "fakeHookToken0123456789";

#[test]
fn receiver_is_recognized_from_the_documented_host_and_path_only() {
    let cases = [
        ("https://hooks.slack.com/services/T0/B0/X", Receiver::Slack),
        ("https://HOOKS.SLACK.COM/services/T0/B0/X", Receiver::Slack),
        ("https://hooks.slack.com./services/T0/B0/X", Receiver::Slack),
        (
            "https://hooks.slack-gov.com/services/T0/B0/X",
            Receiver::Slack,
        ),
        ("https://hooks.slack.com/triggers/T0/1/x", Receiver::Generic),
        ("https://hooks.slack.com/services", Receiver::Generic),
        (
            "https://hooks.slack.com.example.test/services/T0/B0/X",
            Receiver::Generic,
        ),
        ("https://example.test/services/T0/B0/X", Receiver::Generic),
        ("http://127.0.0.1:8080/services/T0/B0/X", Receiver::Generic),
        ("https://discord.com/api/webhooks/1/tok", Receiver::Discord),
        (
            "https://discordapp.com/api/webhooks/1/tok?wait=true",
            Receiver::Discord,
        ),
        (
            "https://ptb.discord.com/api/v10/webhooks/1/tok",
            Receiver::Discord,
        ),
        (
            "https://discord.com/api/webhooks/1/tok/slack",
            Receiver::Generic,
        ),
        (
            "https://discord.com/api/webhooks/1/tok/github",
            Receiver::Generic,
        ),
        (
            "https://discord.com/api/vx/webhooks/1/tok",
            Receiver::Generic,
        ),
        (
            "https://discord.com.example.test/api/webhooks/1/tok",
            Receiver::Generic,
        ),
        ("not a url", Receiver::Generic),
    ];
    for (target, expected) in cases {
        assert_eq!(Receiver::of(target), expected, "{target}");
    }
}

#[test]
fn chat_text_names_a_non_default_severity_and_the_data_context() {
    assert_eq!(chat_text("done", "info", None), "done");
    assert_eq!(chat_text("done", "INFO", None), "done");
    assert_eq!(
        chat_text("disk full", "warning", None),
        "[warning] disk full"
    );
    assert_eq!(
        chat_text("done", "info", Some(&Value::String("run 42".to_owned()))),
        "done\nrun 42"
    );
    assert_eq!(
        chat_text("done", "info", Some(&json!({ "count": 7 }))),
        "done\n{\"count\":7}"
    );
    assert_eq!(chat_text("done", "info", Some(&Value::Null)), "done");
}

#[test]
fn slack_text_carries_its_control_characters_as_entities() {
    assert_eq!(
        slack_text("a & b <!channel> <@U1> <https://x.test|y>"),
        "a &amp; b &lt;!channel&gt; &lt;@U1&gt; &lt;https://x.test|y&gt;"
    );
    assert_eq!(slack_text("plain text"), "plain text");
}

#[test]
fn discord_text_escapes_every_opener_and_keeps_bare_urls() {
    assert_eq!(
        discord_text(r"**b** _i_ ~~s~~ `c` ||p|| [l](u) <@1> \"),
        r"\*\*b\*\* \_i\_ \~\~s\~\~ \`c\` \|\|p\|\| \[l\](u) \<@1\> \\"
    );
    // Heading, list and quote open a line only, indented or not.
    assert_eq!(
        discord_text("# head\n- item\n  > quote\nplain - dash # hash"),
        "\\# head\n\\- item\n  \\> quote\nplain - dash # hash"
    );
    // A bare URL is copied as is; the escape resumes after it.
    assert_eq!(
        discord_text("see https://x.test/a_b*c~d|e and_more"),
        "see https://x.test/a_b*c~d|e and\\_more"
    );
    assert_eq!(discord_text("HTTPS://X.test/_"), "HTTPS://X.test/_");
    // Autolink stops at `<`: a mention glued to a URL is still escaped.
    assert_eq!(discord_text("https://x.test<@1>"), "https://x.test\\<@1\\>");
    // A masked link around a URL loses its brackets, never the URL.
    assert_eq!(
        discord_text("[docs](https://example.test)"),
        "\\[docs\\](https://example.test)"
    );
}

#[test]
fn discord_content_is_bounded_without_splitting_an_escape() {
    let short = "a".repeat(2000);
    assert_eq!(bound_escaped(&short, 2000), short);

    // 1999 letters and an escaped star: 2001 units, cut before the pair.
    let over = format!("{}\\*", "a".repeat(1999));
    let bounded = bound_escaped(&over, 2000);
    assert_eq!(bounded, format!("{}…", "a".repeat(1999)));
    assert_eq!(bounded.encode_utf16().count(), 2000);

    // Two UTF-16 units per emoji: 1000 fit exactly, 1001 do not.
    let fits = "😀".repeat(1000);
    assert_eq!(bound_escaped(&fits, 2000), fits);
    let cut = bound_escaped(&"😀".repeat(1001), 2000);
    assert_eq!(cut, format!("{}…", "😀".repeat(999)));
    assert!(cut.encode_utf16().count() <= 2000);
}

#[test]
fn discord_payload_parses_no_mention_and_keeps_the_limit() {
    let long = "x ".repeat(1500);
    let payload = Receiver::Discord.payload(&long, "info", None);
    let content = payload["content"].as_str().unwrap_or_default();
    assert!(content.encode_utf16().count() <= DISCORD_CONTENT_MAX);
    assert!(content.ends_with('…'));
    assert_eq!(payload["allowed_mentions"], json!({ "parse": [] }));
    assert_eq!(
        payload.as_object().map(serde_json::Map::len),
        Some(2),
        "content and allowed_mentions only"
    );
}

#[test]
fn answer_is_one_bounded_line_without_the_target() {
    let slack = format!("https://hooks.slack.com/services/T0/B0/{TOKEN}");
    assert_eq!(answer(400, b"no_text", &slack).as_deref(), Some("no_text"));
    assert_eq!(answer(400, b"", &slack), None);
    assert_eq!(answer(400, b" \n\t ", &slack), None);

    let echo = format!("Cannot POST /services/T0/B0/{TOKEN}\n\u{1b}[31mred");
    let said = answer(404, echo.as_bytes(), &slack).unwrap_or_default();
    assert!(!said.contains(TOKEN), "the path credential is withheld");
    assert!(said.contains("[withheld]"));
    assert!(!said.contains(['\n', '\u{1b}']), "one line, no control");

    let discord = format!("https://discord.com/api/webhooks/1/{TOKEN}");
    let said = answer(
        400,
        br#"{"message": "Cannot send an empty message", "code": 50006}"#,
        &discord,
    );
    assert_eq!(
        said.as_deref(),
        Some(r#"{"message": "Cannot send an empty message", "code": 50006}"#)
    );

    let wordy = "word ".repeat(1000);
    let said = answer(400, wordy.as_bytes(), &slack).unwrap_or_default();
    assert!(said.ends_with('…'));
    assert!(said.chars().count() <= 401);
}

#[test]
fn target_parts_list_the_credential_carriers_longest_first() {
    let target =
        format!("https://user:{TOKEN}@chat.example.test/v1/spaces/AAAA/messages?key={TOKEN}");
    let parts = target_parts(&target);
    assert!(parts.first() == Some(&target), "the whole target first");
    assert!(parts.iter().any(|part| part == TOKEN));
    assert!(parts.windows(2).all(|pair| pair[0].len() >= pair[1].len()));
    assert!(parts.iter().all(|part| part.chars().count() >= ECHO_MIN));
}
