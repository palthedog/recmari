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

        /// Analyze every Nth frame (default: 2, i.e. 30 samples/sec from 60fps).
        #[arg(short, long, default_value_t = 2)]
        sample_rate: u32,

        /// Directory to save debug frames with HUD region overlays.
        #[arg(long)]
        debug_frames: Option<PathBuf>,
    },
}
