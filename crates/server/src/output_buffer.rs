//! Bounded native output buffering. DSP blocks and driver callbacks can differ.
use pr0_core::MAX_DEVICE_CHANNELS;
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

type Frame = [f32; MAX_DEVICE_CHANNELS];
const CAPACITY: usize = 16_384;

pub struct OutputBuffer {
    producer: rtrb::Producer<Frame>,
    callback_frames: Arc<AtomicUsize>,
    block_size: usize,
    sample_rate: u32,
}

pub struct OutputConsumer {
    consumer: rtrb::Consumer<Frame>,
    callback_frames: Arc<AtomicUsize>,
    underruns: Arc<AtomicU64>,
}

impl OutputBuffer {
    pub fn new(
        block_size: usize,
        sample_rate: u32,
        underruns: Arc<AtomicU64>,
    ) -> (Self, OutputConsumer) {
        let (producer, consumer) = rtrb::RingBuffer::new(CAPACITY);
        // A conservative startup estimate until the driver reports its first callback.
        let callback_frames = Arc::new(AtomicUsize::new(0));
        let mut buffer = Self {
            producer,
            callback_frames: callback_frames.clone(),
            block_size,
            sample_rate,
        };
        // The stream starts before the worker resumes. Supply silence during setup.
        for _ in 0..buffer.target_frames() {
            buffer.push([0.; MAX_DEVICE_CHANNELS]);
        }
        (
            buffer,
            OutputConsumer {
                consumer,
                callback_frames,
                underruns,
            },
        )
    }

    fn target_frames(&self) -> usize {
        let observed = self.callback_frames.load(Ordering::Relaxed);
        let callback = if observed == 0 { 1024 } else { observed };
        callback
            .saturating_mul(2)
            .max(self.block_size * 2)
            .max(self.sample_rate as usize / 100)
            .min(CAPACITY - self.block_size)
    }

    pub fn needs_frames(&self) -> bool {
        CAPACITY - self.producer.slots() < self.target_frames()
    }

    pub fn push(&mut self, frame: Frame) {
        // Secondary devices can still drift relative to the pacing device.
        let _ = self.producer.push(frame);
    }
}

impl OutputConsumer {
    // Called by CPAL: no allocation, locks, logging, or other I/O.
    pub fn write(&mut self, data: &mut [f32], channels: usize) {
        self.callback_frames
            .fetch_max(data.len() / channels, Ordering::Relaxed);
        let mut missing = 0;
        for frame in data.chunks_mut(channels) {
            let samples = self.consumer.pop().unwrap_or_else(|_| {
                missing += 1;
                [0.; MAX_DEVICE_CHANNELS]
            });
            for (ch, value) in frame.iter_mut().enumerate() {
                *value = samples.get(ch).copied().unwrap_or(0.);
            }
        }
        if missing > 0 {
            self.underruns.fetch_add(missing, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_physical_channels_reach_the_callback_and_unmapped_channels_are_silent() {
        let (mut buffer, mut consumer) = OutputBuffer::new(128, 48000, Arc::new(AtomicU64::new(0)));
        let mut startup = vec![0.; buffer.target_frames() * 64];
        consumer.write(&mut startup, 64);
        let mut frame = [0.; MAX_DEVICE_CHANNELS];
        frame[8] = 0.25;
        frame[63] = 0.5;
        buffer.push(frame);
        let mut data = [1.; 64];
        consumer.write(&mut data, 64);
        assert_eq!(data, frame);
        // A stereo device ignores channels addressed beyond its physical width.
        buffer.push(frame);
        let mut stereo = [1.; 2];
        consumer.write(&mut stereo, 2);
        assert_eq!(stereo, [0.; 2]);
    }

    #[test]
    fn old_block_sized_target_starves_a_larger_callback() {
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(4096);
        while producer.slots() >= 4096 - 128 * 2 {
            for _ in 0..128 {
                producer.push(1_f32).unwrap();
            }
        }
        let missing = (0..512).filter(|_| consumer.pop().is_err()).count();
        assert_eq!(missing, 128);
    }

    #[test]
    fn mismatched_callbacks_preserve_every_sine_sample() {
        for rate in [44100, 48000, 88200, 96000] {
            for block in [32, 64, 128, 256, 512, 1024] {
                for callback in [64, 128, 256, 512, 1024, 2048, 4096] {
                    let underruns = Arc::new(AtomicU64::new(0));
                    let (mut buffer, mut consumer) =
                        OutputBuffer::new(block, rate, underruns.clone());
                    // Learn the driver's batch size and drain startup silence before
                    // comparing the signal. A newly discovered larger batch may
                    // underrun once during startup, but must not do so repeatedly.
                    let initial = buffer.target_frames();
                    let mut startup = vec![0.; callback * 2];
                    for _ in 0..initial.div_ceil(callback) {
                        consumer.write(&mut startup, 2);
                    }
                    underruns.store(0, Ordering::Relaxed);
                    let mut produced = 0;
                    let mut consumed = 0;
                    let sine = |i: usize| {
                        (std::f64::consts::TAU * 873. * i as f64 / rate as f64).sin() as f32 * 0.04
                    };
                    for _ in 0..100 {
                        while buffer.needs_frames() {
                            for _ in 0..block {
                                buffer.push([sine(produced); MAX_DEVICE_CHANNELS]);
                                produced += 1;
                            }
                        }
                        // Allow one missed worker wake-up between callbacks.
                        for _ in 0..2 {
                            let mut data = vec![0.; callback * 2];
                            consumer.write(&mut data, 2);
                            for frame in data.chunks_exact(2) {
                                assert_eq!(
                                    frame,
                                    &[sine(consumed); 2],
                                    "rate={rate}, block={block}, callback={callback}, sample={consumed}"
                                );
                                consumed += 1;
                            }
                        }
                    }
                    assert_eq!(underruns.load(Ordering::Relaxed), 0);
                }
            }
        }
    }
}
