import type { CurveKind, Note, Part, Staff, StaffCurve } from './types'
import { metadata, staves } from './score'
import { newId } from './id'
export const defaultHeight = (kind: CurveKind) => (kind === 'slur' ? -26 : 12)
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
    }
    next.height = Math.max(-200, Math.min(200, next.height))
    next.lift = Math.max(-200, Math.min(200, next.lift))
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
