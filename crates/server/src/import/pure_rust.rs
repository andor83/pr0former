//! The in-process, pure-Rust [`AudioImporter`].
//!
//! This is the cross-platform baseline: it needs no executable, no sidecar and
//! no system media library, so an iPadOS or otherwise sandboxed host can import
//! audio with the same code the desktop and standalone server run.
//!
//! It is a thin, heavily bounded wrapper over Symphonia:
//!
//! - the container is identified from the bytes, never from the upload's
//!   filename, and the reader only ever sees the one blob it was given;
//! - the first audio track is selected and every other track — video,
//!   subtitles, timed metadata — is ignored;
//! - sample rate, channel count and declared length are checked before a
//!   decoded buffer is allocated, and the running frame count and memory
//!   footprint are checked again before each buffer grows;
//! - interleaved samples are converted to `f32` and resampled to the project
//!   rate with the existing offline windowed-sinc path in [`crate::samples`];
//!   and
//! - the result must still satisfy every [`DecodedAudio`] bound, so empty,
//!   nonfinite, over-wide or over-long output is refused.
//!
//! Formats Symphonia cannot read are reported as [`ImportError::Unsupported`],
//! which is the only classification a host chain falls through on.
use super::{
    AudioImporter, DecodedAudio, ImportError, ImportLimits, ImportSource, ReadSeek,
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_NULL, DecoderOptions},
    errors::Error as Symphonia,
    formats::FormatOptions,
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
};

/// The container/codec families the pinned Symphonia feature set covers. This
/// is the portable list every host — including mobile — can rely on.
const FORMATS: &[&str] = &[
    "WAV (PCM, IEEE float, ADPCM)",
    "AIFF/AIFF-C",
    "CAF",
    "FLAC",
    "MP3",
    "MP4/M4A (AAC-LC, ALAC)",
    "ADTS AAC",
    "Ogg (Vorbis, FLAC)",
];

/// Decode errors tolerated before the file is called corrupt. Lossy streams
/// routinely carry a damaged frame or two; an endless run of them is not audio.
const MAX_DECODE_ERRORS: u32 = 64;

#[derive(Clone, Copy, Debug, Default)]
pub struct SymphoniaImporter;

impl SymphoniaImporter {
    pub fn new() -> Self {
        Self
    }
}

impl AudioImporter for SymphoniaImporter {
    fn name(&self) -> &str {
        "symphonia"
    }

