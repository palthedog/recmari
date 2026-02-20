use std::path::Path;

use ab_glyph::{FontVec, PxScale};
use anyhow::{Context, Result};
use image::Rgb;
use imageproc::drawing::{draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use tracing::{debug, info, warn};

use recmari_proto::proto::FrameData;

use crate::analysis::Hud;
use crate::video::frame::Frame;

const FONT_PATHS: &[&str] = &[
    "C:\\Windows\\Fonts\\consola.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/System/Library/Fonts/Menlo.ttc",
];

const TEXT_SCALE: f32 = 28.0;
const TEXT_COLOR: Rgb<u8> = Rgb([255, 255, 255]);
const TEXT_LINE_HEIGHT: i32 = 30;

/// Renders debug overlay images with HUD region markers and analysis text.
pub struct DebugRenderer {
    font: Option<FontVec>,
}

impl DebugRenderer {
    pub fn new() -> Self {
        let font = Self::load_font();
        Self { font }
    }

    pub fn save_frame(
        &self,
        frame: &Frame,
        hud: &dyn Hud,
        data: &FrameData,
        dir: &Path,
    ) -> Result<()> {
        let mut img = frame.image.clone();

        for region in hud.debug_regions() {
            let rect = Rect::at(region.rect.x as i32, region.rect.y as i32)
                .of_size(region.rect.w, region.rect.h);
            draw_hollow_rect_mut(&mut img, rect, region.color);
        }

        if let Some(font) = &self.font {
            let scale = PxScale::from(TEXT_SCALE);
            let x = 10;
            let mut y = 10;

            let header = format!("F:{}", data.frame_number);
            draw_text_mut(&mut img, TEXT_COLOR, x, y, scale, font, &header);
            y += TEXT_LINE_HEIGHT;

            let p1_hp = data.player1.as_ref().unwrap().health_ratio;
            let p2_hp = data.player2.as_ref().unwrap().health_ratio;
            let p1_text = format!("P1 HP:{:.0}%", p1_hp * 100.0);
            let p2_text = format!("P2 HP:{:.0}%", p2_hp * 100.0);
            draw_text_mut(&mut img, TEXT_COLOR, x, y, scale, font, &p1_text);
            y += TEXT_LINE_HEIGHT;
            draw_text_mut(&mut img, TEXT_COLOR, x, y, scale, font, &p2_text);
        }

        let path = dir.join(format!("frame_{:08}.png", data.frame_number));
        img.save(&path)
            .with_context(|| format!("failed to save debug frame to {}", path.display()))?;

        debug!(?path, "saved debug frame");
        Ok(())
    }

    fn load_font() -> Option<FontVec> {
        for path in FONT_PATHS {
            if let Ok(data) = std::fs::read(path) {
                if let Ok(font) = FontVec::try_from_vec(data) {
                    info!(path, "loaded debug font");
                    return Some(font);
                }
            }
        }
        warn!("no system font found for debug text overlay");
        None
    }
}
