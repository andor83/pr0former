import type {
  Project,
  ScoreTimeline,
  AutomationEvent,
  MidiCurve,
  StaffMark,
} from './types'
import {
  scoreMeasures,
  metadata,
  staves,
  withNotation,
  atBeat,
  spelling,
} from './score'
import { newId } from './id'
export function sharedTimeline(p: Project): ScoreTimeline {
  return (
    p.score || {
      version: 1,
      length: Math.max(
        4,
        ...p.parts.map((p) => p.loop_beats),
        ...p.parts.flatMap((p) => p.notes.map((n) => n.beat + n.duration)),
      ),
      loop_score: false,
      meters: [],
      keys: [],
      repeats: [],
    }
  )
}
export function measures(p: Project) {
  return scoreMeasures(
    sharedTimeline(p).length,
    p.beats_per_bar,
    p.beat_unit || 4,
    p.score?.meters,
  )
}
export function setMeter(
  project: Project,
  beat: number,
  beats: number,
  unit: number,
) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  p.score = sharedTimeline(p)
  if (
    !Number.isFinite(beat) ||
    beat < 0 ||
    beat >= p.score.length ||
    !Number.isInteger(beats) ||
    beats < 1 ||
    beats > 16 ||
    ![1, 2, 4, 8, 16, 32].includes(unit)
  )
    throw new Error(
      'Choose a beat inside the score and a valid time signature.',
    )
  p.score.meters = [
    ...p.score.meters.filter((m) => m.beat !== beat),
    { beat, beats, unit },
  ].sort((a, b) => a.beat - b.beat)
  if (beat === 0) {
    p.beats_per_bar = beats
    p.beat_unit = unit
  }
  return p
}
export function setClef(
  project: Project,
  partId: string,
  staffId: string,
  beat: number,
  clef: string,
) {
  const p = JSON.parse(JSON.stringify(project)) as Project,
    part = p.parts.find((p) => p.id === partId)
  if (
    !part ||
    !Number.isFinite(beat) ||
    beat < 0 ||
    beat >= sharedTimeline(p).length ||
    !['treble', 'bass', 'alto', 'tenor'].includes(clef)
  )
    throw new Error(
      'Choose a staff, a beat inside the score, and a valid clef.',
    )
  part.staves = staves(part)
  const staff = part.staves.find((s) => s.id === staffId)
  if (!staff) throw new Error('Select a staff.')
  if (beat === 0) {
    staff.clef = clef
    if (part.staves[0]?.id === staffId) part.clef = clef
    staff.clef_changes = staff.clef_changes?.filter((c) => c.beat !== 0)
  } else
    staff.clef_changes = [
      ...(staff.clef_changes || []).filter((c) => c.beat !== beat),
      { beat, clef },
    ].sort((a, b) => a.beat - b.beat)
  return p
}
type Piece = { beat: number; duration: number; from: number; to: number }
function curveValue(curve: MidiCurve, t: number) {
  return curve === 'step'
    ? t < 1
      ? 0
      : 1
    : curve === 'ease_in'
      ? t * t
      : curve === 'ease_out'
        ? 1 - (1 - t) ** 2
        : curve === 's_curve'
          ? t * t * (3 - 2 * t)
          : t
}
/** One atomic time splice across every part and shared timeline. Positions are quarter beats. */
export function editBars(
  project: Project,
  action: 'insert' | 'append' | 'delete',
  bar: number,
  count: number,
): Project {
  const p = JSON.parse(JSON.stringify(project)) as Project
  p.score = sharedTimeline(p)
  const score = p.score,
    ms = measures(p)
  if (!Number.isInteger(count) || count < 1 || count > 1024)
    throw new Error('Choose 1–1024 bars.')
  if (
    action !== 'append' &&
    (!Number.isInteger(bar) || bar < 1 || bar > ms.length)
  )
    throw new Error('Choose an existing bar.')
  const m = action === 'append' ? ms.at(-1)! : ms[bar - 1]!,
    at = action === 'append' ? score.length : m.start
  const end =
    action === 'delete' ? ms[Math.min(ms.length, bar + count - 1) - 1]!.end : at
  const prior = score.meters.filter((x) => x.beat < at).at(-1) || {
    beats: p.beats_per_bar,
    unit: p.beat_unit || 4,
  }
  const barSize = (m.beats * 4) / m.unit
  // Complete a partial final bar before appending whole blank bars.
  const amount =
    action === 'delete'
      ? at - end
      : count * barSize +
        (action === 'append'
          ? Math.max(0, m.start + barSize - score.length)
          : 0)
  if (action === 'delete' && bar + count - 1 > ms.length)
    throw new Error('The deletion extends beyond the last bar.')
  if (score.length + amount < 0.25)
    throw new Error('Keep at least one bar in the score.')
  if (score.length + amount > 4096)
    throw new Error('The score cannot exceed 4096 quarter beats.')
  const inserting = amount > 0
  const position = (x: number, right = true) =>
    inserting
      ? x > at || (right && x === at)
        ? x + amount
        : x
      : x < at
        ? x
        : x < end
          ? at
          : x + amount
  const pieces = (start: number, duration: number): Piece[] => {
    const stop = start + duration
    if (duration === 0)
      return !inserting && start >= at && start < end
        ? []
        : [{ beat: position(start), duration: 0, from: 0, to: 1 }]
    const ranges = inserting
      ? start < at && stop > at
        ? [
            [start, at],
            [at, stop],
          ]
        : [[start, stop]]
      : [
          [start, Math.min(stop, at)],
          [Math.max(start, end), stop],
        ].filter(([a, b]) => b! > a!)
    return ranges.map(([a, b]) => ({
      beat: position(a!),
      duration: b! - a!,
      from: (a! - start) / duration,
      to: (b! - start) / duration,
    }))
  }
  function events(values: AutomationEvent[]) {
    return values.flatMap((e) =>
      pieces(e.beat, e.duration).map((x, i) => ({
        ...e,
        id: i ? newId() : e.id,
        beat: x.beat,
        duration: x.duration,
        start: Math.round(
          e.start + (e.end - e.start) * curveValue(e.curve, x.from),
        ),
        end: Math.round(
          e.start + (e.end - e.start) * curveValue(e.curve, x.to),
        ),
      })),
    )
  }
  function changes<T extends { beat: number }>(values: T[]): T[] {
    const result = values
      .filter((x) => inserting || x.beat < at || x.beat >= end)
      .map((x) => ({ ...x, beat: position(x.beat) }))
    const last = values.filter((x) => x.beat >= at && x.beat < end).at(-1)
    if (
      !inserting &&
      last &&
      !result.some((x) => x.beat === at) &&
      at < score.length + amount
    )
      result.push({ ...last, beat: at })
    return result.sort((a, b) => a.beat - b.beat)
  }
  for (const part of p.parts) {
    part.staves = staves(part)
    part.notes = part.notes.flatMap((n) => {
      const list = pieces(n.beat, n.duration).map((x, i) => {
        const v = metadata(n, part)
        return Math.abs(x.duration - n.duration) < 1e-8
          ? atBeat(n, x.beat)
          : withNotation(
              { ...n, id: i ? newId() : n.id, beat: x.beat },
              { ...v, base: (v.base * x.duration) / n.duration },
              part,
            )
      })
      if (!inserting && list.length === 2 && !n.rest)
        list[0]!.notation!.tie_to = list[1]!.id
      return list
    })
    for (const n of part.notes)
      if (n.notation)
        for (const field of ['tie_to', 'slur_to', 'grace_to'] as const) {
          const other = part.notes.find((x) => x.id === n.notation![field])
          if (
            !other ||
            other.rest ||
            other.beat < n.beat ||
            other.notation?.grace_to ||
            (field === 'tie_to' &&
              (other.pitch !== n.pitch ||
                Math.abs(other.beat - n.beat - n.duration) > 1e-8)) ||
            (field === 'grace_to' && other.beat !== n.beat)
          )
            n.notation[field] = null
        }
    part.loop_beats = Math.max(0.25, position(part.loop_beats))
    if (part.dynamics) part.dynamics.events = events(part.dynamics.events)
    for (const lane of part.automation || []) lane.events = events(lane.events)
    for (const staff of part.staves) {
      staff.clef_changes = changes(staff.clef_changes || [])
      staff.marks = (staff.marks || [])
        .filter((m) => inserting || m.beat < at || m.beat >= end)
        .map((m) => ({ ...m, beat: position(m.beat) }))
      staff.hidden_rests = (staff.hidden_rests || []).flatMap((r) =>
        pieces(r.beat, r.duration).map((x) => ({
          beat: x.beat,
          duration: x.duration,
          voice: r.voice,
        })),
      )
    }
  }
  score.meters = changes(score.meters)
  score.keys = changes(score.keys)
  score.tempos = changes(score.tempos || [])
  score.barlines = score.barlines
    ?.filter((b) => inserting || b.beat < at || b.beat >= end)
    .map((b) => ({ ...b, beat: position(b.beat) }))
  if (
    inserting &&
    (prior.beats !== m.beats || prior.unit !== m.unit) &&
    !score.meters.some((x) => x.beat === at)
  )
    score.meters.push({ beat: at, beats: m.beats, unit: m.unit })
  score.meters.sort((a, b) => a.beat - b.beat)
  score.meters = score.meters.filter(
    (m, i, all) =>
      i === 0 || m.beats !== all[i - 1]!.beats || m.unit !== all[i - 1]!.unit,
  )
  score.repeats = score.repeats
    .map((r) => ({
      ...r,
      start: position(r.start),
      end: position(r.end, false),
      first_ending: r.first_ending == null ? null : position(r.first_ending),
    }))
    .filter((r) => r.end > r.start)
    .map((r) => ({
      ...r,
      first_ending:
        r.first_ending != null &&
        r.first_ending > r.start &&
        r.first_ending < r.end
          ? r.first_ending
          : null,
    }))
  const n = score.navigation
  if (n) {
    n.at = position(n.at, false)
    n.target = position(n.target)
    if (n.fine != null) n.fine = position(n.fine, false)
    if (n.coda) n.coda = [position(n.coda[0]), position(n.coda[1])]
    if (
      n.at <= n.target ||
      (n.fine != null && n.fine <= n.target) ||
      (n.coda && (n.coda[0] <= n.target || n.coda[1] <= n.coda[0]))
    )
      score.navigation = null
  }
  score.length += amount
  return p
}

