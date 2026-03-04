use image::RgbImage;
use tracing::{debug, info};

const X_MIN: u32 = 160;
const X_MAX: u32 = 1760;

const WALL_Y_STEP: usize = 4;
const WALL_Y_MIN: u32 = 200;
const WALL_Y_MAX: u32 = 720;

/// Maximum chroma (max(RGB) - min(RGB)) for a pixel to be considered
/// part of the background wall. Character pixels have high chroma and are skipped.
const CHROMA_MAX: u8 = 80;
/// Minimum number of low-chroma rows needed to compute a valid column average.
const MIN_CONTRIB_ROWS: u32 = 5;
/// Distance (in columns) to compare surroundings for contrast measurement.
const SURROUND_DIST: usize = 25;
/// Minimum contrast (surround brightness - dip brightness) to qualify as a candidate.
const CONTRAST_MIN: f32 = 0.10;
/// Lower contrast threshold used for detecting tile gap features (wall presence check).
const TILE_GAP_CONTRAST_MIN: f32 = 0.03;
/// Center line width range (FWHM on brightness profile).
/// Center line: 8+ pixels. Tile gaps: 1-3 pixels.
const LINE_WIDTH_MIN: u32 = 8;
const LINE_WIDTH_MAX: u32 = 20;

/// Detect the training mode stage center line using column-average brightness profile.
///
/// Builds a per-column brightness profile across the wall area, excluding
/// character pixels via chroma filtering. Finds dark dips in the profile
/// and measures their width (FWHM). The center line is 8+ pixels wide,
/// while tile gaps are 1-3 pixels — width is the primary discriminator.
///
/// Returns the x-coordinate of the center line, or None if not visible.
pub fn detect_center_line(image: &RgbImage) -> Option<u32> {
    let (w, h) = (image.width(), image.height());
    assert!(w == 1920 && h == 1080, "currently only supports 1920x1080");

    let profile = build_brightness_profile(image);

    let valid_cols = profile[X_MIN as usize..X_MAX as usize]
        .iter()
        .filter(|v| !v.is_nan())
        .count();
    if valid_cols * 100 < (X_MAX - X_MIN) as usize * 55 {
        debug!(
            valid_cols,
            total = X_MAX - X_MIN,
            "center line: wall not visible"
        );
        return None;
    }

    let sd = SURROUND_DIST;
    let x_lo = X_MIN as usize + sd;
    let x_hi = X_MAX as usize - sd;

    // Find dips and count distinct tile gaps (narrow features at low contrast threshold).
    // The training stage wall always has multiple tile gaps spaced ~200px apart.
    // If fewer than MIN_TILE_GAPS are found, the wall is not visible.
    const MIN_TILE_GAPS: u32 = 3;
    let mut tile_gap_count = 0u32;
    let mut last_tile_gap_x = 0usize;
    let mut candidates: Vec<(u32, f32)> = Vec::new();
    for x in x_lo..x_hi {
        let v = profile[x];
        let l = profile[x - sd];
        let r = profile[x + sd];
        if v.is_nan() || l.is_nan() || r.is_nan() {
            continue;
        }
        let contrast = l.min(r) - v;
        if contrast > TILE_GAP_CONTRAST_MIN
            && x > last_tile_gap_x + LINE_WIDTH_MAX as usize
            && measure_dip(&profile, x).0 < LINE_WIDTH_MIN
        {
            tile_gap_count += 1;
            last_tile_gap_x = x;
        }
        if contrast > CONTRAST_MIN {
            candidates.push((x as u32, contrast));
        }
    }

    if tile_gap_count < MIN_TILE_GAPS {
        debug!(
            tile_gap_count,
            "center line: too few tile gaps, wall not present"
        );
        return None;
    }

    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let mut suppressed = vec![false; candidates.len()];
    for i in 0..candidates.len() {
        if suppressed[i] {
            continue;
        }
        for j in (i + 1)..candidates.len() {
            if candidates[j].0.abs_diff(candidates[i].0) < LINE_WIDTH_MAX {
                suppressed[j] = true;
            }
        }
    }

    for (i, &(x, contrast)) in candidates.iter().enumerate() {
        if suppressed[i] {
            continue;
        }
        let (width, center) = measure_dip(&profile, x as usize);
        if width >= LINE_WIDTH_MIN && width <= LINE_WIDTH_MAX {
            info!(x = center, width, contrast, "center line detected");
            return Some(center);
        }
        debug!(x, width, contrast, "candidate rejected: width out of range");
    }

    debug!("center line: no candidate found");
    None
}

