use std::sync::OnceLock;

use image::{Rgb, RgbImage};
use tracing::debug;

use crate::analysis::common::{find_bar_boundary, rgb_to_hsv, BarSegment, Hsv, Scanline};
use crate::analysis::OdValue;

use super::REF_WIDTH;

/// P1 OD (Drive) gauge scanline at 1920x1080. Scans right-to-left because the gauge
/// depletes from the outer edge (left) toward center (right).
/// Boundaries measured as distances from the left image edge.
pub(super) const P1_OD_GAUGE: Scanline = Scanline {
    x_start: 888,
    x_end: 561,
    y: 122,
};

/// P2 OD gauge — horizontal mirror of P1.
pub(super) const P2_OD_GAUGE: Scanline = Scanline {
    x_start: REF_WIDTH - P1_OD_GAUGE.x_start,
    x_end: REF_WIDTH - P1_OD_GAUGE.x_end,
    y: P1_OD_GAUGE.y,
};

static P1_OD_SEGMENTS: OnceLock<Vec<Scanline>> = OnceLock::new();
fn get_p1_od_segments() -> &'static Vec<Scanline> {
    P1_OD_SEGMENTS.get_or_init(|| split_scanline_for_segments(&P1_OD_GAUGE))
}

static P2_OD_SEGMENTS: OnceLock<Vec<Scanline>> = OnceLock::new();
fn get_p2_od_segments() -> &'static Vec<Scanline> {
    P2_OD_SEGMENTS.get_or_init(|| split_scanline_for_segments(&P2_OD_GAUGE))
}

/// Pixel width of each OD segment at 1920x1080 (including white border).
const OD_SEG_WIDTH: u32 = 52;
const OD_SEG_CEIL_OFFSET_Y: u32 = 8;
const OD_SEG_FLOOR_OFFSET_Y: u32 = 7;

/// Pixel width of each gap between OD segments at 1920x1080.
const OD_GAP_WIDTH: u32 = 3;

/// Classification of a single OD segment's fill state.
#[derive(Debug, Clone, Copy, PartialEq)]
enum OdSegmentState {
    /// Completely filled — interior at fill_y renders near-white.
    Full,
    /// No fill detected by boundary search at either y-coordinate.
    Empty,
    /// Partially filled with measured fill ratio (0.0–1.0).
    Partial(f64),
    /// Gauge structure not visible (effects obscuring the segment).
    Unknown,
}

//// OD segment Full

fn is_od_segment_full_border(rgb: &Rgb<u8>) -> bool {
    let hsv = rgb_to_hsv(*rgb);
    hsv.s < 0.25 && hsv.v > 0.90
}

fn is_od_segment_full_background(rgb: &Rgb<u8>) -> bool {
    let hsv = rgb_to_hsv(*rgb);

    // Check the center pixel is light-green or not.
    // for od-value > 3.
    if hsv.h >= 72.0 && hsv.h <= 105.0 && hsv.s >= 0.80 && hsv.v >= 0.85 {
        return true;
    }

    // Check the center pixel is light-orange or not.
    // for od-value <= 3.
    if hsv.h >= 25.0 && hsv.h <= 35.0 && hsv.s >= 0.95 && hsv.v >= 0.95 {
        return true;
    }
    false
}

//// OD segment Partial

fn is_partial_fill_border_orange(hsv: Hsv) -> bool {
    hsv.h >= 30.0 && hsv.h <= 60.0 && hsv.s >= 0.20 && hsv.v >= 0.80
}

fn is_partial_background_green(hsv: Hsv) -> bool {
    hsv.h > 110.0 && hsv.h < 195.0 && hsv.s > 0.95 && hsv.v > 0.30 && hsv.v < 0.45
}

fn is_partial_fill_border_green(hsv: Hsv) -> bool {
    hsv.h > 95.0 && hsv.h < 130.0 && hsv.s > 0.50 && hsv.v > 0.80
}

fn is_partial_background(hsv: Hsv) -> bool {
    is_partial_background_orange(hsv) || is_partial_background_green(hsv)
}

fn is_partial_fill_border(hsv: Hsv) -> bool {
    is_partial_fill_border_green(hsv) || is_partial_fill_border_orange(hsv)
}

