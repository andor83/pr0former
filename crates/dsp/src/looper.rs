use crate::Clock;

#[derive(Clone, Copy, Default)]
pub struct Track {
    pub length: usize,
    dirty: bool,
    snapshot_offset: usize,
    cursor: usize,
    pub recording: bool,
    pub playing: bool,
    pub full: bool,
    end_beat: Option<f64>,
    pub record_at: Option<(f64, f64)>,
    pub play_at: Option<f64>,
}
pub struct Looper {
    audio: Vec<f32>,
    capacity: usize,
    channels: usize,
    pub tracks: [Track; 8],
    previous: [u8; 5],
}
impl Looper {
    pub fn prepare(rate: f64, channels: usize, seconds: f64) -> Result<Self, String> {
        let capacity = (rate * seconds).ceil() as usize;
        let samples = capacity
            .checked_mul(channels)
            .and_then(|n| n.checked_mul(8))
            .ok_or("Looper capacity overflow")?;
        let mut audio = Vec::new();
        audio
            .try_reserve_exact(samples)
            .map_err(|_| "Unable to reserve looper recording memory")?;
        audio.resize(samples, 0.);
        Ok(Self {
            audio,
            capacity,
            channels,
            tracks: [Track::default(); 8],
            previous: [0; 5],
        })
    }
    pub fn compatible(&self, other: &Self) -> bool {
        self.capacity == other.capacity && self.channels == other.channels
    }
    pub fn clear(&mut self, track: usize) {
        self.tracks[track] = Track {
            dirty: true,
            ..Track::default()
        };
    }
    pub fn finish(&mut self) {
        for track in &mut self.tracks {
            track.dirty |= track.recording;
            track.recording = false;
            track.record_at = None;
            track.end_beat = None;
        }
    }
    /// Copy at most `limit` samples between blocks; assembly and file I/O run elsewhere.
    pub fn snapshot_chunk(&mut self, limit: usize) -> Option<(u8, usize, usize, usize, Vec<f32>)> {
        let index = self.tracks.iter().position(|t| t.dirty)?;
        let track = &mut self.tracks[index];
        let total = if track.recording {
            0
        } else {
            track.length * self.channels
        };
        let offset = track.snapshot_offset.min(total);
        let end = (offset + limit.max(1)).min(total);
        let base = index * self.capacity * self.channels;
        let audio = self.audio[base + offset..base + end].to_vec();
        track.snapshot_offset = end;
        if end == total {
            track.dirty = false;
            track.snapshot_offset = 0;
        }
        Some((index as u8 + 1, self.channels, offset, total, audio))
    }
    // Called only by preparation/orchestration, never by render or device callbacks.
    pub fn snapshot(&mut self) -> Option<(u8, usize, Vec<f32>)> {
        let index = self.tracks.iter().position(|t| t.dirty)?;
        let track = &mut self.tracks[index];
        let base = index * self.capacity * self.channels;
        let length = if track.recording {
            0
        } else {
            track.length * self.channels
        };
        let audio = self.audio[base..base + length].to_vec();
        track.dirty = false;
        track.snapshot_offset = 0;
        Some((index as u8 + 1, self.channels, audio))
    }
    pub fn restore(
        &mut self,
        track: usize,
        audio: &[f32],
        channels: usize,
        source_rate: u32,
        rate: f64,
    ) -> Result<(), String> {
        if channels == 0 || source_rate == 0 || audio.len() % channels != 0 {
            return Err("Invalid saved loop".into());
        }
        let frames =
            (audio.len() as f64 / channels as f64 * rate / source_rate as f64).round() as usize;
        if frames > self.capacity {
            return Err(
                "Saved loop exceeds capacity; increase capacity or clear the track first".into(),
            );
        }
        let base = track * self.capacity * self.channels;
        for frame in 0..frames {
            let position = frame as f64 * source_rate as f64 / rate;
            let a = (position as usize).min(audio.len() / channels - 1);
            let b = (a + 1).min(audio.len() / channels - 1);
            for ch in 0..self.channels {
                let value = if ch < channels {
                    let x = audio[a * channels + ch];
                    x + (audio[b * channels + ch] - x) * position.fract() as f32
                } else {
                    0.
                };
                self.audio[base + frame * self.channels + ch] = value;
            }
        }
        self.tracks[track] = Track {
            length: frames,
            ..Track::default()
        };
        Ok(())
    }
    fn track(value: f64) -> u8 {
        if value.is_finite() && value.fract() == 0. && (1. ..=8.).contains(&value) {
            value as u8
        } else {
            0
        }
    }
    pub fn tick(
        &mut self,
        input: [f64; 8],
        commands: [f64; 5],
        mode: f64,
        clock: &Clock,
    ) -> [f64; 8] {
        let mode = mode.round().clamp(0., 128.);
        let start = if mode == 0. {
            clock.beat
        } else {
            ((clock.beat / clock.bar_beats + 1e-10).floor() + 1.) * clock.bar_beats
        };
        // Inputs are numeric signals: a changed nonzero track number is a command;
        // returning to zero rearms the same track number. Stops win within a sample.
        for command in [0, 2, 1, 3, 4] {
            let number = Self::track(commands[command]);
            if number > 0 && number != self.previous[command] {
                let track = &mut self.tracks[number as usize - 1];
                match command {
                    0 => track.record_at = Some((start, mode * clock.beat_length)),
                    1 => {
                        track.record_at = None;
                        track.dirty |= track.recording;
                        track.recording = false;
                        track.end_beat = None;
                    }
                    2 => track.play_at = Some(start),
                    3 => {
                        track.play_at = None;
                        track.playing = false;
                    }
                    _ => {
                        *track = Track {
                            dirty: true,
                            ..Track::default()
                        }
                    }
                }
            }
            self.previous[command] = number;
        }
        let mut output = [0.; 8];
        for (index, track) in self.tracks.iter_mut().enumerate() {
            if track.recording && track.end_beat.is_some_and(|end| clock.beat + 1e-9 >= end) {
                track.dirty = true;
                track.recording = false;
                track.end_beat = None;
            }
            if let Some((at, length)) = track.record_at {
                if clock.beat + 1e-9 >= at {
                    track.record_at = None;
                    track.dirty = true;
                    track.length = 0;
                    track.snapshot_offset = 0;
                    track.cursor = 0;
                    track.recording = true;
                    track.playing = false;
                    track.full = false;
                    track.end_beat = (length > 0.).then_some(at + length);
                }
            }
            if track.play_at.is_some_and(|at| clock.beat + 1e-9 >= at) {
                track.play_at = None;
                if track.length > 0 {
                    track.dirty |= track.recording;
                    track.recording = false;
                    track.end_beat = None;
                    track.playing = true;
                    track.cursor = 0;
                }
            }
            let base = index * self.capacity * self.channels;
            if track.recording {
                let offset = base + track.length * self.channels;
                for ch in 0..self.channels {
                    self.audio[offset + ch] = if input[ch].is_finite() {
                        input[ch] as f32
                    } else {
                        0.
                    };
                }
                track.length += 1;
                if track.length == self.capacity {
                    track.dirty |= track.recording;
                    track.recording = false;
                    track.end_beat = None;
                    track.full = true;
                }
            }
            if track.playing && track.length > 0 {
                let offset = base + track.cursor * self.channels;
                for ch in 0..self.channels {
                    output[ch] += self.audio[offset + ch] as f64;
                }
                track.cursor = (track.cursor + 1) % track.length;
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clock() -> Clock {
        let mut c = Clock::new(100.);
        c.running = true;
        c.bpm = 60.;
        c
    }
    fn tick(
        l: &mut Looper,
        c: &mut Clock,
        input: [f64; 8],
        command: [f64; 4],
        mode: f64,
    ) -> [f64; 8] {
        let out = l.tick(
            input,
            [command[0], command[1], command[2], command[3], 0.],
            mode,
            c,
        );
        c.advance();
        out
    }
    #[test]
    fn clear_input_wins_and_snapshots_replace_and_finish_recordings() {
        let mut l = Looper::prepare(100., 1, 1.).unwrap();
        let c = clock();
        l.tick([0.5; 8], [1., 0., 0., 0., 0.], 0., &c);
        assert!(l.snapshot().unwrap().2.is_empty()); // remove prior disk recording
        l.tick([0.25; 8], [0.; 5], 0., &c);
        l.finish();
        assert_eq!(l.snapshot().unwrap().2, [0.5, 0.25]);
        assert!(l.snapshot().is_none());
        l.tick([0.; 8], [0., 0., 1., 0., 1.], 0., &c);
        assert_eq!(l.tracks[0].length, 0);
        assert!(!l.tracks[0].playing);
        assert!(l.snapshot().unwrap().2.is_empty());
        l.tick([0.; 8], [0.; 5], 0., &c);
        l.tick([1.; 8], [8., 0., 8., 0., 8.], 4., &c);
        assert!(l.tracks[7].record_at.is_none() && l.tracks[7].play_at.is_none());
        assert_eq!(l.snapshot().unwrap().0, 8);
    }
    #[test]
    fn freeform_tracks_capture_independent_channels_and_mix_on_playback() {
        let mut l = Looper::prepare(100., 2, 2.).unwrap();
        let mut c = clock();
        for track in 1..=8 {
            let input = [
                track as f64 / 10.,
                -(track as f64) / 10.,
                0.,
                0.,
                0.,
                0.,
                0.,
                0.,
            ];
            tick(&mut l, &mut c, input, [track as f64, 0., 0., 0.], 0.);
            tick(&mut l, &mut c, input, [0.; 4], 0.);
            tick(&mut l, &mut c, [0.; 8], [0., track as f64, 0., 0.], 0.);
            assert_eq!(l.tracks[track - 1].length, 2);
            let out = tick(&mut l, &mut c, [0.; 8], [0., 0., track as f64, 0.], 0.);
            assert!((out[0] - track as f64 / 10.).abs() < 1e-6);
            assert_eq!(out[1], -out[0]);
            tick(&mut l, &mut c, [0.; 8], [0., 0., 0., track as f64], 0.);
        }
        let mut out = [0.; 8];
        for track in 1..=8 {
            out = tick(&mut l, &mut c, [0.; 8], [0., 0., track as f64, 0.], 0.);
        }
        assert!((out[0] - 3.6).abs() < 1e-6);
        assert_eq!(out[1], -out[0]);
        assert_eq!(out[2..], [0.; 6]);
    }
    #[test]
    fn beat_mode_waits_for_next_bar_and_records_exact_meter_beats() {
        let mut l = Looper::prepare(100., 1, 10.).unwrap();
        let mut c = clock();
        c.bar_beats = 1.5;
        c.beat_length = 0.5;
        tick(&mut l, &mut c, [1.; 8], [1., 0., 0., 0.], 2.);
        for _ in 1..150 {
            tick(&mut l, &mut c, [1.; 8], [0.; 4], 2.);
        }
        assert_eq!(l.tracks[0].length, 0);
        assert!(l.tracks[0].record_at.is_some());
        tick(&mut l, &mut c, [1.; 8], [0.; 4], 2.);
        assert_eq!(l.tracks[0].length, 1);
        for _ in 151..251 {
            tick(&mut l, &mut c, [1.; 8], [0.; 4], 2.);
        }
        assert_eq!(l.tracks[0].length, 100);
        assert!(!l.tracks[0].recording);
        assert_eq!(tick(&mut l, &mut c, [0.; 8], [0., 0., 1., 0.], 2.), [0.; 8]);
        while c.sample < 300 {
            assert_eq!(tick(&mut l, &mut c, [0.; 8], [0.; 4], 2.), [0.; 8]);
        }
        assert_eq!(tick(&mut l, &mut c, [0.; 8], [0.; 4], 2.)[0], 1.);
    }
    #[test]
    fn tempo_changes_preserve_queued_bar_and_recording_beat_count() {
        let mut l = Looper::prepare(100., 1, 10.).unwrap();
        let mut c = clock();
        tick(&mut l, &mut c, [1.; 8], [1., 0., 0., 0.], 2.);
        while c.sample < 200 {
            tick(&mut l, &mut c, [1.; 8], [0.; 4], 2.);
        }
        c.bpm = 120.;
        while c.sample < 300 {
            tick(&mut l, &mut c, [1.; 8], [0.; 4], 2.);
        }
        assert_eq!(l.tracks[0].length, 0);
        tick(&mut l, &mut c, [1.; 8], [0.; 4], 2.);
        assert!(l.tracks[0].recording);
        while c.sample < 401 {
            tick(&mut l, &mut c, [1.; 8], [0.; 4], 2.);
        }
        assert_eq!(l.tracks[0].length, 100);
        assert!(!l.tracks[0].recording);
    }
    #[test]
    fn stops_cancel_pending_actions_and_capacity_stops_freeform_safely() {
        let mut l = Looper::prepare(100., 1, 1.).unwrap();
        let mut c = clock();
        tick(&mut l, &mut c, [1.; 8], [1., 0., 1., 0.], 4.);
        tick(&mut l, &mut c, [1.; 8], [0., 1., 0., 1.], 4.);
        assert!(l.tracks[0].record_at.is_none() && l.tracks[0].play_at.is_none());
        for _ in 0..120 {
            tick(&mut l, &mut c, [0.25; 8], [1., 0., 0., 0.], 0.);
        }
        assert_eq!(l.tracks[0].length, 100);
        assert!(l.tracks[0].full && !l.tracks[0].recording);
        assert_eq!(tick(&mut l, &mut c, [0.; 8], [0., 0., 1., 0.], 0.)[0], 0.25);
        assert_eq!(tick(&mut l, &mut c, [0.; 8], [0., 0., 0., 1.], 0.), [0.; 8]);
        for track in [0., 9., 1.5, f64::NAN, f64::INFINITY] {
            tick(&mut l, &mut c, [0.; 8], [track, track, track, track], 0.);
        }
        assert!(l.tracks.iter().all(|t| !t.recording && !t.playing));
        assert_eq!(l.tracks[0].length, 100);
    }
    #[test]
    fn commands_require_change_and_rerecord_replaces_only_selected_track() {
        let mut l = Looper::prepare(100., 1, 1.).unwrap();
        let mut c = clock();
        for _ in 0..10 {
            tick(&mut l, &mut c, [1.; 8], [1., 0., 0., 0.], 0.);
        }
        assert_eq!(l.tracks[0].length, 10);
        tick(&mut l, &mut c, [2.; 8], [2., 1., 0., 0.], 0.);
        assert_eq!(l.tracks[0].length, 10);
        assert_eq!(l.tracks[1].length, 1);
        tick(&mut l, &mut c, [3.; 8], [1., 2., 0., 0.], 0.);
        assert_eq!(l.tracks[0].length, 1);
        assert_eq!(l.tracks[1].length, 1);
        assert_eq!(tick(&mut l, &mut c, [0.; 8], [0., 0., 1., 0.], 0.)[0], 3.);
        assert!(!l.tracks[0].recording);
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn bounded_chunks_reconstruct_audio_and_clear_discards_partial_snapshot() {
        let mut looper = Looper::prepare(48000., 2, 1.).unwrap();
        let audio: Vec<f32> = (0..12000).map(|n| n as f32 / 12000.).collect();
        looper.restore(0, &audio, 2, 48000, 48000.).unwrap();
        looper.tracks[0].dirty = true;
        let mut collected = vec![];
        while let Some((track, channels, offset, total, chunk)) = looper.snapshot_chunk(1024) {
            assert_eq!((track, channels, total), (1, 2, audio.len()));
            assert!(chunk.len() <= 1024);
            assert_eq!(offset, collected.len());
            collected.extend(chunk);
        }
        assert_eq!(collected, audio);
        looper.tracks[0].dirty = true;
        looper.snapshot_chunk(1024).unwrap();
        looper.clear(0);
        let (_, _, offset, total, chunk) = looper.snapshot_chunk(1024).unwrap();
        assert_eq!((offset, total, chunk.len()), (0, 0, 0));
    }
}
