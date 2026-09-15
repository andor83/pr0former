//! Audio import as a host-owned capability.
//!
//! Sample upload used to launch FFmpeg itself. It now asks whichever
//! [`AudioImporter`] the host installed in [`crate::RuntimeConfig`] to turn the
//! uploaded bytes into [`DecodedAudio`]. A host that cannot run an executable —
//! the iPadOS shell of `docs/IPAD_RUNTIME_PLAN.md` — selects the pure-Rust
//! decoder and still imports audio; the standalone server and the desktop
//! application select the pure-Rust decoder first and keep their external
//! converter for the formats it does not cover.
//!
//! The contract is bounded at both ends:
//!
//! - the input is one already-received local blob or file ([`ImportSource`]).
//!   No importer resolves playlists, nested files or network URLs from the
//!   content, and none of them trusts a filename extension;
//! - every ceiling is an explicit [`ImportLimits`] field, checked against the
//!   declared stream *before* a decoded buffer is allocated and again before
//!   each buffer grows; and
//! - the output is interleaved finite `f32` at the project sample rate with
//!   1–8 channels and at most 30 seconds ([`DecodedAudio`]), written as the
//!   canonical 32-bit float WAV that `sample_library` and the DSP sample cache
//!   already expect.
//!
//! Decoding, resampling and WAV writing are synchronous and CPU-bound on
//! purpose: callers run them on a blocking worker, never on an async executor
//! and never anywhere near a render or device callback.
use std::{
    ffi::OsString,
    fmt,
    io::{Read, Seek},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// The external-converter importer exists only on hosts that can run a child
/// process. iOS/iPadOS and Android sandbox `fork`/`exec` away entirely, so there
/// the module is not compiled, `FfmpegImporter` does not exist, and no import
/// path can reach `std::process::Command`. In-process decoding is unaffected: it
/// is the same [`SymphoniaImporter`] every host already runs first.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod ffmpeg;
pub mod pure_rust;
#[cfg(test)]
mod tests;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use ffmpeg::FfmpegImporter;
pub use pure_rust::SymphoniaImporter;

/// Reported when a host configured no importer at all. Kept verbatim from the
/// message sample upload has always returned for a host without a converter.
pub const NO_IMPORTER: &str =
    "This host has no audio converter configured for sample import";

/// Channels an imported sample may carry, matching the audio connection
/// contract in `AGENTS.md`.
pub const MAX_CHANNELS: u16 = 8;
/// Seconds of decoded output an import may produce.
pub const MAX_SECONDS: f64 = 30.;
/// Compressed input bytes accepted, matching the upload body limit.
pub const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
/// Converted bytes an external converter may write before it is stopped.
/// Unchanged from the FFmpeg `-fs` ceiling import has always used.
pub const MAX_OUTPUT_BYTES: u64 = 200_000_000;
/// Highest source sample rate accepted before decoding starts.
pub const MAX_SOURCE_RATE: u32 = 384_000;
/// Interleaved `f32` values that may be held at once: 256 MB, the same
/// prepared-sample ceiling `samples::prepare` enforces.
pub const MAX_DECODED_SAMPLES: usize = 64_000_000;
/// Wall-clock ceiling for an external converter process.
pub const CONVERTER_TIMEOUT: Duration = Duration::from_secs(60);

/// A seekable, already-received import input.
///
/// `Send + Sync` because the pure-Rust decoder's media source requires it and
/// because importers run on blocking workers.
pub trait ReadSeek: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> ReadSeek for T {}

/// Why an import did not produce audio.
///
/// The classification decides fallback: only [`ImportError::Unsupported`] and
/// [`ImportError::Unavailable`] let a chain try the next importer. A limit or
/// policy violation stops the chain, so a file that is too long, too wide, or
/// not audio at all is never handed to a second decoder in the hope of a
/// different answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// This importer does not know the container or codec.
    Unsupported(String),
    /// The input or its decoded output breaks an import limit or policy.
    Rejected(String),
    /// Recognized but unusable: corrupt, truncated, or carrying no audio.
    Invalid(String),
    /// This host configured no importer with the required capability.
    Unavailable(String),
    /// The importer itself failed: I/O, a converter process, or a timeout.
    Failed(String),
}

