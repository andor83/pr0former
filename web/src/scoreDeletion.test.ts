import { expect, it } from 'vitest'
import type { Note, Part, Project } from './types'
import { atBeat, metadata, staves } from './score'
import { deleteScoreNotes } from './scoreDeletion'
import { deleteElement } from './scoreElements'
function fixture() {
  const part = { id: 'p', name: 'Piano', clef: 'treble', view: 'notation', loop_beats: 12, notes: [] } as unknown as Part
  part.staves = [...staves(part), { ...staves(part)[0]!, id: 'lower' }]
  const project = { parts: [part], beats_per_bar: 4, beat_unit: 4, score: { version: 1, length: 12, loop_score: false, meters: [], keys: [], repeats: [] } } as unknown as Project
  function note(id: string, beat: number, duration = 1, rest = false, voice = 1, staff = part.staves![0]!.id) {
    const n: Note = { id, beat, duration, rest, pitch: 60, velocity: 90, tied: false }
    n.notation = { ...metadata(n, part), voice, staff }
    part.notes.push(atBeat(n, beat))
    return part.notes.at(-1)!
  }
  const beats = () => Object.fromEntries(part.notes.map(n => [n.id, n.beat]))
  return { project, part, note, beats }
}
it('closes deleted notes and rests only in the same bar, staff and voice', () => {
  const { project, part, note, beats } = fixture()
  note('a', 0); note('rest', 1, 1, true); note('c', 2); note('d', 3); note('next-bar', 4)
  note('voice2', 2, 1, false, 2); note('lower', 2, 1, false, 1, 'lower')
  deleteScoreNotes(project, 'p', new Set(['a', 'rest']))
  expect(beats()).toEqual({ c: 0, d: 1, 'next-bar': 4, voice2: 2, lower: 2 })
  expect(part.notes[1]!.notation!.onset).toEqual({ numerator: 1, denominator: 1 })
  expect(project.score!.length).toBe(12)
})
it('deleting one chord tone preserves surviving time and counts deleted chords once', () => {
  const { project, note, beats } = fixture()
  note('a', 0); note('chord', 0); note('b', 1); note('b-chord', 1); note('c', 2)
  deleteScoreNotes(project, 'p', new Set(['a']))
  expect(beats()).toEqual({ chord: 0, b: 1, 'b-chord': 1, c: 2 })
  deleteScoreNotes(project, 'p', new Set(['b', 'b-chord']))
  expect(beats()).toEqual({ chord: 0, c: 1 })
})
it('preserves gaps outside the deleted span and handles fractional durations and changing meters', () => {
  const { project, note, beats } = fixture()
  project.score!.meters = [{ beat: 4, beats: 3, unit: 8 }]
  note('a', 4, 1 / 3); note('b', 4 + 2 / 3, 1 / 3); note('boundary', 5.5, 0.5)
  deleteScoreNotes(project, 'p', new Set(['a']))
  expect(beats().b).toBeCloseTo(4 + 1 / 3)
  expect(beats().boundary).toBe(5.5)
})
it('clamps a deleted cross-bar note to its starting bar and cleans invalid ties', () => {
  const { project, part, note, beats } = fixture()
  note('first', 0); const source = note('source', 3); note('target', 4); note('long', 7, 2); note('later', 9)
  source.notation!.tie_to = 'target'
  deleteScoreNotes(project, 'p', new Set(['first', 'long']))
  expect(beats()).toEqual({ source: 2, target: 4, later: 9 })
  expect(part.notes[0]!.notation!.tie_to).toBeNull()
})
it('deletes automatic rests by closing their gap and shifts hidden-rest ranges', () => {
  const { project, part, note, beats } = fixture()
  const first = note('a', 0); note('b', 2); note('c', 3); note('later', 4)
  part.staves![0]!.hidden_rests = [{ beat: 3, duration: 2, voice: 1 }]
  const rest = { ...first, id: 'auto', beat: 1, duration: 1, rest: true }
  deleteElement(project, { kind: 'rest', part: 'p', staff: part.staves![0]!.id, rest })
  expect(beats()).toEqual({ a: 0, b: 1, c: 2, later: 4 })
  expect(part.staves![0]!.hidden_rests).toEqual([{ beat: 2, duration: 1, voice: 1 }, { beat: 4, duration: 1, voice: 1 }])
})
it('keeps grace notes with moving principals and removes orphan grace notes and anchors', () => {
  const { project, part, note, beats } = fixture()
  note('a', 0); note('b', 1); const grace = note('grace', 1, 0.25); grace.notation!.grace_to = 'b'
  part.staves![0]!.curves = [{ id: 'curve', kind: 'slur', start_note: 'a', end_note: 'b', start_beat: 0, end_beat: 1, height: -20, lift: 0 }]
  deleteScoreNotes(project, 'p', new Set(['a']))
  expect(beats()).toEqual({ b: 0, grace: 0 })
  expect(part.staves![0]!.curves![0]!.start_note).toBeNull()
  deleteScoreNotes(project, 'p', new Set(['b']))
  expect(part.notes).toEqual([])
})

it('keeps cross-voice grace attachments valid and removes backward slur links after a cut', () => {
  const { project, part, note } = fixture()
  note('a', 0); note('principal', 1)
  const grace = note('grace', 1, 0.25, false, 2)
  grace.notation!.grace_to = 'principal'
  const slur = note('slur', 0.5, 0.25, false, 3)
  slur.notation!.slur_to = 'principal'
  deleteScoreNotes(project, 'p', new Set(['a']))
  expect(part.notes.find(n => n.id === 'grace')!.beat).toBe(0)
  expect(part.notes.find(n => n.id === 'slur')!.notation!.slur_to).toBeNull()
})
