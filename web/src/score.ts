import type { Note, NoteNotation, Part, Staff, RationalTime } from './types'
import { durationGlyphs } from './notation'
/** Finale Speedy/Simple Entry keypad order: 1 = 64th … 5 = quarter … 8 = double whole. */
export const durationKeys = [0.0625, 0.125, 0.25, 0.5, 1, 2, 4, 8]
export const durationLabels = [
  '64th',
  '32nd',
  '16th',
  'Eighth',
  'Quarter',
  'Half',
  'Whole',
  'Double whole',
]
export const durationSymbols = ['𝅘𝅥𝅱', '𝅘𝅥𝅰', '𝅘𝅥𝅯', '♪', '♩', '𝅗𝅥', '𝅝', '𝅜']
export const stepLetters = ['C', 'D', 'E', 'F', 'G', 'A', 'B']
/** Diatonic step of `letter` nearest to `near` (Finale letter entry picks the closest octave). */
export function nearestLetterStep(letter: string, near: number): number {
  const index = stepLetters.indexOf(letter.toUpperCase())
  if (index < 0) return near
  const current = ((near % 7) + 7) % 7,
    up = (index - current + 7) % 7,
    down = up - 7
  return near + (Math.abs(up) <= Math.abs(down) ? up : down)
}
/** Written duration in quarter beats for a base value with dots and a tuplet ratio. */
export function writtenDuration(
  base: number,
  dots = 0,
  tupletActual = 1,
  tupletNormal = 1,
) {
  return (base * (2 - 2 ** -dots) * tupletNormal) / tupletActual
}
/** Caret stops: note onsets and releases in the staff/voice, plus bar boundaries. */
export function caretStops(
  notes: Note[],
  part: Part,
  staff: string,
  voice: number,
  bars: { start: number; end: number }[],
): number[] {
  const stops = new Set<number>([0])
  for (const m of bars) {
    stops.add(m.start)
    stops.add(m.end)
  }
  for (const n of notes) {
    const v = metadata(n, part)
    if (v.staff !== staff || v.voice !== voice || v.grace_to) continue
    stops.add(n.beat)
    stops.add(n.beat + n.duration)
  }
  return [...stops].sort((a, b) => a - b)
}
/**
 * A write click chooses a bar and its preceding note group. The new entry abuts
 * that group, or starts at the bar boundary when the click precedes all notes.
 */
export function entryBeat(
  notes: Note[],
  part: Part,
  staff: string,
  voice: number,
  hiddenRests: { beat: number; duration: number; voice: number }[] | undefined,
  clicked: number,
  barStart = 0,
  barEnd = Infinity,
): number {
  const occupied: { beat: number; end: number }[] = []
  for (const n of notes) {
    const v = metadata(n, part)
    if (v.staff !== staff || v.voice !== voice || v.grace_to) continue
    if (n.beat < barEnd - 1e-9 && n.beat + n.duration > barStart + 1e-9)
      occupied.push({ beat: n.beat, end: n.beat + n.duration })
  }
  for (const r of hiddenRests || [])
    if (
      r.voice === voice &&
      r.beat < barEnd - 1e-9 &&
      r.beat + r.duration > barStart + 1e-9
    )
      occupied.push({ beat: r.beat, end: r.beat + r.duration })
  occupied.sort((a, b) => a.beat - b.beat || a.end - b.end)
  let cursor = barStart
  for (const event of occupied) {
    if (event.beat >= clicked - 1e-9) break
    cursor = Math.max(cursor, event.end)
  }
  return Math.min(cursor, barEnd)
}
/** Plan an insertion without modifying the score. Chords move as one onset;
 * existing gaps absorb displacement before later events have to move. */
