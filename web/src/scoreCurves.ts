import type { CurveKind, Note, Part, Staff, StaffCurve } from './types'
import { metadata, staves } from './score'
import { newId } from './id'
import {
  interpolate,
  nodesFromEvents,
  staffDynamicsEvents,
} from './scoreRamps'
export const isHairpin = (kind: string) => kind === 'crescendo' || kind === 'decrescendo'
export const defaultHeight = (kind: CurveKind) =>
  kind === 'slur' ? -26 : isHairpin(kind) ? 10 : 12
/** One written dynamic level (ppp→pp→p…) in velocity units. */
export const dynamicStep = 16
export interface CurveEnd {
  note?: string | null
  beat: number
}
export function addCurve(
  part: Part,
  staffId: string,
  kind: CurveKind,
  start: CurveEnd,
  end: CurveEnd,
): Part {
  const list = staves(part).map((s) => ({ ...s }))
  const staff = list.find((s) => s.id === staffId)
  if (!staff) throw new Error('Choose a staff.')
  const [a, b] = start.beat <= end.beat ? [start, end] : [end, start]
  if (b.beat - a.beat < 1e-9 && (a.note ?? null) === (b.note ?? null))
    throw new Error('Drag across at least two notes or some distance.')
  staff.curves = [
    ...(staff.curves || []),
    {
      id: newId(),
      kind,
      start_note: a.note ?? null,
      start_beat: a.beat,
      end_note: b.note ?? null,
      end_beat: b.beat,
      height: defaultHeight(kind),
      lift: 0,
      end_lift: 0,
    },
  ]
  return { ...part, staves: list }
}
export function updateCurve(
  part: Part,
  staffId: string,
  id: string,
  patch: Partial<StaffCurve>,
): Part {
  const list = staves(part).map((s) => ({ ...s }))
  const staff = list.find((s) => s.id === staffId)
  if (!staff) return part
  staff.curves = (staff.curves || []).map((c) => {
    if (c.id !== id) return c
    const next = { ...c, ...patch }
    if (next.end_beat < next.start_beat) {
      ;[next.start_beat, next.end_beat] = [next.end_beat, next.start_beat]
      ;[next.start_note, next.end_note] = [next.end_note ?? null, next.start_note ?? null]
      ;[next.lift, next.end_lift] = [next.end_lift ?? 0, next.lift]
    }
    next.height = Math.max(-200, Math.min(200, next.height))
    next.lift = Math.max(-200, Math.min(200, next.lift))
    next.end_lift = Math.max(-200, Math.min(200, next.end_lift ?? 0))
    return next
  })
  return { ...part, staves: list }
}
/** Notes that vanished (deletion, cut, bar removal) leave curves attached to their beats. */
export function dropCurveAnchors(part: Part, removed: Set<string>): Part {
  if (!removed.size || !part.staves?.some((s) => s.curves?.length)) return part
  return {
    ...part,
    staves: part.staves!.map((s) => ({
      ...s,
      curves: (s.curves || []).map((c) => ({
        ...c,
        start_note: c.start_note && removed.has(c.start_note) ? null : c.start_note,
        end_note: c.end_note && removed.has(c.end_note) ? null : c.end_note,
      })),
    })),
  }
}
/** Current beat of a curve end: its anchor note's onset, else the stored beat. */
export function curveEndBeat(part: Part, end: { note?: string | null; beat: number }) {
  const n = end.note ? part.notes.find((n) => n.id === end.note) : undefined
  return n ? n.beat : end.beat
}
/** Cubic slur between two points; `height` bulges the middle (negative is upward). */
export function slurPath(x1: number, y1: number, x2: number, y2: number, height: number) {
  const dx = x2 - x1
  return `M${x1} ${y1} C${x1 + dx * 0.25} ${y1 + height} ${x2 - dx * 0.25} ${y2 + height} ${x2} ${y2}`
}
/** Horizontal bracket with hooks; `hook` depth is signed (positive hooks downward). */
export function bracketPath(x1: number, x2: number, y: number, hook: number) {
  return `M${x1} ${y + hook} L${x1} ${y} L${x2} ${y} L${x2} ${y + hook}`
}
/** A tie needs adjacent notes of the same pitch in the same staff and voice. */
export function tieTargets(part: Part, from: Note): Note[] {
  const v = metadata(from, part)
  return part.notes.filter((n) => {
    const w = metadata(n, part)
    return (
      n.id !== from.id &&
      !n.rest &&
      n.pitch === from.pitch &&
      w.staff === v.staff &&
      w.voice === v.voice &&
      Math.abs(n.beat - (from.beat + from.duration)) < 1e-6
    )
  })
}
export type { Staff }