/** Meter or key in force at `beat`, including the project defaults before any change. */
export function meterAt(project: Project, beat: number) {
  const change = (project.score?.meters || [])
    .filter((m) => m.beat <= beat + 1e-9)
    .at(-1)
  return change
    ? { beats: change.beats, unit: change.unit }
    : { beats: project.beats_per_bar, unit: project.beat_unit || 4 }
}
export function keyAt(project: Project, beat: number) {
  const change = (project.score?.keys || [])
    .filter((k) => k.beat <= beat + 1e-9)
    .at(-1)
  return change
    ? { key: change.key, mode: change.mode ?? null }
    : { key: 'C', mode: null as 'major' | 'minor' | null }
}
/** Bars whose start lies in [start, end). */
export function barsInRange(project: Project, start: number, end: number) {
  return measures(project).filter(
    (m) => m.start >= start - 1e-9 && m.start < end - 1e-9,
  )
}
/**
 * Finale “Time Signature” with a measure region: the new meter starts at `start`;
 * when `end` is given, the meter in force before the edit resumes at `end`.
 */
export function setMeterRange(
  project: Project,
  start: number,
  end: number | null,
  beats: number,
  unit: number,
) {
  const restore = end != null ? meterAt(project, end - 1e-9) : null
  const p = setMeter(project, start, beats, unit)
  if (
    restore &&
    end! < p.score!.length &&
    (restore.beats !== beats || restore.unit !== unit) &&
    !p.score!.meters.some((m) => m.beat === end)
  )
    p.score!.meters = [
      ...p.score!.meters.filter((m) => m.beat <= start || m.beat >= end!),
      { beat: end!, beats: restore.beats, unit: restore.unit },
    ].sort((a, b) => a.beat - b.beat)
  else if (restore)
    p.score!.meters = p.score!.meters.filter(
      (m) => m.beat <= start || m.beat >= end!,
    )
  return p
}
const fifths = (key: string) =>
  ['Cb', 'Gb', 'Db', 'Ab', 'Eb', 'Bb', 'F', 'C', 'G', 'D', 'A', 'E', 'B', 'F#', 'C#'].indexOf(key) - 7
