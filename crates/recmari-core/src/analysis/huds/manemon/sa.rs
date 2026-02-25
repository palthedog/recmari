use image::{Rgb, RgbImage};
use tracing::{debug, warn};

use crate::analysis::common::{find_bar_boundary, rgb_to_hsv, BarSegment, Hsv, Scanline};
use crate::rect::PixelRect;

use super::REF_WIDTH;

/// P1 SA gauge scanline at 1920x1080. Scans left-to-right (gauge fills from edge toward center).
pub(super) const P1_SA_GAUGE: Scanline = Scanline {
    x_start: 188,
    x_end: 413,
    y: 1002,
};

/// P2 SA gauge — horizontal mirror of P1.
pub(super) const P2_SA_GAUGE: Scanline = Scanline {
    x_start: REF_WIDTH - P1_SA_GAUGE.x_start,
    x_end: REF_WIDTH - P1_SA_GAUGE.x_end,
    y: P1_SA_GAUGE.y,
};

/// P1 SA stock digit bounding box at 1920x1080.
pub(super) const P1_SA_DIGIT: PixelRect = PixelRect {
    x: 120,
    y: 960,
    w: 160 - 120,
    h: 1020 - 960,
};

/// P2 SA stock digit — horizontal mirror of P1.
pub(super) const P2_SA_DIGIT: PixelRect = PixelRect {
    x: REF_WIDTH - P1_SA_DIGIT.x - P1_SA_DIGIT.w,
    y: P1_SA_DIGIT.y,
    w: P1_SA_DIGIT.w,
    h: P1_SA_DIGIT.h,
};

/// Probe point for SA digit recognition, as absolute pixel coordinates
/// at 1920x1080 reference resolution (P1 side).
#[derive(Clone, Copy)]
pub(super) struct Probe {
    pub(super) x: u32,
    pub(super) y: u32,
}

/// Probes used to distinguish digits 0–3.
/// probe[i] is foreground when the displayed digit is i.
/// Found via: `recmari probe-scan --image both_sa0.png:0 --image both_sa1.png:1
///   --image both_sa2.png:2 --image both_sa3.png:3`
pub(super) const SA_DIGIT_PROBES: [Probe; 4] = [
    Probe { x: 129, y: 989 }, // foreground for: 0
    Probe { x: 138, y: 985 }, // foreground for: 1
    Probe { x: 144, y: 961 }, // foreground for: 2
    Probe { x: 133, y: 995 }, // foreground for: 3
];

/// Combine digit recognition with bar fill to produce a 0.0–3.0 SA value.
pub(super) fn read_sa_value(
    image: &RgbImage,
    probes: &[(u32, u32); 4],
    sa_scan: &Scanline,
) -> Option<f64> {
    let Some(stock) = classify_sa_digit(image, probes) else {
        warn!("SA digit classification failed");
        return None;
    };
    assert!(stock <= 3, "SA stock count must be 0–3, got {stock}");

    if stock >= 3 {
        return Some(3.0);
    }

    debug!("SA bar scan");
    let Some(bar_fill) = find_bar_boundary(image, sa_scan, classify_sa_pixel) else {
        warn!(stock, "SA bar fill detection failed");
        return None;
    };

    assert!(bar_fill <= 1.0, "bar_fill must be 0.0–1.0, got {bar_fill}");
    Some(stock as f64 + bar_fill)
}

/// Recognize the SA stock digit (0–3) or CA text from probe positions.
/// Returns None if the digit is unreadable.
fn classify_sa_digit(image: &RgbImage, probes: &[(u32, u32); 4]) -> Option<u8> {
    let ca_count = probes
        .iter()
        .filter(|&&(x, y)| is_ca_text_pixel(*image.get_pixel(x, y)))
        .count();
    if ca_count >= 2 {
        debug!("SA digit classified as CA");
        return Some(3);
    }

    let mut digit = None;
    for (i, &(x, y)) in probes.iter().enumerate() {
        debug!(i, x, y, "checking SA digit probe");
        if is_sa_digit_foreground(image, x, y) {
            digit = Some(i as u8);
            break;
        }
    }

    let Some(digit) = digit else {
        warn!("SA digit: no probe matched");
        return None;
    };

    debug!(digit, "SA digit classified");
    Some(digit)
}

/// Check if a pixel belongs to the golden "CA" text overlay.
/// CA gold has a warm hue (H≈30-50) distinct from digit outline yellow (H≈50-65)
/// and digit fill blue (H≈220-240).
fn is_ca_text_pixel(rgb: Rgb<u8>) -> bool {
    let hsv = rgb_to_hsv(rgb);
    hsv.h >= 25.0 && hsv.h <= 50.0 && hsv.s >= 0.5 && hsv.v >= 0.6
}

