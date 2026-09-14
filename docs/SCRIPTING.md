# JavaScript control scripting

JavaScript control nodes turn event handlers into graph controls. They can read
numeric inputs, produce numeric outputs and MIDI, react to OSC messages, follow
the metronome, and subscribe to named Send/Receive control nodes. Scripts run in
the performance server, including when the editor browser disconnects.

This is a control scripting environment. Audio and spectral processing belong in
DSP nodes. Scripts have no browser, DOM, Node.js, filesystem, network client,
package imports, or asynchronous Promise scheduler. All editor assets, the
runtime, examples and this guide are bundled locally.

## Create and edit a script

1. Add **JavaScript control** from the node library and open its Options.
2. Write a script or choose an example from the editor's selector. Choosing an
   example replaces the current draft, so copy any work you want to keep first.
3. **Check script** executes setup in an isolated runtime and reports syntax,
   setup errors, and the resulting ports. Check does not run event handlers.
4. **Apply script** checks again, saves through normal project revision/role
   validation, and prepares a replacement. This works with the engine enabled.
5. Connect the resulting numeric ports or the fixed MIDI input/output and enable
   the engine. Use **Console** for `console.log` and other server activity.

The small editor includes JavaScript syntax highlighting, line numbers, bracket
matching, folding, undo, helper completion and member suggestions. Ctrl/Cmd+Space
opens completion; Ctrl/Cmd+Enter applies. Escape followed by Tab leaves the editor.
Completion describes the API; it is not a TypeScript type checker or debugger.

Unapplied text stays in the editor while telemetry arrives. Check or compile
errors leave the running version intact. Apply restarts this script's variables,
handlers and timers, even when the source is unchanged; unchanged scripts retain
their runtime across unrelated compatible graph edits. Deleted or renamed ports
lose their attached cables on Apply. Undo restores source and port configuration
through project revisions, not the old JavaScript heap. Close the editor only
after applying or copying any draft you need to keep.

All source edits obey the performance lock and project editing roles. Someone
who can view a project can view its source; only authorized editors can apply it.

## First example

```javascript
const velocity = define_input("velocity", 100);
const gain = define_output("gain", 0);

velocity.on("change", event => {
  gain.write(clamp(event.value / 127));
  console.log("Velocity", event.value, "at sample", event.sample);
});
```

Wire a numeric source to `velocity` and `gain` to an audio Gain node's parameter.
The JS node itself carries no audio. Every input receives an initial observation
when the engine begins rendering, even if it equals its declared default.

## Inputs and outputs

Declare ports at the top level during setup. Names are stable port IDs: use 1–48
ASCII characters, beginning with a letter or underscore, followed by letters,
digits, underscores or hyphens. Names must be unique within their direction;
`midi` is reserved. Each direction supports eight numeric ports plus the fixed
MIDI port. Values and defaults must be finite numbers.

```javascript
const amount = define_input("amount", 0.5);
const result = define_output("result", 0);
amount.on("change", e => result.write(e.value * 2));

// Equivalent handler shorthand:
define_input("trigger", 0, e => console.log(e.value));
```

| Handle | API | Meaning |
| --- | --- | --- |
| Input | `input.read()`, `input.get()`, `input.value` | Latest value delivered to this worker; default before first observation. |
| Input | `.on("change", handler)` | Receives `{name, value, previous, sample, beat, …}` for value changes and explicit repeated events. |
| Input | `.on("rise", handler)`, `.on("fall", handler)` | Crossing from ≤0 to >0 or >0 to ≤0. |
| Output | `output.write(value)` | Queue a held numeric output; `.set()` and `.emit()` are aliases. |
| Output | `output.read()`, `.value` | Most recently requested value, including a future scheduled write; use the modal for the actually applied engine value. |
| Output | `.at_sample(sample, value)` | Schedule at an absolute engine sample. |
| Output | `.at_beat(beat, value)` | Schedule at an absolute graph beat, preserving the target across tempo changes. |
| Output | `.pulse(value = 1, duration_ms = 10)` | Queue a value and a return to zero, with 10 ms initial lookahead. |
| Output | `.on("change", handler)` | Observe writes requested by this script, not engine acknowledgments. |

