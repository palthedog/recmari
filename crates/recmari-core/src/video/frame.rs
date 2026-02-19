use image::RgbImage;

/// A single decoded video frame with metadata.
pub struct Frame {
    /// The frame's image data.
    pub image: RgbImage,
    /// Absolute frame number from the start of the source (0-based).
    pub frame_number: u32,
    /// Elapsed seconds from the start of the source.
    pub timestamp_seconds: f64,
}
