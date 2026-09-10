import type { Note, Part, Project } from './types'
import { atBeat, changeNote, metadata, spelling, staves, withNotation } from './score'
import { newId } from './id'
/** A Finale-style selection region: a beat range on one staff of one part. */
export interface Region {
  part: string
  staff: string
  start: number
  end: number
}
const eps = 1e-9
export function regionNotes(project: Project, r: Region): { part: Part; notes: Note[] } | null {
  const part = project.parts.find((p) => p.id === r.part)
  if (!part) return null
  return {
    part,
    notes: part.notes.filter(
      (n) =>
        n.beat >= r.start - eps &&
        n.beat < r.end - eps &&
        metadata(n, part).staff === r.staff,
    ),
  }
}
function mapRegion(
  project: Project,
  r: Region,
  fn: (n: Note, part: Part) => Note,
): Project {
  const p = JSON.parse(JSON.stringify(project)) as Project,
    part = p.parts.find((x) => x.id === r.part)
  if (!part) return p
  part.staves = staves(part)
  part.notes = part.notes.map((n) =>
    n.beat >= r.start - eps &&
    n.beat < r.end - eps &&
    metadata(n, part).staff === r.staff
      ? fn(n, part)
      : n,
  )
  return p
}
/** Transpose diatonically (staff steps) or chromatically (semitones); rests are untouched. */
export function transposeRegion(
  project: Project,
  r: Region,
  by: { steps?: number; semitones?: number },
): Project {
  return mapRegion(project, r, (n, part) => {
    if (n.rest) return n
    if (by.semitones) {
      const v = metadata(n, part),
        staff = staves(part).find((s) => s.id === v.staff)!,
        pitch = n.pitch + by.semitones
      if (pitch < 0 || pitch > 127) return n
      return withNotation(
        n,
        {
          ...v,
          ...spelling(pitch - (v.octave || 0) * 12, {
            ...staff,
            key_signature: staff.key_signature ?? part.key_signature,
          }),
        },
        part,
      )
    }
    let moved = n
    const steps = by.steps || 0
    for (let i = 0; i < Math.abs(steps) % 7; i++)
      moved = changeNote(moved, part, { kind: 'pitch', value: Math.sign(steps) })
    for (let i = 0; i < Math.abs(Math.trunc(steps / 7)); i++)
      moved = changeNote(moved, part, {
        kind: 'pitch',
        value: Math.sign(steps),
        octave: true,
      })
    return moved
  })
}
/** Finale “Change Note Durations”: scale onsets (relative to the region start) and durations. */
export function scaleRegionDurations(project: Project, r: Region, factor: number): Project {
  if (![0.25, 0.5, 2, 4].includes(factor)) throw new Error('Choose ×2, ÷2, ×4 or ÷4.')
  return mapRegion(project, r, (n, part) => {
    const v = metadata(n, part)
    return withNotation(
      atBeat(n, r.start + (n.beat - r.start) * factor),
      { ...v, base: v.base * factor },
      part,
    )
  })
}
export function moveRegionToVoice(project: Project, r: Region, voice: number): Project {
  if (voice < 1 || voice > 4) throw new Error('Voices are 1–4.')
  return mapRegion(project, r, (n, part) =>
    withNotation(n, { ...metadata(n, part), voice }, part),
  )
}
export function moveRegionToStaff(project: Project, r: Region, staffId: string): Project {
  return mapRegion(project, r, (n, part) => {
    const staff = staves(part).find((s) => s.id === staffId)
    if (!staff) throw new Error('Choose a staff of the same part.')
    const v = metadata(n, part),
      from = staves(part).find((s) => s.id === v.staff)!
    // Keep the sounding pitch; respell for the destination staff's transposition.
    return withNotation(
      n,
      {
        ...v,
        staff: staffId,
        ...(n.rest
          ? {}
          : spelling(n.pitch - (v.octave || 0) * 12, {
              ...staff,
              key_signature:
                staff.key_signature ?? from.key_signature ?? part.key_signature,
            })),
      },
      part,
    )
  })
}
/** Clipboard payload: notes relative to the region start, so paste can land anywhere. */
export interface RegionClipboard {
  length: number
  notes: Note[]
  voices: number[]
}
export function copyRegion(project: Project, r: Region): RegionClipboard | null {
  const found = regionNotes(project, r)
  if (!found) return null
  const notes = found.notes.map((n) => {
    const v = metadata(n, found.part)
    return { ...atBeat({ ...n, notation: v }, n.beat - r.start) }
  })
  return {
    length: r.end - r.start,
    notes: JSON.parse(JSON.stringify(notes)),
    voices: [...new Set(notes.map((n) => n.notation!.voice))],
  }
}
/**
 * Paste at a beat on a staff. Like Finale, the destination range is replaced;
 * ids and tie/slur/grace links are remapped, and pitches are respelled for a
 * different staff transposition. Returns the new project and pasted note ids.
 */
export function pasteRegion(
  project: Project,
  clip: RegionClipboard,
  target: { part: string; staff: string; beat: number },
): { project: Project; ids: string[] } {
  const p = JSON.parse(JSON.stringify(project)) as Project,
    part = p.parts.find((x) => x.id === target.part)
  if (!part) throw new Error('Choose a destination part.')
  part.staves = staves(part)
  const staff = part.staves.find((s) => s.id === target.staff)
  if (!staff) throw new Error('Choose a destination staff.')
  const length = p.score?.length
  if (length != null && target.beat + clip.length > length + eps)
    throw new Error('The pasted music would extend past the end of the score.')
  const start = target.beat,
    end = target.beat + clip.length
  const removed = new Set(
    part.notes
      .filter((n) => {
        const v = metadata(n, part)
        return (
          n.beat >= start - eps &&
          n.beat < end - eps &&
          v.staff === target.staff &&
          clip.voices.includes(v.voice)
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
    s.curves = (s.curves || []).map((c) => ({
      ...c,
      start_note: c.start_note && removed.has(c.start_note) ? null : c.start_note,
      end_note: c.end_note && removed.has(c.end_note) ? null : c.end_note,
    }))
  for (const s of part.staves)
    if (s.id === target.staff)
      s.hidden_rests = (s.hidden_rests || []).filter(
        (h) => !(h.beat >= start - eps && h.beat < end - eps && clip.voices.includes(h.voice)),
      )
  const ids = new Map(clip.notes.map((n) => [n.id, newId()]))
  const pasted: Note[] = []
  for (const source of clip.notes) {
    const v = { ...source.notation!, staff: target.staff }
    for (const field of ['tie_to', 'slur_to', 'grace_to'] as const)
      v[field] = v[field] ? ids.get(v[field]!) || null : null
    const spelled = source.rest
      ? v
      : {
          ...v,
          ...spelling(source.pitch - (v.octave || 0) * 12, {
            ...staff,
            key_signature: staff.key_signature ?? part.key_signature,
          }),
        }
    const n = withNotation(
      atBeat({ ...source, id: ids.get(source.id)! }, start + source.beat),
      spelled,
      part,
    )
    pasted.push(n)
  }
  part.notes = [...part.notes, ...pasted].sort((a, b) => a.beat - b.beat)
  return { project: p, ids: pasted.map((n) => n.id) }
}