The modal displays engine input/output values. A connected input is read-only and
its default is ignored; change the upstream source or disconnect it. Unconnected
defaults are edited in the source and applied with the script. Numeric input
fan-in follows the graph's normal change arbitration. A named input handler is
called when that input is delivered, not once per animation frame.

## Events and subscriptions

Every event emitter supports `.on(name, handler)`, `.off(name, handler)`, and
`.once(name, handler)`. Keep the original function reference for `.off`. Arrow
functions work normally. Handlers are synchronous. Throwing an exception or
returning a Promise stops the script; use the engine-time timer helpers instead.
There are at most 256 subscriptions per script.

Common details include `sample`, `beat`, `bpm`, `sample_rate`, and `generation`.
These are engine timestamps, not browser arrival times. `engine.now()` returns
the current handler's engine time and the latest observed `running` state.

| Emitter | Events | Details |
| --- | --- | --- |
| `engine` | `ready` | Once, immediately before the first delivered event. Initialize engine-dependent work here. |
| `engine` | `tick` | Once per processing block: also `frames` (since previous tick) and `dt` in seconds. First tick has zero elapsed frames. `on_tick(fn)` is shorthand. |
| `engine` | `reset` | A graph-clock reset: play alignment, seek, stop/rewind. |
| `transport` | `play`, `pause` | Show running state changes. The initial stopped state emits `pause`; Stop also produces `reset`. |
| `transport` | `tempo` | Tempo observation, with `previous`; initial observation has no previous value. |
| `transport` | `reset` | Same clock-reset notification as `engine`. |
| `metronome` | `click` | Each denominator-beat boundary, with written `position` and `beat_unit`. |

```javascript
const source = define_input("source");
const result = define_output("result");
on_tick(event => {
  result.write(source.read());
  // event.frames and event.dt describe elapsed engine time.
});
```

Tick means an engine block, not an individual audio sample or a browser animation
frame. At 48 kHz with 128-frame blocks there are 375 ticks per second. The chosen
server block size controls this cadence. Tick is for lightweight control work;
use change handlers when you only need to react to changes.

Metronome subscriptions run even when audible metronome monitoring is off. During
show playback they follow its written meter grid; while the show is stopped they
follow the free-running graph grid. They are not callbacks for every PCM sample
of a click sound, and count-in clicks are not a separate scripting event.

## MIDI

Connect a MIDI Input, Local MIDI Input, Piano, Part MIDI or another MIDI source to
the script's **MIDI** input. Connect its **MIDI** output to an instrument or MIDI
Output node. Device selection and external delivery remain in those nodes.
Incoming MIDI is consumed by the script; forward it explicitly if wanted.

```javascript
midi.on("message", event => {
  console.log(event.bytes, event.channel, event.type);
  midi.send(event.bytes); // explicit passthrough
});
```

`message` fires for every accepted MIDI 1.0 channel message, followed by one of:
`note_on`, `note_off`, `cc`, `pitch_bend`, `program`, `channel_pressure`, or
`poly_pressure`. Velocity-zero note-on is classified as `note_off` while retaining
its original bytes. All events include `bytes`, `status`, `data1`, `data2` and
one-based `channel` (1–16).

| Event | Additional fields |
| --- | --- |
| `note_on`, `note_off` | `note`, `velocity` |
| `cc` | `controller`, `value` (0–127), `normalized` |
| `pitch_bend` | `value` (0–16383; center 8192), `normalized` (approximately −1 to +1) |
| `program` | `program` (0–127) |
| `channel_pressure`, `poly_pressure` | `pressure`; poly pressure also has `note` |

```javascript
const modulation = define_output("modulation");
midi.on("cc", e => {
  if (e.channel === 1 && e.controller === 1) modulation.write(e.normalized);
});
```

Send helpers:

```javascript
midi.send([0x90, 60, 100], {beat: 8});
midi.note_on(60, 100, 1, {sample: engine.now().sample + 480});
midi.note_off(60, 1, {beat: 8.5});
midi.cc(1, 64, 1);
midi.pitch_bend(8192, 1);
midi.program(10, 1);
midi.all_notes_off(1);
midi.note(60, 100, 0.25, 1, 9); // note, velocity, duration in beats, channel, start beat
const decoded = midi.decode([0xb0, 1, 64]);
```

