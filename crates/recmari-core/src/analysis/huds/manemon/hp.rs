use image::RgbImage;
use tracing::debug;

use crate::analysis::common::{rgb_to_hsv, Hsv, Scanline};

use super::REF_WIDTH;

/// P1 health bar scanline at 1920x1080.
pub(super) const P1_HEALTH: Scanline = Scanline {
    x_start: 886,
    x_end: 187,
    y: 80,
};

/// P2 health bar — horizontal mirror of P1.
pub(super) const P2_HEALTH: Scanline = Scanline {
    x_start: REF_WIDTH - P1_HEALTH.x_start,
    x_end: REF_WIDTH - P1_HEALTH.x_end,
    y: P1_HEALTH.y,
};

/// HP bar pixel for detection. Excludes the loose border heuristic
/// to avoid false positives on bright scenes (e.g. white transition screens).
pub(super) fn is_hp_bar_pixel(hsv: Hsv) -> bool {
    is_hp_healthy(hsv) || is_hp_background(hsv) || is_damage(hsv)
}

fn is_hp_bar_frame(hsv: Hsv) -> bool {
    // pi side
    if hsv.h > 210.0 && hsv.h < 230.0 && hsv.s > 0.8 && hsv.v > 0.75 {
        return true;
    }

    // p2 side has darker frame color.
    hsv.h > 210.0 && hsv.h < 230.0 && hsv.s > 0.4 && hsv.s < 0.7 && hsv.v > 0.4 && hsv.v < 0.7
}

/// HP bar fill at normal health levels (yellow, H≈49-64°).
fn is_hp_yellow(hsv: Hsv) -> bool {
    hsv.h >= 48.0 && hsv.h <= 66.0 && hsv.s >= 0.3 && hsv.v >= 0.9
}

/// HP bar fill at low health below ~25% (orange, H≈40-49°).
fn is_hp_orange(hsv: Hsv) -> bool {
    hsv.h >= 38.0 && hsv.h <= 50.0 && hsv.s >= 0.85 && hsv.v >= 0.9
}

/// HP bar fill — yellow (normal) or orange (low health).
fn is_hp_healthy(hsv: Hsv) -> bool {
    is_hp_yellow(hsv) || is_hp_orange(hsv)
}

fn is_hp_border_white(hsv: Hsv) -> bool {
    hsv.s < 0.25 && hsv.v > 0.9
}

fn is_hp_border_orange(hsv: Hsv) -> bool {
    hsv.h > 40.0 && hsv.h < 65.0 && hsv.s > 0.70 && hsv.s < 0.85 && hsv.v > 0.9
}

fn is_provisional_damage(hsv: Hsv) -> bool {
    hsv.s < 0.1 && hsv.v >= 0.6 && hsv.v <= 0.9
}

fn is_hp_background(hsv: Hsv) -> bool {
    hsv.h > 215.0 && hsv.h < 222.0 && hsv.s > 0.95
}

fn is_damage(hsv: Hsv) -> bool {
    hsv.h >= 17.0 && hsv.h <= 25.0 && hsv.s >= 0.9 && hsv.v >= 0.9
}

fn find_border(image: &RgbImage, scanline: &Scanline) -> Option<u32> {
    let mut yellow_count = 0;
    let mut orange_count = 0;
    let mut background_count = 0;
    let mut first_background_i = -1;

    let mut border_count = 0;
    let mut border_i = 0;

    // +1 to check a pixel just outside the HP bar.
    for i in 3..scanline.width() + 1 {
        let x = scanline.x_at(i);
        let hsv = rgb_to_hsv(*image.get_pixel(x, scanline.y));

        debug!("@{x}: {hsv}");

        if orange_count < 8 && is_hp_yellow(hsv) {
            yellow_count += 1;
            border_count = 0;
            debug!("    hp yellow");
            continue;
        }

        if yellow_count < 8 && is_hp_orange(hsv) {
            orange_count += 1;
            border_count = 0;
            debug!("    hp orange");
            continue;
        }

        debug!("{} yellow, {} orange", yellow_count, orange_count);

        // TODO: It might be better to switch the border color by `x`.
        //   - The border color shouldn't be orange if `i / width > 0.25`
        //   - Should we check the scanline in reverse order?
        if (yellow_count >= orange_count && is_hp_border_white(hsv))
            || (orange_count > yellow_count && is_hp_border_orange(hsv))
        {
            border_count += 1;
            border_i = i;

            debug!("@{x} Found border candidate: width: {border_count}");
            continue;
        }

        if is_hp_background(hsv) {
            background_count += 1;
            if first_background_i == -1 {
                first_background_i = i as i32;
            }
        }

        if is_hp_bar_frame(hsv)
            || is_hp_background(hsv)
            || is_damage(hsv)
            || is_provisional_damage(hsv)
        {
            debug!("@{x} Found background pixel");
            debug!("   yellow: {yellow_count}, orange: {orange_count}");

            if border_count >= 1 && border_count <= 4 {
                debug!("Confirmed border at x={border_i}, width={border_count}");
                return Some(border_i + 1);
            } else {
                debug!("Rejected border candidate at x={border_i} with width {border_count}");
            }
        }
    }

    if background_count > 10 && first_background_i < 4 {
        let x = scanline.x_at(first_background_i as u32);
        let mut bg_count = 0;
        for y in (scanline.y - 2)..=(scanline.y + 2) {
            let hsv = rgb_to_hsv(*image.get_pixel(x, y));
            debug!("Background check @({x}, {y}): {hsv}");
            if is_hp_background(hsv) {
                bg_count += 1;
            }
        }
        if bg_count >= 3 {
            return Some(0);
        }
    }

    debug!("HP border not found");
    debug!("   yellow: {yellow_count}, orange: {orange_count}");
    debug!("   last border candidate at x={border_i} with width {border_count}");
    None
}

