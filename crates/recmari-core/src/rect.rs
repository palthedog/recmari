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

impl PixelRect {
    /// Scale this rect from a reference resolution to a target resolution.
    pub fn scale_to(self, target_w: u32, target_h: u32, ref_w: u32, ref_h: u32) -> PixelRect {
        assert!(ref_w > 0 && ref_h > 0, "reference resolution must be > 0");
        assert!(target_w > 0 && target_h > 0, "target resolution must be > 0");
        PixelRect {
            x: (self.x as u64 * target_w as u64 / ref_w as u64) as u32,
            y: (self.y as u64 * target_h as u64 / ref_h as u64) as u32,
            w: (self.w as u64 * target_w as u64 / ref_w as u64) as u32,
            h: (self.h as u64 * target_h as u64 / ref_h as u64) as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_to_same_resolution() {
        let r = PixelRect { x: 100, y: 50, w: 200, h: 30 };
        let scaled = r.scale_to(1920, 1080, 1920, 1080);
        assert_eq!(scaled.x, 100);
        assert_eq!(scaled.y, 50);
        assert_eq!(scaled.w, 200);
        assert_eq!(scaled.h, 30);
    }

    #[test]
    fn scale_to_half_resolution() {
        let r = PixelRect { x: 100, y: 40, w: 800, h: 20 };
        let scaled = r.scale_to(960, 540, 1920, 1080);
        assert_eq!(scaled.x, 50);
        assert_eq!(scaled.y, 20);
        assert_eq!(scaled.w, 400);
        assert_eq!(scaled.h, 10);
    }

    #[test]
    fn normalized_to_pixel() {
        let n = NormalizedRect { x: 0.5, y: 0.25, w: 0.1, h: 0.05 };
        let p = n.to_pixel_rect(1920, 1080);
        assert_eq!(p.x, 960);
        assert_eq!(p.y, 270);
        assert_eq!(p.w, 192);
        assert_eq!(p.h, 54);
    }
}