impl ImportError {
    /// Whether a chain may try the next importer after this error.
    pub fn may_fall_back(&self) -> bool {
        matches!(self, Self::Unsupported(_) | Self::Unavailable(_))
    }

    /// A limit or policy violation, which never falls back.
    pub fn is_rejection(&self) -> bool {
        matches!(self, Self::Rejected(_))
    }

    /// Stable classification label for tests and diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Unsupported(_) => "unsupported",
            Self::Rejected(_) => "rejected",
            Self::Invalid(_) => "invalid",
            Self::Unavailable(_) => "unavailable",
            Self::Failed(_) => "failed",
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::Unsupported(m)
            | Self::Rejected(m)
            | Self::Invalid(m)
            | Self::Unavailable(m)
            | Self::Failed(m) => m,
        }
    }
}

/// Renders only the message, so the HTTP layer keeps returning the same
/// actionable text it always has.
impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for ImportError {}

/// Every ceiling an import is held to, supplied by the caller rather than
/// hard-coded inside a decoder.
#[derive(Clone, Copy, Debug)]
pub struct ImportLimits {
    /// The configured project rate the decoded audio is resampled to.
    pub target_rate: u32,
    pub max_channels: u16,
    pub max_seconds: f64,
    pub max_source_rate: u32,
    pub max_input_bytes: u64,
    pub max_output_bytes: u64,
    /// Interleaved `f32` values that may be held at once. Checked before each
    /// growth, never after an allocation has already happened.
    pub max_decoded_samples: usize,
    /// Wall-clock ceiling for an external converter process.
    pub timeout: Duration,
}

impl ImportLimits {
    /// The standard limits for a project running at `target_rate`.
    pub fn for_rate(target_rate: u32) -> Self {
        Self {
            target_rate,
            max_channels: MAX_CHANNELS,
            max_seconds: MAX_SECONDS,
            max_source_rate: MAX_SOURCE_RATE,
            max_input_bytes: MAX_INPUT_BYTES,
            max_output_bytes: MAX_OUTPUT_BYTES,
            max_decoded_samples: MAX_DECODED_SAMPLES,
            timeout: CONVERTER_TIMEOUT,
        }
    }

    /// The message shown for channel-count and duration violations. Identical
    /// to the text sample upload has always returned for these two limits.
    pub fn limit_message(&self) -> String {
        format!(
            "Use audio with 1\u{2013}{} channels and at most {:.0} seconds",
            self.max_channels, self.max_seconds
        )
    }

    /// Frames at `rate` that would still fit inside [`Self::max_seconds`].
    pub fn max_frames_at(&self, rate: u32) -> u64 {
        (self.max_seconds * rate as f64).ceil() as u64
    }

    /// Rejects an input blob by size before anything reads it.
    pub fn check_input_len(&self, len: u64) -> Result<(), ImportError> {
        if len == 0 {
            return Err(ImportError::Rejected("Select an audio file".into()));
        }
        if len > self.max_input_bytes {
            return Err(ImportError::Rejected(format!(
                "Audio uploads are limited to {} MB",
                self.max_input_bytes / (1024 * 1024)
            )));
        }
        Ok(())
    }

