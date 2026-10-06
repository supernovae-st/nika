// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The parts a doubted request is asked in (R4 A11), kept beside the verifier's tests to bound
//! their size: each part an exact excerpt of the request (a byte slice, the request's own
//! separators kept where phrases merge), cut where a phrase ends and never inside a closed
//! literal; a list marker at a line start is no part, a label introduces the part after it, and
//! a phrase too short to be judged alone, of function words only or of no letter, stays with its
//! neighbour: no word of the request is dropped. The cut stays linear.

use std::time::{Duration, Instant};

use super::super::parts::phrases;
use super::parts;

/// Asserts the parts of each request exactly, and that every part is a byte slice of it.
fn cut_as(cases: &[(&str, &[&str])]) {
    for (intent, expected) in cases {
        let found = parts(intent);
        assert_eq!(found, *expected, "{intent:?}");
        for part in &found {
            assert!(intent.contains(part.as_str()), "{part:?} of {intent:?}");
        }
    }
}

/// A located part is the request's own phrase: punctuation cuts only where it ends a phrase,
/// so a path, a URL and a decimal stay whole (R4 A11, E36: « write the sum to
/// ./out/result.json » was offered, and repaired from, as « write the sum to »).
#[test]
fn a_part_keeps_its_path_url_and_decimal_whole() {
    cut_as(&[
        (
            "read ./data/input.csv, keep the rows above 3.5 units; fetch https://example.com/a.b; write the sum to ./out/result.json",
            &[
                "read ./data/input.csv",
                "keep the rows above 3.5 units",
                "fetch https://example.com/a.b",
                "write the sum to ./out/result.json",
            ],
        ),
        (
            "sum qty per status. Then write it to ./out/a.json.",
            &["sum qty per status", "Then write it to ./out/a.json"],
        ),
    ]);
}

/// Phrases merged into one part keep the request's own separators between them, never a
/// separator the cut invents: the part is the very slice of the request it spans.
#[test]
fn merged_phrases_keep_the_requests_own_separators() {
    cut_as(&[
        (
            "Read ./a.csv.\nDeduplicate.",
            &["Read ./a.csv.\nDeduplicate"],
        ),
        (
            "Read ./a.csv, deduplicate, write ./b.csv",
            &["Read ./a.csv, deduplicate", "write ./b.csv"],
        ),
        (
            "Sort. Then write it to ./out/a.json",
            &["Sort. Then write it to ./out/a.json"],
        ),
        ("Append the line Ok. Done.", &["Append the line Ok. Done"]),
    ]);
}

/// Every line end cuts a phrase, and so does each full-width end mark; a list marker (« - »,
/// « * », « • », « · », « 1. », « 2) ») is stripped from the phrase it opens. A phrase with no
/// letter (a number alone) is never judged alone: standing mid-line it stays with the part
/// before it (no word of the request is dropped); only one standing at a line start is a list
/// marker.
#[test]
fn list_markers_are_no_part_and_a_bare_number_stays_with_its_neighbour() {
    cut_as(&[
        (
            "Read ./data/a.csv\n- keep the rows where status is open\n- write them to ./out/b.csv",
            &[
                "Read ./data/a.csv",
                "keep the rows where status is open",
                "write them to ./out/b.csv",
            ],
        ),
        (
            "1. Read ./data/a.csv\n2. write them to ./out/b.csv",
            &["Read ./data/a.csv", "write them to ./out/b.csv"],
        ),
        (
            "1) Read ./data/a.csv\n2) write them to ./out/b.csv",
            &["Read ./data/a.csv", "write them to ./out/b.csv"],
        ),
        (
            "* read ./a.csv\n• keep the open rows\n· write ./b.csv",
            &["read ./a.csv", "keep the open rows", "write ./b.csv"],
        ),
        (
            "Read ./a.csv, 42, write ./b.csv",
            &["Read ./a.csv, 42", "write ./b.csv"],
        ),
        (
            "Read ./a.csv\n\nwrite ./b.csv\n",
            &["Read ./a.csv", "write ./b.csv"],
        ),
        (
            "1. Read ./a.csv\n2. Filter the paid rows\n3. Write ./b.csv",
            &["Read ./a.csv", "Filter the paid rows", "Write ./b.csv"],
        ),
    ]);
}

/// A phrase too short to be judged alone (one word, or under four letters or digits) is never
/// dropped: it is kept with the part before it, or with the next part when it opens the
/// request, or alone when nothing else is. A lone « No » answers the phrase before it and stays
/// with it, so the part asked is never the opposite of what the request says.
#[test]
fn a_short_phrase_and_a_lone_no_stay_with_their_neighbour() {
    cut_as(&[
        (
            "Sort, then read ./a.csv and write ./b.csv",
            &["Sort, then read ./a.csv and write ./b.csv"],
        ),
        (
            "Sort, dedupe, then read ./a.csv",
            &["Sort, dedupe, then read ./a.csv"],
        ),
        (
            "Read ./a.csv; write ./b.csv; done!",
            &["Read ./a.csv", "write ./b.csv; done"],
        ),
        ("deduplicate", &["deduplicate"]),
        (
            "Delete the old rows? No. Keep them and write ./b.csv",
            &["Delete the old rows? No", "Keep them and write ./b.csv"],
        ),
        ("Send it to Slack? No.", &["Send it to Slack? No"]),
    ]);
}

