use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use tracing::info;

use recmari_proto::proto::{
    source_metadata::Source, FrameData, Match, PlayerState, Round, SourceMetadata, VideoFileSource,
    Winner,
};

use crate::analysis::huds::manemon::ManemonHud;
use crate::analysis::{Hud, OdValue};
use crate::debug::DebugRenderer;
use crate::video::decoder::VideoDecoder;
use crate::video::frame::Frame;

/// Both players' health must be at or above this to count as "full".
const ROUND_RESET_THRESHOLD: f64 = 0.95;
/// At least one player's health must drop below this to arm round detection.
const DAMAGE_THRESHOLD: f64 = 0.5;
/// Number of round wins required to win a match.
const ROUNDS_TO_WIN: u32 = 2;

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

/// Carries forward last-known gauge values across frames when a reading is temporarily unavailable.
#[derive(Default)]
struct GapFillState {
    p1_hp: Option<f64>,
    p2_hp: Option<f64>,
    p1_sa: Option<f64>,
    p2_sa: Option<f64>,
    p1_od: Option<OdValue>,
    p2_od: Option<OdValue>,
}

impl GapFillState {
    fn clear(&mut self) {
        *self = Self::default();
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

    let debug_renderer = config.debug_frames_dir.as_ref().map(|dir| {
        std::fs::create_dir_all(dir).expect("failed to create debug frames directory");
        info!(?dir, "debug frames directory ready");
        DebugRenderer::new()
    });

    let frame_data = collect_frame_data(&mut decoder, config, &debug_renderer)?;
    info!(
        total_sampled_frames = frame_data.len(),
        "frame collection complete"
    );

    let matches = segment_into_matches(&frame_data, input);
    for (i, m) in matches.iter().enumerate() {
        log_match_summary(i + 1, m);
    }
    info!(match_count = matches.len(), "pipeline complete");

    Ok(matches)
}

fn collect_frame_data(
    decoder: &mut VideoDecoder,
    config: &PipelineConfig,
    debug_renderer: &Option<DebugRenderer>,
) -> Result<Vec<FrameData>> {
    let hud = ManemonHud::new(decoder.width(), decoder.height());
    let mut results: Vec<FrameData> = Vec::new();
    let mut gap = GapFillState::default();
    let mut frames_examined = 0u32;

    loop {
        let Some(frame) = decoder.next_frame()? else {
            break;
        };

        if config.max_frames.is_none() && frame.frame_number % config.sample_rate != 0 {
            continue;
        }

        let detected = hud.detect_hud(&frame);
        info!(
            frame_number = frame.frame_number,
            hud_detected = detected,
            "processing frame"
        );

        let fd = if detected {
            Some(analyze_frame(&hud, &frame, &mut gap))
        } else {
            gap.clear();
            None
        };

        if let (Some(renderer), Some(dir)) = (debug_renderer, &config.debug_frames_dir) {
            renderer
                .save_frame(&frame, &hud, fd.as_ref(), dir)
                .context("failed to save debug frame")?;
        }

        if let Some(fd) = fd {
            results.push(fd);
        }

        frames_examined += 1;
        if let Some(max) = config.max_frames {
            if frames_examined >= max {
                break;
            }
        }
    }

    Ok(results)
}

/// Read HP, SA, and OD from a detected HUD frame, applying gap-fill from previous readings.
fn analyze_frame(hud: &dyn Hud, frame: &Frame, gap: &mut GapFillState) -> FrameData {
    let hp = hud.analyze_hp(frame);
    let p1 = hp.p1.or(gap.p1_hp);
    let p2 = hp.p2.or(gap.p2_hp);

    if hp.p1.is_some() {
        gap.p1_hp = p1;
    }
    if hp.p2.is_some() {
        gap.p2_hp = p2;
    }

    let sa = hud.analyze_sa(frame);
    let p1_sa = sa.p1.or(gap.p1_sa);
    let p2_sa = sa.p2.or(gap.p2_sa);

    if sa.p1.is_some() {
        gap.p1_sa = p1_sa;
    }
    if sa.p2.is_some() {
        gap.p2_sa = p2_sa;
    }

    let od = hud.analyze_od(frame);
    let p1_od = od.p1.or(gap.p1_od);
    let p2_od = od.p2.or(gap.p2_od);

    if od.p1.is_some() {
        gap.p1_od = p1_od;
    }
    if od.p2.is_some() {
        gap.p2_od = p2_od;
    }

    FrameData {
        frame_number: frame.frame_number,
        timestamp_seconds: frame.timestamp_seconds,
        player1: Some(od_to_player_state(p1, p1_sa, p1_od)),
        player2: Some(od_to_player_state(p2, p2_sa, p2_od)),
    }
}

fn od_to_player_state(hp: Option<f64>, sa: Option<f64>, od: Option<OdValue>) -> PlayerState {
    let (od_gauge, burnout_gauge) = match od {
        Some(OdValue::Normal(v)) => (Some(v), None),
        Some(OdValue::Burnout(v)) => (None, Some(v)),
        None => (None, None),
    };
    PlayerState {
        health_ratio: hp,
        sa_gauge: sa,
        od_gauge,
        burnout_gauge,
    }
}

struct RoundResult {
    winner: Winner,
    p1_hp: Option<f64>,
    p2_hp: Option<f64>,
}

fn round_result(frames: &[FrameData]) -> RoundResult {
    for fd in frames.iter().rev() {
        let p1_hp = fd.player1.as_ref().and_then(|p| p.health_ratio);
        let p2_hp = fd.player2.as_ref().and_then(|p| p.health_ratio);
        match (p1_hp, p2_hp) {
            (Some(p1), Some(p2)) => {
                let winner = if p1 > p2 {
                    Winner::P1
                } else if p2 > p1 {
                    Winner::P2
                } else {
                    Winner::Unknown
                };
                return RoundResult {
                    winner,
                    p1_hp: Some(p1),
                    p2_hp: Some(p2),
                };
            }
            _ => continue,
        }
    }
    RoundResult {
        winner: Winner::Unknown,
        p1_hp: None,
        p2_hp: None,
    }
}

fn segment_into_matches(frames: &[FrameData], input: &Path) -> Vec<Match> {
    let all_rounds = split_into_rounds(frames);
    let file_path = input.to_string_lossy().into_owned();

    let mut matches: Vec<Match> = Vec::new();
    let mut current_rounds: Vec<Vec<FrameData>> = Vec::new();
    let mut p1_wins = 0u32;
    let mut p2_wins = 0u32;

    for round_frames in all_rounds {
        match round_result(&round_frames).winner {
            Winner::P1 => p1_wins += 1,
            Winner::P2 => p2_wins += 1,
            Winner::Unknown => {}
        }
        current_rounds.push(round_frames);

        if p1_wins >= ROUNDS_TO_WIN || p2_wins >= ROUNDS_TO_WIN {
            let m = build_match(
                &file_path,
                current_rounds.drain(..).collect(),
                p1_wins,
                p2_wins,
            );
            matches.push(m);
            p1_wins = 0;
            p2_wins = 0;
        }
    }

    if !current_rounds.is_empty() {
        let m = build_match(&file_path, current_rounds, p1_wins, p2_wins);
        matches.push(m);
    }

    info!(match_count = matches.len(), "match segmentation complete");
    matches
}

fn build_match(
    file_path: &str,
    round_frames: Vec<Vec<FrameData>>,
    p1_wins: u32,
    p2_wins: u32,
) -> Match {
    let start_seconds = round_frames
        .first()
        .and_then(|r| r.first())
        .map(|f| f.timestamp_seconds)
        .unwrap_or(0.0);

    let rounds = round_frames
        .into_iter()
        .enumerate()
        .map(|(i, f)| make_round(i as u32, f))
        .collect();

    let winner = if p1_wins >= ROUNDS_TO_WIN {
        Winner::P1
    } else if p2_wins >= ROUNDS_TO_WIN {
        Winner::P2
    } else {
        Winner::Unknown
    };

    Match {
        source: Some(SourceMetadata {
            source: Some(Source::VideoFile(VideoFileSource {
                file_path: file_path.to_owned(),
                start_seconds,
            })),
        }),
        rounds,
        winner: winner.into(),
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
        let p1 = fd.player1.as_ref().and_then(|p| p.health_ratio);
        let p2 = fd.player2.as_ref().and_then(|p| p.health_ratio);

        if let (Some(p1), Some(p2)) = (p1, p2) {
            if p1 < DAMAGE_THRESHOLD || p2 < DAMAGE_THRESHOLD {
                had_damage = true;
            }

            let is_reset = had_damage && p1 >= ROUND_RESET_THRESHOLD && p2 >= ROUND_RESET_THRESHOLD;

            if is_reset {
                info!(
                    at_frame = fd.frame_number,
                    p1, p2, "round boundary detected"
                );
                had_damage = false;
                rounds.push(Vec::new());
            }
        }

        rounds.last_mut().unwrap().push(fd.clone());
    }

    rounds.retain(|r| !r.is_empty() && !is_reset_only(r));
    info!(round_count = rounds.len(), "round splitting complete");
    rounds
}

/// Returns true if every frame with readable HP shows both players near full health.
/// These rounds are artifacts from match-to-match transitions (HP reset visible briefly
/// before HUD disappears for the rematch screen).
fn is_reset_only(frames: &[FrameData]) -> bool {
    frames.iter().all(|fd| {
        let p1 = fd.player1.as_ref().and_then(|p| p.health_ratio);
        let p2 = fd.player2.as_ref().and_then(|p| p.health_ratio);
        match (p1, p2) {
            (Some(p1), Some(p2)) => p1 >= ROUND_RESET_THRESHOLD && p2 >= ROUND_RESET_THRESHOLD,
            _ => true,
        }
    })
}

fn make_round(round_index: u32, frames: Vec<FrameData>) -> Round {
    let result = round_result(&frames);
    Round {
        round_index,
        frames,
        winner: result.winner.into(),
    }
}

fn log_match_summary(match_number: usize, m: &Match) {
    info!("* Match number {}", match_number);
    for round in &m.rounds {
        let result = round_result(&round.frames);
        let round_index = round.round_index;
        let p1_hp = result.p1_hp;
        let p2_hp = result.p2_hp;
        info!(
            "  round {round_index}: winner: {:?}, final HP: P1={p1_hp:.2?}, P2={p2_hp:.2?}",
            result.winner,
        );
    }

    let winner = Winner::try_from(m.winner).unwrap_or(Winner::Unknown);
    info!(match_number, winner = ?winner, "  match result");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fd(frame_number: u32, ts: f64, p1: f64, p2: f64) -> FrameData {
        FrameData {
            frame_number,
            timestamp_seconds: ts,
            player1: Some(PlayerState {
                health_ratio: Some(p1),
                sa_gauge: None,
                od_gauge: None,
                burnout_gauge: None,
            }),
            player2: Some(PlayerState {
                health_ratio: Some(p2),
                sa_gauge: None,
                od_gauge: None,
                burnout_gauge: None,
            }),
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
    fn segment_two_rounds_one_match() {
        let frames = vec![
            fd(0, 0.0, 1.0, 1.0),
            fd(1, 0.5, 0.6, 0.3),
            fd(2, 1.0, 0.8, 0.0), // P1 wins round 1
            fd(3, 1.5, 1.0, 1.0), // reset
            fd(4, 2.0, 0.7, 0.0), // P1 wins round 2 → match complete
        ];
        let input = Path::new("test.mp4");
        let matches = segment_into_matches(&frames, input);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].rounds.len(), 2);
        assert_eq!(matches[0].winner, Winner::P1 as i32);
        assert_eq!(matches[0].rounds[0].winner, Winner::P1 as i32);
        assert_eq!(matches[0].rounds[1].winner, Winner::P1 as i32);
    }

    #[test]
    fn segment_six_rounds_into_multiple_matches() {
        let frames = vec![
            // Match 1: P1 wins 2-0
            fd(0, 0.0, 1.0, 1.0),
            fd(1, 0.5, 0.5, 0.0), // P1 wins R1
            fd(2, 1.0, 1.0, 1.0), // reset
            fd(3, 1.5, 0.6, 0.0), // P1 wins R2 → match 1 done
            fd(4, 2.0, 1.0, 1.0), // reset
            // Match 2: P2 wins 2-1
            fd(5, 2.5, 0.0, 0.4),  // P2 wins R1
            fd(6, 3.0, 1.0, 1.0),  // reset
            fd(7, 3.5, 0.7, 0.0),  // P1 wins R2
            fd(8, 4.0, 1.0, 1.0),  // reset
            fd(9, 4.5, 0.0, 0.3),  // P2 wins R3 → match 2 done
            fd(10, 5.0, 1.0, 1.0), // reset
            // Match 3: incomplete (1 round)
            fd(11, 5.5, 0.8, 0.0), // P1 wins R1
        ];
        let input = Path::new("test.mp4");
        let matches = segment_into_matches(&frames, input);
        assert_eq!(matches.len(), 3);

        assert_eq!(matches[0].rounds.len(), 2);
        assert_eq!(matches[0].winner, Winner::P1 as i32);

        assert_eq!(matches[1].rounds.len(), 3);
        assert_eq!(matches[1].winner, Winner::P2 as i32);
        assert_eq!(matches[1].rounds[0].winner, Winner::P2 as i32);
        assert_eq!(matches[1].rounds[1].winner, Winner::P1 as i32);
        assert_eq!(matches[1].rounds[2].winner, Winner::P2 as i32);

        assert_eq!(matches[2].rounds.len(), 1);
        assert_eq!(matches[2].winner, Winner::Unknown as i32);
    }

    #[test]
    fn round_winner_skips_trailing_none_hp() {
        let frames = vec![
            fd(0, 0.0, 1.0, 1.0),
            fd(1, 0.5, 0.3, 0.0), // KO visible
            FrameData {
                frame_number: 2,
                timestamp_seconds: 1.0,
                player1: Some(PlayerState {
                    health_ratio: None,
                    sa_gauge: None,
                    od_gauge: None,
                    burnout_gauge: None,
                }),
                player2: Some(PlayerState {
                    health_ratio: None,
                    sa_gauge: None,
                    od_gauge: None,
                    burnout_gauge: None,
                }),
            },
        ];
        assert_eq!(round_result(&frames).winner, Winner::P1);
    }
}
