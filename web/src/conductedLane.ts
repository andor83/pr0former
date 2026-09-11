import type { Part, PartPlayback, Project, Staff } from './types'
import { staves } from './score'

export interface ConductedLaneHistory {
  part: string
  start: number
}

interface Segment {
  key: string
  part: Part
  engineStart: number
  engineEnd: number
  displayStart: number
  displayEnd: number
  future: boolean
}

const initialMeter = (project: Project, part: Part) =>
  part.performance_meters?.find(meter => meter.beat === 0) || {
    beat: 0,
    beats: project.beats_per_bar,
    unit: project.beat_unit || 4,
  }

export function localToEngine(project: Project, part: Part, beat: number) {
  const pulse = 4 / (project.conducted?.pulse_unit || 4)
  const meters = [...(part.performance_meters || [])]
  if (!meters.some(meter => meter.beat === 0)) meters.unshift(initialMeter(project, part))
  meters.sort((a, b) => a.beat - b.beat)
  let elapsed = 0
  for (let index = 0; index < meters.length; index += 1) {
    const meter = meters[index]!
    const end = Math.min(beat, meters[index + 1]?.beat ?? beat)
    if (end > meter.beat) elapsed += (end - meter.beat) * pulse / (4 / meter.unit)
    if (beat < (meters[index + 1]?.beat ?? Infinity)) break
  }
  return elapsed
}

export function engineToLocal(project: Project, part: Part, elapsed: number) {
  const pulse = 4 / (project.conducted?.pulse_unit || 4)
  const meters = [...(part.performance_meters || [])]
  if (!meters.some(meter => meter.beat === 0)) meters.unshift(initialMeter(project, part))
  meters.sort((a, b) => a.beat - b.beat)
  let engineStart = 0
  for (let index = 0; index < meters.length; index += 1) {
    const meter = meters[index]!
    const scale = pulse / (4 / meter.unit)
    const next = meters[index + 1]
    const engineEnd = next ? engineStart + (next.beat - meter.beat) * scale : Infinity
    if (elapsed < engineEnd) return meter.beat + (elapsed - engineStart) / scale
    engineStart = engineEnd
  }
  return 0
}

function shiftedStaff(source: Staff | undefined, index: number): Staff {
  return {
    id: `live-staff-${index}`,
    name: source?.name || (index ? `Staff ${index + 1}` : 'Live staff'),
    clef: source?.clef || 'treble',
    transpose: source?.transpose || 0,
    key_signature: null,
    clef_changes: [],
    hidden_rests: [],
    marks: [],
    curves: [],
    dynamics: { mode: 'velocity', controller: 11, events: [] },
  }
}