    /// Validates a declared stream *before* any decoded buffer is allocated,
    /// returning the accepted rate and channel count.
    ///
    /// `frames` is the container's declared length when it has one; a container
    /// that does not declare its length is still bounded during decoding by
    /// [`Self::max_frames_at`] and [`Self::max_decoded_samples`].
    pub fn check_stream(
        &self,
        rate: Option<u32>,
        channels: Option<usize>,
        frames: Option<u64>,
    ) -> Result<(u32, u16), ImportError> {
        let rate = rate.filter(|rate| *rate > 0).ok_or_else(|| {
            ImportError::Invalid("The audio stream declares no sample rate".into())
        })?;
        if rate > self.max_source_rate {
            return Err(ImportError::Rejected(format!(
                "Audio sample rates above {} Hz are not imported",
                self.max_source_rate
            )));
        }
        let channels = channels.ok_or_else(|| {
            ImportError::Invalid("The audio stream declares no channel layout".into())
        })?;
        if channels == 0 || channels > self.max_channels as usize {
            return Err(ImportError::Rejected(self.limit_message()));
        }
        let channels = channels as u16;
        if let Some(frames) = frames {
            if frames > self.max_frames_at(rate) {
                return Err(ImportError::Rejected(self.limit_message()));
            }
            self.check_capacity(frames.saturating_mul(channels as u64))?;
        }
        Ok((rate, channels))
    }

    /// Rejects a buffer that would exceed the memory ceiling. Callers pass the
    /// size they are *about* to need, so no oversized allocation ever happens.
    pub fn check_capacity(&self, samples: u64) -> Result<(), ImportError> {
        if samples > self.max_decoded_samples as u64 {
            return Err(ImportError::Rejected(format!(
                "Decoded audio exceeds the {} MB import memory limit",
                self.max_decoded_samples * std::mem::size_of::<f32>() / (1024 * 1024)
            )));
        }
        Ok(())
    }
}

/// One already-received local input.
///
/// There is deliberately no URL, stream or directory variant: an importer can
/// only ever read the bytes the host handed it.
#[derive(Clone, Debug)]
pub enum ImportSource {
    /// Uploaded bytes still in memory.
    Bytes(Arc<Vec<u8>>),
    /// A local file the host already wrote. Nothing else is ever opened.
    File(PathBuf),
}

impl ImportSource {
    pub fn bytes(data: impl Into<Vec<u8>>) -> Self {
        Self::Bytes(Arc::new(data.into()))
    }

    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(path.into())
    }

    pub fn len(&self) -> Result<u64, ImportError> {
        match self {
            Self::Bytes(data) => Ok(data.len() as u64),
            Self::File(path) => std::fs::metadata(path)
                .map(|meta| meta.len())
                .map_err(|e| ImportError::Failed(format!("Read the uploaded audio: {e}"))),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len().is_ok_and(|len| len == 0)
    }

    /// A seekable reader over the input. Importers inspect the bytes; they
    /// never consult the upload's filename.
    pub fn reader(&self) -> Result<Box<dyn ReadSeek>, ImportError> {
        match self {
            Self::Bytes(data) => Ok(Box::new(std::io::Cursor::new(SharedBytes(data.clone())))),
            Self::File(path) => std::fs::File::open(path)
                .map(|file| Box::new(file) as Box<dyn ReadSeek>)
                .map_err(|e| ImportError::Failed(format!("Open the uploaded audio: {e}"))),
        }
    }

    /// The input as a file on disk, writing a temporary copy only when the
    /// input is in memory. Used by the external-converter importer, which must
    /// hand the child process one already-open file descriptor.
    ///
    /// Staging is not itself a process operation, so it stays compiled on the
    /// mobile targets that have no converter; only its one caller disappears
    /// there, and the tests below still cover it.
    #[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
    pub(crate) fn local_file(&self) -> Result<LocalFile, ImportError> {
        match self {
            Self::File(path) => Ok(LocalFile {
                path: path.clone(),
                _scratch: None,
            }),
            Self::Bytes(data) => {
                let scratch = Scratch::new("import")?;
                let path = scratch.0.join("input");
                std::fs::write(&path, data.as_slice()).map_err(|e| {
                    ImportError::Failed(format!("Stage the uploaded audio: {e}"))
                })?;
                Ok(LocalFile {
                    path,
                    _scratch: Some(scratch),
                })
            }
        }
    }
}

