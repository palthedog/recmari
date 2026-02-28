mod hp;
mod od;
mod sa;

pub use sa::{scan_sa_digit_probes, ProbeScanEntry};

use image::{Rgb, RgbImage};
use tracing::{debug, info};

use crate::analysis::common::{rgb_to_hsv, Hsv, Scanline};
use crate::analysis::{DebugRegion, HpReading, Hud, HudType, OdReading, OdValue, SaReading};
use crate::rect::PixelRect;
use crate::video::frame::Frame;

use hp::{is_hp_bar_pixel, P1_HEALTH, P2_HEALTH};
use od::{is_od_gauge_pixel, read_od_value, P1_OD_GAUGE, P2_OD_GAUGE};
use sa::{
    is_sa_gauge_pixel, read_sa_value, P1_SA_DIGIT, P1_SA_GAUGE, P2_SA_DIGIT, P2_SA_GAUGE,
    SA_DIGIT_PROBES,
};

const SA_FRAME: Scanline = Scanline {
    x_start: 208,
    x_end: 220,
    y: 1027,
};

const REF_WIDTH: u32 = 1920;
const REF_HEIGHT: u32 = 1080;

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
    p1_sa_digit_probes: [(u32, u32); 4],
    p2_sa_digit_probes: [(u32, u32); 4],
    p1_od_scan: Scanline,
    p2_od_scan: Scanline,
}

impl ManemonHud {
    pub fn new(frame_width: u32, frame_height: u32) -> Self {
        assert!(
            frame_width == 1920 && frame_height == 1080,
            "currently only supports 1920x1080 videos"
        );

        let p1_scan = P1_HEALTH.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);
        let p2_scan = P2_HEALTH.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);
        let p1_sa_scan = P1_SA_GAUGE.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);
        let p2_sa_scan = P2_SA_GAUGE.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);

        let p1_sa_digit_probes = SA_DIGIT_PROBES.map(|p| (p.x, p.y));
        let p2_sa_digit_probes =
            SA_DIGIT_PROBES.map(|p| (P2_SA_DIGIT.x + p.x - P1_SA_DIGIT.x, p.y));
        let p1_od_scan = P1_OD_GAUGE.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);
        let p2_od_scan = P2_OD_GAUGE.scale_to(frame_width, frame_height, REF_WIDTH, REF_HEIGHT);

        info!(frame_width, frame_height, "manemon HUD initialized");

        Self {
            p1_scan,
            p2_scan,
            p1_sa_scan,
            p2_sa_scan,
            p1_sa_digit_probes,
            p2_sa_digit_probes,
            p1_od_scan,
            p2_od_scan,
        }
    }
}

fn is_ca_frame(hsv: Hsv) -> bool {
    hsv.h > 190.0 && hsv.h < 210.0 && hsv.s > 0.9 && hsv.v > 0.9
}

fn is_sa_frame(hsv: Hsv) -> bool {
    hsv.h > 210.0 && hsv.h < 230.0 && hsv.s > 0.9 && hsv.v > 0.9
}

impl Hud for ManemonHud {
    fn hud_type(&self) -> HudType {
        HudType::Manemon
    }

    fn detect_hud(&self, frame: &Frame) -> bool {
        // Check SA gauge's frame since it's not covered by other objects.
        for i in 0..SA_FRAME.width() {
            let x = SA_FRAME.x_at(i);
            let pixel = frame.image.get_pixel(x, SA_FRAME.y);
            let hsv = rgb_to_hsv(*pixel);
            debug!("SA frame check @{x}: {hsv}");
            if !is_sa_frame(hsv) && !is_ca_frame(hsv) {
                return false;
            }
        }
        true
    }

    fn analyze_hp(&self, frame: &Frame) -> HpReading {
        let p1 = hp::analyze_hp(&frame.image, &self.p1_scan);
        let p2 = hp::analyze_hp(&frame.image, &self.p2_scan);

        debug!(
            frame_number = frame.frame_number,
            p1, p2, "manemon HP reading"
        );

        HpReading { p1, p2 }
    }

    fn analyze_sa(&self, frame: &Frame) -> SaReading {
        let p1 = read_sa_value(&frame.image, &self.p1_sa_digit_probes, &self.p1_sa_scan);
        let p2 = read_sa_value(&frame.image, &self.p2_sa_digit_probes, &self.p2_sa_scan);

        debug!(
            frame_number = frame.frame_number,
            p1, p2, "manemon SA reading"
        );

        SaReading { p1, p2 }
    }

    fn analyze_od(&self, frame: &Frame) -> OdReading {
        let p1 = read_od_value(&frame.image, true);
        let p2 = read_od_value(&frame.image, false);

        debug!(
            frame_number = frame.frame_number,
            p1 = p1.map(|v| match v {
                OdValue::Normal(x) => x,
                OdValue::Burnout(x) => -x,
            }),
            p2 = p2.map(|v| match v {
                OdValue::Normal(x) => x,
                OdValue::Burnout(x) => -x,
            }),
            "manemon OD reading"
        );

        OdReading { p1, p2 }
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
            DebugRegion {
                rect: scanline_to_rect(&self.p1_sa_scan),
                color: Rgb([255, 255, 0]),
            },
            DebugRegion {
                rect: scanline_to_rect(&self.p2_sa_scan),
                color: Rgb([255, 255, 0]),
            },
            DebugRegion {
                rect: scanline_to_rect(&self.p1_od_scan),
                color: Rgb([0, 255, 128]),
            },
            DebugRegion {
                rect: scanline_to_rect(&self.p2_od_scan),
                color: Rgb([0, 255, 128]),
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

    let mut count = 0;
    for i in 0..DETECT_SAMPLE_COUNT {
        let t = i as f64 / (DETECT_SAMPLE_COUNT - 1) as f64;
        let offset = (t * (width - 1) as f64) as u32;
        let x = scanline.x_at(offset);
        let pixel = image.get_pixel(x, scanline.y);
        let hsv = rgb_to_hsv(*pixel);

        if classifier(hsv) {
            count += 1;
        }
    }

    count
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

    fn load_fixture_frame(name: &str) -> Frame {
        let image = load_fixture(name);
        Frame {
            frame_number: 0,
            timestamp_seconds: 0.0,
            image,
        }
    }

    #[test]
    #[traced_test]
    fn test_classify_hp_pixel_covered() {
        let frame = load_fixture_frame("p2_hp_head_covered.png");
        let hud = ManemonHud::new(frame.image.width(), frame.image.height());

        let hp = hud.analyze_hp(&frame);
        assert!(hp.p1.is_some());
        assert!(hp.p2.is_some());
        assert!((hp.p1.unwrap() - 1.0).abs() < 0.05);
        assert!((hp.p2.unwrap() - 0.93).abs() < 0.05);
    }

    #[test]
    #[traced_test]
    fn test_detect_hud_sa() {
        let frame = load_fixture_frame("round1_fight.png");
        let hud = ManemonHud::new(frame.image.width(), frame.image.height());
        assert!(hud.detect_hud(&frame));
    }

    #[test]
    #[traced_test]
    fn test_detect_hud_ca() {
        let frame = load_fixture_frame("p1_ca.png");
        let hud = ManemonHud::new(frame.image.width(), frame.image.height());
        assert!(hud.detect_hud(&frame));
    }

    #[test]
    #[traced_test]
    fn test_detect_hud_no_hud() {
        let frame = load_fixture_frame("no_hud.png");
        let hud = ManemonHud::new(frame.image.width(), frame.image.height());
        assert!(!hud.detect_hud(&frame));
    }
}
