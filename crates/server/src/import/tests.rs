//! Corpus-style import tests.
//!
//! The corpus is generated rather than checked in: every fixture is built byte
//! by byte here, so the expected PCM is known exactly and the tests do not
//! depend on any encoder's header choices, on FFmpeg, or on a binary blob
//! nobody can review. Nothing in this file runs a converter process, opens a
//! device, or touches the DSP engine.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// A generated WAV fixture's format.
#[derive(Clone, Copy, Debug)]
struct Spec {
    channels: u16,
    rate: u32,
    bits: u16,
    float: bool,
}

fn encode(spec: Spec, value: f32, out: &mut Vec<u8>) {
    if spec.float {
        out.extend_from_slice(&value.to_le_bytes());
        return;
    }
    let value = value as f64;
    match spec.bits {
        16 => out.extend_from_slice(
            &((value * 32768.).round().clamp(-32768., 32767.) as i16).to_le_bytes(),
        ),
        24 => out.extend_from_slice(
            &((value * 8388608.).round().clamp(-8388608., 8388607.) as i32).to_le_bytes()[..3],
        ),
        32 => out.extend_from_slice(
            &((value * 2147483648.)
                .round()
                .clamp(-2147483648., 2147483647.) as i32)
                .to_le_bytes(),
        ),
        other => panic!("unsupported fixture depth {other}"),
    }
}

/// Builds a canonical RIFF/WAVE file. Fixtures with more than two channels use
/// WAVE_FORMAT_EXTENSIBLE with an explicit channel mask, which is what real
/// multichannel files carry.
fn wav(spec: Spec, samples: &[f32]) -> Vec<u8> {
    let block_align = spec.channels * spec.bits / 8;
    let mut data = Vec::with_capacity(samples.len() * (spec.bits as usize / 8));
    for sample in samples {
        encode(spec, *sample, &mut data);
    }
    let extensible = spec.channels > 2;
    let fmt_size: u32 = if extensible { 40 } else { 16 };
    let tag: u16 = if extensible {
        0xFFFE
    } else if spec.float {
        3
    } else {
        1
    };
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(4 + 8 + fmt_size + 8 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&fmt_size.to_le_bytes());
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&spec.channels.to_le_bytes());
    out.extend_from_slice(&spec.rate.to_le_bytes());
    out.extend_from_slice(&(spec.rate * block_align as u32).to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&spec.bits.to_le_bytes());
    if extensible {
        out.extend_from_slice(&22u16.to_le_bytes());
        out.extend_from_slice(&spec.bits.to_le_bytes());
        let mask: u32 = if spec.channels >= 32 {
            u32::MAX
        } else {
            (1u32 << spec.channels) - 1
        };
        out.extend_from_slice(&mask.to_le_bytes());
        out.extend_from_slice(&(if spec.float { 3u32 } else { 1u32 }).to_le_bytes());
        // The fixed tail of KSDATAFORMAT_SUBTYPE_PCM / _IEEE_FLOAT.
        out.extend_from_slice(&[
            0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
        ]);
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// Interleaved tone with a slightly different level per channel, so a decoder
/// that reorders or duplicates channels cannot pass.
fn tone(channels: u16, rate: u32, seconds: f64, hertz: f64) -> Vec<f32> {
    let frames = (rate as f64 * seconds) as usize;
    let mut samples = Vec::with_capacity(frames * channels as usize);
    for frame in 0..frames {
        let phase = std::f64::consts::TAU * hertz * frame as f64 / rate as f64;
        for channel in 0..channels {
            samples.push((phase.sin() * (0.5 - channel as f64 * 0.05)) as f32);
        }
    }
    samples
}

/// The argument immediately after `flag`, for asserting converter flag pairs.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn following<'a>(arguments: &'a [String], flag: &str) -> Option<&'a str> {
    arguments
        .iter()
        .position(|argument| argument == flag)
        .and_then(|index| arguments.get(index + 1))
        .map(String::as_str)
}

fn worst_difference(actual: &[f32], expected: &[f32]) -> f32 {
    assert_eq!(actual.len(), expected.len(), "sample count");
    actual
        .iter()
        .zip(expected)
        .map(|(a, b)| (a - b).abs())
        .fold(0., f32::max)
}

