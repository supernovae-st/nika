use super::{fmt_scope_totals, fmt_wh};

/// The display grain is ceiling-honest: rounding is UP, and the
/// floor of the grain is 0.001 — `0.000` would claim free
/// inference for a task that does spend.
#[test]
fn fmt_wh_never_prints_zero_for_a_positive_bound() {
    assert_eq!(fmt_wh(0.0004), "0.001");
    assert_eq!(fmt_wh(0.004), "0.004");
    assert_eq!(fmt_wh(0.087), "0.087");
    assert_eq!(fmt_wh(2.34), "2.3");
    assert_eq!(fmt_wh(660.1), "660.1");
}

/// The scope-total display: one class states the number bare (the
/// class rides the count line); several classes join, each wearing
/// its class; nothing measured → no claim at all. (The partition
/// MATH is `nika_check::energy`'s — these pin the RENDER.)
#[test]
fn scope_totals_render_one_claim_per_class() {
    assert_eq!(
        fmt_scope_totals(&[
            ("device".to_owned(), 2.0),
            ("fleet".to_owned(), 4.0),
            ("gpu".to_owned(), 1.5),
        ]),
        "device ≤ 2.0 Wh · fleet ≤ 4.0 Wh · gpu ≤ 1.5 Wh"
    );
    assert_eq!(fmt_scope_totals(&[("gpu".to_owned(), 0.087)]), "≤ 0.087 Wh");
    assert_eq!(fmt_scope_totals(&[]), "");
}
