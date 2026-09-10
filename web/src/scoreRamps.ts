import type { AutomationEvent, AutomationLane, Dynamics, Part, Staff } from './types'
import { newId } from './id'
import { staves } from './score'
/** A breakpoint on a ramp line: value at a written beat, linear in between. */
export interface RampNode {
  beat: number
  value: number
}
export const dynamicLevels: [string, number][] = [
  ['ppp', 16],
  ['pp', 32],
  ['p', 48],
  ['mp', 64],
  ['mf', 80],
  ['f', 96],
  ['ff', 112],
  ['fff', 127],
]
/** The velocity multiplier is level / 90, so 90 is the neutral level with no marks. */
export const neutralLevel = 90
export function dynamicName(value: number) {
  return dynamicLevels.find(([, v]) => v === value)?.[0]
}
const eps = 1e-9
/** Breakpoints implied by ramp events: each event contributes its start and (if it ramps) its end. */
export function nodesFromEvents(events: AutomationEvent[]): RampNode[] {
  const byBeat = new Map<number, number>()
  for (const e of [...events].sort((a, b) => a.beat - b.beat)) {
    byBeat.set(e.beat, e.start)
    if (e.duration > 0) byBeat.set(e.beat + e.duration, e.end)
  }
  return [...byBeat.entries()]
    .map(([beat, value]) => ({ beat, value }))
    .sort((a, b) => a.beat - b.beat)
}
/** Linear ramps between consecutive breakpoints; the last one holds its value. */
export function eventsFromNodes(
  nodes: RampNode[],
  previous: AutomationEvent[] = [],
): AutomationEvent[] {
  const sorted = [...nodes].sort((a, b) => a.beat - b.beat)
  return sorted.map((n, i) => {
    const next = sorted[i + 1]
    return {
      id: previous[i]?.id ?? newId(),
      beat: n.beat,
      duration: next ? Math.max(0, next.beat - n.beat) : 0,
      start: n.value,
      end: next ? next.value : n.value,
      curve: 'linear' as const,
    }
  })
}
/** Value the line has at `beat` (what a note triggered there would receive). */
export function interpolate(nodes: RampNode[], beat: number, fallback = neutralLevel) {
  const sorted = [...nodes].sort((a, b) => a.beat - b.beat)
  if (!sorted.length) return fallback
  if (beat <= sorted[0]!.beat + eps) return sorted[0]!.value
  const last = sorted.at(-1)!
  if (beat >= last.beat - eps) return last.value
  for (let i = 0; i < sorted.length - 1; i++) {
    const a = sorted[i]!,
      b = sorted[i + 1]!
    if (beat >= a.beat - eps && beat <= b.beat + eps) {
      const t = b.beat === a.beat ? 1 : (beat - a.beat) / (b.beat - a.beat)
      return a.value + (b.value - a.value) * t
    }
  }
  return last.value
}
/** Replace or insert the node at `beat`. */
export function setNode(nodes: RampNode[], beat: number, value: number): RampNode[] {
  return [...nodes.filter((n) => Math.abs(n.beat - beat) > eps), { beat, value }].sort(
    (a, b) => a.beat - b.beat,
  )
}
export function removeNode(nodes: RampNode[], beat: number): RampNode[] {
  return nodes.filter((n) => Math.abs(n.beat - beat) > eps)
}
/**
 * A written dynamic: a node at the mark's level preceded by a rapid ramp from the
 * value the line had just before, so earlier music keeps its level.
 */
export function applyDynamicMark(
  nodes: RampNode[],
  beat: number,
  value: number,
  ramp = 0.125,
): RampNode[] {
  const from = Math.max(0, beat - ramp)
  let next = nodes
  if (from < beat - eps && !nodes.some((n) => n.beat > from - eps && n.beat < beat + eps))
    next = setNode(next, from, Math.round(interpolate(nodes, from)))
  return setNode(next, beat, value)
}
export function laneMax(lane: AutomationLane) {
  return lane.message === 'bend' ? 16383 : 127
}
export function laneLabel(lane: AutomationLane) {
  const kind =
    lane.message === 'cc'
      ? `CC${lane.number}`
      : lane.message === 'bend'
        ? 'Bend'
        : lane.message === 'pressure'
          ? 'Pressure'
          : lane.message === 'poly_pressure'
            ? `Poly pressure ${lane.number}`
            : lane.message === 'program'
              ? 'Program'
              : lane.message
  return `${lane.name} · ${kind}`
}
export const velocityLine = (staffId: string) => `velocity:${staffId}`
/** Ramp events for a staff: its own dynamics, else the legacy part-level dynamics on the first staff. */
export function staffDynamicsEvents(part: Part, staff: Staff): AutomationEvent[] {
  if (staff.dynamics) return staff.dynamics.events
  return staves(part)[0]?.id === staff.id ? part.dynamics?.events || [] : []
}
/** Lanes drawn as ramps; program changes are steps, notes are edited in the score itself. */
export const rampMessages = ['cc', 'bend', 'pressure', 'poly_pressure', 'program'] as const
/** Program lanes hold values between points instead of ramping. */
export function eventsForLane(lane: AutomationLane, nodes: RampNode[]): AutomationEvent[] {
  const events = eventsFromNodes(nodes, lane.events)
  return lane.message === 'program'
    ? events.map((e) => ({ ...e, duration: 0, end: e.start }))
    : events
}
/** Write a node list back into a staff's velocity dynamics or a continuous lane. */
export function writeRamp(part: Part, lineId: string, nodes: RampNode[]): Part {
  if (lineId.startsWith('velocity:')) {
    const staffId = lineId.slice('velocity:'.length),
      list = staves(part).map((s) => ({ ...s }))
    const index = list.findIndex((s) => s.id === staffId)
    if (index < 0) return part
    const previous = staffDynamicsEvents(part, list[index]!)
    const dynamics: Dynamics = {
      mode: 'velocity',
      controller: 11,
      ...(list[index]!.dynamics ?? (index === 0 ? part.dynamics : null)),
      events: eventsFromNodes(nodes, previous),
    }
    list[index] = { ...list[index]!, dynamics }
    // The first staff takes over the legacy part-level dynamics so they are not applied twice.
    return { ...part, staves: list, dynamics: index === 0 ? null : part.dynamics }
  }
  return {
    ...part,
    automation: (part.automation || []).map((l) =>
      l.id === lineId ? { ...l, events: eventsForLane(l, nodes) } : l,
    ),
  }
}
export type LaneSettings = Pick<AutomationLane, 'name' | 'channel' | 'message' | 'number'>
export function addLane(part: Part, settings: LaneSettings): Part {
  const lane: AutomationLane = {
    id: newId(),
    events: [],
    initial: null,
    ...settings,
    name: settings.name.trim() || 'Lane',
  }
  return { ...part, automation: [...(part.automation || []), lane] }
}
export function updateLane(part: Part, id: string, settings: Partial<LaneSettings>): Part {
  return {
    ...part,
    automation: (part.automation || []).map((l) => (l.id === id ? { ...l, ...settings } : l)),
  }
}
export function removeLane(part: Part, id: string): Part {
  return { ...part, automation: (part.automation || []).filter((l) => l.id !== id) }
}