#[test]
fn the_integer_and_float_wav_corpus_decodes_to_canonical_float() {
    let importer = SymphoniaImporter::new();
    let limits = ImportLimits::for_rate(48000);
    for channels in [1u16, 2, 8] {
        for (bits, float, tolerance) in [
            (16u16, false, 2e-4f32),
            (24, false, 2e-6),
            (32, false, 2e-6),
            (32, true, 1e-7),
        ] {
            let spec = Spec {
                channels,
                rate: 48000,
                bits,
                float,
            };
            let expected = tone(channels, 48000, 0.25, 440.);
            let label = format!("{channels} channels, {bits} bit, float={float}");
            let audio = importer
                .decode(&ImportSource::bytes(wav(spec, &expected)), &limits)
                .unwrap_or_else(|e| panic!("{label}: {e}"));
            assert_eq!(audio.channels(), channels, "{label}");
            assert_eq!(audio.sample_rate(), 48000, "{label}");
            assert_eq!(audio.frames(), expected.len() / channels as usize, "{label}");
            assert!(audio.samples().iter().all(|v| v.is_finite()), "{label}");
            let worst = worst_difference(audio.samples(), &expected);
            assert!(worst <= tolerance, "{label}: worst difference {worst}");
        }
    }
}

#[test]
fn float_wav_at_the_project_rate_is_not_resampled() {
    let expected = tone(2, 44100, 0.1, 220.);
    let spec = Spec {
        channels: 2,
        rate: 44100,
        bits: 32,
        float: true,
    };
    let audio = SymphoniaImporter::new()
        .decode(
            &ImportSource::bytes(wav(spec, &expected)),
            &ImportLimits::for_rate(44100),
        )
        .expect("decode float WAV");
    assert_eq!(audio.sample_rate(), 44100);
    assert_eq!(audio.samples(), expected.as_slice());
}

#[test]
fn resampling_uses_the_existing_offline_windowed_sinc_path() {
    for (source_rate, target_rate) in [(44100u32, 48000u32), (96000, 48000), (48000, 88200)] {
        let expected_pcm = tone(2, source_rate, 0.1, 330.);
        let spec = Spec {
            channels: 2,
            rate: source_rate,
            bits: 32,
            float: true,
        };
        let audio = SymphoniaImporter::new()
            .decode(
                &ImportSource::bytes(wav(spec, &expected_pcm)),
                &ImportLimits::for_rate(target_rate),
            )
            .unwrap_or_else(|e| panic!("{source_rate} -> {target_rate}: {e}"));
        let expected = crate::samples::resample(&expected_pcm, 2, source_rate, target_rate);
        assert_eq!(audio.sample_rate(), target_rate);
        assert_eq!(audio.channels(), 2);
        let worst = worst_difference(audio.samples(), &expected);
        assert!(worst <= 1e-6, "{source_rate} -> {target_rate}: {worst}");
    }
}

#[test]
fn stream_limits_are_checked_before_any_decoded_buffer_is_allocated() {
    let limits = ImportLimits::for_rate(48000);
    assert_eq!(
        limits.limit_message(),
        "Use audio with 1\u{2013}8 channels and at most 30 seconds"
    );
    // Channel count.
    assert!(limits.check_stream(Some(48000), Some(1), None).is_ok());
    assert!(limits.check_stream(Some(48000), Some(8), None).is_ok());
    for channels in [0usize, 9, 64] {
        assert_eq!(
            limits.check_stream(Some(48000), Some(channels), None),
            Err(ImportError::Rejected(limits.limit_message())),
            "{channels} channels"
        );
    }
    // Declared duration, one frame either side of the ceiling.
    assert!(
        limits
            .check_stream(Some(48000), Some(2), Some(48000 * 30))
            .is_ok()
    );
    assert_eq!(
        limits.check_stream(Some(48000), Some(2), Some(48000 * 30 + 1)),
        Err(ImportError::Rejected(limits.limit_message()))
    );
    // Declared memory footprint, before any allocation.
    assert!(
        limits
            .check_stream(Some(384000), Some(8), Some(384000 * 30))
            .unwrap_err()
            .is_rejection()
    );
    assert!(limits.check_capacity(limits.max_decoded_samples as u64).is_ok());
    assert!(
        limits
            .check_capacity(limits.max_decoded_samples as u64 + 1)
            .unwrap_err()
            .is_rejection()
    );
    // Source rate and missing declarations.
    assert!(
        limits
            .check_stream(Some(limits.max_source_rate + 1), Some(2), None)
            .unwrap_err()
            .is_rejection()
    );
    assert!(matches!(
        limits.check_stream(None, Some(2), None),
        Err(ImportError::Invalid(_))
    ));
    assert!(matches!(
        limits.check_stream(Some(48000), None, None),
        Err(ImportError::Invalid(_))
    ));
    // Input size.
    assert_eq!(
        limits.check_input_len(0),
        Err(ImportError::Rejected("Select an audio file".into()))
    );
    assert!(limits.check_input_len(limits.max_input_bytes).is_ok());
    assert!(
        limits
            .check_input_len(limits.max_input_bytes + 1)
            .unwrap_err()
            .is_rejection()
    );
}

