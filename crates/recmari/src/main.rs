mod cli;

use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::Parser;
use prost::Message;
use tracing::{info, warn};

use recmari_core::analysis::huds::manemon;
use recmari_core::pipeline::{self, PipelineConfig};
use recmari_proto::proto::Match;

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
            frame,
        } => {
            info!(?input, ?output, sample_rate, ?frame, "starting analysis");

            let config = PipelineConfig {
                sample_rate,
                start_frame: frame.unwrap_or(0),
                max_frames: frame.map(|_| 1),
                debug_frames_dir: debug_frames,
            };

            let matches =
                pipeline::run_pipeline(&input, &config).context("pipeline failed")?;

            if matches.is_empty() {
                warn!("no matches detected in video");
            }

            write_matches(&matches, &output)?;

            info!(
                match_count = matches.len(),
                total_rounds = matches.iter().map(|m| m.rounds.len()).sum::<usize>(),
                ?output,
                "analysis complete"
            );

            Ok(())
        }

        cli::Command::ProbeScan { image } => {
            let digit_images = parse_image_args(&image)?;
            let entries = manemon::scan_sa_digit_probes(&digit_images);

            // For each digit in the cascade, find the best probe position.
            // Required: probe[i] must be foreground for digit i and background
            // for all digits checked after i. Prefer fewer total foreground digits.
            let cascade_masks: [(u8, u8); 4] = [
                (0b0001, 0b1111), // digit 0: must be foreground, 1/2/3 must be background
                (0b0010, 0b1110), // digit 1: must be foreground, 2/3 must be background
                (0b0100, 0b1100), // digit 2: must be foreground, 3 must be background
                (0b1000, 0b1000), // digit 3: must be foreground
            ];

            let cx = entries.iter().map(|e| e.x).min().unwrap()
                + entries.iter().map(|e| e.x).max().unwrap();
            let cx = cx / 2;
            let cy = entries.iter().map(|e| e.y).min().unwrap()
                + entries.iter().map(|e| e.y).max().unwrap();
            let cy = cy / 2;

            for (digit, (required, check_mask)) in cascade_masks.iter().enumerate() {
                let best = entries
                    .iter()
                    .filter(|e| e.fg_mask & check_mask == *required)
                    .min_by_key(|e| {
                        let dist = e.x.abs_diff(cx) + e.y.abs_diff(cy);
                        (e.fg_mask.count_ones(), dist)
                    });

                match best {
                    Some(c) => println!(
                        "Probe {{ x: {x}, y: {y} }}, // foreground for: {digits}",
                        x = c.x,
                        y = c.y,
                        digits = (0..4u8)
                            .filter(|d| c.fg_mask & (1 << d) != 0)
                            .map(|d| d.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    None => warn!(digit, "no valid probe position found"),
                }
            }

            Ok(())
        }
    }
}

/// Serialize matches as length-delimited protobuf and write to file.
fn write_matches(matches: &[Match], output: &Path) -> Result<()> {
    info!(?output, match_count = matches.len(), "writing protobuf output");

    let mut buf = Vec::new();
    for m in matches {
        m.encode_length_delimited(&mut buf)
            .context("failed to encode Match")?;
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .context("failed to create output directory")?;
    }

    std::fs::write(output, &buf)
        .with_context(|| format!("failed to write {}", output.display()))?;

    info!(?output, bytes = buf.len(), "protobuf output written");
    Ok(())
}

/// Parse "--image path:digit" arguments into (RgbImage, digit) pairs.
fn parse_image_args(args: &[String]) -> Result<Vec<(image::RgbImage, u8)>> {
    let mut result = Vec::with_capacity(args.len());

    for arg in args {
        let (path_str, digit_str) = arg
            .rsplit_once(':')
            .with_context(|| format!("expected 'path:digit' format, got '{arg}'"))?;

        let digit: u8 = digit_str
            .parse()
            .with_context(|| format!("invalid digit '{digit_str}' in '{arg}'"))?;

        if digit > 3 {
            bail!("digit must be 0–3, got {digit} in '{arg}'");
        }

        let img = image::open(path_str)
            .with_context(|| format!("failed to open image '{path_str}'"))?
            .into_rgb8();

        info!(path = path_str, digit, "loaded image");
        result.push((img, digit));
    }

    Ok(result)
}
