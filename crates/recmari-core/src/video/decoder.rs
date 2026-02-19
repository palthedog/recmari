use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use anyhow::{bail, Context, Result};
use image::RgbImage;
use tracing::{debug, error, info, warn};

use super::frame::Frame;

/// Video metadata obtained by probing with ffprobe.
struct ProbeResult {
    width: u32,
    height: u32,
    fps: f64,
}

fn probe(path: &Path) -> Result<ProbeResult> {
    info!(?path, "probing video metadata with ffprobe");

    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height,r_frame_rate",
            "-of", "csv=p=0",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run ffprobe — is ffmpeg installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(%stderr, ?path, "ffprobe failed");
        bail!("ffprobe failed: {stderr}");
    }

    // Output format: "width,height,num/den"
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split(',').collect();
    if parts.len() < 3 {
        error!(%stdout, "unexpected ffprobe output format, expected width,height,fps");
        bail!("unexpected ffprobe output: {stdout}");
    }

    let width: u32 = parts[0].parse().context("failed to parse width")?;
    let height: u32 = parts[1].parse().context("failed to parse height")?;

    let fps = if let Some((num, den)) = parts[2].split_once('/') {
        let num: f64 = num.parse().context("failed to parse fps numerator")?;
        let den: f64 = den.parse().context("failed to parse fps denominator")?;
        if den > 0.0 { num / den } else { 0.0 }
    } else {
        parts[2].parse().context("failed to parse fps")?
    };

    if fps <= 0.0 {
        warn!(fps, ?path, "video has non-positive fps, timestamps will be 0.0");
    }

    info!(width, height, fps, "probe completed");
    Ok(ProbeResult { width, height, fps })
}

/// Decodes video frames by piping raw RGB24 data from the ffmpeg CLI.
pub struct VideoDecoder {
    child: Child,
    width: u32,
    height: u32,
    fps: f64,
    frame_count: u32,
    frame_bytes: usize,
}

impl VideoDecoder {
    /// Open a video file for decoding.
    pub fn open(path: &Path) -> Result<Self> {
        assert!(path.exists(), "video file does not exist: {}", path.display());

        let info = probe(path)?;
        assert!(info.width > 0 && info.height > 0, "invalid video dimensions: {}x{}", info.width, info.height);

        info!(?path, "spawning ffmpeg decoder process");

        let child = Command::new("ffmpeg")
            .args(["-i"])
            .arg(path)
            .args([
                "-f", "rawvideo",
                "-pix_fmt", "rgb24",
                "-v", "error",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn ffmpeg — is ffmpeg installed?")?;

        let frame_bytes = (info.width as usize) * (info.height as usize) * 3;

        info!(
            width = info.width,
            height = info.height,
            fps = info.fps,
            frame_bytes,
            "video decoder opened"
        );

        Ok(Self {
            child,
            width: info.width,
            height: info.height,
            fps: info.fps,
            frame_count: 0,
            frame_bytes,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn fps(&self) -> f64 {
        self.fps
    }

    /// Read the next frame from the ffmpeg pipe, or `None` if the video is finished.
    pub fn next_frame(&mut self) -> Result<Option<Frame>> {
        let stdout = self
            .child
            .stdout
            .as_mut()
            .context("ffmpeg stdout not available")?;

        let mut buf = vec![0u8; self.frame_bytes];
        let mut read = 0;

        while read < self.frame_bytes {
            match stdout.read(&mut buf[read..]) {
                Ok(0) => {
                    if read == 0 {
                        info!(total_frames = self.frame_count, "video stream ended");
                        return Ok(None);
                    }
                    error!(
                        read_bytes = read,
                        expected_bytes = self.frame_bytes,
                        frame = self.frame_count,
                        "ffmpeg stream ended mid-frame"
                    );
                    bail!(
                        "ffmpeg stream ended mid-frame (read {read}/{} bytes)",
                        self.frame_bytes,
                    );
                }
                Ok(n) => read += n,
                Err(e) => {
                    error!(frame = self.frame_count, %e, "failed to read from ffmpeg pipe");
                    return Err(e).context("failed to read from ffmpeg pipe");
                }
            }
        }

        let image = RgbImage::from_raw(self.width, self.height, buf)
            .context("failed to create RgbImage from raw frame data")?;

        let frame_number = self.frame_count;
        let timestamp_seconds = if self.fps > 0.0 {
            frame_number as f64 / self.fps
        } else {
            0.0
        };
        self.frame_count += 1;

        debug!(frame_number, timestamp_seconds, "decoded frame");

        Ok(Some(Frame {
            image,
            frame_number,
            timestamp_seconds,
        }))
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        info!(total_frames = self.frame_count, "closing video decoder");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