#[test]
fn over_wide_and_over_long_audio_is_refused_without_decoding_it() {
    let importer = SymphoniaImporter::new();
    let limits = ImportLimits::for_rate(48000);

    let wide = wav(
        Spec {
            channels: 9,
            rate: 48000,
            bits: 16,
            float: false,
        },
        &tone(9, 48000, 0.05, 440.),
    );
    let error = importer
        .decode(&ImportSource::bytes(wide), &limits)
        .expect_err("nine channels must not import");
    assert!(
        matches!(
            error,
            ImportError::Rejected(_) | ImportError::Unsupported(_)
        ),
        "nine channels classified as {}: {error}",
        error.kind()
    );

    // 31 seconds at a low rate keeps the fixture small; the WAV data chunk
    // declares the length, so it is refused before a sample is decoded.
    let long = wav(
        Spec {
            channels: 1,
            rate: 8000,
            bits: 16,
            float: false,
        },
        &tone(1, 8000, 31., 200.),
    );
    let error = importer
        .decode(&ImportSource::bytes(long), &limits)
        .expect_err("31 seconds must not import");
    assert_eq!(error, ImportError::Rejected(limits.limit_message()));
    assert!(!error.may_fall_back(), "a limit violation must not fall back");

    // Exactly at the ceiling still imports.
    let exact = wav(
        Spec {
            channels: 1,
            rate: 8000,
            bits: 16,
            float: false,
        },
        &tone(1, 8000, 30., 200.),
    );
    let audio = importer
        .decode(&ImportSource::bytes(exact), &ImportLimits::for_rate(8000))
        .expect("30 seconds imports");
    assert_eq!(audio.frames(), 8000 * 30);
}

#[test]
fn invalid_input_is_classified_so_only_unknown_formats_fall_back() {
    let importer = SymphoniaImporter::new();
    let limits = ImportLimits::for_rate(48000);

    let empty = importer
        .decode(&ImportSource::bytes(Vec::<u8>::new()), &limits)
        .expect_err("an empty upload must not import");
    assert_eq!(
        empty,
        ImportError::Rejected("Select an audio file".into())
    );

    // Deliberately contains no 0xFF byte, so it cannot be mistaken for an MPEG
    // or ADTS sync word by any probe.
    let noise: Vec<u8> = (0..8192u32).map(|i| (i % 251) as u8).collect();
    let unknown = importer
        .decode(&ImportSource::bytes(noise), &limits)
        .expect_err("noise must not import");
    assert!(
        unknown.may_fall_back(),
        "an unidentifiable blob must reach the fallback converter, got {}",
        unknown.kind()
    );

    // A well-formed header with no audio in it is recognized and rejected; it
    // is not an unknown format, so it must not reach a second decoder.
    let headers_only = wav(
        Spec {
            channels: 2,
            rate: 48000,
            bits: 16,
            float: false,
        },
        &[],
    );
    let silent = importer
        .decode(&ImportSource::bytes(headers_only), &limits)
        .expect_err("a WAV with no samples must not import");
    assert!(
        matches!(
            silent,
            ImportError::Invalid(_) | ImportError::Unsupported(_)
        ),
        "an empty container classified as {}: {silent}",
        silent.kind()
    );

    let truncated = importer
        .decode(&ImportSource::bytes(b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec()), &limits);
    assert!(truncated.is_err(), "a truncated header must not import");
}