/** Semitone shift from one key to another, chosen in the requested direction (or nearest). */
export function keyInterval(from: string, to: string, direction: 0 | 1 | -1) {
  const semis = (((fifths(to) - fifths(from)) * 7) % 12 + 12) % 12
  if (direction > 0) return semis === 0 ? 0 : semis
  if (direction < 0) return semis === 0 ? 0 : semis - 12
  return semis > 6 ? semis - 12 : semis
}
/**
 * Finale “Key Signature” with a measure region and “transpose notes” or “hold notes to
 * original pitches”. Region-limited changes restore the previous key at `end`.
 */
export function setKeyRange(
  project: Project,
  start: number,
  end: number | null,
  key: string,
  mode: 'major' | 'minor' | null,
  transpose: 0 | 1 | -1 | 'hold' = 'hold',
) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  p.score = sharedTimeline(p)
  const length = p.score.length
  if (!Number.isFinite(start) || start < 0 || start >= length)
    throw new Error('Choose a bar inside the score.')
  if (fifths(key) < -7 || fifths(key) > 7) throw new Error('Choose a key.')
  const previous = keyAt(p, start),
    restore = end != null ? keyAt(p, end - 1e-9) : null
  p.score.keys = [
    ...p.score.keys.filter(
      (k) => k.beat !== start && (end == null || k.beat <= start || k.beat >= end),
    ),
    { beat: start, key, mode },
  ]
  if (restore && end! < length && !p.score.keys.some((k) => k.beat === end))
    p.score.keys.push({ beat: end!, key: restore.key, mode: restore.mode })
  p.score.keys.sort((a, b) => a.beat - b.beat)
  p.score.keys = p.score.keys.filter(
    (k, i, all) =>
      i === 0 || k.key !== all[i - 1]!.key || (k.mode ?? null) !== (all[i - 1]!.mode ?? null),
  )
  if (start === 0)
    for (const part of p.parts) {
      part.key_signature = key
    }
  const shift =
    transpose === 'hold' ? 0 : keyInterval(previous.key, key, transpose)
  if (shift)
    for (const part of p.parts) {
      part.staves = staves(part)
      part.notes = part.notes.map((n) => {
        if (n.rest || n.beat < start - 1e-9 || (end != null && n.beat >= end - 1e-9))
          return n
        const v = metadata(n, part),
          staff = part.staves!.find((s) => s.id === v.staff)!,
          pitch = n.pitch + shift
        if (pitch < 0 || pitch > 127) return n
        return withNotation(
          n,
          { ...v, ...spelling(pitch - (v.octave || 0) * 12, { ...staff, key_signature: key }), octave: v.octave || 0 },
          part,
        )
      })
    }
  return p
}
/** Finale Repeat tool “Create Simple Repeat” across a bar range; replaces overlapping repeats. */
export function setRepeat(
  project: Project,
  start: number,
  end: number,
  times = 2,
  firstEnding: number | null = null,
) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  p.score = sharedTimeline(p)
  if (!(end > start) || times < 2 || times > 32)
    throw new Error('A repeat needs at least one bar and 2–32 passes.')
  if (firstEnding != null && (firstEnding <= start || firstEnding >= end))
    throw new Error('The first ending must start inside the repeated bars.')
  p.score.repeats = [
    ...p.score.repeats.filter((r) => r.end <= start || r.start >= end),
    { start, end, times, first_ending: firstEnding },
  ].sort((a, b) => a.start - b.start)
  return p
}
export function removeRepeats(project: Project, start: number, end: number) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  if (p.score)
    p.score.repeats = p.score.repeats.filter(
      (r) => r.end <= start || r.start >= end,
    )
  return p
}
/** Special barline at `beat`; `null` restores a normal barline. */
export function setBarline(project: Project, beat: number, style: string | null) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  p.score = sharedTimeline(p)
  p.score.barlines = [
    ...(p.score.barlines || []).filter((b) => b.beat !== beat),
    ...(style ? [{ beat, style }] : []),
  ].sort((a, b) => a.beat - b.beat)
  return p
}
/**
 * Finale text repeats: “D.C. al Fine”, “D.S. al Fine”, “D.S. al Coda”. `at` is the jump
 * (end of the region), `target` the segno (0 for D.C.).
 */
