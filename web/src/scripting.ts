import type { GraphEdge, ScriptConfig } from './types'

export const starterScript = `const velocity = define_input("velocity", 100);
const gain = define_output("gain", 0);

velocity.on("change", event => {
  gain.write(clamp(event.value / 127));
});

metronome.on("click", event => {
  console.log("Click", event.beat);
});

// Runs once per engine block, outside audio rendering.
on_tick(event => {
  // Read current input: velocity.read()
  // event.frames, event.dt, event.sample, event.beat
});`

export const examples: Record<string, string> = {
  'Input → gain': starterScript,
  'MIDI transpose': `const semitones = define_input("semitones", 12);
const held = new Map();
midi.on("message", event => {
  const key = event.channel + ":" + event.note;
  if (event.type === "note_on") {
    const pitch = Math.round(clamp(event.note + semitones.read(), 0, 127));
    held.set(key, pitch);
    midi.note_on(pitch, event.velocity, event.channel);
  } else if (event.type === "note_off") {
    midi.note_off(held.get(key) ?? event.note, event.channel);
    held.delete(key);
  } else midi.send(event.bytes);
});
engine.on("reset", () => held.clear());`,
  'Euclidean rhythm': `const hits = define_input("hits", 5);
const trigger = define_output("trigger");
let step = 0;
metronome.on("click", event => {
  if (euclidean(step++, Math.round(clamp(hits.read(), 0, 8)), 8)) {
    trigger.pulse();
    midi.note(36, 110, 0.1);
  }
});
engine.on("reset", () => { step = 0; });`,
  'OSC → named control': `const level = control.send("master-level");
osc.bind("/fader/1").on("message", event => {
  const first = event.args[0];
  if (first && ["float", "double", "int"].includes(first.type)) {
    level.write(clamp(first.value));
    console.log(event.address, first.value);
  }
});
// Add a Receive control node with target master-level.`,
  'Named route monitor': `const source = bind_send("gesture");
const value = define_output("value");
source.on("change", event => {
  if (typeof event.value === "number") value.write(event.value);
});
bind_receive("master-level").on("change", event => {
  console.log("Receiver", event.name, event.value);
});`,
}

export const completions = [
  ['define_input', 'define_input(name, initial = 0, handler?) → input.read(), input.on("change" | "rise" | "fall", handler)'],
  ['define_output', 'define_output(name, initial = 0) → output.write(value), at_beat(), at_sample(), pulse()'],
  ['on_tick', 'on_tick(event => …); once per engine block; event.frames and event.dt'],
  ['engine', 'engine.on("ready" | "tick" | "reset", handler); engine.now()'],
  ['metronome', 'metronome.on("click", event => …); engine-timestamped denominator beat'],
  ['transport', 'transport.on("play" | "pause" | "tempo" | "reset", handler)'],
  ['midi', 'midi.on("message" | "note_on" | "note_off" | "cc" | "pitch_bend", handler)'],
  ['osc', 'osc.on("message", handler); osc.bind("/address").on("message", handler)'],
  ['bind_send', 'bind_send(name).on("change", handler); observe named Send control nodes'],
  ['bind_receive', 'bind_receive(name).on("change", handler); observe named Receive control nodes'],
  ['control', 'control.send(name).write(value); control.receive(name).on("change", handler)'],
  ['after_beats', 'after_beats(delay, handler) → timer ID; follows graph beats and tempo'],
  ['every_beats', 'every_beats(interval, handler) → timer ID'],
  ['after_ms', 'after_ms(delay, handler) → timer ID; based on engine samples'],
  ['every_ms', 'every_ms(interval, handler) → timer ID'],
  ['cancel', 'cancel(timerId)'], ['clamp', 'clamp(value, min = 0, max = 1)'],
  ['map_range', 'map_range(value, inputMin, inputMax, outputMin, outputMax)'],
  ['lerp', 'lerp(a, b, amount)'], ['wrap', 'wrap(value, min = 0, max = 1)'],
  ['quantize', 'quantize(value, step = 1)'], ['midi_to_hz', 'midi_to_hz(note)'], ['hz_to_midi', 'hz_to_midi(hz)'],
  ['db_to_gain', 'db_to_gain(db)'], ['gain_to_db', 'gain_to_db(gain)'],
  ['seed_random', 'seed_random(unsignedInteger)'], ['random', 'random() → [0, 1)'],
  ['random_int', 'random_int(min, max), inclusive'], ['choose', 'choose(nonemptyArray)'],
  ['chance', 'chance(probability)'], ['euclidean', 'euclidean(step, hits, length) → boolean'],
  ['console', 'console.log/info/warn/error/debug(...values) → project GUI Console'],
].map(([label, info]) => ({label: label!, info: info!, type: 'function'}))

export const memberCompletions: Record<string, {label:string;info:string;type:string}[]> = {
  midi: ['on','off','once','send','decode','note_on','note_off','note','cc','pitch_bend','program','all_notes_off'].map(label=>({label,info:`midi.${label} — see scripting guide for arguments`,type:'function'})),
  osc: ['on','bind'].map(label=>({label,info:'Receive typed OSC arguments from the configured project receiver',type:'function'})),
  console: ['log','info','warn','error','debug'].map(label=>({label,info:'Write to the GUI Console, tagged with node and engine sample',type:'function'})),
  engine: ['on','off','once','now'].map(label=>({label,info:'Engine time and lifecycle',type:'function'})),
  control: ['send','receive'].map(label=>({label,info:'Bind a named control route during setup',type:'function'})),
}
export function removedScriptEdges(edges:GraphEdge[],node:string,script:ScriptConfig) {
  const inputs=new Set(['midi',...script.inputs.map(p=>p.name)]),outputs=new Set(['midi',...script.outputs.map(p=>p.name)])
  return edges.filter(e=>(e.target===node&&!inputs.has(e.target_port))||(e.source===node&&!outputs.has(e.source_port)))
}