fn is_partial_background_orange(hsv: Hsv) -> bool {
    // Partial segment has slightly orange-ish blue background.

    if hsv.h > 210.0
        && hsv.h < 320.0
        && hsv.s > 0.15
        && hsv.s < 0.65
        && hsv.v > 0.30
        && hsv.v < 0.60
    {
        return true;
    }

    if hsv.h > 30.0 && hsv.h < 60.0 && hsv.s > 0.30 && hsv.v > 0.30 && hsv.v < 0.55 {
        return true;
    }

    false
}

/// Read OD gauge value by classifying each segment and finding the boundary.
///
/// The OD gauge has 6 discrete segments (52px each) that fill monotonically
/// from segment 0 outward. Each segment is classified as Full, Empty, Partial,
/// or Unknown, then the boundary segment determines the reading.
pub(super) fn read_od_value(image: &RgbImage, player_one: bool) -> Option<OdValue> {
    let od_scanline = if player_one {
        &P1_OD_GAUGE
    } else {
        &P2_OD_GAUGE
    };

    if is_burnout(image, od_scanline) {
        return read_burnout_recovery(image, od_scanline);
    }

    let seg_scanlines = if player_one {
        get_p1_od_segments()
    } else {
        get_p2_od_segments()
    };

    let mut last_state = OdSegmentState::Full;
    for i in 0..6 {
        let state: OdSegmentState = classify_od_segment(image, &seg_scanlines[i]);
        match state {
            OdSegmentState::Full => {
                debug!(segment = i, "OD segment classified as FULL");
            }
            OdSegmentState::Empty => {
                debug!(segment = i, "OD segment classified as EMPTY");
                if last_state == OdSegmentState::Full {
                    return Some(OdValue::Normal(i as f64));
                }
            }
            OdSegmentState::Partial(p) => {
                debug!(segment = i, partial = p, "OD segment classified as PARTIAL");
                return Some(OdValue::Normal(i as f64 + p));
            }
            OdSegmentState::Unknown => {
                debug!(segment = i, "OD segment classified as UNKNOWN");
            }
        }
        last_state = state;
    }

    if last_state == OdSegmentState::Full {
        return Some(OdValue::Normal(6.0));
    }

    None
}

// Fast check for OD segments.
fn is_segment_full_fast(image: &RgbImage, seg_scan: &Scanline) -> bool {
    let ceil_y = seg_scan.y - OD_SEG_CEIL_OFFSET_Y;
    let floor_y = seg_scan.y + OD_SEG_FLOOR_OFFSET_Y;
    let first = seg_scan.first_pos();
    let last = seg_scan.last_pos();
    let center_x = (first.0 + last.0) / 2;

    let border_positions = [
        seg_scan.first_pos(),
        seg_scan.last_pos(),
        (center_x, ceil_y),
        (center_x, floor_y),
    ];

    debug!("OD segment fast full check");

    let mut white_count = 0;
    for &(x, y) in &border_positions {
        let rgb = image.get_pixel(x, y);

        debug!(x, y, hsv = format!("{}", rgb_to_hsv(*rgb)),);

        // Border pixels are near-white
        if is_od_segment_full_border(rgb) {
            white_count += 1;
        }
    }

    debug!(white_count, "OD segment border check");

    if white_count < 3 {
        return false;
    }

    let center_rgb = image.get_pixel(center_x as u32, seg_scan.y);
    // Check the center pixel is light-green or not.
    // for od-value > 3.
    if !is_od_segment_full_background(center_rgb) {
        debug!(
            center_x,
            y = seg_scan.y,
            hsv = format!("{}", rgb_to_hsv(*center_rgb)),
            "OD segment background check failed"
        );
        return false;
    }

    true
}

// Fast check for OD segments.
fn is_segment_full(image: &RgbImage, seg_scan: &Scanline) -> bool {
    let ceil_y = seg_scan.y - OD_SEG_CEIL_OFFSET_Y;
    let floor_y = seg_scan.y + OD_SEG_FLOOR_OFFSET_Y;
    let mut full_count = 0;

    for i in 0..seg_scan.width() {
        let x = seg_scan.x_at(i);
        if !is_od_segment_full_border(image.get_pixel(x, ceil_y)) {
            continue;
        }

        if !is_od_segment_full_border(image.get_pixel(x, floor_y)) {
            continue;
        }

        if !is_od_segment_full_background(image.get_pixel(x, seg_scan.y)) {
            continue;
        }

        full_count += 1;

        if full_count >= 4 {
            return true;
        }
    }

    false
}

