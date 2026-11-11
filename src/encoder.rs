use std::{path::Path, process::Command};

use miette::{Result, bail};

use crate::ResultContextExt;

use crate::browser::shellish_join;

pub trait VideoEncoder {
    fn encode(&self, frames_dir: &Path, output: &Path, output_fps: u32) -> Result<()>;
    /// Check the encoder is ready before a capture starts. Defaults to a no-op.
    fn preflight(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FfmpegCliEncoder {
    binary: String,
    crf: u8,
}

impl Default for FfmpegCliEncoder {
    fn default() -> Self {
        Self {
            binary: "ffmpeg".into(),
            crf: 18,
        }
    }
}

impl FfmpegCliEncoder {
    #[must_use]
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    #[must_use]
    pub fn args_for(&self, frames_dir: &Path, output: &Path, output_fps: u32) -> Vec<String> {
        vec![
            "-y".into(),
            "-framerate".into(),
            output_fps.to_string(),
            "-i".into(),
            frames_dir.join("frame-%06d.png").display().to_string(),
            "-vf".into(),
            // Tag the frames as sRGB (BT.709 primaries, sRGB transfer). ffmpeg 8
            // and 9 take these two tags from the frames and drop the encoder
            // options below; without the sRGB transfer tag, Chrome and
            // QuickTime lift the midtones (#808080 plays back as #8b8b8b).
            "pad=ceil(iw/2)*2:ceil(ih/2)*2,setparams=color_primaries=bt709:color_trc=iec61966-2-1"
                .into(),
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            self.crf.to_string(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            // Screencast frames are RGB PNGs, while GitHub-friendly H.264 MP4s
            // are YUV. `-colorspace` and `-color_range` pick the BT.709 limited
            // range conversion and signal it, so players do not have to guess.
            "-color_primaries".into(),
            "bt709".into(),
            "-color_trc".into(),
            "iec61966-2-1".into(),
            "-colorspace".into(),
            "bt709".into(),
            "-color_range".into(),
            "tv".into(),
            "-movflags".into(),
            "+faststart".into(),
            output.display().to_string(),
        ]
    }
}

impl VideoEncoder for FfmpegCliEncoder {
    fn encode(&self, frames_dir: &Path, output: &Path, output_fps: u32) -> Result<()> {
        let args = self.args_for(frames_dir, output, output_fps);
        let result = Command::new(&self.binary)
            .args(&args)
            .output()
            .with_context(|| format!("failed to start `{}`", self.binary))?;

        if !result.status.success() {
            bail!(
                "Could not assemble demo.mp4 with ffmpeg.\n\nCommand:\n  {} {}\n\nFrames are still available at:\n  {}\n\nStderr:\n{}",
                self.binary,
                shellish_join(&args),
                frames_dir.display(),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        Ok(())
    }

    fn preflight(&self) -> Result<()> {
        crate::preflight::ensure_binary(
            &self.binary,
            "Install Sepia with Pixi to bundle it: \
             `pixi global install -c https://prefix.dev/tim -c conda-forge sepia`, \
             or install it on its own with `pixi global install ffmpeg`.",
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn builds_constant_frame_rate_mp4_args() {
        let encoder = FfmpegCliEncoder::default();
        let args = encoder.args_for(&PathBuf::from("frames"), &PathBuf::from("demo.mp4"), 24);
        assert_eq!(args[0], "-y");
        assert!(args.contains(&"-framerate".to_string()));
        assert!(args.contains(&"24".to_string()));
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"yuv420p".to_string()));
        assert!(args.contains(&"-color_primaries".to_string()));
        assert!(args.contains(&"bt709".to_string()));
        assert!(args.contains(&"-color_range".to_string()));
        assert!(args.contains(&"tv".to_string()));
    }
}