export function insertEntry(
  part: Part, staff: string, voice: number, beat: number, duration: number, barEnd: number,
): { notes: Note[] } | string {
  const eps = 1e-9
  const events = part.notes.filter(n => {
    const v = metadata(n, part)
    return v.staff === staff && v.voice === voice && !v.grace_to
  }).sort((a,b) => a.beat-b.beat)
  if (beat + duration > barEnd + eps)
    return 'Cannot insert note: this would overflow the bar.'
  if (events.some(n => n.beat < beat-eps && n.beat+n.duration > beat+eps))
    return 'Cannot insert inside a sustained note; place the caret after it.'
  const moves = new Map<string, number>()
  let end = beat + duration
  for (let i = 0; i < events.length;) {
    const onset = events[i]!.beat
    const group: Note[] = []
    while (i < events.length && Math.abs(events[i]!.beat-onset) < eps) group.push(events[i++]!)
    if (onset < beat-eps || onset >= barEnd-eps) continue
    const next = Math.max(onset, end)
    const release = next + Math.max(...group.map(n => n.duration))
    if (release > barEnd+eps) return 'Cannot insert note: this would overflow the bar.'
    for (const n of group) moves.set(n.id,next)
    end = release
  }
  return {notes: part.notes.map(n => {
    const v = metadata(n,part)
    const position = moves.get(n.id) ?? (v.grace_to ? moves.get(v.grace_to) : undefined)
    return position === undefined ? n : atBeat(n,position)
  })}
}
const glyphBeats: Record<string, number> = {
  '1/2': 8,
  w: 4,
  h: 2,
  q: 1,
  '8': 0.5,
  '16': 0.25,
  '32': 0.125,
  '64': 0.0625,
}
/**
 * Make a new entry fit its staff/voice the way Finale does: rests it covers are
 * replaced, and a value longer than the room before the next pitched onset is
 * shortened to the largest plain value that fits. Returns a message when the
 * onset lies inside another note or nothing representable fits.
 */
