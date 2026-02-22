pub mod common;
pub mod huds;

use std::fmt;

use image::Rgb;

use crate::rect::PixelRect;
use crate::video::frame::Frame;

/// Identifies which HUD layout is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudType {
    Manemon,
}

impl fmt::Display for HudType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HudType::Manemon => write!(f, "manemon"),
        }
    }
}

/// HP reading for a single frame. Each player's value is None if unreadable.
#[derive(Debug, Clone, Copy)]
pub struct HpReading {
    pub p1: Option<f64>,
    pub p2: Option<f64>,
}

/// SA gauge reading for a single frame. Each player's value is None if unreadable.
/// The value is stock count (integer part) + bar fill ratio (fractional part), ranging 0.0 to 3.0.
#[derive(Debug, Clone, Copy)]
pub struct SaReading {
    pub p1: Option<f64>,
    pub p2: Option<f64>,
}

/// A region to draw on debug frames.
pub struct DebugRegion {
    pub rect: PixelRect,
    pub color: Rgb<u8>,
}

/// Common interface that every HUD implementation must provide.
pub trait Hud {
    /// Return the type of this HUD.
    fn hud_type(&self) -> HudType;

    /// Detect whether this HUD is present in the frame.
    fn detect_hud(&self, frame: &Frame) -> bool;

    /// Read HP ratios from a single frame.
    fn analyze_hp(&self, frame: &Frame) -> HpReading;

    /// Read SA gauge level (0.0-3.0) from a single frame.
    fn analyze_sa(&self, frame: &Frame) -> SaReading;

    /// Return the regions to draw on debug frames.
    fn debug_regions(&self) -> Vec<DebugRegion>;
}
