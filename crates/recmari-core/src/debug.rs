use std::path::Path;

use ab_glyph::{FontVec, PxScale};
use anyhow::{Context, Result};
use image::{Rgb, RgbImage};
use imageproc::drawing::{draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use tracing::{debug, info, warn};

use recmari_proto::proto::FrameData;

use crate::analysis::Hud;
use crate::video::frame::Frame;

const FONT_PATH: &str = "C:\\Windows\\Fonts\\consola.ttf";

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
        data: Option<&FrameData>,
        dir: &Path,
    ) -> Result<()> {
        let mut img = frame.image.clone();

        for region in hud.debug_regions() {
            let rect = Rect::at(region.rect.x as i32, region.rect.y as i32)
                .of_size(region.rect.w, region.rect.h);
            draw_hollow_rect_mut(&mut img, rect, region.color);
        }

        self.draw_text_overlay(&mut img, frame, hud, data);

        let path = dir.join(format!("frame_{:08}.png", frame.frame_number));
        img.save(&path)
            .with_context(|| format!("failed to save debug frame to {}", path.display()))?;

        debug!(?path, "saved debug frame");
        Ok(())
    }

    fn draw_text_overlay(
        &self,
        img: &mut RgbImage,
        frame: &Frame,
        hud: &dyn Hud,
        data: Option<&FrameData>,
    ) {
        let Some(font) = &self.font else { return };
        let scale = PxScale::from(TEXT_SCALE);
        let x = 10;
        let mut y = 10;

        let header = format!("F:{}", frame.frame_number);
        draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &header);
        y += TEXT_LINE_HEIGHT;

        let Some(fd) = data else {
            draw_text_mut(img, TEXT_COLOR, x, y, scale, font, "HUD:none");
            return;
        };

        let hud_text = format!("HUD:{}", hud.hud_type());
        draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &hud_text);
        y += TEXT_LINE_HEIGHT;

        let p1_hp = fd.player1.as_ref().unwrap().health_ratio;
        let p2_hp = fd.player2.as_ref().unwrap().health_ratio;
        let p1_text = format!("P1 HP:{:.0}%", p1_hp * 100.0);
        let p2_text = format!("P2 HP:{:.0}%", p2_hp * 100.0);
        draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &p1_text);
        y += TEXT_LINE_HEIGHT;
        draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &p2_text);
    }

    fn load_font() -> Option<FontVec> {
        let data = match std::fs::read(FONT_PATH) {
            Ok(data) => data,
            Err(e) => {
                warn!(path = FONT_PATH, error = %e, "failed to read font file");
                return None;
            }
        };
        match FontVec::try_from_vec(data) {
            Ok(font) => {
                info!(path = FONT_PATH, "loaded debug font");
                Some(font)
            }
            Err(e) => {
                warn!(path = FONT_PATH, error = %e, "failed to parse font file");
                None
            }
        }
    }
}
