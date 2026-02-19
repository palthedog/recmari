/// A rectangle defined in normalized coordinates (0.0 to 1.0),
/// independent of the actual frame resolution.
#[derive(Debug, Clone, Copy)]
pub struct NormalizedRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A rectangle in absolute pixel coordinates.
#[derive(Debug, Clone, Copy)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl NormalizedRect {
    pub fn to_pixel_rect(self, frame_width: u32, frame_height: u32) -> PixelRect {
        PixelRect {
            x: (self.x * frame_width as f64) as u32,
            y: (self.y * frame_height as f64) as u32,
            w: (self.w * frame_width as f64) as u32,
            h: (self.h * frame_height as f64) as u32,
        }
    }
}
