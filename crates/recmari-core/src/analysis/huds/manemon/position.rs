use image::RgbImage;
use tracing::{debug, info};

use crate::analysis::common::rgb_to_hsv;

const X_MIN: u32 = 160;
const X_MAX: u32 = 1760;
const SAT_MAX: f32 = 0.20;
const SURROUND_DIST: u32 = 25;

const WALL_Y_MIN: u32 = 200;
const WALL_Y_MAX: u32 = 620;
const WALL_Y_STEP: usize = 4;

const SMOOTH_HALF: usize = 3;
const CONTRAST_MIN: f32 = 0.03;
const MIN_CONTRIB_ROWS: usize = 20;

/// Detect the training mode stage center line.
///
/// Computes per-column brightness using the lower-half mean (robust to bright
/// outliers like FIGHT text), smooths the profile to reduce per-column noise,
/// then returns the strongest contrast peak. The center line is the most
/// prominent vertical dark feature on the wall at any camera position.
///
/// Returns the x-coordinate of the center line, or None if not visible.
pub fn detect_center_line(image: &RgbImage) -> Option<u32> {
    let (w, h) = (image.width(), image.height());
    assert!(w == 1920 && h == 1080, "currently only supports 1920x1080");

    let raw = column_brightness_profile(image, w);
    let col_brightness = smooth_profile(&raw);

    let x_lo = X_MIN + SURROUND_DIST;
    let x_hi = X_MAX - SURROUND_DIST;

    let mut best_x = 0u32;
    let mut best_c = 0.0f32;
    for x in x_lo..x_hi {
        let xi = x as usize;
        let li = (x - SURROUND_DIST) as usize;
        let ri = (x + SURROUND_DIST) as usize;
        if col_brightness[xi].is_nan() || col_brightness[li].is_nan() || col_brightness[ri].is_nan()
        {
            continue;
        }
        let c = col_brightness[li].min(col_brightness[ri]) - col_brightness[xi];
        if c > best_c {
            best_c = c;
            best_x = x;
        }
    }

    if best_c < CONTRAST_MIN {
        debug!(contrast = best_c, "center line: not detected");
        return None;
    }

    info!(x = best_x, contrast = best_c, "center line detected");
    Some(best_x)
}

/// Compute per-column brightness using the lower-half mean.
///
/// For each column, collect all low-saturation pixel brightnesses, sort them,
/// and average only the darker half. This excludes bright outliers (FIGHT text,
/// white effects) while preserving the wall brightness profile.
fn column_brightness_profile(image: &RgbImage, w: u32) -> Vec<f32> {
    let mut col_values: Vec<Vec<f32>> = vec![Vec::with_capacity(105); w as usize];
    for y in (WALL_Y_MIN..WALL_Y_MAX).step_by(WALL_Y_STEP) {
        for x in X_MIN..X_MAX {
            let hsv = rgb_to_hsv(*image.get_pixel(x, y));
            if hsv.s <= SAT_MAX {
                col_values[x as usize].push(hsv.v);
            }
        }
    }

    let mut result = vec![f32::NAN; w as usize];
    for x in X_MIN as usize..X_MAX as usize {
        let values = &mut col_values[x];
        if values.len() < MIN_CONTRIB_ROWS {
            continue;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let half_n = values.len() / 2;
        let lower_sum: f32 = values[..half_n].iter().sum();
        result[x] = lower_sum / half_n as f32;
    }
    result
}

/// Smooth the brightness profile with a box filter.
///
/// Averages out per-column noise from FIGHT text and character pixels without
/// blurring the 15-20px center line.
fn smooth_profile(raw: &[f32]) -> Vec<f32> {
    let mut result = vec![f32::NAN; raw.len()];
    for x in SMOOTH_HALF..raw.len() - SMOOTH_HALF {
        let mut sum = 0.0f32;
        let mut count = 0u32;
        for dx in 0..=2 * SMOOTH_HALF {
            let v = raw[x - SMOOTH_HALF + dx];
            if !v.is_nan() {
                sum += v;
                count += 1;
            }
        }
        if count as usize > SMOOTH_HALF {
            result[x] = sum / count as f32;
        }
    }
    result
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

    #[test]
    #[traced_test]
    fn center_line_at_round_start() {
        let image = load_fixture("round1_fight.png");
        let result = detect_center_line(&image);
        let x = result.expect("center line should be detected at round start");
        assert!(x.abs_diff(960) < 10, "expected center near 960, got {x}");
    }

    #[test]
    fn video_detection_rate() {
        use crate::analysis::huds::manemon::ManemonHud;
        use crate::analysis::Hud;
        use crate::video::decoder::VideoDecoder;

        let video = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../videos/cpu8_vs_cpu8.mp4");
        if !video.exists() {
            println!("SKIP: video not found at {}", video.display());
            return;
        }

        let mut decoder = VideoDecoder::open_at_frame(&video, 0).unwrap();
        let hud = ManemonHud::new(decoder.width(), decoder.height());

        let mut hud_frames = 0u32;
        let mut detected = 0u32;
        let mut not_detected = 0u32;

        loop {
            let Some(frame) = decoder.next_frame().unwrap() else {
                break;
            };
            if frame.frame_number % 60 != 0 {
                continue;
            }
            if !hud.detect_hud(&frame) {
                continue;
            }
            hud_frames += 1;
            match detect_center_line(&frame.image) {
                Some(x) => {
                    detected += 1;
                    println!("  frame {:>6}: DETECTED x={}", frame.frame_number, x);
                }
                None => {
                    not_detected += 1;
                    println!("  frame {:>6}: NOT detected", frame.frame_number);
                }
            }
        }

        let rate = if hud_frames > 0 {
            detected as f64 / hud_frames as f64 * 100.0
        } else {
            0.0
        };
        println!(
            "\nTotal HUD frames: {hud_frames}, detected: {detected}, missed: {not_detected}, rate: {rate:.1}%"
        );
        assert!(
            rate >= 90.0,
            "detection rate {rate:.1}% is below 90% target"
        );
    }

    #[test]
    #[traced_test]
    fn center_line_at_corner() {
        let cases: &[(&str, u32)] = &[
            ("p1_left_corner.png", 1599),
            ("p1_right_corner.png", 324),
            ("p2_left_corner.png", 1599),
            ("p2_right_corner.png", 324),
        ];
        for &(fixture, expected_x) in cases {
            let image = load_fixture(fixture);
            let x = detect_center_line(&image)
                .unwrap_or_else(|| panic!("{fixture}: center line should be detected"));
            assert!(
                x.abs_diff(expected_x) < 10,
                "{fixture}: expected x near {expected_x}, got {x}"
            );
        }
    }
}