fn is_segment_empty_fast(image: &RgbImage, seg_scan: &Scanline) -> bool {
    let ceil_y = seg_scan.y - OD_SEG_CEIL_OFFSET_Y;
    let floor_y = seg_scan.y + OD_SEG_FLOOR_OFFSET_Y;
    let first = seg_scan.first_pos();
    let last = seg_scan.last_pos();
    let center_x = (first.0 + last.0) / 2;

    let border_positions = [
        seg_scan.first_pos(),
        seg_scan.last_pos(),
        (center_x, ceil_y),
        (center_x, floor_y),
        (center_x, seg_scan.y),
    ];

    let mut blue_count = 0;
    for &(x, y) in &border_positions {
        let hsv = rgb_to_hsv(*image.get_pixel(x, y));

        // Empty segment has dark-blue background.
        if hsv.h > 210.0 && hsv.h < 230.0 && hsv.s > 0.90 && hsv.v > 0.6 {
            blue_count += 1;
        }
    }

    blue_count >= 4
}

fn split_scanline_for_segments(od_scanline: &Scanline) -> Vec<Scanline> {
    let mut v = Vec::with_capacity(6);

    let dx = od_scanline.dx();
    for i in 0..6 {
        let (start, end) = if dx == 1 {
            // Left to right
            let s = od_scanline.x_start + i * (OD_SEG_WIDTH + OD_GAP_WIDTH);
            (s, s + OD_SEG_WIDTH)
        } else {
            // Right to left
            let s = od_scanline.x_start - i * (OD_SEG_WIDTH + OD_GAP_WIDTH);
            (s, s - OD_SEG_WIDTH)
        };
        let seg_scan = Scanline {
            x_start: start,
            x_end: end,
            y: od_scanline.y,
        };
        v.push(seg_scan);
    }

    return v;
}

fn read_maybe_partial_segment(image: &RgbImage, seg_scan: &Scanline) -> Option<f64> {
    // Find the vertical border of the segment.
    // std::println!("Is partial?");
    for i in 0..seg_scan.width() {
        debug!("i: {}", i);

        let x = seg_scan.x_at(i);
        let x_hsv = rgb_to_hsv(*image.get_pixel(x, seg_scan.y));
        let upper_hsv = rgb_to_hsv(*image.get_pixel(x, seg_scan.y - 1));
        let lower_hsv = rgb_to_hsv(*image.get_pixel(x, seg_scan.y + 1));
        let next_hsv = rgb_to_hsv(*image.get_pixel(seg_scan.x_at(i + 1), seg_scan.y));
        debug!(
            "x: {}, x_hsv: {:?}, upper_hsv: {:?}, lower_hsv: {:?}, next_hsv: {:?}",
            x, x_hsv, upper_hsv, lower_hsv, next_hsv
        );
        if is_partial_fill_border(x_hsv)
            && is_partial_fill_border(upper_hsv)
            && is_partial_fill_border(lower_hsv)
            && is_partial_background(next_hsv)
        {
            let ratio = i as f64 / seg_scan.width() as f64;
            return Some(ratio);
        }
    }
    None
}

/// Classify a single OD segment by examining pixel colors.
fn classify_od_segment(image: &RgbImage, seg_scan: &Scanline) -> OdSegmentState {
    if is_segment_full_fast(image, seg_scan) {
        return OdSegmentState::Full;
    }

    if is_segment_empty_fast(image, seg_scan) {
        return OdSegmentState::Empty;
    }

    if is_segment_full(image, seg_scan) {
        return OdSegmentState::Full;
    }

    if let Some(ratio) = read_maybe_partial_segment(image, seg_scan) {
        return OdSegmentState::Partial(ratio);
    }

    OdSegmentState::Unknown
}