/** Build one persistent display timeline from engine-timed current and queued parts. */
export function buildConductedLane(
  project: Project,
  userId: string,
  playback: PartPlayback[],
  globalBeat: number,
  history: ConductedLaneHistory[],
) {
  const assigned = project.parts.filter(part => part.performer === userId)
  const byId = new Map(assigned.map(part => [part.id, part]))
  const states = new Map(playback.map(state => [state.id, state]))
  const starts = new Map<string, { part: Part; start: number; future: boolean }>()
  for (const entry of history) {
    const part = byId.get(entry.part)
    if (part) starts.set(`${part.id}:${entry.start}`, { part, start: entry.start, future: false })
  }
  for (const part of assigned) {
    const state = states.get(part.id)
    const start = state?.playing
      ? state.start
      : state?.pending?.[1]
        ? state.pending[0]
        : state?.scheduled_start
    if (start != null) starts.set(`${part.id}:${start}`, { part, start, future: start > globalBeat })
  }

  const ordered = [...starts.values()].sort((a, b) => a.start - b.start)
  const expanded: { part: Part; start: number; future: boolean }[] = []
  for (const entry of ordered) {
    const state = states.get(entry.part.id)
    const duration = localToEngine(project, entry.part, entry.part.loop_beats)
    const nextStart = ordered.find(other => other.start > entry.start)?.start
    const repeatEnd = state?.pending?.[1] === false ? state.pending[0] : undefined
    const horizon = nextStart ?? repeatEnd ?? globalBeat + 16 * 4 / (project.conducted?.pulse_unit || 4)
    const cycles = state?.repeating || repeatEnd != null && repeatEnd > entry.start + duration
      ? Math.max(1, Math.ceil((horizon - entry.start) / duration))
      : 1
    for (let cycle = 0; cycle < cycles; cycle += 1) {
      const start = entry.start + cycle * duration
      if (nextStart != null && start >= nextStart - 1e-9) break
      expanded.push({ part: entry.part, start, future: entry.future && cycle === 0 })
    }
  }

  const segments: Segment[] = []
  for (const entry of expanded.sort((a, b) => a.start - b.start)) {
    const duration = localToEngine(project, entry.part, entry.part.loop_beats)
    const previous = segments.at(-1)
    const displayStart = previous
      ? previous.displayEnd + Math.max(0, entry.start - previous.engineEnd)
      : entry.start
    segments.push({
      key: `${entry.part.id}:${entry.start}`,
      part: entry.part,
      engineStart: entry.start,
      engineEnd: entry.start + duration,
      displayStart,
      displayEnd: displayStart + entry.part.loop_beats,
      future: entry.future,
    })
  }

  const staffCount = Math.max(1, ...segments.map(segment => staves(segment.part).length))
  const laneStaves = Array.from({ length: staffCount }, (_, index) =>
    shiftedStaff(staves(segments[0]?.part || assigned[0] || project.parts[0]!)[index], index),
  )
  const notes = segments.flatMap(segment => segment.part.notes.map(note => {
    const sourceStaves = staves(segment.part)
    const sourceStaff = note.notation?.staff
    const staffIndex = Math.max(0, sourceStaves.findIndex(staff => staff.id === sourceStaff))
    const prefix = `${segment.key}:`
    return {
      ...note,
      id: prefix + note.id,
      beat: segment.displayStart + note.beat,
      notation: note.notation ? {
        ...note.notation,
        staff: laneStaves[staffIndex]?.id || laneStaves[0]!.id,
        tie_to: note.notation.tie_to ? prefix + note.notation.tie_to : null,
        slur_to: note.notation.slur_to ? prefix + note.notation.slur_to : null,
        grace_to: note.notation.grace_to ? prefix + note.notation.grace_to : null,
      } : undefined,
    }
  }))
  for (const segment of segments) {
    const source = staves(segment.part)
    laneStaves.forEach((staff, index) => {
      const original = source[index]
      if (!original) return
      if (segment.displayStart > 0) staff.clef_changes!.push({ beat: segment.displayStart, clef: original.clef })
      staff.clef_changes!.push(...(original.clef_changes || []).map(change => ({ ...change, beat: segment.displayStart + change.beat })))
      staff.marks!.push(...(original.marks || []).map(mark => ({ ...mark, id: `${segment.key}:${mark.id}`, beat: segment.displayStart + mark.beat })))
      staff.dynamics!.events.push(...(original.dynamics?.events || segment.part.dynamics?.events || []).map(event => ({ ...event, id: `${segment.key}:${event.id}`, beat: segment.displayStart + event.beat })))
    })
  }
  const meters = segments.flatMap(segment => {
    const meters = [...(segment.part.performance_meters || [])]
    if (!meters.some(meter => meter.beat === 0)) meters.unshift(initialMeter(project, segment.part))
    return meters.map(meter => ({ ...meter, beat: segment.displayStart + meter.beat }))
  })
  if (!meters.some(meter => meter.beat === 0)) meters.unshift({ beat: 0, beats: project.beats_per_bar, unit: project.beat_unit || 4 })
  meters.sort((a, b) => a.beat - b.beat)

  const active = [...segments].reverse().find(segment => globalBeat >= segment.engineStart - 1e-9 && globalBeat < segment.engineEnd - 1e-9)
  const previous = [...segments].reverse().find(segment => segment.engineEnd <= globalBeat)
  const next = segments.find(segment => segment.engineStart > globalBeat)
  const displayBeat = active
    ? active.displayStart + engineToLocal(project, active.part, globalBeat - active.engineStart)
    : previous
      ? previous.displayEnd + Math.max(0, globalBeat - previous.engineEnd)
      : globalBeat
  const length = Math.max(displayBeat + 16, next?.displayEnd || 0, segments.at(-1)?.displayEnd || 0, 16)
  const base = segments.find(segment => !segment.future)?.part || next?.part || assigned[0] || project.parts[0]
  const lanePart: Part | undefined = base && {
    ...base,
    id: `conducted-live-${userId}`,
    name: 'Live part',
    performer: userId,
    notes,
    staves: laneStaves,
    loop_beats: length,
    performance_meters: meters,
    dynamics: null,
    automation: [],
  }
  const laneProject: Project = {
    ...project,
    id: `${project.id}-conducted-live`,
    parts: lanePart ? [lanePart] : [],
    score: {
      version: 1,
      length,
      loop_score: false,
      meters,
      keys: [],
      repeats: [],
      tempos: [],
    },
  }
  return {
    project: laneProject,
    beat: displayBeat,
    partId: lanePart?.id || '',
    upcoming: next?.part.name || null,
    segmentKeys: segments.map(segment => segment.key),
  }
}