Channel defaults to 1; note-on velocity defaults to 100. `midi.note` defaults to
a quarter of a graph beat and a start 0.05 beats after the current event. Output
helpers other than `note` accept `{sample}` or `{beat}`; if both are provided,
beat scheduling wins. Future note releases follow tempo changes.

Raw means the original two/three channel-message bytes. SysEx, MIDI realtime
clock/start/stop bytes, system-common messages and MIDI 2.0 UMP are outside the
existing graph MIDI contract. Same-channel repeated-pitch notes share MIDI 1.0's
note identity; do not depend on independent ownership for overlapping pitches.

## OSC

Enable OSC reception in System settings and choose the server receive interfaces
and port. The active engine project receives messages through its configured UDP
receiver; scripts cannot create sockets or subscribe to another project.

```javascript
const level = define_output("level");
osc.bind("/fader/1").on("message", event => {
  const arg = event.args[0];
  if (arg && ["float", "double", "int"].includes(arg.type)) {
    level.write(clamp(arg.value));
  }
});
osc.on("message", event => console.log(event.address, event.args));
```

Bindings use exact, case-sensitive addresses. Register at most 32 during setup.
`osc.on("message", …)` receives all admitted addresses, including messages not
handled by the ordinary OSC value nodes. Events carry `address`, ordered `args`,
and the engine timestamp at server admission. Arguments preserve their type:

| Type | Representation |
| --- | --- |
| `int`, `float`, `double`, `bool`, `string`, `char` | `{type, value}`; character is a string. |
| `int64` | Decimal string value to avoid JavaScript integer precision loss. |
| `blob` | Byte-array value. |
| `midi`, `color` | Four-byte array value. |
| `time` | `{type: "time", seconds, fractional}`. |
| `array` | Value is an array of typed argument records. |
| `nil`, `inf` | `value: null`, distinguished by type. |

This version accepts OSC messages, not bundles or timetag scheduling. UDP packets
use the existing 8 KiB receive buffer; script JSON payloads are capped at 16 KiB.
The script OSC queue holds 32 messages; overflow drops incoming OSC and increments
the dropped counter. External UDP timing is best effort. To send ordinary OSC
values, wire a numeric script output to an OSC Output node and configure that
node's destination/address. Scripts do not directly emit arbitrary OSC packets.

## Named Send and Receive controls

```javascript
const gesture = bind_send("gesture");
const applied = bind_receive("master-level");
const master = control.send("master-level");

gesture.on("change", e => {
  if (typeof e.value === "number") master.write(clamp(e.value));
});
applied.on("change", e => console.log("Receiver now has", e.value));
```

`bind_send(name)` observes Send control nodes publishing under that target name;
`bind_receive(name)` observes matching Receive control nodes after routing.
`control.receive(name)` is an alias for `bind_send(name)`. Handles support
`.read()`, `.on("change", fn)`, `.write(value)` and `.emit(value)`.
Writing creates a script-owned numeric publication on that name, visible to
ordinary Receive control nodes. `control.send(name)` creates a publishing handle
without subscribing. It does not change a Send node's connected input or persisted
parameters. Script publications and ordinary senders arbitrate by graph priority
when changing together. Values hold until another source changes, the script
restarts, or the publication disappears.

Declare at most sixteen bindings during setup. Names match exactly within the
loaded graph, including subgraphs. Observed values may be numeric or text; this
version publishes finite numbers only so numeric graph contracts remain valid.
If several matching nodes publish, bindings report observations in engine node
order; use a unique name per logical source when order matters.

## Timers, timing and output scheduling

```javascript
engine.on("ready", () => {
  const id = every_beats(4, e => console.log("Four beats", e.beat));
  after_beats(16, () => cancel(id));
  after_ms(250, e => console.log("250 ms of engine sample time", e.sample));
});
```

`after_beats`, `every_beats`, `after_ms` and `every_ms` return timer IDs.
`cancel(id)` removes one. Delays must be positive; at most 128 timers can be
active. Timers fire on a delivered engine tick. Repeating timers skip missed
intervals rather than generating an unbounded catch-up burst. Timers declared at
setup start from the first engine observation, including during live Apply.
Use `engine.on("ready", …)` for relative `pulse`/`midi.note` scheduling and other
initialization needing the current sample rate or beat. Graph resets clear
timers; register replacements in a reset handler if desired. Graph beats continue
while the show is paused, so beat timers also continue then.