/// Measure burnout recovery progress (0.0 = just entered, 1.0 = fully recovered).
/// The gauge transitions from dark gray (unrecovered) to bright white (recovered).
fn read_burnout_recovery(image: &RgbImage, od_scan: &Scanline) -> Option<OdValue> {
    if let Some(fill) = find_bar_boundary(image, od_scan, classify_burnout_pixel) {
        return Some(OdValue::Burnout(fill));
    }
    // All dark gray → just entered burnout, recovery = 0.0
    Some(OdValue::Burnout(0.0))
}

/// Detect burnout by sampling multiple points along the scanline.
/// Normal gauge pixels (green filled or blue empty) have S > 0.50.
/// Burnout pixels (both recovered bright and unrecovered dark) have S < 0.20.
/// If none of the sample points have high saturation, the gauge is in burnout.
fn is_burnout(image: &RgbImage, od_scan: &Scanline) -> bool {
    let width = od_scan.width();
    for frac in [1, 2, 3, 4, 5] {
        let i = width * frac / 6;
        let x = od_scan.x_at(i);
        let hsv = rgb_to_hsv(*image.get_pixel(x, od_scan.y));
        if hsv.s > 0.50 {
            return false;
        }
    }
    true
}

/// Recovered portion of burnout gauge: near-white (very bright, minimal saturation).
fn is_burnout_recovered(hsv: Hsv) -> bool {
    hsv.s < 0.05 && hsv.v > 0.80
}

/// Unrecovered portion of burnout gauge: dark gray.
fn is_burnout_unrecovered(hsv: Hsv) -> bool {
    hsv.s < 0.15 && hsv.v < 0.50
}

fn classify_burnout_pixel(rgb: Rgb<u8>) -> BarSegment {
    let hsv = rgb_to_hsv(rgb);
    if is_burnout_recovered(hsv) {
        BarSegment::Foreground
    } else if is_burnout_unrecovered(hsv) {
        BarSegment::Background
    } else {
        BarSegment::Unknown
    }
}

/// OD segment fill: green at high gauge (H≈75-120°).
fn is_od_green(hsv: Hsv) -> bool {
    hsv.h >= 70.0 && hsv.h <= 120.0 && hsv.s >= 0.75 && hsv.v >= 0.80
}

/// OD segment fill: orange at low gauge (H≈22-55°), appears below ~3 bars.
/// Upper bound extended past 50° to capture yellow-orange pixels at gauge edges
/// that have H≈50-51° due to sub-pixel blending.
fn is_od_orange(hsv: Hsv) -> bool {
    hsv.h >= 22.0 && hsv.h <= 55.0 && hsv.s >= 0.60 && hsv.v >= 0.70
}

/// Filled OD segment — either green (high gauge) or orange (low gauge).
fn is_od_filled(hsv: Hsv) -> bool {
    is_od_green(hsv) || is_od_orange(hsv)
}

/// Empty/depleted OD segment (dark blue, same family as SA empty).
fn is_od_empty(hsv: Hsv) -> bool {
    hsv.h >= 213.0 && hsv.h <= 225.0 && hsv.s >= 0.85 && hsv.v >= 0.55
}

