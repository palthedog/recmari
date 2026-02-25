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

        let p1 = fd.player1.as_ref().unwrap();
        let p2 = fd.player2.as_ref().unwrap();

        let p1_text = format!("P1 HP:{:.0}%", p1.health_ratio * 100.0);
        draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &p1_text);
        y += TEXT_LINE_HEIGHT;

        if let Some(sa) = p1.sa_gauge {
            let sa_text = format!("P1 SA:{:.2}", truncate_decimal(sa, 2));
            draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &sa_text);
            y += TEXT_LINE_HEIGHT;
        }

        let p1_od_text = format_od_text("P1", p1.od_gauge, p1.burnout_gauge);
        draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &p1_od_text);
        y += TEXT_LINE_HEIGHT;

        let p2_text = format!("P2 HP:{:.0}%", p2.health_ratio * 100.0);
        draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &p2_text);
        y += TEXT_LINE_HEIGHT;

        if let Some(sa) = p2.sa_gauge {
            let sa_text = format!("P2 SA:{:.2}", truncate_decimal(sa, 2));
            draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &sa_text);
            y += TEXT_LINE_HEIGHT;
        }

        let p2_od_text = format_od_text("P2", p2.od_gauge, p2.burnout_gauge);
        draw_text_mut(img, TEXT_COLOR, x, y, scale, font, &p2_od_text);
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

/// Format OD gauge text: shows burnout if active, otherwise normal OD value.
fn format_od_text(player: &str, od_gauge: Option<f64>, burnout_gauge: Option<f64>) -> String {
    if let Some(bo) = burnout_gauge {
        format!("{} BO:{:.2}", player, truncate_decimal(bo, 2))
    } else if let Some(od) = od_gauge {
        format!("{} OD:{:.2}", player, truncate_decimal(od, 2))
    } else {
        format!("{} OD:--", player)
    }
}

/// Truncate a float to the given number of decimal places (floor toward zero).
fn truncate_decimal(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).floor() / factor
}
