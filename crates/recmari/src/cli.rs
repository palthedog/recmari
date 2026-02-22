use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "recmari", about = "SF6 gameplay analyzer")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Analyze a recorded video file.
    Analyze {
        /// Path to the input video file (MP4, etc.).
        #[arg(short, long)]
        input: PathBuf,

        /// Path to write the output protobuf file.
        #[arg(short, long)]
        output: PathBuf,

        /// Analyze every Nth frame (default: 60, i.e. 60 samples/sec from 60fps).
        #[arg(short, long, default_value_t = 60)]
        sample_rate: u32,

        /// Directory to save debug frames with HUD region overlays.
        #[arg(long)]
        debug_frames: Option<PathBuf>,

        /// Analyze only this single frame (seek + analyze + debug overlay).
        #[arg(long)]
        frame: Option<u32>,
    },

    /// Scan SA digit bounding box for unique probe positions.
    ProbeScan {
        /// Image:digit pairs (e.g. "path/to/both_sa0.png:0").
        /// Each image must show the specified digit for both P1 and P2.
        #[arg(long, required = true)]
        image: Vec<String>,
    },
}