export function setNavigation(
  project: Project,
  navigation: {
    at: number
    target: number
    fine?: number | null
    coda?: [number, number] | null
  } | null,
) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  p.score = sharedTimeline(p)
  p.score.navigation = navigation
    ? {
        at: navigation.at,
        target: navigation.target,
        fine: navigation.fine ?? null,
        coda: navigation.coda ?? null,
      }
    : null
  return p
}
/** Remove every note, hidden rest and automation event of one staff (or all staves) in a range. */
export function clearRange(
  project: Project,
  partId: string,
  staffId: string | null,
  start: number,
  end: number,
  voices?: number[],
) {
  const p = JSON.parse(JSON.stringify(project)) as Project,
    part = p.parts.find((x) => x.id === partId)
  if (!part) return p
  part.staves = staves(part)
  const inside = (beat: number) => beat >= start - 1e-9 && beat < end - 1e-9
  const removed = new Set(
    part.notes
      .filter((n) => {
        const v = metadata(n, part)
        return (
          inside(n.beat) &&
          (!staffId || v.staff === staffId) &&
          (!voices || voices.includes(v.voice))
        )
      })
      .map((n) => n.id),
  )
  part.notes = part.notes.filter((n) => !removed.has(n.id))
  for (const n of part.notes)
    if (n.notation)
      for (const field of ['tie_to', 'slur_to', 'grace_to'] as const)
        if (n.notation[field] && removed.has(n.notation[field]!))
          n.notation[field] = null
  for (const s of part.staves)
    if (!staffId || s.id === staffId)
      s.hidden_rests = (s.hidden_rests || []).filter((r) => !inside(r.beat))
  return p
}