/// `Cursor<T>` needs `AsRef<[u8]>`; sharing the upload avoids a second copy.
#[derive(Clone, Debug)]
struct SharedBytes(Arc<Vec<u8>>);
impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

/// A private temporary directory removed when it goes out of scope, however the
/// import ends.
#[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
pub(crate) struct Scratch(pub PathBuf);
#[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
impl Scratch {
    pub(crate) fn new(label: &str) -> Result<Self, ImportError> {
        let path = std::env::temp_dir().join(format!("pr0-{label}-{}", crate::uid()));
        std::fs::create_dir(&path)
            .map_err(|e| ImportError::Failed(format!("Create the import directory: {e}")))?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An input materialized on disk. The temporary copy, when one was needed,
/// disappears with this value.
#[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
pub(crate) struct LocalFile {
    path: PathBuf,
    /// Held only so the temporary copy outlives the conversion.
    _scratch: Option<Scratch>,
}
#[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
impl LocalFile {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

/// Decoded audio that has already satisfied every output bound.
///
/// Constructing one is the only way to produce it, and the constructor is where
/// empty, nonfinite, over-wide and over-long output is refused — so an
/// `AudioImporter` implementation cannot return audio the rest of the runtime
/// would have to re-check.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudio {
    samples: Vec<f32>,
    channels: u16,
    sample_rate: u32,
}

impl DecodedAudio {
    /// Validates and takes ownership of interleaved `f32` samples.
    pub fn new(
        samples: Vec<f32>,
        channels: u16,
        sample_rate: u32,
        limits: &ImportLimits,
    ) -> Result<Self, ImportError> {
        if channels == 0 || channels > limits.max_channels {
            return Err(ImportError::Rejected(limits.limit_message()));
        }
        if sample_rate == 0 {
            return Err(ImportError::Invalid(
                "Decoded audio declares no sample rate".into(),
            ));
        }
        if samples.len() % channels as usize != 0 {
            return Err(ImportError::Invalid(
                "Decoded audio ended mid-frame".into(),
            ));
        }
        let frames = (samples.len() / channels as usize) as u64;
        if frames == 0 {
            return Err(ImportError::Rejected(
                "The file contains no audio to import".into(),
            ));
        }
        if frames > limits.max_frames_at(sample_rate) {
            return Err(ImportError::Rejected(limits.limit_message()));
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(ImportError::Rejected(
                "Decoded audio contains nonfinite samples".into(),
            ));
        }
        Ok(Self {
            samples,
            channels,
            sample_rate,
        })
    }

    /// Interleaved samples, `channels` values per frame.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / self.sample_rate as f64
    }

    /// Writes the canonical import product: 32-bit float WAV at this audio's
    /// rate and channel count, which is exactly what `sample_library::register`
    /// catalogues and what `samples::cache_asset` reads back.
    pub fn write_wav(&self, path: &Path) -> Result<(), ImportError> {
        let failed = |e: hound::Error| ImportError::Failed(format!("Write converted audio: {e}"));
        let mut writer = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels: self.channels,
                sample_rate: self.sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .map_err(failed)?;
        for sample in &self.samples {
            writer.write_sample(*sample).map_err(failed)?;
        }
        writer.finalize().map_err(failed)
    }
}

/// The host-owned import capability.
///
/// Object-safe on purpose: [`crate::RuntimeConfig`] carries one as
/// `Arc<dyn AudioImporter>`, so an embedded host can install a pure-Rust
/// decoder, a converter-backed one, a chain, or a test double without the
/// runtime knowing which it got.
pub trait AudioImporter: Send + Sync + fmt::Debug {
    /// Stable identifier used in diagnostics and tests.
    fn name(&self) -> &str;

