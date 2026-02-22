use std::fmt::{self, Formatter};

use image::{Rgb, RgbImage};
use tracing::{debug, info};

/// Horizontal scanline defined by y coordinate and x range.
/// Both start and end are inclusive, and can be in either order (e.g. right-to-left).
#[derive(Debug, Clone, Copy)]
pub struct Scanline {
    pub x_start: u32,
    pub x_end: u32,
    pub y: u32,
}

/// Pixel classification for bar-fill boundary detection.
/// Used by both HP and SA gauge scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarSegment {
    /// Bar region is obscured by effects — reading should be skipped.
    Unknown,
    /// Active bar fill (e.g. yellow HP, cyan SA, purple CA).
    Foreground,
    /// Depleted region or non-bar area (e.g. blue background, orange damage).
    Background,
}

/// Detailed HP bar pixel classification.
/// Preserves per-color semantics beyond the Foreground/Background split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HpSegment {
    Unknown,
    /// Yellow HP fill (the remaining health).
    Healthy,
    /// White/bright border at the edge of the HP bar.
    Border,
    /// Red/orange damage flash.
    Damage,
    /// Gray provisional damage (white-life).
    ProvisionalDamage,
    /// Dark blue depleted region.
    Background,
}

impl From<HpSegment> for BarSegment {
    fn from(seg: HpSegment) -> Self {
        match seg {
            HpSegment::Healthy | HpSegment::Border => BarSegment::Foreground,
            HpSegment::Damage | HpSegment::ProvisionalDamage | HpSegment::Background => {
                BarSegment::Background
            }
            HpSegment::Unknown => BarSegment::Unknown,
        }
    }
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

    /// Scan direction: +1 for left-to-right, -1 for right-to-left.
    pub fn dx(&self) -> i32 {
        if self.x_end > self.x_start { 1 } else { -1 }
    }

    /// Pixel width of the scanline.
    pub fn width(&self) -> u32 {
        self.x_start.abs_diff(self.x_end)
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

pub fn rgb_to_hsv(rgb: Rgb<u8>) -> Hsv {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;

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
    classifier: impl Fn(Rgb<u8>) -> BarSegment,
) -> Option<f64> {
    assert!(scanline.x_end <= image.width(), "x_end exceeds image width");
    assert!(scanline.y < image.height(), "y exceeds image height");

    let dx = scanline.dx();
    let width = scanline.width();
    let mut prev_segment = BarSegment::Foreground;
    let mut last_fg_i: Option<u32> = None;
    debug!(
        x_start = scanline.x_start,
        x_end = scanline.x_end,
        y = scanline.y,
        width,
        "scanning bar boundary"
    );
    for i in 0..=width {
        let x = scanline.x_start.saturating_add_signed(i as i32 * dx);

        let pixel_rgb = image.get_pixel(x, scanline.y);
        let segment = classifier(*pixel_rgb);

        debug!(
            x,
            hsv = format!("{}", rgb_to_hsv(*pixel_rgb)),
            segment = format!("{:?}", segment),
            "pixel sample"
        );

        match segment {
            BarSegment::Foreground => {
                last_fg_i = Some(i);
            }
            BarSegment::Unknown => {}
            BarSegment::Background => {
                if prev_segment == BarSegment::Foreground {
                    let boundary = (i as f64) / width as f64;
                    return Some(boundary);
                } else if prev_segment == BarSegment::Unknown {
                    if let Some(fg_i) = last_fg_i {
                        let boundary = (fg_i as f64 + 1.0) / width as f64;
                        return Some(boundary);
                    }
                    info!(
                        "border between foreground and background is hidden by unknown object",
                    );
                    return None;
                }
            }
        }

        prev_segment = segment;
    }

    if prev_segment == BarSegment::Unknown {
        if let Some(fg_i) = last_fg_i {
            return Some((fg_i as f64 + 1.0) / width as f64);
        }
    }

    Some(1.0)
}
