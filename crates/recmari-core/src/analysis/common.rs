use std::fmt::{self, Formatter};

use image::RgbImage;
use tracing::{debug, info};

/// Horizontal scanline defined by y coordinate and x range.
/// Both start and end are inclusive, and can be in either order (e.g. right-to-left).
#[derive(Debug, Clone, Copy)]
pub struct Scanline {
    pub x_start: u32,
    pub x_end: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HpSegmentType {
    // the HP bar can be hidden by certain effects. In that case, we should simply ignore it.
    Unknown,
    Healthy,
    Damage,            // e.g. orange
    ProvisionalDamage, // e.g. white
    Background,
}

impl Scanline {
    /// Scale from a reference resolution to a target resolution.
    pub fn scale_to(&self, frame_w: u32, frame_h: u32, ref_w: u32, ref_h: u32) -> Scanline {
        Scanline {
            x_start: self.x_start * frame_w / ref_w,
            x_end: self.x_end * frame_w / ref_w,
            y: self.y * frame_h / ref_h,
        }
    }
}

/// HSV representation. H in [0, 360), S and V in [0.0, 1.0].
#[derive(Debug, Clone, Copy)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl fmt::Display for Hsv {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[H: {:.0}°, S: {:.2}, V: {:.2}]", self.h, self.s, self.v)
    }
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> Hsv {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let v = max;
    let s = if max > 0.0 { delta / max } else { 0.0 };

    let h = if delta < 1e-6 {
        0.0
    } else if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };

    Hsv { h, s, v }
}

pub fn find_bar_boundary(
    image: &RgbImage,
    scanline: &Scanline,
    classifier: impl Fn(u8, u8, u8) -> HpSegmentType,
) -> Option<f64> {
    assert!(scanline.x_end <= image.width(), "x_end exceeds image width");
    assert!(scanline.y < image.height(), "y exceeds image height");

    let dx: i32 = if scanline.x_end > scanline.x_start {
        1
    } else {
        -1
    };

    let mut prev_segment = HpSegmentType::Healthy;

    // Find the rightmost matching pixel by scanning from x_end inward.
    let width = scanline.x_start.abs_diff(scanline.x_end);
    debug!(
        x_start = scanline.x_start,
        x_end = scanline.x_end,
        y = scanline.y,
        width,
        "scanning HP bar boundary"
    );
    for i in 0..=width {
        let x = scanline.x_start.saturating_add_signed(i as i32 * dx);

        let pixel_rgb = image.get_pixel(x, scanline.y);
        let segment = classifier(pixel_rgb[0], pixel_rgb[1], pixel_rgb[2]);

        debug!(
            x,
            hsv = format!("{}", rgb_to_hsv(pixel_rgb[0], pixel_rgb[1], pixel_rgb[2])),
            segment = format!("{:?}", segment),
            "pixel sample"
        );

        match segment {
            HpSegmentType::Unknown | HpSegmentType::Healthy => {
                continue;
            }
            HpSegmentType::Background
            | HpSegmentType::Damage
            | HpSegmentType::ProvisionalDamage => {
                if prev_segment == HpSegmentType::Healthy {
                    let boundary = (i as f64) / width as f64;
                    return Some(boundary);
                } else if prev_segment == HpSegmentType::Unknown {
                    info!(
                        "border pixel between Healthy and Background is hidden by Unknown object.",
                    );
                    return None;
                }
            }
        }

        prev_segment = segment;
    }

    Some(1.0)
}