/// OD gauge pixel — either filled or empty.
pub(super) fn is_od_gauge_pixel(hsv: Hsv) -> bool {
    is_od_filled(hsv) || is_od_empty(hsv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use tracing_test::traced_test;

    fn load_fixture(name: &str) -> RgbImage {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/frames")
            .join(name);
        image::open(&path)
            .unwrap_or_else(|e| panic!("failed to load {}: {}", path.display(), e))
            .into_rgb8()
    }

    fn assert_od_segment(mes: &str, val: OdSegmentState, expected: OdSegmentState) {
        match expected {
            OdSegmentState::Partial(ratio) => {
                if let OdSegmentState::Partial(actual) = val {
                    assert!(
                        (actual - ratio).abs() < 0.05,
                        "{mes}, expected partial ratio ~{ratio}, got {actual}"
                    );
                } else {
                    panic!("{mes}, expected Partial, got {val:?}");
                }
            }
            _ => {
                assert_eq!(val, expected, "{mes}, expected {expected:?}, got {val:?}");
            }
        }
    }

    #[test]
    fn test_classify_segment_hidden() {
        // Test classify_segment. A sprite obscures the right half of the OD gauge.

        let image = load_fixture("frame_1920.png");
        let p1_seg_scanlines = get_p1_od_segments();
        let expected = [
            OdSegmentState::Unknown,
            OdSegmentState::Unknown,
            OdSegmentState::Unknown,
            OdSegmentState::Unknown,
            OdSegmentState::Partial(0.8),
            OdSegmentState::Empty,
        ];
        for i in 0..6 {
            let state = classify_od_segment(&image, &p1_seg_scanlines[i]);
            assert_od_segment(&format!("P1 Segment {i}"), state, expected[i as usize]);
        }

        let p2_seg_scanlines = get_p2_od_segments();
        let expected = [
            OdSegmentState::Full,
            OdSegmentState::Full,
            OdSegmentState::Partial(0.6),
            OdSegmentState::Empty,
            OdSegmentState::Empty,
            OdSegmentState::Empty,
        ];
        for i in 0..6 {
            let state = classify_od_segment(&image, &p2_seg_scanlines[i]);
            assert_od_segment(&format!("P2 Segment {i}"), state, expected[i as usize]);
        }
    }

    #[test]
    #[traced_test]
    fn classify_segment_impossible() {
        let image = load_fixture("frame_5700.png");
        let p1_seg_scanlines = get_p1_od_segments();
        let expected = [
            OdSegmentState::Unknown,
            OdSegmentState::Unknown,
            OdSegmentState::Unknown,
            OdSegmentState::Unknown,
            OdSegmentState::Unknown,
            OdSegmentState::Empty,
        ];
        for i in 0..6 {
            let state = classify_od_segment(&image, &p1_seg_scanlines[i]);
            assert_od_segment(&format!("P1 Segment {i}"), state, expected[i as usize]);
        }
    }

    #[test]
    #[traced_test]
    fn classify_segment_aaa() {
        // Test classify_segment. A sprite obscures the right half of the OD gauge.

        let image = load_fixture("frame_6120.png");
        let p1_seg_scanlines = get_p1_od_segments();
        let expected = [
            OdSegmentState::Full,
            OdSegmentState::Partial(0.7),
            OdSegmentState::Empty,
            OdSegmentState::Empty,
            OdSegmentState::Empty,
            OdSegmentState::Empty,
        ];
        for i in 0..6 {
            let state = classify_od_segment(&image, &p1_seg_scanlines[i]);
            assert_od_segment(&format!("P1 Segment {i}"), state, expected[i as usize]);
        }

        /*
        let p2_seg_scanlines = get_p2_od_segments();
        let expected = [
            OdSegmentState::Full,
            OdSegmentState::Full,
            OdSegmentState::Full,
            OdSegmentState::Partial(0.37),
            OdSegmentState::Empty,
            OdSegmentState::Empty,
        ];
        for i in 0..6 {
            let state = classify_od_segment(&image, &p2_seg_scanlines[i]);
            assert_od_segment(&format!("P2 Segment {i}"), state, expected[i as usize]);
        }
        */
    }

    #[test]
    fn test_classify_segment_hidden_2p() {
        let image = load_fixture("frame_2520.png");
        let p1_seg_scanlines = get_p1_od_segments();
        let expected = [
            OdSegmentState::Full,
            OdSegmentState::Full,
            OdSegmentState::Full,
            OdSegmentState::Full,
            OdSegmentState::Full,
            OdSegmentState::Full,
        ];
        for i in 0..6 {
            println!("P1 Segment {i}: ");
            let state = classify_od_segment(&image, &p1_seg_scanlines[i]);
            assert_od_segment(&format!("P1 Segment {i}"), state, expected[i as usize]);
        }

        let p2_seg_scanlines = get_p2_od_segments();
        let expected = [
            OdSegmentState::Full,
            OdSegmentState::Full,
            OdSegmentState::Partial(0.77),
            OdSegmentState::Unknown,
            OdSegmentState::Unknown,
            OdSegmentState::Unknown,
        ];
        for i in 0..6 {
            println!("P2 Segment {i}: ");
            let state = classify_od_segment(&image, &p2_seg_scanlines[i]);
            assert_od_segment(&format!("P2 Segment {i}"), state, expected[i as usize]);
        }
    }

    #[traced_test]
    #[test]
    fn read_od_value_cases() {
        use OdValue::*;

        let cases: &[(&str, Option<OdValue>, Option<OdValue>)] = &[
            ("frame_3600.png", Some(Normal(6.0)), Some(Normal(6.0))),
            ("frame_1560.png", Some(Normal(6.0)), Some(Normal(5.65))),
            ("frame_1920.png", Some(Normal(4.81)), Some(Normal(2.64))),
            ("frame_2040.png", Some(Normal(5.13)), Some(Normal(2.9))),
            ("frame_2520.png", Some(Normal(6.0)), Some(Normal(2.77))),
            ("frame_2640.png", Some(Normal(5.48)), Some(Normal(3.33))),
            ("frame_4080.png", Some(Normal(3.0)), Some(Normal(4.83))),
            ("frame_4920.png", Some(Normal(1.0)), Some(Normal(3.37))),
            ("frame_5160.png", Some(Normal(1.55)), Some(Burnout(0.0))),
            ("frame_5700.png", None, Some(Burnout(0.34))),
            ("frame_6120.png", Some(Normal(1.7)), Some(Burnout(0.73))),
            (
                "2p_od0_0_burnout.jpg",
                Some(Normal(6.0)),
                Some(Burnout(0.0)),
            ),
            ("2p_od0_0.jpg", Some(Normal(6.0)), Some(Normal(0.0))),
            ("2p_od0_5.jpg", Some(Normal(6.0)), Some(Normal(0.5))),
            ("2p_od1_0.jpg", Some(Normal(6.0)), Some(Normal(1.0))),
            ("2p_od1_5.jpg", Some(Normal(6.0)), Some(Normal(1.5))),
            ("2p_od2_0.jpg", Some(Normal(6.0)), Some(Normal(2.0))),
            ("2p_od2_5.jpg", Some(Normal(6.0)), Some(Normal(2.5))),
            ("2p_od3_0.jpg", Some(Normal(6.0)), Some(Normal(3.0))),
            ("2p_od3_5.jpg", Some(Normal(6.0)), Some(Normal(3.5))),
            ("2p_od4_0.jpg", Some(Normal(6.0)), Some(Normal(4.0))),
            ("2p_od4_5.jpg", Some(Normal(6.0)), Some(Normal(4.5))),
            ("2p_od5_0.jpg", Some(Normal(6.0)), Some(Normal(5.0))),
            ("2p_od5_5.jpg", Some(Normal(6.0)), Some(Normal(5.5))),
        ];

        for &(file, ref p1_expected, ref p2_expected) in cases {
            let img = load_fixture(file);
            let p1 = read_od_value(&img, true);
            let p2 = read_od_value(&img, false);
            let short = file.trim_start_matches("frame_").trim_start_matches("2p_");
            eprintln!("{short:>16} P1: {p1:?}  P2: {p2:?}");

            let check = |actual: Option<OdValue>,
                         expected: &Option<OdValue>,
                         label: &str|
             -> Option<String> {
                match (actual, expected) {
                    (None, None) => None,
                    (Some(_), None) => Some(format!("{label}: expected None, got {actual:?}")),
                    (None, Some(_)) => Some(format!("{label}: expected {expected:?}, got None")),
                    (Some(OdValue::Normal(a)), Some(OdValue::Normal(e)))
                        if (a - e).abs() > 0.05 =>
                    {
                        Some(format!(
                            "{label}: expected Normal({e:.2})±0.05, got Normal({a:.4})"
                        ))
                    }
                    (Some(OdValue::Burnout(a)), Some(OdValue::Burnout(e)))
                        if (a - e).abs() > 0.05 =>
                    {
                        Some(format!(
                            "{label}: expected Burnout({e:.2})±0.05, got Burnout({a:.4})"
                        ))
                    }
                    (Some(OdValue::Normal(_)), Some(OdValue::Normal(_))) => None,
                    (Some(OdValue::Burnout(_)), Some(OdValue::Burnout(_))) => None,
                    _ => Some(format!(
                        "{label}: variant mismatch — expected {expected:?}, got {actual:?}"
                    )),
                }
            };
            if let Some(msg) = check(p1, p1_expected, &format!("{file} P1")) {
                //failures.push(msg);
                panic!("{}", msg);
            }
            if let Some(msg) = check(p2, p2_expected, &format!("{file} P2")) {
                //failures.push(msg);
                panic!("{}", msg);
            }
        }
    }
}
