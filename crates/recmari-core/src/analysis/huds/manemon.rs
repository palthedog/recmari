use image::{Rgb, RgbImage};
use tracing::{debug, info};

use crate::analysis::common::{find_bar_boundary, rgb_to_hsv, HpSegmentType, Hsv, Scanline};
use crate::analysis::{DebugRegion, HpReading, Hud, HudType};
use crate::rect::PixelRect;
use crate::video::frame::Frame;

const REF_WIDTH: u32 = 1920;
const REF_HEIGHT: u32 = 1080;

/// P1 health bar scanline at 1920x1080.
const P1_HEALTH: Scanline = Scanline {
    x_start: 883,
    x_end: 190,
    y: 80,
};

/// P2 health bar — horizontal mirror of P1.
const P2_HEALTH: Scanline = Scanline {
    x_start: REF_WIDTH - P1_HEALTH.x_start,
    x_end: REF_WIDTH - P1_HEALTH.x_end,
    y: P1_HEALTH.y,
};

/// P1 SA gauge scanline at 1920x1080.
const P1_SA_GAUGE: Scanline = Scanline {
    x_start: 380,
    x_end: 190,
    y: 1000,
};

/// P2 SA gauge — horizontal mirror of P1.
const P2_SA_GAUGE: Scanline = Scanline {
    x_start: REF_WIDTH - P1_SA_GAUGE.x_start,
    x_end: REF_WIDTH - P1_SA_GAUGE.x_end,
    y: P1_SA_GAUGE.y,
};

/// Thickness of the debug overlay line (pixels at target resolution).
const DEBUG_LINE_H: u32 = 3;

/// Number of evenly-spaced sample points per scanline for HUD detection.
const DETECT_SAMPLE_COUNT: u32 = 16;

/// Minimum fraction of recognized pixels to confirm a region as HUD.
const DETECT_THRESHOLD: f64 = 0.5;

/// The "manemon" HUD analyzer. All layout details are internal.
pub struct ManemonHud {
    p1_scan: Scanline,
    p2_scan: Scanline,
    p1_sa_scan: Scanline,
    p2_sa_scan: Scanline,
}

impl ManemonHud {
    pub fn new(frame_width: u32, frame_height: u32) -> Self {
        assert!(frame_width > 0 && frame_height > 0);

        let p1_scan = P1_HEALTH.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);
        let p2_scan = P2_HEALTH.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);
        let p1_sa_scan = P1_SA_GAUGE.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);
        let p2_sa_scan = P2_SA_GAUGE.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);

        info!(frame_width, frame_height, "manemon HUD initialized");

        Self {
            p1_scan,
            p2_scan,
            p1_sa_scan,
            p2_sa_scan,
        }
    }
}

impl Hud for ManemonHud {
    fn hud_type(&self) -> HudType {
        HudType::Manemon
    }

    fn detect_hud(&self, frame: &Frame) -> bool {
        let total = DETECT_SAMPLE_COUNT * 2;

        let hp_p1 = count_matching_pixels(&frame.image, &self.p1_scan, is_hp_bar_pixel);
        let hp_p2 = count_matching_pixels(&frame.image, &self.p2_scan, is_hp_bar_pixel);
        let hp_ratio = (hp_p1 + hp_p2) as f64 / total as f64;

        let sa_p1 = count_matching_pixels(&frame.image, &self.p1_sa_scan, is_sa_gauge_pixel);
        let sa_p2 = count_matching_pixels(&frame.image, &self.p2_sa_scan, is_sa_gauge_pixel);
        let sa_ratio = (sa_p1 + sa_p2) as f64 / total as f64;

        debug!(
            frame_number = frame.frame_number,
            hp_p1, hp_p2, hp_ratio, sa_p1, sa_p2, sa_ratio, "HUD detection"
        );

        hp_ratio >= DETECT_THRESHOLD || sa_ratio >= DETECT_THRESHOLD
    }

    fn analyze_hp(&self, frame: &Frame) -> HpReading {
        let p1 = find_bar_boundary(&frame.image, &self.p1_scan, rgb_to_hp_segment);
        let p2 = find_bar_boundary(&frame.image, &self.p2_scan, rgb_to_hp_segment);

        debug!(
            frame_number = frame.frame_number,
            p1, p2, "manemon HP reading"
        );

        HpReading { p1: p1, p2: p2 }
    }