export function fitEntry(
  notes: Note[],
  part: Part,
  staff: string,
  voice: number,
  beat: number,
  duration: number,
): { remove: string[]; shorten: { base: number; dots: number } | null } | string {
  const eps = 1e-6
  const events = notes.filter((n) => {
    const v = metadata(n, part)
    return v.staff === staff && v.voice === voice && !v.grace_to
  })
  const inside = events.find(
    (n) => !n.rest && n.beat < beat - eps && n.beat + n.duration > beat + eps,
  )
  if (inside)
    return `Beat ${Math.round((beat + 1) * 1000) / 1000} lies inside another note in voice ${voice}. Use another voice or shorten that note first.`
  const remove = events
    .filter(
      (n) =>
        n.rest && n.beat < beat + duration - eps && n.beat + n.duration > beat + eps,
    )
    .map((n) => n.id)
  const next = events
    .filter((n) => !remove.includes(n.id) && n.beat > beat + eps)
    .reduce((min, n) => Math.min(min, n.beat), Infinity)
  if (beat + duration <= next + eps) return { remove, shorten: null }
  const fit = durationGlyphs(next - beat)?.[0]
  if (!fit || !glyphBeats[fit.duration])
    return `Nothing fits in the ${Math.round((next - beat) * 1000) / 1000} beats before the next note in voice ${voice}.`
  return { remove, shorten: { base: glyphBeats[fit.duration]!, dots: fit.dots } }
}
export function nextCaretStop(
  stops: number[],
  beat: number,
  direction: 1 | -1,
  fallback: number,
): number {
  const eps = 1e-6
  if (direction > 0) return stops.find((s) => s > beat + eps) ?? beat + fallback
  const previous = [...stops].reverse().find((s) => s < beat - eps)
  return Math.max(0, previous ?? beat - fallback)
}
export const keyNames = [
  'Cb',
  'Gb',
  'Db',
  'Ab',
  'Eb',
  'Bb',
  'F',
  'C',
  'G',
  'D',
  'A',
  'E',
  'B',
  'F#',
  'C#',
]
export const naturalPitches = [0, 2, 4, 5, 7, 9, 11]
export function staves(part: Part): Staff[] {
  return part.staves?.length
    ? part.staves
    : [
        {
          id: `${part.id}-staff`,
          name: part.name,
          clef: part.clef,
          key_signature: null,
          transpose: 0,
        },
      ]
}
export function pitchAt(step: number, alter = 0, transpose = 0) {
  return (
    (Math.floor(step / 7) + 1) * 12 +
    naturalPitches[((step % 7) + 7) % 7]! +
    alter +
    transpose
  )
}
export function keyAlter(step: number, key?: string | null) {
  const fifths = keyNames.indexOf(key || 'C') - 7
  return (fifths >= 0 ? [3, 0, 4, 1, 5, 2, 6] : [6, 2, 5, 1, 4, 0, 3])
    .slice(0, Math.abs(fifths))
    .includes(((step % 7) + 7) % 7)
    ? Math.sign(fifths)
    : 0
}
export function spelling(
  pitch: number,
  staff: Staff,
): { step: number; alter: number } {
  const written = pitch - staff.transpose
  const octave = Math.floor(written / 12) - 1,
    pc = ((written % 12) + 12) % 12
  const exact = naturalPitches.indexOf(pc)
  if (exact >= 0) return { step: octave * 7 + exact, alter: 0 }
  const flat = keyNames.indexOf(staff.key_signature || 'C') < 7
  const index = flat
    ? naturalPitches.findIndex((v) => v > pc)
    : naturalPitches.filter((v) => v < pc).length - 1
  return { step: octave * 7 + index, alter: flat ? -1 : 1 }
}
export function metadata(n: Note, part: Part): NoteNotation {
  if (n.notation) return { ...n.notation }
  const staff = staves(part)[0]!,
    glyph = durationGlyphs(n.duration)
  const exact = glyph?.length === 1 ? glyph[0] : undefined
  return {
    staff: staff.id,
    ...spelling(n.pitch, {...staff,key_signature:staff.key_signature??part.key_signature}),
    voice: 1,
    base: exact ? exact.beats / (2 - 2 ** -exact.dots) : n.duration,
    dots: exact?.dots || 0,
    tuplet_actual: 1,
    tuplet_normal: 1,
  }
}
/** Canonical finite musical fractions, with continued-fraction recovery for imported times. */
export function rational(value: number): RationalTime {
  if (!Number.isFinite(value) || value < 0)
    throw new Error('Musical time must be finite and nonnegative.')
  let x = value,
    a0 = 0,
    a1 = 1,
    b0 = 1,
    b1 = 0
  for (let i = 0; i < 48; i++) {
    const a = Math.floor(x),
      num = a * a1 + a0,
      den = a * b1 + b0
    if (den > 1_000_000_000 || !Number.isSafeInteger(num)) break
    a0 = a1
    a1 = num
    b0 = b1
    b1 = den
    if (Math.abs(num / den - value) < 1e-12 || x === a) break
    x = 1 / (x - a)
  }
  return { numerator: a1, denominator: b1 || 1 }
}
export function atBeat(n: Note, beat: number): Note {
  return {
    ...n,
    beat,
    ...(n.notation
      ? { notation: { ...n.notation, onset: rational(beat) } }
      : {}),
  }
}
export function withNotation(n: Note, v: NoteNotation, part: Part): Note {
  const staff = staves(part).find((s) => s.id === v.staff)!
  const pitch = pitchAt(v.step, v.alter, staff.transpose + (v.octave || 0) * 12)
  if (pitch < 0 || pitch > 127)
    throw new Error('The note would exceed the MIDI pitch range.')
  const duration =
    (v.base * (2 - 2 ** -v.dots) * v.tuplet_normal) / v.tuplet_actual
  return {
    ...n,
    notation: {
      ...v,
      onset: rational(n.beat),
      written_duration: rational(duration),
    },
    pitch,
    duration,
  }
}
export type NoteCommand =
  | { kind: 'duration'; value: number }
  | { kind: 'dots' }
  | { kind: 'pitch'; value: number; octave?: boolean }
  | { kind: 'alter'; value: number }
  | { kind: 'natural' }
  | { kind: 'rest' }
  | { kind: 'tuplet'; actual: number; normal: number }
export function changeNote(n: Note, part: Part, command: NoteCommand): Note {
  const v = metadata(n, part),
    staff = staves(part).find((s) => s.id === v.staff)!
  if (command.kind === 'rest') return { ...n, rest: !n.rest }
  if (command.kind === 'duration') v.base = command.value
  if (command.kind === 'dots') v.dots = (v.dots + 1) % 3
  if (command.kind === 'tuplet') {
    v.tuplet_actual = command.actual
    v.tuplet_normal = command.normal
  }
  if (command.kind === 'pitch') {
    if (n.rest) return n
    v.step += command.value * (command.octave ? 7 : 1)
    if (!command.octave) v.alter = keyAlter(v.step, staff.key_signature??part.key_signature)
  }
  if (command.kind === 'alter') {
    if (n.rest) return n
    v.alter = Math.max(-2, Math.min(2, v.alter + command.value))
  }
  if (command.kind === 'natural') {
    if (n.rest) return n
    v.alter = 0
  }
  return withNotation(n, v, part)
}
export function performanceParts(
  parts: Part[],
  userId: string,
  showAll: boolean,
): Part[] {
  const assigned = parts.filter((p) => p.performer === userId)
  return showAll
    ? [...assigned, ...parts.filter((p) => p.performer !== userId)]
    : assigned
}
export function bottomStep(clef: string) {
  return (
    ({ treble: 30, bass: 18, alto: 24, tenor: 22 } as Record<string, number>)[
      clef
    ] ?? 30
  )
}
export function noteKey(partId: string, noteId: string) {
  return JSON.stringify([partId, noteId])
}