#[test]
fn decoded_audio_enforces_the_output_contract() {
    let limits = ImportLimits::for_rate(48000);
    assert!(DecodedAudio::new(vec![0.1, -0.1], 2, 48000, &limits).is_ok());
    assert_eq!(
        DecodedAudio::new(Vec::new(), 1, 48000, &limits),
        Err(ImportError::Rejected(
            "The file contains no audio to import".into()
        ))
    );
    assert_eq!(
        DecodedAudio::new(vec![0.1, f32::NAN], 1, 48000, &limits),
        Err(ImportError::Rejected(
            "Decoded audio contains nonfinite samples".into()
        ))
    );
    assert_eq!(
        DecodedAudio::new(vec![f32::INFINITY], 1, 48000, &limits),
        Err(ImportError::Rejected(
            "Decoded audio contains nonfinite samples".into()
        ))
    );
    assert_eq!(
        DecodedAudio::new(vec![0.; 9], 9, 48000, &limits),
        Err(ImportError::Rejected(limits.limit_message()))
    );
    assert_eq!(
        DecodedAudio::new(vec![0.; 48000 * 30 + 1], 1, 48000, &limits),
        Err(ImportError::Rejected(limits.limit_message()))
    );
    assert!(matches!(
        DecodedAudio::new(vec![0.; 5], 2, 48000, &limits),
        Err(ImportError::Invalid(_))
    ));
    let audio = DecodedAudio::new(vec![0.25; 96000], 2, 48000, &limits).unwrap();
    assert_eq!(audio.frames(), 48000);
    assert!((audio.seconds() - 1.).abs() < 1e-9);
}

#[test]
fn the_canonical_output_is_the_float_wav_the_sample_library_reads() {
    let scratch = Scratch::new("import-canonical").unwrap();
    let path = scratch.0.join("converted.wav");
    let expected = tone(2, 44100, 0.2, 440.);
    let bytes = wav(
        Spec {
            channels: 2,
            rate: 44100,
            bits: 24,
            float: false,
        },
        &expected,
    );
    let audio = SymphoniaImporter::new()
        .decode(&ImportSource::bytes(bytes), &ImportLimits::for_rate(48000))
        .expect("decode");
    audio.write_wav(&path).expect("write canonical WAV");

    // Read it back exactly as `sample_library::register` and
    // `samples::cache_asset` do.
    let reader = hound::WavReader::open(&path).expect("reopen canonical WAV");
    let specification = reader.spec();
    assert_eq!(specification.channels, 2);
    assert_eq!(specification.sample_rate, 48000);
    assert_eq!(specification.bits_per_sample, 32);
    assert_eq!(specification.sample_format, hound::SampleFormat::Float);
    assert_eq!(reader.duration() as usize, audio.frames());
    let samples: Vec<f32> = reader
        .into_samples::<f32>()
        .collect::<Result<_, _>>()
        .expect("read canonical samples");
    assert_eq!(samples.as_slice(), audio.samples());
    assert!(samples.iter().all(|v| v.is_finite()));
}

/// A stub importer that records whether the chain reached it.
#[derive(Debug)]
struct Stub {
    label: String,
    result: Result<DecodedAudio, ImportError>,
    calls: Arc<AtomicUsize>,
}

impl AudioImporter for Stub {
    fn name(&self) -> &str {
        &self.label
    }

    fn formats(&self) -> Vec<&'static str> {
        vec!["Stub"]
    }

    fn decode(
        &self,
        _source: &ImportSource,
        _limits: &ImportLimits,
    ) -> Result<DecodedAudio, ImportError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.clone()
    }
}