/// Check if a pixel at (x, y) is part of an SA stock digit glyph.
/// Samples a 3x3 neighborhood and uses majority voting for anti-aliasing robustness.
fn is_sa_digit_foreground(image: &RgbImage, cx: u32, cy: u32) -> bool {
    let mut fg_count = 0;
    let mut total = 0;

    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let x = (cx as i32 + dx).clamp(0, image.width() as i32 - 1) as u32;
            let y = (cy as i32 + dy).clamp(0, image.height() as i32 - 1) as u32;
            let rgb = image.get_pixel(x, y);
            let hsv = rgb_to_hsv(*rgb);
            debug!(
                x,
                y,
                hsv = format!("{}", rgb_to_hsv(*rgb)),
                "checking SA digit pixel"
            );
            total += 1;
            if is_digit_fill_pixel(hsv) || is_digit_outline_pixel(hsv) {
                fg_count += 1;
            }
        }
    }

    fg_count > total / 2
}

/// Blue interior fill of SA stock digit glyphs (0–3).
fn is_digit_fill_pixel(hsv: Hsv) -> bool {
    hsv.h >= 200.0 && hsv.h <= 230.0 && hsv.s >= 0.7 && hsv.v >= 0.7
}

/// Yellow/gold outline of SA stock digit glyphs (0–3).
/// B threshold accounts for anti-aliasing variation between P1/P2 sides.
fn is_digit_outline_pixel(hsv: Hsv) -> bool {
    hsv.h >= 55.0 && hsv.h <= 75.0 && hsv.s >= 0.25 && hsv.v >= 0.75
}

/// SA gauge pixel — any recognized gauge state.
pub(super) fn is_sa_gauge_pixel(hsv: Hsv) -> bool {
    is_gauge_empty(hsv) || is_gauge_sa(hsv) || is_gauge_ca(hsv)
}

/// Depleted SA gauge background. Same hue as HP bar background,
/// with a V floor to reject VS screen's dim blue (V≈0.30-0.39).
fn is_gauge_empty(hsv: Hsv) -> bool {
    hsv.h > 215.0 && hsv.h < 222.0 && hsv.s > 0.95 && hsv.v >= 0.60
}

/// SA gauge filled — P1 is pink (H≈320–360), P2 is cyan (H≈175–210).
fn is_gauge_sa(hsv: Hsv) -> bool {
    let p1_pink = hsv.h >= 320.0;
    let p2_cyan = hsv.h >= 175.0 && hsv.h <= 210.0;
    (p1_pink || p2_cyan) && hsv.s >= 0.15 && hsv.v >= 0.80
}

/// CA ready.
fn is_gauge_ca(hsv: Hsv) -> bool {
    hsv.h >= 300.0 && hsv.h <= 345.0 && hsv.s >= 0.15 && hsv.v >= 0.85
}

fn classify_sa_pixel(rgb: Rgb<u8>) -> BarSegment {
    let hsv = rgb_to_hsv(rgb);
    if is_gauge_sa(hsv) {
        BarSegment::Foreground
    } else if is_gauge_empty(hsv) {
        BarSegment::Background
    } else {
        BarSegment::Unknown
    }
}

/// An absolute pixel position (P1 side, 1920x1080) with its foreground bitmask.
/// Bit N is set if digit N has foreground at this position.
pub struct ProbeScanEntry {
    pub x: u32,
    pub y: u32,
    /// Bitmask: bit 0 = digit "0", bit 1 = digit "1", etc.
    pub fg_mask: u8,
}

