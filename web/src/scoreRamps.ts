import type { AutomationEvent, AutomationLane, Dynamics, Part, Staff } from './types'
import { newId } from './id'
import { staves } from './score'
/** A breakpoint on a ramp line: value at a written beat, linear in between. */
export interface RampNode {
  beat: number
  value: number
  /** Authored outgoing event and identities of ramps ending at this point. Editor-only. */
  event?: AutomationEvent
  ends?: Record<string, number>
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
/** Preserve event identities, holds and interpolation; terminal points are handles, not events. */
export function nodesFromEvents(events: AutomationEvent[]): RampNode[] {
  const byBeat = new Map<number, RampNode>()
  for (const e of events) {
    const old = byBeat.get(e.beat)
    byBeat.set(e.beat, { ...old, beat: e.beat, value: e.duration === 0 ? e.end : e.start, event: { ...e } })
    if (e.duration > 0) {
      const beat = e.beat + e.duration, end = byBeat.get(beat)
      byBeat.set(beat, { beat, value: e.end, ...end, ends: { ...end?.ends, [e.id]: e.end } })
    }
  }
  return [...byBeat.values()].sort((a,b) => a.beat-b.beat)
}
export function eventsFromNodes(nodes: RampNode[], _previous: AutomationEvent[] = []): AutomationEvent[] {
  const sorted = [...nodes].sort((a,b) => a.beat-b.beat)
  return sorted.flatMap((n, i) => {
    if (!n.event && n.ends && Object.keys(n.ends).length) return []
    const next = sorted[i+1]
    if (n.event) {
      const endpoint = sorted.find(p => p.ends?.[n.event!.id] !== undefined)
      const endBeat = endpoint ? Math.max(n.beat, Math.min(endpoint.beat, next?.beat ?? Infinity)) : n.beat
      return [{ ...n.event, beat: n.beat, start: n.event.start + n.value - (n.event.duration === 0 ? n.event.end : n.event.start),
        duration: n.event.duration > 0 ? endBeat-n.beat : 0,
        end: n.event.duration > 0 && endpoint ? (next && next.beat < endpoint.beat ? next.value : endpoint.ends![n.event.id]!) : n.value }]
    }
    return [{ id: newId(), beat: n.beat, duration: next ? next.beat-n.beat : 0,
      start: n.value, end: next?.value ?? n.value, curve: 'linear' as const }]
  })
}
export function curveValue(curve: AutomationEvent['curve'], t: number) {
  t = Math.max(0, Math.min(1, t))
  return curve === 'step' ? (t >= 1 ? 1 : 0) : curve === 'ease_in' ? t*t : curve === 'ease_out' ? 1-(1-t)*(1-t) : curve === 's_curve' ? t*t*(3-2*t) : t
}
/** The same pre-event fallback, interpolation and final hold as the engine. */
export function interpolate(nodes: RampNode[], beat: number, fallback = neutralLevel) {
  const events = eventsFromNodes(nodes)
  return automationValue(events, beat, fallback)
}
export function automationValue(events: AutomationEvent[], beat: number, fallback = neutralLevel) {
  const e = [...events].reverse().find(e => e.beat <= beat)
  if (!e) return fallback
  return e.start + (e.end-e.start) * curveValue(e.curve, e.duration === 0 ? 1 : (beat-e.beat)/e.duration)
}
/** Replace or insert the node at `beat`. */
function withValue(node: RampNode | undefined, beat: number, value: number): RampNode {
  return { ...node, beat, value, ...(node?.ends ? { ends: Object.fromEntries(Object.entries(node.ends).map(([id,end]) => [id, end === node.value ? value : end])) } : {}) }
}
export function setNode(nodes: RampNode[], beat: number, value: number): RampNode[] {
  return [...nodes.filter(n => Math.abs(n.beat-beat) > eps), withValue(nodes.find(n => Math.abs(n.beat-beat) <= eps), beat, value)].sort((a,b) => a.beat-b.beat)
}
export function moveNode(nodes: RampNode[], from: number, beat: number, value?: number): RampNode[] {
  const node = nodes.find(n => Math.abs(n.beat-from) <= eps)
  if (!node) return nodes
  if (Math.abs(from-beat) > eps && nodes.some(n => Math.abs(n.beat-beat) <= eps))
    throw new Error('A ramp point already occupies that beat.')
  return [...removeNode(nodes, from), withValue(node, beat, value ?? node.value)].sort((a,b) => a.beat-b.beat)
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
/** Ramp events for a staff: its own dynamics, else the legacy part-level dynamics. */
export function staffDynamicsEvents(part: Part, staff: Staff): AutomationEvent[] {
  if (staff.dynamics) return staff.dynamics.events
  return part.dynamics?.events || []
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
      ...(list[index]!.dynamics ?? part.dynamics),
      events: eventsFromNodes(nodes, previous),
    }
    const previousIds = new Set(previous.map(e => e.id))
    const curves = list[index]!.curves?.flatMap(c => {
      if (!previousIds.has(`hairpin:${c.id}`)) return [c]
      const event = dynamics.events.find(e => e.id === `hairpin:${c.id}`)
      let retained = c.start_dynamic
      if (retained && (!event || event.beat !== c.start_beat || event.duration === 0)) {
        dynamics.events.push({ ...retained, beat:c.start_beat, duration:0 })
        retained = null
      }
      return event && event.duration > 0 ? [{ ...c, start_dynamic:retained, start_beat:event.beat, end_beat:event.beat+event.duration }] : []
    })
    dynamics.events.sort((a,b)=>a.beat-b.beat)
    list[index] = { ...list[index]!, dynamics, curves }
    // Overrides are staff-local; other staves still inherit the part defaults.
    return { ...part, staves: list }
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
