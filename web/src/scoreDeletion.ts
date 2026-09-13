import type { Note, Project } from './types'
import { atBeat, metadata, staves } from './score'
import { measures } from './scoreBars'
import { dropCurveAnchors } from './scoreCurves'

const epsilon = 1e-8
type Span = { start: number; end: number }
function merge(spans: Span[]): Span[] {
  const result: Span[] = []
  for (const span of spans.sort((a, b) => a.start - b.start)) {
    const last = result.at(-1)
    if (last && span.start <= last.end + epsilon) last.end = Math.max(last.end, span.end)
    else if (span.end > span.start + epsilon) result.push({ ...span })
  }
  return result
}

/** Delete rhythmic entries and close only the newly freed time in their bar/staff/voice.
 * Surviving chord tones keep their occupied time; every cut uses the original positions.
 * Mutates the caller's score draft, which is saved and undone as a single edit.
 */
export function deleteScoreNotes(project: Project, partId: string, ids: Set<string>, automaticRest?: Note) {
  const part = project.parts.find(p => p.id === partId)
  if (!part) return
  const removed = new Set(ids)
  // Grace notes belong to their principal and occupy no independent rhythmic slot.
  for (const n of part.notes) if (removed.has(metadata(n, part).grace_to ?? '')) removed.add(n.id)
  const victims = part.notes.filter(n => removed.has(n.id))
  if (automaticRest) victims.push(automaticRest)
  if (!victims.length) return
  const bars = measures(project)
  const groups = new Map<string, { staff: string; voice: number; bar: Span; cuts: Span[] }>()
  for (const n of victims) {
    const v = metadata(n, part)
    if (v.grace_to) continue
    const bar = bars.find(b => n.beat >= b.start - epsilon && n.beat < b.end - epsilon)
    if (!bar) continue
    const key = JSON.stringify([v.staff, v.voice, bar.start])
    let group = groups.get(key)
    if (!group) {
      group = { staff: v.staff, voice: v.voice, bar, cuts: [] }
      groups.set(key, group)
    }
    group.cuts.push({ start: n.beat, end: Math.min(bar.end, n.beat + n.duration) })
  }
  const survivors = part.notes.filter(n => !removed.has(n.id))
  for (const group of groups.values()) {
    let cuts = merge(group.cuts)
    for (const n of survivors) {
      const v = metadata(n, part)
      if (v.staff !== group.staff || v.voice !== group.voice || v.grace_to) continue
      const start = n.beat, end = n.beat + n.duration
      cuts = cuts.flatMap(c => end <= c.start || start >= c.end ? [c] : [
        { start: c.start, end: Math.min(c.end, start) },
        { start: Math.max(c.start, end), end: c.end },
      ].filter(c => c.end > c.start + epsilon))
    }
    group.cuts = cuts
  }
  function position(beat: number, staff: string, voice: number) {
    const group = [...groups.values()].find(g => g.staff === staff && g.voice === voice && beat >= g.bar.start - epsilon && beat < g.bar.end - epsilon)
    return group ? beat - group.cuts.reduce((sum, c) => sum + Math.max(0, Math.min(beat, c.end) - c.start), 0) : beat
  }
  part.notes = survivors.map(n => {
    const v = metadata(n, part)
    return atBeat(n, position(n.beat, v.staff, v.voice))
  })
  // Rest visibility ranges use the same cut, split at barlines so later bars stay fixed.
  part.staves = staves(part).map(staff => ({ ...staff, hidden_rests: (staff.hidden_rests ?? []).flatMap(rest => {
    let pieces = [{ start: rest.beat, end: rest.beat + rest.duration }]
    for (const group of groups.values()) {
      if (group.staff !== staff.id || group.voice !== rest.voice) continue
      pieces = pieces.flatMap(piece => {
        const a = Math.max(piece.start, group.bar.start), b = Math.min(piece.end, group.bar.end)
        if (b <= a) return [piece]
        const map = (beat: number) => beat - group.cuts.reduce((sum, c) => sum + Math.max(0, Math.min(beat, c.end) - c.start), 0)
        return [
          { start: piece.start, end: a },
          { start: map(a), end: map(b) },
          { start: b, end: piece.end },
        ].filter(p => p.end > p.start + epsilon)
      })
    }
    return pieces.map(p => ({ beat: p.start, duration: p.end - p.start, voice: rest.voice }))
  }) }))
  const remaining = new Map(part.notes.map(n => [n.id, n]))
  for (const n of part.notes) {
    const principal = remaining.get(n.notation?.grace_to ?? '')
    if (principal) Object.assign(n, atBeat(n, principal.beat))
  }
  for (const n of part.notes) {
    if (!n.notation) continue
    for (const field of ['tie_to', 'slur_to', 'grace_to'] as const) {
      const target = remaining.get(n.notation[field] ?? '')
      if (!target || target.beat < n.beat - epsilon) n.notation[field] = null
      else if (field === 'tie_to' && Math.abs(n.beat + n.duration - target.beat) > epsilon) n.notation.tie_to = null
    }
  }
  Object.assign(part, dropCurveAnchors(part, removed))
}
