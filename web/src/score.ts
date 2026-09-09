import type { Note, NoteNotation, Part, Staff, RationalTime } from './types'
import { durationGlyphs } from './notation'
export const durationKeys = [0.125, 0.25, 0.5, 1, 2, 4, 8, 0.0625]
export const durationLabels = [
  '32nd',
  '16th',
  'Eighth',
  'Quarter',
  'Half',
  'Whole',
  'Double whole',
  '64th',
]
export const durationSymbols = ['𝅘𝅥𝅰', '𝅘𝅥𝅯', '♪', '♩', '𝅗𝅥', '𝅝', '𝅜', '𝅘𝅥𝅱']
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
  let x = origin
  return sorted.map((beat, i) => {
    if (i)
      x +=
        Math.max(24, (beat - sorted[i - 1]!) * scale) +
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