pub(super) fn analyze_hp(image: &image::RgbImage, scanline: &Scanline) -> Option<f64> {
    let border = find_border(image, scanline);
    if border.is_none() {
        debug!("HP border not found, classifying entire bar as unknown");
        return None;
    }

    let border = border.unwrap();

    // TODO: The current implmentation supports only calculating its health.
    // We should also check other segments.

    let healthy_count = border;
    let total_count = scanline.width();
    Some(healthy_count as f64 / total_count as f64)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use image::RgbImage;
    use tracing_test::traced_test;

    use super::*;

    fn load_fixture(name: &str) -> RgbImage {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/frames")
            .join(name);
        image::open(&path)
            .unwrap_or_else(|e| panic!("failed to load {}: {}", path.display(), e))
            .into_rgb8()
    }

    fn assert_hp(expected: Option<f64>, actual: Option<f64>) {
        match (expected, actual) {
            (Some(e), Some(a)) => assert!((e - a).abs() < 0.05, "expected {:.2}, got {:.2}", e, a),
            (None, None) => {}
            _ => panic!("expected {:?}, got {:?}", expected, actual),
        }
    }

    #[test]
    #[traced_test]
    fn test_find_border() {
        let image = load_fixture("p2_hp_head_covered.png");
        let border = find_border(&image, &P2_HEALTH);

        assert_eq!(P2_HEALTH.x_at(border.unwrap()), 1684);
    }

    #[test]
    #[traced_test]
    fn test_find_border_orange() {
        let image = load_fixture("frame_6120.png");
        let border = find_border(&image, &P1_HEALTH);

        assert_eq!(P1_HEALTH.x_at(border.unwrap()), 851);
    }

    #[test]
    #[traced_test]
    fn test_find_border_frame_7080() {
        let image = load_fixture("frame_7080.png");
        let border = find_border(&image, &P2_HEALTH);

        assert_eq!(P2_HEALTH.x_at(border.unwrap()), 1543);
    }

    #[test]
    #[traced_test]
    fn test_find_border_ko() {
        let image = load_fixture("p1_ko.png");
        let border = find_border(&image, &P1_HEALTH);

        assert_eq!(Some(0), border);
    }

    #[test]
    #[traced_test]
    fn test_find_border_p2_hp_border_hidden() {
        let image = load_fixture("p2_hp_border_hidden.png");
        let border = find_border(&image, &P2_HEALTH);

        assert_eq!(None, border);
    }

    #[test]
    #[traced_test]
    fn test_analyze_hp() {
        let image = load_fixture("p2_hp_head_covered.png");
        let hp = analyze_hp(&image, &P2_HEALTH);
        assert_hp(Some(0.93), hp);
    }

    #[test]
    #[traced_test]
    fn test_analyze_hp_full() {
        let image = load_fixture("p2_hp_head_covered.png");
        let hp = analyze_hp(&image, &P1_HEALTH);
        assert_hp(Some(1.0), hp);
    }

    #[test]
    #[traced_test]
    fn test_analyze_hp_fully_covered() {
        let image = load_fixture("frame_5280.png");

        let hp = analyze_hp(&image, &P1_HEALTH);
        assert_hp(Some(0.19), hp);

        // P2's HP bar is fully covered by a character.
        let hp = analyze_hp(&image, &P2_HEALTH);
        assert_hp(None, hp);
    }

    #[test]
    #[traced_test]
    fn test_analyze_hp_orange() {
        let image = load_fixture("frame_2760.png");
        let hp = analyze_hp(&image, &P1_HEALTH);
        assert_hp(Some(0.20), hp);
    }

    #[test]
    #[traced_test]
    fn test_analyze_hp_frame1800() {
        let image = load_fixture("frame_1800.png");
        let hp = analyze_hp(&image, &P1_HEALTH);
        assert_hp(Some(0.85), hp);
    }

    #[test]
    #[traced_test]
    fn test_round_start() {
        let image = load_fixture("round1_fight.png");
        let hp = analyze_hp(&image, &P1_HEALTH);
        assert_hp(Some(1.0), hp);

        let hp = analyze_hp(&image, &P2_HEALTH);
        assert_hp(Some(1.0), hp);
    }
}