/// A phrase of function words only that restricts nothing (« and then », « the », « Then write
/// it ») is never judged alone: it stays with the part before it, so no word of the request is
/// dropped; a part stated twice is asked once.
#[test]
fn function_words_alone_stay_with_their_neighbour_and_a_repeated_part_is_asked_once() {
    cut_as(&[
        (
            "Read ./a.csv, and then, write ./b.csv",
            &["Read ./a.csv, and then", "write ./b.csv"],
        ),
        (
            "Read ./a.csv, the, write ./b.csv",
            &["Read ./a.csv, the", "write ./b.csv"],
        ),
        (
            "Read ./a.csv. Read ./a.csv. write ./b.csv",
            &["Read ./a.csv", "write ./b.csv"],
        ),
        (
            "Read ./a.csv. Sort it by date. Then write it.",
            &["Read ./a.csv", "Sort it by date. Then write it"],
        ),
    ]);
}

/// A phrase of no letter after a label (« 09:00 ») completes the label's part, never dropped; a
/// comma between two numbers (« 1, 2 or 3 ») and the period of « e.g. » end no phrase; a
/// straight double quote after a digit is an inch, never an opening: the phrases after it are
/// cut as usual and the quoted literal later stays whole.
#[test]
fn a_value_a_number_list_an_abbreviation_and_an_inch_stay_in_their_phrase() {
    let numbers = "Keep the rows whose status is 1, 2 or 3 and write ./b.csv";
    cut_as(&[
        (
            "Send the report at this time: 09:00.",
            &["Send the report at this time: 09:00"],
        ),
        (numbers, &[numbers]),
        (
            "Read ./a.csv, e.g. the sales file",
            &["Read ./a.csv", "e.g. the sales file"],
        ),
        (
            "It is 5\" wide, write \"Done\" to ./c.txt",
            &["It is 5\" wide", "write \"Done\" to ./c.txt"],
        ),
        (
            "It is 5\" wide, write \"a, b\" to ./c.txt, then stop",
            &["It is 5\" wide", "write \"a, b\" to ./c.txt", "then stop"],
        ),
    ]);
    assert_eq!(phrases(numbers), [(0, numbers.len(), None)]);
    let example = "Read ./a.csv, e.g. the sales file";
    assert_eq!(
        phrases(example),
        [(0, 12, Some(',')), (13, example.len(), None)]
    );
}

/// A phrase ending at a colon is a label: it introduces the next phrase that can be judged
/// alone and joins it (« Change: … », a heading, the session's correction frame), so no part is
/// made of label words alone. A label nothing follows extends the last part, or stands alone.
#[test]
fn a_label_attaches_forward_to_the_part_it_introduces() {
    cut_as(&[
        ("Change: use 12-hour times", &["Change: use 12-hour times"]),
        (
            "Read ./data/calendar.json and write the list to ./out/week.md\nChange: use 12-hour times",
            &[
                "Read ./data/calendar.json and write the list to ./out/week.md",
                "Change: use 12-hour times",
            ],
        ),
        (
            "Original request:\nRead ./a.csv\nCorrection (it takes precedence over the original where they differ; every other requirement stands):\nwrite ./b.csv",
            &[
                "Original request:\nRead ./a.csv",
                "Correction (it takes precedence over the original where they differ; every other requirement stands):\nwrite ./b.csv",
            ],
        ),
        (
            "Steps:\n- read ./a.csv\n- write ./b.csv",
            &["Steps:\n- read ./a.csv", "write ./b.csv"],
        ),
        (
            "Read ./a.csv and write ./b.csv. Note:",
            &["Read ./a.csv and write ./b.csv. Note"],
        ),
        ("Note:", &["Note"]),
    ]);
}

/// Nothing inside a closed literal is a cut, in each quoting the request may use: double
/// quotes, guillemets, curly quotes, backticks, parentheses, and single quotes opened after a
/// space (an apostrophe inside a word opens nothing).
#[test]
fn a_closed_literal_is_never_cut() {
    cut_as(&[
        (
            "Write \"a, b, c\" to ./out/a.txt, then stop.",
            &["Write \"a, b, c\" to ./out/a.txt", "then stop"],
        ),
        (
            "Écris « bonjour, monde » dans ./out/a.txt, puis arrête.",
            &["Écris « bonjour, monde » dans ./out/a.txt", "puis arrête"],
        ),
        (
            "Write “x, y” to ./out/a.txt, then stop.",
            &["Write “x, y” to ./out/a.txt", "then stop"],
        ),
        (
            "Write the totals (one line per day, sorted) to ./out/a.txt, then stop",
            &[
                "Write the totals (one line per day, sorted) to ./out/a.txt",
                "then stop",
            ],
        ),
        (
            "Append the line 'Done. Thanks.' to ./out/log.txt, then stop.",
            &[
                "Append the line 'Done. Thanks.' to ./out/log.txt",
                "then stop",
            ],
        ),
        (
            "Don't overwrite ./out/a.txt, write ./out/b.txt",
            &["Don't overwrite ./out/a.txt", "write ./out/b.txt"],
        ),
    ]);
}