#[test]
fn a_chain_falls_back_only_for_unknown_formats() {
    let limits = ImportLimits::for_rate(48000);
    let source = ImportSource::bytes(vec![0u8; 4]);
    let audio = DecodedAudio::new(vec![0.25, -0.25], 1, 48000, &limits).unwrap();

    let stub = |label: &str,
                result: Result<DecodedAudio, ImportError>|
     -> (Arc<dyn AudioImporter>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Arc::new(Stub {
                label: label.into(),
                result,
                calls: calls.clone(),
            }),
            calls,
        )
    };

    // An unknown format reaches the next importer.
    let (first, first_calls) = stub("first", Err(ImportError::Unsupported("unknown".into())));
    let (second, second_calls) = stub("second", Ok(audio.clone()));
    let chain = ImporterChain::new(vec![first, second]);
    assert_eq!(chain.name(), "first+second");
    assert_eq!(chain.decode(&source, &limits), Ok(audio.clone()));
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_calls.load(Ordering::SeqCst), 1);

    // A limit, policy or converter failure stops the chain where it happened.
    for refusal in [
        ImportError::Rejected("too long".into()),
        ImportError::Invalid("corrupt".into()),
        ImportError::Failed("no converter process".into()),
    ] {
        let (first, _) = stub("first", Err(refusal.clone()));
        let (second, second_calls) = stub("second", Ok(audio.clone()));
        let chain = ImporterChain::new(vec![first, second]);
        assert_eq!(chain.decode(&source, &limits), Err(refusal.clone()));
        assert_eq!(
            second_calls.load(Ordering::SeqCst),
            0,
            "{} must not fall back",
            refusal.kind()
        );
    }

    // The first success wins and nothing after it runs.
    let (first, _) = stub("first", Ok(audio.clone()));
    let (second, second_calls) = stub("second", Err(ImportError::Failed("unreachable".into())));
    let chain = ImporterChain::new(vec![first, second]);
    assert_eq!(chain.decode(&source, &limits), Ok(audio.clone()));
    assert_eq!(second_calls.load(Ordering::SeqCst), 0);

    // Exhausting the chain reports the last importer's reason.
    let (first, _) = stub("first", Err(ImportError::Unsupported("one".into())));
    let (second, _) = stub("second", Err(ImportError::Unsupported("two".into())));
    let chain = ImporterChain::new(vec![first, second]);
    assert_eq!(
        chain.decode(&source, &limits),
        Err(ImportError::Unsupported("two".into()))
    );
    assert_eq!(chain.formats(), vec!["Stub"]);
}

#[test]
fn a_host_without_any_importer_reports_a_missing_capability() {
    let limits = ImportLimits::for_rate(48000);
    let source = ImportSource::bytes(
        wav(
            Spec {
                channels: 1,
                rate: 48000,
                bits: 16,
                float: false,
            },
            &tone(1, 48000, 0.05, 440.),
        ),
    );
    let none = unavailable();
    assert_eq!(none.name(), "none");
    assert!(none.formats().is_empty());
    assert_eq!(
        none.decode(&source, &limits),
        Err(ImportError::Unavailable(NO_IMPORTER.into()))
    );
    // Exactly the message sample upload has always returned for this host.
    assert_eq!(
        NO_IMPORTER,
        "This host has no audio converter configured for sample import"
    );
}

#[test]
fn host_chains_choose_pure_rust_first_and_mobile_needs_no_executable() {
    let pure = pure_rust();
    assert_eq!(pure.name(), "symphonia");
    assert!(pure.formats().iter().any(|format| format.contains("FLAC")));

    let mobile = standard(None);
    assert_eq!(mobile.name(), "symphonia");

    let desktop = standard(Some("ffmpeg".into()));
    // On a host that cannot spawn a process the converter is not compiled at
    // all, so naming one cannot add it to the chain.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        assert_eq!(desktop.name(), "symphonia+ffmpeg");
        assert!(
            desktop
                .formats()
                .iter()
                .any(|format| format.contains("FFmpeg"))
        );
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        assert_eq!(desktop.name(), "symphonia");
        assert!(
            !desktop
                .formats()
                .iter()
                .any(|format| format.contains("FFmpeg"))
        );
    }

    // Both hosts decode the portable corpus identically.
    let limits = ImportLimits::for_rate(48000);
    let expected = tone(2, 48000, 0.05, 440.);
    let source = ImportSource::bytes(wav(
        Spec {
            channels: 2,
            rate: 48000,
            bits: 32,
            float: true,
        },
        &expected,
    ));
    assert_eq!(
        mobile.decode(&source, &limits).map(|a| a.samples().to_vec()),
        desktop.decode(&source, &limits).map(|a| a.samples().to_vec())
    );
}