    /// The container/codec families this importer offers, for documentation and
    /// diagnostics. Not a promise about any particular file.
    fn formats(&self) -> Vec<&'static str> {
        Vec::new()
    }

    /// Decodes `source` to interleaved `f32` at `limits.target_rate`.
    ///
    /// Synchronous and CPU-bound: callers run it on a blocking worker.
    fn decode(
        &self,
        source: &ImportSource,
        limits: &ImportLimits,
    ) -> Result<DecodedAudio, ImportError>;
}

/// Tries each importer in order.
///
/// It stops at the first success, and at the first limit or policy violation.
/// Only [`ImportError::Unsupported`] and [`ImportError::Unavailable`] fall
/// through to the next importer, so the fallback exists for formats the earlier
/// importer cannot read — never for files the earlier importer refused.
#[derive(Debug)]
pub struct ImporterChain {
    label: String,
    importers: Vec<Arc<dyn AudioImporter>>,
}

impl ImporterChain {
    pub fn new(importers: Vec<Arc<dyn AudioImporter>>) -> Self {
        let label = if importers.is_empty() {
            "none".to_string()
        } else {
            importers
                .iter()
                .map(|importer| importer.name())
                .collect::<Vec<_>>()
                .join("+")
        };
        Self { label, importers }
    }

    pub fn importers(&self) -> &[Arc<dyn AudioImporter>] {
        &self.importers
    }
}

impl AudioImporter for ImporterChain {
    fn name(&self) -> &str {
        &self.label
    }

    fn formats(&self) -> Vec<&'static str> {
        let mut formats: Vec<&'static str> = Vec::new();
        for importer in &self.importers {
            for format in importer.formats() {
                if !formats.contains(&format) {
                    formats.push(format);
                }
            }
        }
        formats
    }

    fn decode(
        &self,
        source: &ImportSource,
        limits: &ImportLimits,
    ) -> Result<DecodedAudio, ImportError> {
        let mut last: Option<ImportError> = None;
        for importer in &self.importers {
            match importer.decode(source, limits) {
                Ok(audio) => return Ok(audio),
                Err(error) if error.may_fall_back() => last = Some(error),
                Err(error) => return Err(error),
            }
        }
        Err(last.unwrap_or_else(|| ImportError::Unavailable(NO_IMPORTER.into())))
    }
}

/// The standard host chain: the pure-Rust decoder first, then the configured
/// external converter for the formats it does not cover.
///
/// Passing `None` produces a chain with no executable in it, which is what a
/// mobile or embedded host installs.
///
/// On a host that cannot spawn a process the converter is not merely skipped at
/// run time — it is not compiled — so this returns the pure-Rust chain whatever
/// `ffmpeg` says. The signature is deliberately unchanged so a caller that
/// names a converter still builds everywhere; the caller does not have to know
/// which platform it is on, and there is no configuration that can make a
/// mobile host reach for an executable.
pub fn standard(ffmpeg: Option<OsString>) -> Arc<dyn AudioImporter> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        // Nothing here can run a converter, so the name is discarded rather
        // than remembered and quietly ignored later.
        drop(ffmpeg);
        return pure_rust();
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let mut importers: Vec<Arc<dyn AudioImporter>> =
            vec![Arc::new(SymphoniaImporter::new())];
        if let Some(program) = ffmpeg {
            importers.push(Arc::new(FfmpegImporter::new(program)));
        }
        Arc::new(ImporterChain::new(importers))
    }
}

/// In-process decoding only: no child process, no executable, no sidecar.
pub fn pure_rust() -> Arc<dyn AudioImporter> {
    Arc::new(ImporterChain::new(vec![Arc::new(SymphoniaImporter::new())]))
}

/// No import capability at all. Upload reports the missing capability instead
/// of failing somewhere deeper.
pub fn unavailable() -> Arc<dyn AudioImporter> {
    Arc::new(ImporterChain::new(Vec::new()))
}