export function scoreMeasures(
  length: number,
  beats: number,
  unit: number,
  changes: { beat: number; beats: number; unit: number }[] = [],
) {
  const result: {
    start: number
    end: number
    beats: number
    unit: number
    number: number
  }[] = []
  let start = 0,
    index = 0
  while (start < length - 1e-8 && result.length < 16384) {
    while (index < changes.length && changes[index]!.beat <= start + 1e-8) {
      beats = changes[index]!.beats
      unit = changes[index]!.unit
      index++
    }
    const end = Math.min(
      length,
      start + (beats * 4) / unit,
      changes[index]?.beat ?? Infinity,
    )
    if (end <= start) break
    result.push({ start, end, beats, unit, number: result.length + 1 })
    start = end
  }
  return result
}

export interface ScoreAnchor {
  beat: number
  x: number
}
export function scoreAnchors(
  parts: Part[],
  length: number,
  scale: number,
  origin: number,
  measures: { start: number; end: number }[],
  changes: number[] = [],
): ScoreAnchor[] {
  const beats = new Set<number>([
    0,
    length,
    ...changes.filter((b) => b >= 0 && b <= length),
  ])
  for (const m of measures) {
    beats.add(m.start)
    beats.add(m.end)
  }
  for (const p of parts)
    for (const n of p.notes) {
      if (n.beat <= length) beats.add(n.beat)
      if (n.beat + n.duration <= length) beats.add(n.beat + n.duration)
    }
  const sorted = [...beats].sort((a, b) => a - b)
  const barEnds = new Set(measures.map(m => m.end))
  let x = origin
  return sorted.map((beat, i) => {
    if (i)
      x +=
        // Barline strokes sit 12px before their beat anchor. Leave additional
        // room for the preceding notehead/flag instead of using onset spacing.
        Math.max(barEnds.has(beat) ? 48 : 24, (beat - sorted[i - 1]!) * scale) +
        (changes.includes(beat) ? 180 : 0)
    return { beat, x }
  })
}
export function scoreX(
  beat: number,
  anchors: ScoreAnchor[],
  scale: number,
  origin = 210,
): number {
  if (!anchors.length) return origin + beat * scale
  if (beat <= anchors[0]!.beat)
    return anchors[0]!.x + (beat - anchors[0]!.beat) * scale
  let lo = 0,
    hi = anchors.length
  while (lo < hi) {
    const mid = (lo + hi) >>> 1
    if (anchors[mid]!.beat <= beat) lo = mid + 1
    else hi = mid
  }
  const a = anchors[Math.max(0, lo - 1)]!,
    b = anchors[lo]
  return b
    ? a.x + ((beat - a.beat) / (b.beat - a.beat)) * (b.x - a.x)
    : a.x + (beat - a.beat) * scale
}
export function scoreBeat(
  x: number,
  anchors: ScoreAnchor[],
  scale: number,
  origin = 210,
): number {
  if (!anchors.length) return (x - origin) / scale
  if (x <= anchors[0]!.x) return anchors[0]!.beat + (x - anchors[0]!.x) / scale
  let lo = 0,
    hi = anchors.length
  while (lo < hi) {
    const mid = (lo + hi) >>> 1
    if (anchors[mid]!.x <= x) lo = mid + 1
    else hi = mid
  }
  const a = anchors[Math.max(0, lo - 1)]!,
    b = anchors[lo]
  return b
    ? a.beat + ((x - a.x) / (b.x - a.x)) * (b.beat - a.beat)
    : a.beat + (x - a.x) / scale
}

export function keyLabel(key: string, mode?: string | null) {
  return mode === 'minor'
    ? `${['Ab', 'Eb', 'Bb', 'F', 'C', 'G', 'D', 'A', 'E', 'B', 'F#', 'C#', 'G#', 'D#', 'A#'][keyNames.indexOf(key)]} minor`
    : `${key} major`
}