/** Tempo in force at a written position: the latest map entry, else the project tempo. */
export function tempoAt(project: Project, beat: number) {
  return (
    (project.score?.tempos || []).filter((t) => t.beat <= beat + 1e-9).at(-1)
      ?.bpm ?? project.bpm
  )
}
export function setTempo(project: Project, beat: number, bpm: number) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  p.score = sharedTimeline(p)
  if (!Number.isFinite(beat) || beat < 0 || beat >= p.score.length)
    throw new Error('Choose a position inside the score.')
  if (!Number.isFinite(bpm) || bpm < 1 || bpm > 400)
    throw new Error('Tempo must be 1–400 quarter notes per minute.')
  p.score.tempos = [
    ...(p.score.tempos || []).filter((t) => t.beat !== beat),
    { beat, bpm },
  ].sort((a, b) => a.beat - b.beat)
  if (beat === 0) p.bpm = bpm
  return p
}
export function removeTempo(project: Project, beat: number) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  if (p.score) p.score.tempos = (p.score.tempos || []).filter((t) => t.beat !== beat)
  return p
}
export function addMark(
  project: Project,
  partId: string,
  staffId: string,
  mark: Omit<StaffMark, 'id'> & { id?: string },
) {
  const p = JSON.parse(JSON.stringify(project)) as Project,
    part = p.parts.find((x) => x.id === partId)
  if (!part) throw new Error('Choose a part.')
  part.staves = staves(part)
  const staff = part.staves.find((s) => s.id === staffId)
  if (!staff) throw new Error('Choose a staff.')
  const text = mark.text.trim()
  if (!text || text.length > 256) throw new Error('Enter up to 256 characters.')
  if (!Number.isFinite(mark.beat) || mark.beat < 0)
    throw new Error('Choose a position inside the score.')
  const id = mark.id ?? newId()
  staff.marks = [
    ...(staff.marks || []).filter((m) => m.id !== id),
    { id, beat: mark.beat, kind: mark.kind, text },
  ].sort((a, b) => a.beat - b.beat)
  return p
}
export function removeMark(project: Project, partId: string, staffId: string, id: string) {
  const p = JSON.parse(JSON.stringify(project)) as Project,
    part = p.parts.find((x) => x.id === partId)
  if (!part) return p
  part.staves = staves(part)
  const staff = part.staves.find((s) => s.id === staffId)
  if (staff) staff.marks = (staff.marks || []).filter((m) => m.id !== id)
  return p
}