    fn debug_regions(&self) -> Vec<DebugRegion> {
        let scanline_to_rect = |scan: &Scanline| PixelRect {
            x: if scan.x_start < scan.x_end {
                scan.x_start
            } else {
                scan.x_end
            },
            y: scan.y - DEBUG_LINE_H / 2,
            w: scan.x_end.abs_diff(scan.x_start) + 1,
            h: DEBUG_LINE_H,
        };
        vec![
            DebugRegion {
                rect: scanline_to_rect(&self.p1_scan),
                color: Rgb([0, 255, 0]),
            },
            DebugRegion {
                rect: scanline_to_rect(&self.p2_scan),
                color: Rgb([0, 100, 255]),
            },
        ]
    }
}

/// Sample evenly-spaced pixels along a scanline and count how many pass the classifier.
fn count_matching_pixels(
    image: &RgbImage,
    scanline: &Scanline,
    classifier: fn(Hsv) -> bool,
) -> u32 {
    let width = scanline.width();
    assert!(width > 0, "scanline has zero width");
    let dx = scanline.dx();

    let mut count = 0;
    for i in 0..DETECT_SAMPLE_COUNT {
        let t = i as f64 / (DETECT_SAMPLE_COUNT - 1) as f64;
        let offset = (t * width as f64) as i32;
        let x = scanline.x_start.saturating_add_signed(offset * dx);
        let pixel = image.get_pixel(x, scanline.y);
        let hsv = rgb_to_hsv(*pixel);

        if classifier(hsv) {
            count += 1;
        }
    }

    count
}

/// HP bar pixel for detection. Excludes the loose border heuristic
/// to avoid false positives on bright scenes (e.g. white transition screens).
fn is_hp_bar_pixel(hsv: Hsv) -> bool {
    is_strict_healthy(hsv) || is_background(hsv) || is_damage(hsv)
}

/// Yellow HP bar pixel without the loose border check.
fn is_strict_healthy(hsv: Hsv) -> bool {
    hsv.h >= 50.0 && hsv.h <= 65.0 && hsv.s >= 0.3 && hsv.v >= 0.9
}

/// SA gauge pixel — any recognized gauge state.
fn is_sa_gauge_pixel(hsv: Hsv) -> bool {
    is_gauge_empty(hsv) || is_gauge_sa(hsv) || is_gauge_ca(hsv)
}

/// Depleted SA gauge background. Same hue as HP bar background,
/// with a V floor to reject VS screen's dim blue (V≈0.30-0.39).
fn is_gauge_empty(hsv: Hsv) -> bool {
    hsv.h > 215.0 && hsv.h < 222.0 && hsv.s > 0.95 && hsv.v >= 0.60
}

/// SA gauge stocked.
fn is_gauge_sa(hsv: Hsv) -> bool {
    hsv.h >= 175.0 && hsv.h <= 210.0 && hsv.s >= 0.15 && hsv.v >= 0.80
}

/// CA ready.
fn is_gauge_ca(hsv: Hsv) -> bool {
    hsv.h >= 300.0 && hsv.h <= 345.0 && hsv.s >= 0.15 && hsv.v >= 0.85
}

fn is_healthy(hsv: Hsv) -> bool {
    (hsv.h >= 50.0 && hsv.h <= 65.0 && hsv.s >= 0.3 && hsv.v >= 0.9) || is_border(hsv)
}

fn is_border(hsv: Hsv) -> bool {
    hsv.s < 0.25 && hsv.v > 0.9
}

fn is_privisional_damage(hsv: Hsv) -> bool {
    hsv.s < 0.1 && hsv.v >= 0.6 && hsv.v <= 0.9
}
fn is_background(hsv: Hsv) -> bool {
    hsv.h > 215.0 && hsv.h < 222.0 && hsv.s > 0.95
}

fn is_damage(hsv: Hsv) -> bool {
    hsv.h >= 17.0 && hsv.h <= 25.0 && hsv.s >= 0.9 && hsv.v >= 0.9
}

fn rgb_to_hp_segment(rgb: Rgb<u8>) -> HpSegmentType {
    let hsv = rgb_to_hsv(rgb);
    if is_healthy(hsv) {
        HpSegmentType::Healthy
    } else if is_background(hsv) {
        HpSegmentType::Background
    } else if is_damage(hsv) {
        HpSegmentType::Damage
    } else if is_privisional_damage(hsv) {
        HpSegmentType::ProvisionalDamage
    } else {
        HpSegmentType::Unknown
    }
}

