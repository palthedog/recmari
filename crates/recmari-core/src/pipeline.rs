use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use tracing::{info, warn};

use recmari_proto::proto::{
    source_metadata::Source, FrameData, Match, PlayerState, Round, SourceMetadata, VideoFileSource,
};

use crate::analysis::huds::manemon::ManemonHud;
use crate::analysis::Hud;
use crate::debug::DebugRenderer;
use crate::video::decoder::VideoDecoder;

/// Both players' health must be at or above this to count as "full".
const ROUND_RESET_THRESHOLD: f64 = 0.95;
/// At least one player's health must drop below this to arm round detection.
const DAMAGE_THRESHOLD: f64 = 0.5;
/// Parameters for the analysis pipeline.
pub struct PipelineConfig {
    /// Analyze every Nth decoded frame (1 = every frame).
    pub sample_rate: u32,
    /// Frame number to start decoding from.
    pub start_frame: u32,
    /// Maximum number of frames to process, or None for the entire video.
    pub max_frames: Option<u32>,
    /// Directory to write debug frame images, or None to skip.
    pub debug_frames_dir: Option<PathBuf>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 60,
            start_frame: 0,
            max_frames: None,
            debug_frames_dir: None,
        }
    }
}

/// Run the analysis pipeline on a video file.
///
/// When `max_frames` is set, collects up to that many frames (skipping sample_rate filtering),
/// saves debug overlays if configured, and returns an empty Vec.
/// Otherwise, processes the full video and returns detected matches.
pub fn run_pipeline(input: &Path, config: &PipelineConfig) -> Result<Vec<Match>> {
    if !input.exists() {
        bail!("input video does not exist: {}", input.display());
    }
    if config.sample_rate < 1 {
        bail!("sample_rate must be >= 1, got {}", config.sample_rate);
    }

    info!(
        ?input,
        start_frame = config.start_frame,
        max_frames = ?config.max_frames,
        sample_rate = config.sample_rate,
        "pipeline starting"
    );

    let mut decoder =
        VideoDecoder::open_at_frame(input, config.start_frame).context("failed to open video")?;
    let hud = ManemonHud::new(decoder.width(), decoder.height());

    let debug_renderer = config.debug_frames_dir.as_ref().map(|dir| {
        std::fs::create_dir_all(dir).expect("failed to create debug frames directory");
        info!(?dir, "debug frames directory ready");
        DebugRenderer::new()
    });

    let frame_data = collect_frame_data(&mut decoder, &hud, config, &debug_renderer)?;
    info!(
        total_sampled_frames = frame_data.len(),
        "frame collection complete"
    );

    let m = segment_into_match(&frame_data, input);
    info!(round_count = m.rounds.len(), "pipeline complete");

    Ok(vec![m])
}

fn collect_frame_data(
    decoder: &mut VideoDecoder,
    hud: &dyn Hud,
    config: &PipelineConfig,
    debug_renderer: &Option<DebugRenderer>,
) -> Result<Vec<FrameData>> {
    let mut results: Vec<FrameData> = Vec::new();
    let mut last_p1: Option<f64> = None;
    let mut last_p2: Option<f64> = None;

    loop {
        if let Some(max) = config.max_frames {
            if results.len() >= max as usize {
                break;
            }
        }

        let Some(frame) = decoder.next_frame()? else {
            break;
        };

        // When max_frames is set, process every decoded frame without sampling.
        if config.max_frames.is_none() && frame.frame_number % config.sample_rate != 0 {
            continue;
        }

        info!(frame_number = frame.frame_number, "processing frame");

        let hp = hud.analyze_hp(&frame);

        let p1 = hp.p1.or(last_p1);
        let p2 = hp.p2.or(last_p2);

        let (Some(p1), Some(p2)) = (p1, p2) else {
            warn!(
                frame_number = frame.frame_number,
                p1_available = p1.is_some(),
                p2_available = p2.is_some(),
                "no HP data available yet, skipping"
            );
            continue;
        };

        if hp.p1.is_some() {
            last_p1 = Some(p1);
        }
        if hp.p2.is_some() {
            last_p2 = Some(p2);
        }

        let fd = FrameData {
            frame_number: frame.frame_number,
            timestamp_seconds: frame.timestamp_seconds,
            player1: Some(PlayerState { health_ratio: p1 }),
            player2: Some(PlayerState { health_ratio: p2 }),
        };

        if let (Some(renderer), Some(dir)) = (debug_renderer, &config.debug_frames_dir) {
            renderer
                .save_frame(&frame, hud, &fd, dir)
                .context("failed to save debug frame")?;
        }

        results.push(fd);
    }

    Ok(results)
}

