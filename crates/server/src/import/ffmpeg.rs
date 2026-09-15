//! The external-converter [`AudioImporter`], retained for desktop and
//! standalone hosts.
//!
//! This is the hardened process import path sample upload used to run inline,
//! moved behind the capability contract without loosening any of it:
//!
//! - the child reads one already-open file descriptor and the protocol
//!   whitelist is `fd` alone, so an uploaded playlist cannot reach another
//!   local file or a network URL;
//! - exactly the first audio stream is mapped, and video, subtitle, data and
//!   metadata streams are dropped;
//! - the conversion is capped by duration (`-t`), by written bytes (`-fs`) and
//!   by a wall-clock timeout that kills the child;
//! - the child is single-threaded, reads no stdin prompts and writes only into
//!   a private temporary directory that is removed however the import ends; and
//! - failures carry the converter's own diagnostics so the message tells
//!   somebody what to do about it.
//!
//! A host that cannot run executables simply does not install this importer.
use super::{
    AudioImporter, DecodedAudio, ImportError, ImportLimits, ImportSource, Scratch,
};
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

/// How often the parent checks a running conversion against its deadline.
const POLL: Duration = Duration::from_millis(20);
/// Converter diagnostics kept in a failure message.
const DIAGNOSTIC_CHARACTERS: usize = 600;

#[derive(Clone, Debug)]
pub struct FfmpegImporter {
    program: OsString,
}

impl FfmpegImporter {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// The configured converter. Hosts choose it; the runtime never searches
    /// for one and never falls back to a name found on `PATH` by itself.
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    /// The complete, hardened argument list. Separated from the spawn so the
    /// security-relevant flags can be asserted without running a converter.
    pub(crate) fn arguments(&self, output: &Path, limits: &ImportLimits) -> Vec<OsString> {
        let mut arguments: Vec<OsString> = [
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-threads",
            "1",
            // Seekable, but unable to open nested local files or network URLs
            // named by an uploaded playlist.
            "-protocol_whitelist",
            "fd",
            "-i",
            "fd:",
            // The first audio stream only; no video, subtitles, data or tags.
            "-map",
            "0:a:0",
            "-vn",
            "-sn",
            "-dn",
            "-map_metadata",
            "-1",
            "-t",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        // One second past the limit, so audio that is merely over-long is
        // rejected by the frame check with the documented message instead of
        // being silently truncated to exactly the limit.
        arguments.push(format!("{:.0}", limits.max_seconds + 1.).into());
        arguments.push("-ar".into());
        arguments.push(limits.target_rate.to_string().into());
        arguments.push("-c:a".into());
        arguments.push("pcm_f32le".into());
        arguments.push("-fs".into());
        arguments.push(limits.max_output_bytes.to_string().into());
        arguments.push("-f".into());
        arguments.push("wav".into());
        arguments.push(output.into());
        arguments
    }
}

impl AudioImporter for FfmpegImporter {
    fn name(&self) -> &str {
        "ffmpeg"
    }

    fn formats(&self) -> Vec<&'static str> {
        vec!["Additional formats decoded by the configured FFmpeg build"]
    }

    fn decode(
        &self,
        source: &ImportSource,
        limits: &ImportLimits,
    ) -> Result<DecodedAudio, ImportError> {
        limits.check_input_len(source.len()?)?;
        let input = source.local_file()?;
        let scratch = Scratch::new("convert")?;
        let output = scratch.0.join("converted.wav");
        let stdin = std::fs::File::open(input.path())
            .map_err(|e| ImportError::Failed(format!("Open the uploaded audio: {e}")))?;
        let mut child = std::process::Command::new(&self.program)
            .args(self.arguments(&output, limits))
            .stdin(std::process::Stdio::from(stdin))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                ImportError::Failed(format!("FFmpeg is required for audio import: {e}"))
            })?;
        // Drained on its own thread: a converter that filled the pipe while the
        // parent polled the deadline would otherwise block forever.
        let mut pipe = child.stderr.take().expect("piped converter diagnostics");
        let diagnostics = std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = pipe.read_to_end(&mut text);
            text
        });
        let deadline = Instant::now() + limits.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = diagnostics.join();
                    return Err(ImportError::Failed("Audio conversion timed out".into()));
                }
                Ok(None) => std::thread::sleep(POLL),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = diagnostics.join();
                    return Err(ImportError::Failed(format!(
                        "Audio conversion could not be supervised: {e}"
                    )));
                }
            }
        };
        let reported = diagnostics.join().unwrap_or_default();
        if !status.success() {
            return Err(ImportError::Invalid(format!(
                "Audio conversion failed. Install FFmpeg with the fd protocol and select a supported audio file: {}",
                String::from_utf8_lossy(&reported)
                    .chars()
                    .take(DIAGNOSTIC_CHARACTERS)
                    .collect::<String>()
            )));
        }
        let audio = read_converted(&output, limits);
        // The temporary input copy and the converted file go away here whatever
        // the outcome was.
        drop(scratch);
        drop(input);
        audio
    }
}

/// Reads the converter's canonical float WAV back under the same bounds the
/// in-process decoder applies.
fn read_converted(path: &Path, limits: &ImportLimits) -> Result<DecodedAudio, ImportError> {
    let unreadable =
        |e: hound::Error| ImportError::Invalid(format!("Read the converted audio: {e}"));
    let mut reader = hound::WavReader::open(path).map_err(unreadable)?;
    let specification = reader.spec();
    // Channel count, rate and length are checked before the sample buffer is
    // allocated, not after.
    let (rate, channels) = limits.check_stream(
        Some(specification.sample_rate),
        Some(specification.channels as usize),
        Some(reader.duration() as u64),
    )?;
    let lanes = channels as usize;
    let mut samples: Vec<f32> =
        Vec::with_capacity((reader.duration() as usize).saturating_mul(lanes));
    if specification.sample_format == hound::SampleFormat::Float {
        for sample in reader.samples::<f32>() {
            samples.push(sample.map_err(unreadable)?);
        }
    } else {
        let scale = 2_f32.powi(specification.bits_per_sample as i32 - 1);
        for sample in reader.samples::<i32>() {
            samples.push(sample.map_err(unreadable)? as f32 / scale);
        }
    }
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err(ImportError::Rejected(
            "Decoded audio contains nonfinite samples".into(),
        ));
    }
    // `-ar` already produced the project rate; the conversion below is a
    // safety net for a converter that ignored it, not a normal code path.
    if rate == limits.target_rate {
        return DecodedAudio::new(samples, channels, limits.target_rate, limits);
    }
    let frames = (samples.len() / lanes) as f64 * limits.target_rate as f64 / rate as f64;
    limits.check_capacity((frames.round() as u64).saturating_mul(channels as u64))?;
    let converted = crate::samples::resample(&samples, lanes, rate, limits.target_rate);
    drop(samples);
    DecodedAudio::new(converted, channels, limits.target_rate, limits)
}