/**
 * A hairpin is a wedge on the staff plus a ramp on the staff's velocity line: it
 * starts from the level in force and, unless a dynamic mark already sits at its end,
 * rises or falls one level over its length.
 */
export function addHairpin(
  part: Part,
  staffId: string,
  kind: 'crescendo' | 'decrescendo',
  from: number,
  to: number,
): Part {
  const [a, b] = from <= to ? [from, to] : [to, from]
  if (b - a < 0.25) throw new Error('Drag a hairpin across at least a quarter beat.')
  const next = addCurve(part, staffId, kind, { beat: a }, { beat: b })
  const curve = staves(next).find(s => s.id === staffId)!.curves!.at(-1)!
  const staff = staves(next).find(s => s.id === staffId)!
  const events = staffDynamicsEvents(next, staff)
  if (events.some(e => e.beat < b && e.beat + e.duration > a || e.beat > a && e.beat < b))
    throw new Error('Place the hairpin between existing ramp points without crossing another ramp.')
  const prior = events.find(e => Math.abs(e.beat-a) < 1e-9)
  if (prior) curve.start_dynamic = { id:prior.id,start:prior.start,end:prior.end,curve:prior.curve }
  const nodes = nodesFromEvents(events)
  const startValue = Math.round(interpolate(nodes, a))
  const endValue = events.find(e => Math.abs(e.beat-b) < 1e-9)?.start ??
    Math.max(0, Math.min(127, startValue + (kind === 'crescendo' ? dynamicStep : -dynamicStep)))
  staff.dynamics = { ...(staff.dynamics ?? next.dynamics ?? { mode: 'velocity' as const, controller: 11 }),
    events: [...events.filter(e => Math.abs(e.beat-a) > 1e-9), {
      id: `hairpin:${curve.id}`, beat: a, duration: b-a, start: startValue, end: endValue, curve: 'linear' as const,
    }].sort((x,y) => x.beat-y.beat) }
  return next
}
/** A hairpin owns only its generated event; authored dynamics remain independent. */
export function retimeHairpin(part: Part, staffId: string, curve: StaffCurve, start: number, end: number): Part {
  const [a,b] = start <= end ? [start,end] : [end,start]
  if (a < 0 || b-a < 0.25) throw new Error('A hairpin needs at least a quarter beat.')
  const next = updateCurve(part, staffId, curve.id, { start_beat: a, end_beat: b, start_note: null, end_note: null })
  const staff = staves(next).find(s => s.id === staffId)!
  let events = staffDynamicsEvents(next, staff)
  if (curve.start_dynamic && a !== curve.start_beat) {
    events = [...events, { ...curve.start_dynamic, beat:curve.start_beat, duration:0 }]
    const moved = staff.curves!.find(c => c.id === curve.id)!
    moved.start_dynamic = null
  }
  // Older wedges have no owned event: moving their notation cannot steal authored points.
  const owned = events.find(e => e.id === `hairpin:${curve.id}`)
  if (!owned) return next
  if (events.some(e => e !== owned && (e.beat >= a && e.beat < b || e.beat < a && e.beat+e.duration > a)))
    throw new Error('The hairpin would overlap another dynamic or ramp.')
  staff.dynamics = { ...(staff.dynamics ?? next.dynamics!), events: events.map(e => e === owned ? { ...e, beat: a, duration: b-a } : e).sort((x,y) => x.beat-y.beat) }
  return next
}
/** Wedge for a hairpin: crescendo opens to the right, decrescendo to the left. */
export function hairpinPath(x1: number, x2: number, y: number, opening: number, crescendo: boolean) {
  const h = Math.max(2, opening) / 2
  return crescendo
    ? `M${x2} ${y - h} L${x1} ${y} L${x2} ${y + h}`
    : `M${x1} ${y - h} L${x2} ${y} L${x1} ${y + h}`
}