fn segment_into_match(frames: &[FrameData], input: &Path) -> Match {
    let round_frames = split_into_rounds(frames);
    let rounds: Vec<Round> = round_frames
        .into_iter()
        .enumerate()
        .map(|(i, f)| make_round(i as u32, f))
        .collect();

    let start_seconds = frames
        .first()
        .map(|f| f.timestamp_seconds)
        .unwrap_or(0.0);

    info!(round_count = rounds.len(), start_seconds, "match built");

    Match {
        source: Some(SourceMetadata {
            source: Some(Source::VideoFile(VideoFileSource {
                file_path: input.to_string_lossy().into_owned(),
                start_seconds,
            })),
        }),
        rounds,
    }
}

/// Partition frame data into rounds by detecting health resets.
fn split_into_rounds(frames: &[FrameData]) -> Vec<Vec<FrameData>> {
    if frames.is_empty() {
        return Vec::new();
    }

    let mut rounds: Vec<Vec<FrameData>> = vec![Vec::new()];
    let mut had_damage = false;

    for fd in frames {
        let p1 = fd.player1.as_ref().unwrap().health_ratio;
        let p2 = fd.player2.as_ref().unwrap().health_ratio;

        if p1 < DAMAGE_THRESHOLD || p2 < DAMAGE_THRESHOLD {
            had_damage = true;
        }

        let is_reset =
            had_damage && p1 >= ROUND_RESET_THRESHOLD && p2 >= ROUND_RESET_THRESHOLD;

        if is_reset {
            info!(
                at_frame = fd.frame_number,
                p1, p2, "round boundary detected"
            );
            had_damage = false;
            rounds.push(Vec::new());
        }

        rounds.last_mut().unwrap().push(fd.clone());
    }

    rounds.retain(|r| !r.is_empty());
    info!(round_count = rounds.len(), "round splitting complete");
    rounds
}

fn make_round(round_index: u32, frames: Vec<FrameData>) -> Round {
    Round {
        round_index,
        frames,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fd(frame_number: u32, ts: f64, p1: f64, p2: f64) -> FrameData {
        FrameData {
            frame_number,
            timestamp_seconds: ts,
            player1: Some(PlayerState { health_ratio: p1 }),
            player2: Some(PlayerState { health_ratio: p2 }),
        }
    }

    #[test]
    fn split_no_damage_yields_one_round() {
        let frames = vec![fd(0, 0.0, 1.0, 1.0), fd(1, 0.5, 0.9, 0.9)];
        let rounds = split_into_rounds(&frames);
        assert_eq!(rounds.len(), 1);
    }

    #[test]
    fn split_detects_round_boundary() {
        let frames = vec![
            fd(0, 0.0, 1.0, 1.0),
            fd(1, 0.5, 0.6, 0.3), // P2 takes significant damage
            fd(2, 1.0, 0.8, 0.0), // KO
            fd(3, 1.5, 1.0, 1.0), // reset — new round
            fd(4, 2.0, 0.9, 0.8),
        ];
        let rounds = split_into_rounds(&frames);
        assert_eq!(rounds.len(), 2, "expected 2 rounds, got {}", rounds.len());
        assert_eq!(rounds[0].len(), 3);
        assert_eq!(rounds[1].len(), 2);
    }

    #[test]
    fn split_empty_input() {
        let rounds = split_into_rounds(&[]);
        assert!(rounds.is_empty());
    }

    #[test]
    fn segment_builds_single_match() {
        let frames = vec![
            fd(0, 0.0, 1.0, 1.0),
            fd(1, 0.5, 0.6, 0.3),
            fd(2, 1.0, 0.8, 0.0),
            fd(3, 1.5, 1.0, 1.0), // reset — new round
            fd(4, 2.0, 0.9, 0.8),
        ];
        let input = Path::new("test.mp4");
        let m = segment_into_match(&frames, input);
        assert_eq!(m.rounds.len(), 2);
        assert_eq!(m.rounds[0].round_index, 0);
        assert_eq!(m.rounds[1].round_index, 1);
    }
}