/// Build per-column average brightness across the wall area, excluding character pixels.
fn build_brightness_profile(image: &RgbImage) -> Vec<f32> {
    let w = image.width() as usize;
    let mut profile = vec![f32::NAN; w];
    for x in X_MIN as usize..X_MAX as usize {
        let mut sum = 0.0f32;
        let mut count = 0u32;
        for y in (WALL_Y_MIN..WALL_Y_MAX).step_by(WALL_Y_STEP) {
            let [r, g, b] = image.get_pixel(x as u32, y).0;
            if r.max(g).max(b) - r.min(g).min(b) > CHROMA_MAX {
                continue;
            }
            sum += (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;
            count += 1;
        }
        if count >= MIN_CONTRIB_ROWS {
            profile[x] = sum / count as f32;
        }
    }
    profile
}

/// Measure the FWHM (full width at half maximum depth) of a brightness dip.
/// Returns (width, center_x) where center_x is the midpoint of the FWHM range.
/// Uses the midpoint between the dip bottom and the surrounding brightness as threshold.
fn measure_dip(profile: &[f32], x: usize) -> (u32, u32) {
    let sd = SURROUND_DIST;
    let li = x.saturating_sub(sd);
    let ri = (x + sd).min(profile.len() - 1);

    if profile[x].is_nan() || profile[li].is_nan() || profile[ri].is_nan() {
        return (0, x as u32);
    }
    let surround = profile[li].min(profile[ri]);
    let threshold = profile[x] + (surround - profile[x]) * 0.5;

    let mut left = x;
    while left > 0 && !profile[left - 1].is_nan() && profile[left - 1] < threshold {
        left -= 1;
    }

    let mut right = x;
    while right < profile.len() - 1
        && !profile[right + 1].is_nan()
        && profile[right + 1] < threshold
    {
        right += 1;
    }
    let width = (right - left + 1) as u32;
    let center = ((left + right) / 2) as u32;
    (width, center)
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
            rate >= 85.0,
            "detection rate {rate:.1}% is below 85% target"
        );
    }

    #[test]
    #[traced_test]
    fn center_line_regression() {
        let cases: &[(&str, u32)] = &[
            ("round1_fight.png", 960),
            ("p1_left_corner.png", 1599),
            ("p1_right_corner.png", 324),
            ("p2_left_corner.png", 1599),
            ("p2_right_corner.png", 324),
            ("frame_1320.png", 961),
            ("frame_1440.png", 1079),
            ("frame_1560.png", 756),
            ("frame_1800.png", 764),
            ("frame_1920.png", 952),
            ("frame_2040.png", 1253),
            ("frame_2160.png", 912),
            ("frame_2280.png", 890),
            ("frame_2400.png", 752),
            ("frame_2520.png", 783),
            ("frame_2640.png", 680),
            ("frame_2760.png", 317),
            ("frame_2880.png", 324),
            ("frame_3000.png", 504),
            ("frame_3120.png", 324),
            ("frame_3240.png", 322),
            ("frame_3360.png", 321),
            ("frame_3600.png", 961),
            ("frame_3720.png", 882),
            ("frame_3840.png", 600),
            ("frame_3960.png", 430),
            ("frame_4080.png", 322),
            ("frame_4200.png", 322),
            ("frame_4320.png", 322),
            ("frame_4440.png", 320),
            ("frame_4560.png", 345),
            ("frame_4800.png", 505),
            ("frame_4920.png", 974),
            ("frame_5040.png", 1034),
            ("frame_5160.png", 811),
            ("frame_5880.png", 366),
            ("frame_6000.png", 408),
            ("frame_6960.png", 938),
            ("frame_8880.png", 1075),
            ("frame_10800.png", 1112),
            ("frame_14640.png", 777),
        ];
        let none_cases: &[&str] = &[
            "frame_1680.png",
            "frame_4680.png",
            "frame_5280.png",
            "frame_5400.png",
            "frame_5520.png",
            "frame_5760.png",
            // frame_6240 (KO frame) is skipped at the pipeline level via HP check
        ];

        let mut pass = 0u32;
        let mut fail = 0u32;
        for &(fixture, expected_x) in cases {
            let image = load_fixture(fixture);
            match detect_center_line(&image) {
                Some(x) if x.abs_diff(expected_x) <= 8 => {
                    pass += 1;
                }
                Some(x) => {
                    fail += 1;
                    println!("FAIL {fixture}: expected {expected_x}, got {x}");
                }
                None => {
                    fail += 1;
                    println!("FAIL {fixture}: expected {expected_x}, got None");
                }
            }
        }
        for &fixture in none_cases {
            let image = load_fixture(fixture);
            match detect_center_line(&image) {
                None => pass += 1,
                Some(x) => {
                    fail += 1;
                    println!("FAIL {fixture}: expected None, got {x}");
                }
            }
        }
        println!(
            "\nResults: {pass} passed, {fail} failed out of {}",
            pass + fail
        );
        assert_eq!(fail, 0, "{fail} test cases failed");
    }
}