    fn formats(&self) -> Vec<&'static str> {
        FORMATS.to_vec()
    }

    fn decode(
        &self,
        source: &ImportSource,
        limits: &ImportLimits,
    ) -> Result<DecodedAudio, ImportError> {
        limits.check_input_len(source.len()?)?;
        let stream = MediaSourceStream::new(
            Box::new(Blob {
                len: source.len()?,
                inner: source.reader()?,
            }),
            MediaSourceStreamOptions::default(),
        );
        // No extension hint: the container is identified from the bytes.
        let probed = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                stream,
                &FormatOptions {
                    enable_gapless: true,
                    ..FormatOptions::default()
                },
                &MetadataOptions::default(),
            )
            .map_err(|e| classify(e, "identifying the file"))?;
        let mut reader = probed.format;

        // The first audio track wins; video, subtitle and data tracks are not
        // audio tracks and never appear here with a real codec.
        let track = reader
            .tracks()
            .iter()
            .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| ImportError::Invalid("The file contains no audio track".into()))?;
        let track_id = track.id;
        let parameters = track.codec_params.clone();

        // Every bound below is checked before the first decoded buffer exists.
        let (source_rate, channels) = limits.check_stream(
            parameters.sample_rate,
            parameters.channels.map(|channels| channels.count()),
            parameters.n_frames,
        )?;
        let lanes = channels as usize;
        let max_frames = limits.max_frames_at(source_rate);

        let mut decoder = symphonia::default::get_codecs()
            .make(&parameters, &DecoderOptions::default())
            .map_err(|e| classify(e, "opening the audio codec"))?;

        let mut samples: Vec<f32> = Vec::new();
        let mut interleaved: Option<(SampleBuffer<f32>, usize)> = None;
        let mut frames: u64 = 0;
        let mut decode_errors = 0;
        loop {
            let packet = match reader.next_packet() {
                Ok(packet) => packet,
                // Symphonia signals the end of a stream as an unexpected EOF.
                Err(Symphonia::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                // The track layout changed. Stop rather than splice two streams.
                Err(Symphonia::ResetRequired) => break,
                Err(e) => return Err(classify(e, "reading the audio stream")),
            };
            if packet.track_id() != track_id {
                continue;
            }
            let decoded = match decoder.decode(&packet) {
                Ok(decoded) => decoded,
                // Recoverable per Symphonia's contract, but a file that is
                // nothing but damaged frames is corrupt.
                Err(Symphonia::DecodeError(_)) => {
                    decode_errors += 1;
                    if decode_errors > MAX_DECODE_ERRORS {
                        return Err(ImportError::Invalid(
                            "The audio file is too damaged to import".into(),
                        ));
                    }
                    continue;
                }
                Err(Symphonia::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => return Err(classify(e, "decoding the audio stream")),
            };
            let specification = *decoded.spec();
            let produced = decoded.frames();
            let capacity = decoded.capacity();
            if produced == 0 {
                continue;
            }
            if specification.rate != source_rate || specification.channels.count() != lanes {
                return Err(ImportError::Rejected(
                    "The audio stream changes sample rate or channel count mid-file".into(),
                ));
            }
            // Both ceilings are applied to what this packet *would* add.
            frames += produced as u64;
            if frames > max_frames {
                return Err(ImportError::Rejected(limits.limit_message()));
            }
            limits.check_capacity(frames.saturating_mul(channels as u64))?;

            let reusable = interleaved
                .as_ref()
                .is_some_and(|(_, allocated)| *allocated >= capacity);
            if !reusable {
                interleaved = Some((
                    SampleBuffer::<f32>::new(capacity as u64, specification),
                    capacity,
                ));
            }
            let (buffer, _) = interleaved.as_mut().expect("interleaved buffer");
            buffer.copy_interleaved_ref(decoded);
            samples.extend_from_slice(buffer.samples());
        }

        if samples.is_empty() {
            return Err(ImportError::Invalid(
                "The file contains no decodable audio".into(),
            ));
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(ImportError::Rejected(
                "Decoded audio contains nonfinite samples".into(),
            ));
        }

        let decoded_frames = samples.len() / lanes;
        if source_rate == limits.target_rate {
            return DecodedAudio::new(samples, channels, limits.target_rate, limits);
        }
        let target_frames = (decoded_frames as f64 * limits.target_rate as f64
            / source_rate as f64)
            .round() as u64;
        limits.check_capacity(target_frames.saturating_mul(channels as u64))?;
        // The same offline windowed-sinc conversion the rate caches use. Never
        // called from render or device callbacks.
        let converted = crate::samples::resample(&samples, lanes, source_rate, limits.target_rate);
        drop(samples);
        DecodedAudio::new(converted, channels, limits.target_rate, limits)
    }
}

/// The already-received upload, presented to Symphonia as a seekable source of
/// known length. There is no way for it to reach any other byte.
struct Blob {
    inner: Box<dyn ReadSeek>,
    len: u64,
}

impl std::io::Read for Blob {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl std::io::Seek for Blob {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

impl MediaSource for Blob {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

/// Maps a Symphonia failure onto the runtime's fallback policy.
///
/// Only "this decoder does not know the format" becomes
/// [`ImportError::Unsupported`], the one classification a chain may fall
/// through on. Corrupt data, decoder limits and I/O failures stop the chain.
fn classify(error: Symphonia, stage: &str) -> ImportError {
    match error {
        Symphonia::Unsupported(what) => ImportError::Unsupported(format!(
            "The built-in decoder does not support this audio format ({stage}: {what}). Supported formats: {}",
            FORMATS.join(", ")
        )),
        Symphonia::DecodeError(what) => {
            ImportError::Invalid(format!("The audio file is corrupt ({stage}: {what})"))
        }
        Symphonia::LimitError(what) => ImportError::Rejected(format!(
            "The audio file exceeds a decoder limit ({stage}: {what})"
        )),
        Symphonia::IoError(e) => ImportError::Invalid(format!(
            "The audio file could not be read while {stage}: {e}"
        )),
        Symphonia::SeekError(_) => {
            ImportError::Invalid(format!("The audio file is not seekable while {stage}"))
        }
        Symphonia::ResetRequired => ImportError::Invalid(format!(
            "The audio stream changes format mid-file while {stage}"
        )),
    }
}
