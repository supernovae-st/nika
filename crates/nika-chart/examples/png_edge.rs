//! PNG edge battery: dimensions + incompressible data (all-literal deflate).
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::type_complexity,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    clippy::float_cmp
)]
fn main() {
    // 1×1 · 1×N · N×1 · odd width · noise (LCG · no RLE wins → pure literals)
    let cases: Vec<(&str, u32, u32, Box<dyn Fn(u32, u32) -> [u8; 3]>)> = vec![
        ("edge-1x1", 1, 1, Box::new(|_, _| [200, 100, 50])),
        (
            "edge-1x64",
            1,
            64,
            Box::new(|_, y| [(y * 4) as u8, 0, 255 - (y * 4) as u8]),
        ),
        ("edge-64x1", 64, 1, Box::new(|x, _| [(x * 4) as u8, 128, 7])),
        (
            "edge-641x33",
            641,
            33,
            Box::new(|x, y| [(x % 256) as u8, (y * 7) as u8, ((x + y) % 256) as u8]),
        ),
        (
            "edge-noise-256",
            256,
            256,
            Box::new(|x, y| {
                let mut s = (u64::from(x) * 2_654_435_761) ^ u64::from(y).wrapping_mul(40_503);
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                [(s >> 33) as u8, (s >> 41) as u8, (s >> 49) as u8]
            }),
        ),
    ];
    for (name, w, h, f) in &cases {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..*h {
            for x in 0..*w {
                rgb.extend_from_slice(&f(x, y));
            }
        }
        let png = nika_chart::png::encode_rgb(*w, *h, &rgb);
        let png2 = nika_chart::png::encode_rgb(*w, *h, &rgb);
        assert_eq!(png, png2, "determinism {name}");
        std::fs::write(format!("{name}.png"), &png).expect("write");
        // Raw RGB sidecar for the pixel-exact python check.
        std::fs::write(format!("{name}.rgb"), &rgb).expect("write rgb");
        println!(
            "{name}.png · {}x{} · {} bytes (raw {})",
            w,
            h,
            png.len(),
            rgb.len()
        );
    }
}