JavaScript callbacks are asynchronous relative to rendering. The render worker
never waits for them. A reactive `.write` becomes eligible at the input event's
sample and applies when its queued command reaches the engine, usually later.
The late counter includes these ordinary reactive outputs; it is not an audio
underrun count. This is unsuitable for sample-synchronous feedback or audio-rate
modulation. Use native control/DSP nodes for those tasks.

For planned events, choose a future beat or sample with enough lookahead for
worker scheduling. Commands already admitted into the prepared queue can apply
at the chosen engine sample; this does not establish hardware or external MIDI
deadline reliability. A delayed worker can miss lookahead, and an overdue pulse
can collapse. Beat targets preserve tempo relationships. Seek, Stop and Play's
graph alignment cancel old-generation scheduled output and release held notes.

## Math and musical helpers

| Helper | Behavior |
| --- | --- |
| `clamp(v, min=0, max=1)` | Clamp to a range. |
| `map_range(v, inMin, inMax, outMin, outMax)` | Linear mapping without clamping. |
| `lerp(a, b, t)` | Linear interpolation; extrapolates outside 0–1. |
| `wrap(v, min=0, max=1)` | Wrap, including negative values, into `[min,max)`. |
| `quantize(v, step=1)` | Round to a positive step. |
| `midi_to_hz(note)`, `hz_to_midi(hz)` | Equal temperament, A4 = 440 Hz; fractional pitches allowed. |
| `db_to_gain(db)`, `gain_to_db(gain)` | Amplitude conversion; gain-to-dB floors gain at 1e−12. |
| `seed_random(uint32)`, `random()` | Seeded per-runtime generator; default seed 1, result in `[0,1)`. |
| `random_int(min,max)`, `choose(array)` | Inclusive integer choice or a nonempty array choice. |
| `chance(probability)` | Random boolean; probability clamped to 0–1. |
| `euclidean(step,hits,length)` | Boolean rhythm step; length 1–1024, hits 0–length. |

Ordinary `Math`, arrays, maps, sets, strings, closures and JSON are available.
Use `random()` when repeatability matters; JavaScript's built-in `Math.random()`
and `Date` are not musical clocks. Helpers are also described in completion.

## Console and runtime failures

`console.log`, `console.info`, `console.warn`, `console.error` and `console.debug`
write to the existing project GUI Console. Entries include the node label/ID,
engine sample and level. Objects/arrays serialize as JSON; cyclic objects fail
serialization. The editor shows a small recent console view and runtime error.
Logs, counters and actual input/output values are telemetry; they are never
project revisions or undo entries.

Each script has a separate runtime/thread, a 16 MiB JS heap limit, a 256 KiB JS
stack limit, a 250 ms setup budget and a 5 ms interrupt budget per delivered event.
These interrupt checks bound runaway work cooperatively; they are not hard
wall-clock guarantees for native builtins or operating-system scheduling.

Projects support sixteen scripts, each with at most 64 KiB source. Each script
has 1,024 event/command queue slots and 256 pending output slots. Admission is
limited to 32 output commands per sample. An event can request at most 256
commands and 64 console entries, each truncated to 2,048 characters/bytes at the
JS/host boundaries; the recent script console retains 64 entries.

An exception, timeout, invalid output or graph-event/command overflow stops that
script. Numeric outputs reset to zero, its named publications disappear and
tracked MIDI notes are released. Apply to restart. Failed scripts do not stop
the rest of the graph. Releases exceeding the fixed MIDI buffer use sustain-off
and all-notes-off on affected channels; other sources sharing those channels can
also be silenced. Very high-rate numeric input changes can fill a queue;
this feature is designed for control events, not arbitrary audio-rate sources.
Do not use `console.log` every tick in a performance patch.

## Useful future additions

Potential additions beyond this version include scale/chord and note-name
helpers, explicit debounce/throttle operators, snapshotable user state, named
text-output contracts, multiple MIDI ports, OSC bundles and output packets,
sample-accurate automation ramps, score-part/cue bindings with conductor
authorization, and a debugger with handler profiling. These are suggestions,
not currently callable APIs.
