// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shape of a date-time text: its form and its offset. Text order is time order only
//! between date-times that share both, so a judgment that orders or compares two date-time
//! texts reads their shapes first. Lexical only: no calendar, zone or instant is validated,
//! normalized or parsed. The behavioural judge reads it where it sorts and orders values;
//! `nika-compile-fidelity` reads this same classifier for Law 25 (`TEXT ORDER ON INSTANTS`) and
//! its masked temporal observation, at its historical path `fidelity::instant_shape`.

/// The form (every digit masked as `9`) and the offset (`Z`, `+02:00`, `+0200`, `+02`, or empty
/// when none) of an ISO-8601 date-time such as `2026-09-01T02:30:00+02:00`
/// (`9999-99-99T99:99:99`, `+02:00`); `None` for any other text, a bare date included. Text order
/// is time order only between date-times that share both.
#[must_use]
pub fn instant_shape(text: &str) -> Option<(String, String)> {
    let bytes = text.as_bytes();
    let digit = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_digit);
    let is = |at: usize, c: u8| bytes.get(at) == Some(&c);
    let head = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15]
        .into_iter()
        .all(digit)
        && is(4, b'-')
        && is(7, b'-')
        && (is(10, b'T') || is(10, b' '))
        && is(13, b':');
    if !head {
        return None;
    }
    let mut end = 16;
    if is(end, b':') && digit(end + 1) && digit(end + 2) {
        end += 3;
    }
    if is(end, b'.') && digit(end + 1) {
        end += 1;
        while digit(end) {
            end += 1;
        }
    }
    let (form, offset) = text.split_at(end);
    let zone = offset.strip_prefix(['+', '-']).map(str::as_bytes);
    let hours = |z: &[u8]| z.len() >= 2 && z[..2].iter().all(u8::is_ascii_digit);
    let valid = match zone {
        None => offset.is_empty() || offset == "Z",
        Some(z) => {
            hours(z)
                && match z.len() {
                    2 => true,
                    4 => z[2..].iter().all(u8::is_ascii_digit),
                    5 => z[2] == b':' && z[3..].iter().all(u8::is_ascii_digit),
                    _ => false,
                }
        }
    };
    let masked = form
        .chars()
        .map(|c| if c.is_ascii_digit() { '9' } else { c })
        .collect();
    valid.then(|| (masked, offset.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::instant_shape as shape;

    #[test]
    fn a_date_time_has_a_form_and_an_offset() {
        let pair = |form: &str, offset: &str| Some((form.to_owned(), offset.to_owned()));
        assert_eq!(
            shape("2026-09-01T02:30:00+02:00"),
            pair("9999-99-99T99:99:99", "+02:00")
        );
        assert_eq!(
            shape("2026-09-01T00:30:00Z"),
            pair("9999-99-99T99:99:99", "Z")
        );
        assert_eq!(
            shape("2026-09-01T00:30:00"),
            pair("9999-99-99T99:99:99", "")
        );
        assert_eq!(shape("2026-09-01 00:30"), pair("9999-99-99 99:99", ""));
        assert_eq!(
            shape("2026-09-01T00:30:00.125-0500"),
            pair("9999-99-99T99:99:99.999", "-0500")
        );
        assert_eq!(
            shape("2026-09-01T00:30:00+02"),
            pair("9999-99-99T99:99:99", "+02")
        );
    }

    #[test]
    fn anything_else_has_no_shape() {
        for text in [
            "2026-09-01",
            "2026-09-01T00:30:00+2:00",
            "2026-09-01T00:30:00 UTC",
            "2026-09-01T00:30:00z",
            "01/09/2026 00:30",
            "sale",
            "",
        ] {
            assert_eq!(shape(text), None, "{text}");
        }
    }
}