/// An opening mark that never closes is an ordinary character: the phrases after it are cut as
/// usual.
#[test]
fn an_unmatched_quote_is_an_ordinary_character() {
    cut_as(&[
        (
            "Write `a, b` to ./a.txt, write \"unclosed, here, then ./b.txt",
            &[
                "Write `a, b` to ./a.txt",
                "write \"unclosed, here",
                "then ./b.txt",
            ],
        ),
        (
            "Écris « bonjour, puis lis ./a.txt, écris ./b.txt",
            &["Écris « bonjour", "puis lis ./a.txt", "écris ./b.txt"],
        ),
        (
            "Write 'draft to ./a.txt, then stop",
            &["Write 'draft to ./a.txt", "then stop"],
        ),
    ]);
}

/// The full-width marks cut text written without spaces (a comma « ， » and « 、 » too), and a
/// phrase of four or more of its characters is judged alone; a shorter one stays with the part
/// before it.
#[test]
fn full_width_marks_cut_cjk_text_and_four_characters_stand_alone() {
    cut_as(&[
        (
            "读取数据文件，删除重复行、写入结果文件",
            &["读取数据文件", "删除重复行", "写入结果文件"],
        ),
        (
            "读取数据文件，排序，写入结果文件",
            &["读取数据文件，排序", "写入结果文件"],
        ),
        (
            "读取 ./a.csv。写入 ./b.csv",
            &["读取 ./a.csv", "写入 ./b.csv"],
        ),
        (
            "ファイルを読む。結果を書く",
            &["ファイルを読む", "結果を書く"],
        ),
    ]);
}

/// A request that is one quoted literal is one part: the whole request, its marks included.
#[test]
fn a_request_of_only_a_quote_is_one_part() {
    cut_as(&[
        (
            "\"Read ./a.csv, then write ./b.csv\"",
            &["\"Read ./a.csv, then write ./b.csv\""],
        ),
        (
            "« Lis ./a.csv, puis écris ./b.csv »",
            &["« Lis ./a.csv, puis écris ./b.csv »"],
        ),
    ]);
}

/// Twenty thousand guillemets and twenty thousand other opening marks that never close are
/// ordinary characters, read in one pass: the request stays one phrase and one part, cut in far
/// less than the bound a scan per mark would need.
#[test]
fn many_unmatched_marks_are_cut_in_linear_time() {
    let marks = format!("{}{}", "«".repeat(20_000), "“(".repeat(10_000));
    let intent = format!("Read ./a.csv and write ./b.csv {marks}");
    let started = Instant::now();
    let found = parts(&intent);
    let spent = started.elapsed();
    assert_eq!(found, [intent.as_str()]);
    assert_eq!(phrases(&intent), [(0, intent.len(), None)]);
    assert!(spent < Duration::from_secs(2), "{spent:?}");
}

/// Emoji, a skin-tone modifier, a joined sequence and combining accents never split a
/// character: every part is a slice of the request on its own boundaries, and a phrase of
/// emoji alone has no letter: never judged alone, it opens the part after it.
#[test]
fn emoji_and_combining_marks_stay_whole() {
    cut_as(&[
        (
            "Write 👋🏽 to ./out/a.txt, then read ./cafe\u{301}.csv",
            &["Write 👋🏽 to ./out/a.txt", "then read ./cafe\u{301}.csv"],
        ),
        ("🎉🎉, write ./b.txt", &["🎉🎉, write ./b.txt"]),
        (
            "👩\u{200d}💻 writes ./a.txt; e\u{301}cris ./b.txt",
            &["👩\u{200d}💻 writes ./a.txt", "e\u{301}cris ./b.txt"],
        ),
    ]);
}

/// Twenty thousand periods are read in one pass, as the opening marks are: the request stays one
/// part, cut in far less than the bound a scan per period would need. RED: the abbreviation check
/// (`parts::abbreviated`) lowercases the whole text before every period, a quadratic cut
/// (measured 3.4 s for ten thousand periods, 14.7 s for twenty thousand, 122 s for forty
/// thousand on a loaded host).
#[test]
fn many_periods_are_cut_in_linear_time() {
    let intent = format!("Read ./a.csv and write ./b.csv {}", ".".repeat(20_000));
    let started = Instant::now();
    let found = parts(&intent);
    let spent = started.elapsed();
    // The last period ends the one phrase; every other one is followed by a period.
    assert_eq!(found, [&intent[..intent.len() - 1]]);
    assert!(spent < Duration::from_secs(2), "{spent:?}");
}
