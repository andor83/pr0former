# Plan: one-cable MIDI routing across note nodes

Status: design only. This does not describe implemented universal MIDI support.

## Intent and existing foundation

Every MIDI source, destination and note instrument should expose an additional typed MIDI port while retaining its existing scalar control ports and behavior. A user can patch one MIDI cable for ordinary routing, or explicitly use pitch/velocity/gate/trigger/note-off controls for graph processing. Connect matching ports should prefer the MIDI cable when both representations overlap.

The repository already defines `Signal::Midi`, fixed-size `pr0_core::midi::Message` channel messages, prepared 256-event buffers, MIDI subgraph boundaries, Part MIDI `events` output, MIDI output `events` input and MIDI-to-control decoding. Extend those contracts rather than adding a second signal enum or converting MIDI through scalar values.

**Working default:** v1 transports MIDI 1.0 channel messages: note on/off, poly pressure, CC, program, channel pressure and 14-bit bend. SysEx, MIDI Clock/Start/Stop, active sensing, system common messages and MIDI 2.0 need a separate variable-length/timestamped contract. The user has been asked whether those should join this scope; channel messages remain the default until answered.

## Ports and behavior

| Nodes | Add/retain typed ports | Behavior |
| --- | --- | --- |
| Part MIDI, physical MIDI input | `events` output | Emit every supported channel message in received/scheduled order, preserving status/channel/data bytes; keep current scalar note outputs. Existing explicit part/staff/channel filters apply before emission. |
| Piano | `events` input and output | Forward incoming messages unchanged. Reflect supported note messages in held-key feedback; append locally played notes using the configured MIDI channel (new modal parameter, default channel 1). Retain scalar input/output behavior. |
| Polyphonic synth, FM synth, polyphonic sampler, granular synth | `events` input and output | Consume supported messages for local sound and provide an unchanged MIDI-thru stream for chaining. Initially interpret notes, all-notes-off and all-sound-off; preserve/forward other channel messages without claiming they affect the instrument. Retain all scalar controls and direct score routing. |
| MIDI output | `events` input and output | Send each input message to the selected native port and expose unchanged thru. Preserve legacy scalar note/CC modes. |
| MIDI-to-OSC, OSC-to-MIDI | `events` input/output as appropriate, with thru on the sender | Preserve the existing address/pitch/velocity scalar OSC contract. Typed messages use an additional reserved `<configured-address>/midi` address with three integer arguments `[status,data1,data2]`; two-byte MIDI messages retain canonical data2=0. Validate bytes at ingress. The receiving node handles both contracts separately, without synthesizing duplicate events from raw messages. |
| MIDI to control | Existing `events` input plus `events` output | Decode for scalar inspection and pass the full stream unchanged. Decoder filters affect scalar output only, not thru. |
| MIDI subgraph boundaries | Existing typed boundary ports | Forward the identical stream across nested subgraphs, with no channel-width requirement. |

Add a small catalog metadata field identifying scalar ports represented by a MIDI port (default absent for old descriptors): for note interfaces this is `pitch`, `velocity`, `gate`, `trigger`, `note_off`. This lets auto-connect suppress only redundant scalar pairs. Other parameters/audio connections remain eligible. Keep node names, scalar port IDs, saved edges and defaults compatible.

Transport copies events directly, preserving same-sample ordering and simultaneous chords. Interpretation may normalize note-on velocity zero internally, but thru must preserve original bytes. MIDI channel numbers are independent of an audio node’s 1–8 channel width. Existing physical input block polling remains best effort, not sample-timestamped hardware ingress.

Explicitly wiring both representations intentionally enables both input paths; existing manual edges are never removed. Process typed events before scalar transitions on a sample. Track typed/scalar/direct-score note ownership separately so one path’s release cannot stop another path’s note. Repeated pitches follow the existing oldest-held-first convention within a source. A source that emits typed events and scalar pulses must not feed its own scalar mirror back into typed thru a second time.

## Auto-connect and presentation

1. Resolve the two descriptors, including nested boundary ports.
2. Match compatible typed MIDI output/input first; MIDI compatibility does not compare audio widths. If a MIDI input already has an edge from this source, regard the pair as covered and do not add its redundant controls.
3. For each covered MIDI pair, suppress the scalar pairs listed in its representation metadata. Continue ordinary matching for unrelated audio, spectral and scalar ports.
4. If no MIDI route is available because the destination is occupied by a different source, do not silently fall back to its mirrored scalar controls and create duplicate note routing. Return a concise “MIDI input already connected” result; users can wire controls explicitly. When either node lacks a compatible MIDI interface, retain current scalar auto-connect behavior.
5. Make the entire operation one undo entry, validate it on the server, and leave existing graphs unchanged. Repeated auto-connect is idempotent.

Label ports “MIDI” in the UI while retaining `events` as the stable identifier. Use a distinct green MIDI cable/handle treatment and legend entry that works in both graph themes; preserve charcoal/cyan/amber/violet node bodies and existing audio/control/spectral styles. Add accessible signal labels and reject invalid drop targets. MIDI ports do not display editable fake scalar values.

## Implementation sequence and acceptance

- Core: extend catalog coverage and representation metadata; validate typed edges, limits, ingress bytes and ownership contracts server-side. Read existing schema-1 projects with all defaults intact.
- DSP: dispatch and forward prepared event buffers across each node; add typed note ownership and sample-ordered consumption without allocation, locks or I/O. Keep legacy scalar tests unchanged. Preserve channel bytes and all supported non-note messages through a chain.
- Server: emit physical MIDI channel messages without reducing them to note controls; preserve output lengths/order, add the explicit raw OSC envelope, keep physical I/O off render, and expose overflow diagnostics.
- Frontend: mirror new descriptor metadata, update matching and its result/message contract, add MIDI styles/legend/modal channel setting, and document manual versus automatic wiring.
- Tests: every node/port combination; all 16 MIDI channels; program/pressure two-byte lengths; complete 14-bit bend range; note-on-zero preservation; same-sample chords and releases; repeated pitches and mixed-path ownership; nested graphs; live replacement; unknown message forwarding; invalid edges/bytes; audio-width-independent MIDI; auto-connect preference, unrelated matches, occupied input and repeated calls; JSON round trips and undo.
- Overload: retain bounded queues and explicit drop counters. A saturated internal event buffer enters recovery for that sample, emits one all-notes-off recovery set, and does not append later note-ons into that same recovered frame. “Unaltered” applies to admitted messages, not an impossible lossless guarantee under overload.
- Browser integration: one cable from Piano/Part MIDI to each instrument yields audio; native ingress uses synthetic test bytes; nested thru preserves values; controls still support deliberate pitch/velocity operations. Physical MIDI/OSC devices, LAN timing and endurance remain manual unless actually exercised.

No automatic graph migration or recabling. Existing control patches continue sounding exactly as before; only newly requested matching connections prefer MIDI.
