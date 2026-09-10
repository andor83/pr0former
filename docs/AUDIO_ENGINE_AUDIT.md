# Audio engine audit — 2026-09-08

Historical audit. The September 10 stabilization addresses retirement barriers, bounded loop extraction and subscribed-feed collection; see [current status](STATUS.md) and [architecture](ARCHITECTURE.md#stabilization-contracts-2026-09-10) for the current implementation. Risks below describe the audited checkpoint.

The audit covered DSP rendering, native callbacks and queue pacing, browser monitor
conversion/packet delivery, MIDI input polling, visualization analysis, and loop/archive
retirement. Concrete efficiency and queue fixes are implemented. Physical playback on
the reported Mac Studio and a sustained multi-device performance were not tested.

## Changes and evidence

| Finding | Change | Evidence |
| --- | --- | --- |
| Visualization FFTs ran every DSP block, even with no viewers. At 48 kHz/128 frames that was 375 analyses per second per node, for a 20 Hz display. | Analyze only at telemetry cadence while subscribed. Rolling capture and spectral audio processing remain sample-driven; the display sequence now counts published analyses. | Browser regression verifies one analysis per telemetry update and suspension while the graph is hidden, with the sample clock continuing. |
| Stereo rate conversion evaluated 65 windowed-sinc taps with sine/cosine calls for each output frame, for every monitor feed. | Prepare the finite rational-rate phase kernels once, retain the original filter/history, and reserve output storage to its calculated size. | All supported system rates tested in both directions against the original trigonometric reference, including irregular packet boundaries, startup, stereo separation and bounded history. |
| Browser broadcast retained eight DSP blocks, so its time capacity depended on block size/rate. A native callback refill could overwrite queued monitor audio before consumers ran. | Assemble 480-frame stereo packets before broadcast. Capacity is 64 packets (640 ms), enough for a full 16,384-frame native-ring refill at the lowest supported rate. This is burst capacity, not an intentional prebuffer. Reset partial packets and converter history together on project/rate/feed changes or after all monitor listeners leave. | Synthetic full-ring refill tests at 44.1/96 kHz and every DSP block size preserve master/cue samples without receiver lag. Real Chromium/WebRTC count-in tests exercise master at 44.1 kHz and cue at 96 kHz, both with 32-frame blocks. |
| Synths recalculated a constant MIDI pitch ratio for every held voice every sample; samplers did likewise for unchanged roots. | Compute synth ratios at note-on; cache each sampler voice's increment until its root changes. | Existing synthesis/pitch/voice regressions plus a held-note test for immediate integer and fractional root changes. |
| Physical MIDI input queues were polled inside the per-sample loop. | Drain bounded MIDI input batches once per DSP block. | Existing routing tests pass. Input arriving during a render block is now handled at the next block, so physical MIDI onset may wait up to one configured block; this path is not sample-timestamped. |

An isolated release-build comparison processing one second of stereo PCM measured
22.07 ms versus 3.41 ms for 44.1 → 48 kHz, and 22.58 ms versus 3.79 ms for
96 → 48 kHz (about 6× faster conversion). These are local software throughput
measurements, not overall engine speedups, worst-case execution bounds, or device
latency measurements. Reproduce with:

```sh
cargo test -p pr0-server --release resampler_throughput -- --ignored --nocapture
```

A thread-local allocator regression renders a prepared eight-channel graph containing
64-note MIDI input, FM, sampler, granular synthesis, pitch shifting, convolution,
FFT/IFFT, named spectral routing, a visualizer, pitch tracker, looper and recorder.
It verifies zero allocation **and** deallocation calls inside render, including
buffer-capacity paths. It does not prove absence of locks or establish hardware deadlines.

## Remaining risks, in priority order

1. **Live edits can still stall the shared worker.** `audio::Command::Replace` flushes
   retired loop/archive work through blocking writer barriers, then drops the old
   engine on the orchestration thread. Looper snapshot extraction also copies an
   entire completed track there. Disk work itself is on file workers, but a slow
   disk, a large snapshot, or freeing large prepared buffers can still delay refill.
   A follow-up should transfer retirement ownership and snapshot work off the
   render worker while preserving writer order and disable/shutdown acknowledgments.
2. **Secondary hardware clocks are not synchronized.** Native pacing follows the
   first output. Other devices use bounded queues; a faster consumer can underrun
   and a slower one can eventually overflow and discard frames. Fixed latency
   offsets do not correct clock drift. Adaptive per-device resampling and drift
   telemetry require dedicated design and physical multi-device endurance testing.
3. **The worker is not a dedicated real-time thread.** Command handling, telemetry
   allocation/serialization, monitor collection and conversion still share it.
   Device startup/configuration and graph installation can block. A cap of 16
   commands limits command count, not wall-clock duration. OS scheduling pauses
   and heavy diagnostic graphs can exceed queue headroom despite low average CPU.
4. **Monitor overload remains bounded loss.** A receiver delayed beyond 640 ms can
   still lag; it clears its partial Opus input as before. All monitor feeds are
   still collected when any listener exists. Moving conversion/encoding handoff
   out of the worker and collecting only subscribed feeds would reduce load further.
5. **No crossfade on incompatible graph replacement.** Resetting oscillator,
   filter, convolution, delay or spectral state can produce audible discontinuities
   even when all deadlines are met. Compatible state transfer does not solve
   transitions between incompatible graphs.

Validation: 156 regular Rust tests, 17 frontend tests, the frontend build, and ten
affected Chromium cases passed across focused runs. Release server and macOS app builds passed. The ignored resampler benchmark
was also run explicitly. Existing native callback-buffer tests cover synthetic
callback/block mismatch; actual audio/MIDI hardware, iOS, multi-interface clock drift,
sleep/wake and long-session endurance remain unverified.