/**
 * Finale “Create Forward Repeat Bar”: 𝄆 at the start of the bar containing `beat`.
 * With no matching end yet, the repeat runs to the end of the score (or to the next
 * repeat); an end placed later shortens it. Inside an existing repeat, it moves that
 * repeat's start.
 */
export function placeRepeatBegin(project: Project, beat: number) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  p.score = sharedTimeline(p)
  const bar = measures(p).find((m) => m.start <= beat + 1e-9 && m.end > beat + 1e-9)
  if (!bar) throw new Error('Click inside a bar.')
  const start = bar.start,
    inside = p.score.repeats.find((r) => r.start < start && r.end > start)
  if (inside) {
    inside.start = start
    if (inside.first_ending != null && inside.first_ending <= start) inside.first_ending = null
    return p
  }
  if (p.score.repeats.some((r) => r.start === start)) return p
  const next = p.score.repeats.filter((r) => r.start > start).sort((a, b) => a.start - b.start)[0]
  const end = next ? next.start : p.score.length
  if (!(end > start)) throw new Error('There is no room for a repeat here.')
  p.score.repeats = [...p.score.repeats, { start, end, times: 2, first_ending: null }].sort(
    (a, b) => a.start - b.start,
  )
  return p
}
/**
 * Finale “Create Backward Repeat Bar”: 𝄇 at the end of the bar containing `beat`.
 * Closes an open repeat that spans this bar; otherwise repeats from the top of the
 * score, or from the end of the previous repeat when one exists (repeats cannot nest).
 */
export function placeRepeatEnd(project: Project, beat: number, times = 2) {
  const p = JSON.parse(JSON.stringify(project)) as Project
  p.score = sharedTimeline(p)
  const bar = measures(p).find((m) => m.start <= beat + 1e-9 && m.end > beat + 1e-9)
  if (!bar) throw new Error('Click inside a bar.')
  const end = bar.end,
    spanning = p.score.repeats.find((r) => r.start < end && r.end > end)
  if (spanning) {
    spanning.end = end
    if (spanning.first_ending != null && spanning.first_ending >= end) spanning.first_ending = null
    return p
  }
  if (p.score.repeats.some((r) => r.end === end)) return p
  const previous = p.score.repeats.filter((r) => r.end <= end).sort((a, b) => b.end - a.end)[0]
  const start = previous ? previous.end : 0
  if (!(end > start)) throw new Error('There is no room for a repeat here.')
  p.score.repeats = [...p.score.repeats, { start, end, times, first_ending: null }].sort(
    (a, b) => a.start - b.start,
  )
  return p
}