/// Scan all positions in the SA digit bounding box.
/// Each entry in `digit_images` is (image, digit_value) where both P1 and P2
/// show the specified digit. A position counts as foreground for a digit
/// only if both P1 and P2 sides agree. Positions where P1/P2 disagree
/// are excluded.
pub fn scan_sa_digit_probes(digit_images: &[(RgbImage, u8)]) -> Vec<ProbeScanEntry> {
    assert!(!digit_images.is_empty(), "need at least one image");
    let ref_width = super::REF_WIDTH;
    let ref_height = super::REF_HEIGHT;
    for (img, d) in digit_images {
        assert!(*d <= 3, "digit must be 0–3, got {d}");
        assert!(
            img.width() >= ref_width && img.height() >= ref_height,
            "image too small: {}x{}, need at least {ref_width}x{ref_height}",
            img.width(),
            img.height(),
        );
    }

    let mut entries = Vec::new();

    for y in P1_SA_DIGIT.y..P1_SA_DIGIT.y + P1_SA_DIGIT.h {
        for x in P1_SA_DIGIT.x..P1_SA_DIGIT.x + P1_SA_DIGIT.w {
            let p2_x = P2_SA_DIGIT.x + (x - P1_SA_DIGIT.x);

            let mut fg_mask: u8 = 0;
            let mut ambiguous = false;

            for (img, digit) in digit_images {
                let p1_fg = is_sa_digit_foreground(img, x, y);
                let p2_fg = is_sa_digit_foreground(img, p2_x, y);

                if p1_fg != p2_fg {
                    ambiguous = true;
                    break;
                }

                if p1_fg {
                    fg_mask |= 1 << digit;
                }
            }

            if !ambiguous {
                entries.push(ProbeScanEntry { x, y, fg_mask });
            }
        }
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn load_fixture(name: &str) -> RgbImage {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/frames")
            .join(name);
        image::open(&path)
            .unwrap_or_else(|e| panic!("failed to load {}: {}", path.display(), e))
            .into_rgb8()
    }

    #[test]
    fn classify_sa_digit_cases() {
        let p1_probes = SA_DIGIT_PROBES.map(|p| (p.x, p.y));
        let p2_probes = SA_DIGIT_PROBES.map(|p| (P2_SA_DIGIT.x + p.x - P1_SA_DIGIT.x, p.y));

        let cases: &[(&str, &[(u32, u32); 4], u8)] = &[
            ("both_sa0.png", &p1_probes, 0),
            ("both_sa0.png", &p2_probes, 0),
            ("both_sa1.png", &p1_probes, 1),
            ("both_sa1.png", &p2_probes, 1),
            ("both_sa2.png", &p1_probes, 2),
            ("both_sa2.png", &p2_probes, 2),
            ("both_sa3.png", &p1_probes, 3),
            ("both_sa3.png", &p2_probes, 3),
            ("p1_ca.png", &p1_probes, 3),
            ("p1_ca.png", &p2_probes, 0),
            ("frame_1560.png", &p1_probes, 0),
            ("frame_1560.png", &p2_probes, 0),
            ("frame_3600.png", &p1_probes, 1),
            ("frame_3600.png", &p2_probes, 1),
            ("frame_4080.png", &p1_probes, 1),
            ("frame_4080.png", &p2_probes, 2),
            ("frame_2640.png", &p1_probes, 0),
            ("frame_2640.png", &p2_probes, 1),
            ("frame_4920.png", &p1_probes, 2),
            ("frame_4920.png", &p2_probes, 3),
        ];

        for &(file, probes, expected) in cases {
            let img = load_fixture(file);
            assert_eq!(
                classify_sa_digit(&img, probes),
                Some(expected),
                "file={file} probe0=({}, {})",
                probes[0].0,
                probes[0].1,
            );
        }
    }

    fn assert_sa_approx(actual: Option<f64>, expected: f64, tolerance: f64, label: &str) {
        let val = actual.unwrap_or_else(|| panic!("{label}: expected Some, got None"));
        assert!(
            (val - expected).abs() <= tolerance,
            "{label}: expected {expected:.2}±{tolerance}, got {val:.4}",
        );
    }

    #[test]
    fn read_sa_value_cases() {
        let p1_probes = SA_DIGIT_PROBES.map(|p| (p.x, p.y));
        let p2_probes = SA_DIGIT_PROBES.map(|p| (P2_SA_DIGIT.x + p.x - P1_SA_DIGIT.x, p.y));

        let cases: &[(&str, f64, f64)] = &[
            ("frame_1560.png", 0.10, 0.06),
            ("frame_2640.png", 0.99, 1.11),
            ("frame_3600.png", 1.44, 1.91),
            ("frame_4080.png", 1.92, 2.26),
            ("frame_4920.png", 2.99, 3.00),
        ];

        for &(file, expected_p1, expected_p2) in cases {
            let img = load_fixture(file);
            let p1 = read_sa_value(&img, &p1_probes, &P1_SA_GAUGE);
            let p2 = read_sa_value(&img, &p2_probes, &P2_SA_GAUGE);
            assert_sa_approx(p1, expected_p1, 0.05, &format!("{file} P1"));
            assert_sa_approx(p2, expected_p2, 0.05, &format!("{file} P2"));
        }
    }
}
