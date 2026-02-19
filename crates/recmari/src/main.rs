mod cli;

use std::fs;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::info;

use recmari_core::video::decoder::VideoDecoder;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = cli::Cli::parse();

    match cli.command {
        cli::Command::Analyze {
            input,
            output,
            sample_rate,
            debug_frames,
        } => {
            info!(?input, ?output, sample_rate, "starting analysis");

            let mut decoder =
                VideoDecoder::open(&input).context("failed to open video")?;

            info!(
                width = decoder.width(),
                height = decoder.height(),
                fps = decoder.fps(),
                "video opened"
            );

            // Phase 1: decode first frame and save as PNG for verification.
            if let Some(frame) = decoder.next_frame()? {
                let debug_dir = debug_frames.unwrap_or_else(|| output.with_extension("debug"));
                fs::create_dir_all(&debug_dir)
                    .context("failed to create debug frames directory")?;

                let png_path = debug_dir.join("frame_0000.png");
                frame
                    .image
                    .save(&png_path)
                    .context("failed to save debug frame")?;

                info!(?png_path, "saved first frame as PNG");
            }

            // TODO: full pipeline (Phase 2)
            let _ = sample_rate;

            Ok(())
        }
    }
}
