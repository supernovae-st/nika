//! 2-pass layout (CHT §3bis law 7) — pass 1 picks ticks with default margins ·
//! pass 2 measures labels and finalizes. Closed-form, never a fixpoint loop.

use crate::metrics::measure;

/// The resolved plot frame inside the canvas.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub canvas_w: u32,
    pub canvas_h: u32,
    /// Plot origin (top-left) and size.
    pub x0: f64,
    pub y0: f64,
    pub pw: f64,
    pub ph: f64,
}

pub const TITLE_PX: f64 = 13.0;
pub const LABEL_PX: f64 = 11.0;

/// Finalize the frame from measured y labels (pass 2).
pub fn frame(canvas_w: u32, canvas_h: u32, y_labels: &[String], has_legend: bool) -> Frame {
    let top = 16.0 + TITLE_PX + if has_legend { 20.0 } else { 8.0 };
    let bottom = 20.0 + LABEL_PX + 8.0;
    let right = 14.0;
    let widest = y_labels
        .iter()
        .map(|l| measure(l, LABEL_PX))
        .fold(0.0_f64, f64::max);
    let left = widest + 8.0 + 6.0;
    let x0 = left;
    let y0 = top;
    let pw = (f64::from(canvas_w) - left - right).max(40.0);
    let ph = (f64::from(canvas_h) - top - bottom).max(30.0);
    Frame {
        canvas_w,
        canvas_h,
        x0,
        y0,
        pw,
        ph,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;

    #[test]
    fn wider_labels_push_plot_right() {
        let a = frame(480, 300, &["5".to_owned()], false);
        let b = frame(480, 300, &["$0.000062".to_owned()], false);
        assert!(b.x0 > a.x0);
        assert!(b.pw < a.pw);
    }

    #[test]
    fn legend_reserves_top_space() {
        let a = frame(480, 300, &["5".to_owned()], false);
        let b = frame(480, 300, &["5".to_owned()], true);
        assert!(b.y0 > a.y0);
    }
}