/// The external converter does not exist on hosts without process spawning, so
/// neither do the two tests that pin its behavior.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[test]
fn the_converter_process_keeps_its_hardened_argument_contract() {
    let limits = ImportLimits::for_rate(44100);
    let importer = FfmpegImporter::new("ffmpeg");
    assert_eq!(importer.program(), std::ffi::OsStr::new("ffmpeg"));
    let output = std::path::PathBuf::from("/tmp/pr0-convert/converted.wav");
    let arguments: Vec<String> = importer
        .arguments(&output, &limits)
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    // Only a seekable file descriptor: no nested local file or network URL an
    // uploaded playlist could name.
    assert_eq!(following(&arguments, "-protocol_whitelist"), Some("fd"));
    assert_eq!(following(&arguments, "-i"), Some("fd:"));
    // The first audio stream only, with no video, subtitles, data or tags.
    assert_eq!(following(&arguments, "-map"), Some("0:a:0"));
    assert_eq!(following(&arguments, "-map_metadata"), Some("-1"));
    for flag in ["-vn", "-sn", "-dn", "-nostdin", "-hide_banner"] {
        assert!(arguments.iter().any(|a| a == flag), "missing {flag}");
    }
    assert_eq!(following(&arguments, "-threads"), Some("1"));
    assert_eq!(following(&arguments, "-loglevel"), Some("error"));
    // Duration, rate, codec and written-byte ceilings.
    assert_eq!(following(&arguments, "-t"), Some("31"));
    assert_eq!(following(&arguments, "-ar"), Some("44100"));
    assert_eq!(following(&arguments, "-c:a"), Some("pcm_f32le"));
    let ceiling = MAX_OUTPUT_BYTES.to_string();
    assert_eq!(following(&arguments, "-fs"), Some(ceiling.as_str()));
    assert_eq!(following(&arguments, "-f"), Some("wav"));
    let expected_output = output.to_string_lossy().into_owned();
    assert_eq!(arguments.last(), Some(&expected_output));
    assert!(
        !arguments.iter().any(|a| a.contains("http") || a.contains("file,")),
        "no extra protocol may be whitelisted"
    );
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[test]
fn a_missing_converter_is_reported_and_never_treated_as_an_unknown_format() {
    let limits = ImportLimits::for_rate(48000);
    let source = ImportSource::bytes(wav(
        Spec {
            channels: 1,
            rate: 48000,
            bits: 16,
            float: false,
        },
        &tone(1, 48000, 0.02, 440.),
    ));
    let error = FfmpegImporter::new("pr0former-no-such-audio-converter")
        .decode(&source, &limits)
        .expect_err("a converter that is not installed cannot import");
    assert!(
        matches!(error, ImportError::Failed(_)),
        "classified as {}: {error}",
        error.kind()
    );
    assert!(error.to_string().contains("FFmpeg is required for audio import"));
    assert!(!error.may_fall_back());
}

#[test]
fn an_import_source_only_ever_reads_the_bytes_it_was_handed() {
    let scratch = Scratch::new("import-source").unwrap();
    let path = scratch.0.join("input");
    std::fs::write(&path, b"0123456789").unwrap();
    let file = ImportSource::file(&path);
    assert_eq!(file.len().unwrap(), 10);
    assert!(!file.is_empty());
    let mut read = Vec::new();
    let mut reader = file.reader().unwrap();
    std::io::Read::read_to_end(&mut reader, &mut read).unwrap();
    assert_eq!(read, b"0123456789".to_vec());

    let bytes = ImportSource::bytes(b"abc".to_vec());
    assert_eq!(bytes.len().unwrap(), 3);
    assert!(ImportSource::bytes(Vec::<u8>::new()).is_empty());
    // An in-memory source materializes only inside its own temporary directory.
    let local = bytes.local_file().unwrap();
    assert!(local.path().exists());
    let staged = local.path().to_path_buf();
    assert_eq!(std::fs::read(&staged).unwrap(), b"abc".to_vec());
    drop(local);
    assert!(!staged.exists(), "the staged copy must not survive the import");

    // A file source is used in place, and is not removed by the importer.
    let local = file.local_file().unwrap();
    assert_eq!(local.path(), path.as_path());
    drop(local);
    assert!(path.exists());
}
