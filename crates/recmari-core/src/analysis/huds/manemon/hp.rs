use image::Rgb;

use crate::analysis::common::{rgb_to_hsv, HpSegment, Hsv, Scanline};

use super::REF_WIDTH;

/// P1 health bar scanline at 1920x1080.
pub(super) const P1_HEALTH: Scanline = Scanline {
    x_start: 883,
    x_end: 190,
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

/// HP bar fill at normal health levels (yellow, H≈49-64°).
fn is_hp_yellow(hsv: Hsv) -> bool {
    hsv.h >= 48.0 && hsv.h <= 66.0 && hsv.s >= 0.3 && hsv.v >= 0.9
}

/// HP bar fill at low health below ~25% (orange, H≈40-49°).
fn is_hp_orange(hsv: Hsv) -> bool {
    hsv.h >= 38.0 && hsv.h <= 50.0 && hsv.s >= 0.3 && hsv.v >= 0.9
}

/// HP bar fill — yellow (normal) or orange (low health).
fn is_hp_healthy(hsv: Hsv) -> bool {
    is_hp_yellow(hsv) || is_hp_orange(hsv)
}

fn is_hp_border(hsv: Hsv) -> bool {
    hsv.s < 0.25 && hsv.v > 0.9
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

pub(super) fn classify_hp_pixel(rgb: Rgb<u8>) -> HpSegment {
    let hsv = rgb_to_hsv(rgb);
    if is_hp_healthy(hsv) {
        HpSegment::Healthy
    } else if is_hp_border(hsv) {
        HpSegment::Border
    } else if is_damage(hsv) {
        HpSegment::Damage
    } else if is_provisional_damage(hsv) {
        HpSegment::ProvisionalDamage
    } else if is_hp_background(hsv) {
        HpSegment::Background
    } else {
        HpSegment::Unknown
    }
}
