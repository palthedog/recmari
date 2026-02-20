use image::Rgb;
use tracing::{debug, info};

use crate::analysis::common::{find_bar_boundary, rgb_to_hsv, HpSegmentType, Hsv, Scanline};
use crate::analysis::{DebugRegion, HpReading, Hud};
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

/// Thickness of the debug overlay line (pixels at target resolution).
const DEBUG_LINE_H: u32 = 3;

/// The "manemon" HUD analyzer. All layout details are internal.
pub struct ManemonHud {
    p1_scan: Scanline,
    p2_scan: Scanline,
}

impl ManemonHud {
    pub fn new(frame_width: u32, frame_height: u32) -> Self {
        assert!(frame_width > 0 && frame_height > 0);

        let p1_scan = P1_HEALTH.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);
        let p2_scan = P2_HEALTH.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);

        info!(
            frame_width,
            frame_height,
            p1_y = p1_scan.y,
            p1_x0 = p1_scan.x_start,
            p1_x1 = p1_scan.x_end,
            p2_y = p2_scan.y,
            p2_x0 = p2_scan.x_start,
            p2_x1 = p2_scan.x_end,
            "manemon HUD initialized"
        );

        Self { p1_scan, p2_scan }
    }
}

impl Hud for ManemonHud {
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

fn rgb_to_hp_segment(r: u8, g: u8, b: u8) -> HpSegmentType {
    let hsv = rgb_to_hsv(r, g, b);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_regions_returns_two() {
        let hud = ManemonHud::new(1920, 1080);
        assert_eq!(hud.debug_regions().len(), 2);
    }
}
