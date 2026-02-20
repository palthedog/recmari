mod cli;

use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use prost::Message;
use tracing::{info, warn};

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
