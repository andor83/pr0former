import { isHairpin, retimeHairpin } from './scoreCurves'
import type { Note, Project } from './types'
import { atBeat, metadata, staves, withNotation } from './score'
import { newId } from './id'
import { deleteScoreNotes } from './scoreDeletion'
import { measures } from './scoreBars'
import {
  moveNode,
  nodesFromEvents,
  removeNode,
  setNode,
  staffDynamicsEvents,
  velocityLine,
  writeRamp,
} from './scoreRamps'
export interface ScoreElement {
  kind: string
  part: string
  staff: string
  note?: string
  notes?: string[]
  beat?: number
  index?: number
  rest?: Note
  /** Staff mark id for kind 'mark'. */
  mark?: string
  /** Phrasing curve id for kind 'curve'. */
  curve?: string
}
export const elementKey = (e: ScoreElement) => JSON.stringify(e)
export function hideRest(project: Project, element: ScoreElement) {
  const p = project.parts.find((p) => p.id === element.part)!,
    s = staves(p).map((s) => ({ ...s }))
  p.staves = s
  const staff = s.find((s) => s.id === element.staff)!,
    n = element.rest!
  staff.hidden_rests = [
    ...(staff.hidden_rests || []),
    { beat: n.beat, duration: n.duration, voice: metadata(n, p).voice },
  ]
}
export function materializeRest(project: Project, element: ScoreElement) {
  hideRest(project, element)
  const p = project.parts.find((p) => p.id === element.part)!,
    n = { ...element.rest!, id: newId() }
  p.notes.push(withNotation(atBeat(n, n.beat), metadata(n, p), p))
  return n
}
export function deleteElement(
  project: Project,
  e: ScoreElement,
): string | undefined {
  const p = project.parts.find((p) => p.id === e.part)
  if (!p) return
  if (e.kind === 'tempo' && e.beat !== undefined) {
    if (project.score)
      project.score.tempos = (project.score.tempos || []).filter(
        (t) => t.beat !== e.beat,
      )
    return
  }
  if (e.kind === 'mark' && e.mark) {
    p.staves = staves(p)
    const s = p.staves.find((s) => s.id === e.staff)
    if (s) s.marks = (s.marks || []).filter((m) => m.id !== e.mark)
    return
  }
  if (e.kind === 'curve' && e.curve) {
    p.staves = staves(p)
    const s = p.staves.find((s) => s.id === e.staff)
    if (s) {
      const curve = s.curves?.find(c => c.id === e.curve)
      s.curves = (s.curves || []).filter(c => c.id !== e.curve)
      if (s.dynamics) s.dynamics = { ...s.dynamics, events: [...s.dynamics.events.filter(event => event.id !== `hairpin:${e.curve}`), ...(curve?.start_dynamic ? [{...curve.start_dynamic,beat:curve.start_beat,duration:0}] : [])].sort((a,b)=>a.beat-b.beat) }
    }
    return
  }
  if (e.kind === 'dynamic' && e.beat !== undefined) {
    const s = staves(p).find((s) => s.id === e.staff)
    if (s)
      Object.assign(
        p,
        writeRamp(
          p,
          velocityLine(s.id),
          removeNode(nodesFromEvents(staffDynamicsEvents(p, s)), e.beat),
        ),
      )
    return
  }
  const n = p.notes.find((n) => n.id === e.note)
  if (e.kind === 'rest' && e.rest) {
    const rest = e.rest, voice = metadata(rest, p).voice
    const bar = measures(project).find(b => rest.beat >= b.start && rest.beat < b.end)
    const following = p.notes.some(n => n.beat >= rest.beat + rest.duration - 1e-8 && n.beat < (bar?.end ?? 0) && metadata(n, p).staff === e.staff && metadata(n, p).voice === voice)
    if (following) deleteScoreNotes(project, p.id, new Set(), rest)
    else hideRest(project, e)
    return
  }
  if (e.kind === 'staff') {
    if (staves(p).length === 1)
      return 'A part needs at least one staff. Edit its clef or other settings instead.'
    p.notes = p.notes.filter((n) => metadata(n, p).staff !== e.staff)
    p.staves = staves(p).filter((s) => s.id !== e.staff)
    for (const n of p.notes)
      if (n.notation)
        for (const field of ['tie_to', 'slur_to', 'grace_to'] as const)
          if (!p.notes.some((other) => other.id === n.notation![field]))
            n.notation[field] = null
    return
  }
  if (n) {
    const v = metadata(n, p)
    p.staves = staves(p)
    if (e.kind === 'articulation') v.articulation = null
    else if (e.kind === 'tie') v.tie_to = null
    else if (e.kind === 'slur') v.slur_to = null
    else if (e.kind === 'grace') v.grace_to = null
    else if (e.kind === 'octave') v.octave = 0
    else if (e.kind === 'accidental') v.alter = 0
    else if (e.kind === 'dot') v.dots = Math.max(0, v.dots - 1)
    else return 'Use note settings to edit this rhythmic grouping.'
    p.notes = p.notes.map((note) =>
      note.id === n.id ? withNotation(note, v, p) : note,
    )
    return
  }
  if (e.kind === 'special-barline') project.score?.barlines?.splice(e.index!, 1)
  else if (e.kind === 'repeat') project.score?.repeats.splice(e.index!, 1)
  else if (e.kind === 'navigation' && project.score)
    project.score.navigation = null
  else if (e.kind === 'meter' && project.score && e.beat !== undefined) {
    project.score.meters = project.score.meters.filter((m) => m.beat !== e.beat)
  } else if (e.kind === 'key' && e.beat !== undefined && project.score) {
    project.score.keys = project.score.keys.filter((k) => k.beat !== e.beat)
  } else if (e.kind === 'clef' && e.beat !== undefined) {
    p.staves = staves(p)
    const s = p.staves.find((s) => s.id === e.staff)!
    s.clef_changes = s.clef_changes?.filter((c) => c.beat !== e.beat)
  } else if (e.kind === 'key') {
    p.staves = staves(p)
    p.staves.find((s) => s.id === e.staff)!.key_signature = 'C'
  } else if (e.kind === 'meter') p.show_time_signature = false
  else
    return 'The initial clef and staff define the part. Edit them in Part settings.'
}
export function moveElement(
  project: Project,
  e: ScoreElement,
  delta: number,
  step: number,
  target?: { part: string; note: string },
) {
  const p = project.parts.find((p) => p.id === e.part)
  if (!p) return
  if (e.kind === 'tempo' && e.beat !== undefined && project.score?.tempos) {
    const t = project.score.tempos.find((t) => t.beat === e.beat)
    if (t) {
      t.beat = Math.max(0, t.beat + delta)
      project.score.tempos.sort((a, b) => a.beat - b.beat)
    }
    return
  }
  if (e.kind === 'mark' && e.mark) {
    p.staves = staves(p)
    const list = p.staves.find((s) => s.id === e.staff)?.marks,
      m = list?.find((m) => m.id === e.mark)
    if (m) {
      m.beat = Math.max(0, m.beat + delta)
      list!.sort((a, b) => a.beat - b.beat)
    }
    return
  }
  if (e.kind === 'dynamic' && e.beat !== undefined) {
    const s = staves(p).find((s) => s.id === e.staff)
    if (!s) return
    const nodes = nodesFromEvents(staffDynamicsEvents(p, s)),
      node = nodes.find((n) => Math.abs(n.beat - e.beat!) < 1e-9)
    if (node)
      Object.assign(
        p,
        writeRamp(
          p,
          velocityLine(s.id),
          moveNode(nodes, node.beat, Math.max(0, node.beat + delta), node.value),
        ),
      )
    return
  }
  if (e.kind === 'curve' && e.curve) {
    p.staves = staves(p)
    const c = p.staves.find((s) => s.id === e.staff)?.curves?.find((c) => c.id === e.curve)
    if (c) {
      // Dragging the whole curve detaches it from its notes and shifts both ends.
      const shift = Math.max(-c.start_beat, delta)
      if (isHairpin(c.kind)) {
        Object.assign(p, retimeHairpin(p, e.staff!, c, c.start_beat+shift, c.end_beat+shift))
        return
      }
      c.start_beat += shift
      c.end_beat += shift
      c.start_note = null
      c.end_note = null
      c.lift = Math.max(-200, Math.min(200, c.lift - step * 5))
      c.end_lift = Math.max(-200, Math.min(200, (c.end_lift ?? 0) - step * 5))
    }
    return
  }
  if (e.kind === 'rest') {
    const n = materializeRest(project, e),
      v = metadata(n, p)
    v.step += step
    p.notes = p.notes.map((x) =>
      x.id === n.id
        ? withNotation(atBeat(n, Math.max(0, n.beat + delta)), v, p)
        : x,
    )
    return
  }
  if (e.note && target?.part === p.id && target.note !== e.note) {
    const source = p.notes.find((n) => n.id === e.note)!,
      dest = p.notes.find((n) => n.id === target.note)
    if (!dest) return
    const a = metadata(source, p),
      b = metadata(dest, p),
      field = (
        {
          articulation: 'articulation',
          tie: 'tie_to',
          slur: 'slur_to',
          octave: 'octave',
          accidental: 'alter',
          dot: 'dots',
        } as const
      )[e.kind as 'articulation']
    if (!field) return
    Object.assign(b, { [field]: a[field] })
    Object.assign(a, {
      [field]: ['octave', 'alter', 'dots'].includes(field) ? 0 : null,
    })
    p.staves = staves(p)
    p.notes = p.notes.map((n) =>
      n.id === source.id
        ? withNotation(n, a, p)
        : n.id === dest.id
          ? withNotation(n, b, p)
          : n,
    )
    return
  }
  if (e.kind === 'special-barline') {
    const b = project.score?.barlines?.[e.index!]
    if (b) b.beat = Math.max(0, b.beat + delta)
    return
  }
  if (e.kind === 'repeat') {
    const r = project.score?.repeats[e.index!]
    if (r) {
      delta = Math.max(-r.start, delta)
      r.start += delta
      r.end += delta
      if (r.first_ending != null) r.first_ending += delta
    }
  }
  if (e.kind === 'navigation' && project.score?.navigation)
    project.score.navigation.at = Math.max(
      0,
      project.score.navigation.at + delta,
    )
  if (e.beat === undefined) return
  if (e.kind === 'key' || e.kind === 'meter') {
    const list = e.kind === 'key' ? project.score?.keys : project.score?.meters,
      x = list?.find((x) => x.beat === e.beat)
    if (x) {
      x.beat = Math.max(0, x.beat + delta)
      list!.sort((a, b) => a.beat - b.beat)
    }
  }
  if (e.kind === 'clef') {
    p.staves = staves(p)
    const list = p.staves.find((s) => s.id === e.staff)?.clef_changes,
      x = list?.find((x) => x.beat === e.beat)
    if (x) {
      x.beat = Math.max(0, x.beat + delta)
      list!.sort((a, b) => a.beat - b.beat)
    }
  }
}
