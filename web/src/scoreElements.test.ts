import { it, expect } from 'vitest'
import type { Project, Part } from './types'
import { metadata, staves } from './score'
import {
  deleteElement,
  materializeRest,
  moveElement,
  type ScoreElement,
} from './scoreElements'
function fixture() {
  const part: Part = {
    id: 'p',
    name: 'Piano',
    performer: null,
    view: 'notation',
    clef: 'treble',
    notes: [
      {
        id: 'a',
        pitch: 60,
        beat: 0,
        duration: 1,
        velocity: 90,
        rest: false,
        tied: false,
      },
      {
        id: 'b',
        pitch: 62,
        beat: 2,
        duration: 1,
        velocity: 90,
        rest: false,
        tied: false,
      },
    ],
    loop_beats: 8,
    instrument_node: null,
    osc_address: '/note',
  }
  part.staves = staves(part)
  for (const n of part.notes) n.notation = metadata(n, part)
  return { parts: [part] } as Project
}
it('removes and reattaches articulations independently of their notes', () => {
  const p = fixture(),
    part = p.parts[0]!,
    n = part.notes[0]!
  n.notation!.articulation = 'accent'
  const e = {
    kind: 'articulation',
    part: 'p',
    staff: part.staves![0]!.id,
    note: 'a',
  }
  moveElement(p, e, 0, 0, { part: 'p', note: 'b' })
  expect(part.notes.map((n) => n.notation!.articulation)).toEqual([
    null,
    'accent',
  ])
  expect(part.notes.map((n) => n.pitch)).toEqual([60, 62])
  deleteElement(p, { ...e, note: 'b' })
  expect(part.notes).toHaveLength(2)
  expect(part.notes[1]!.notation!.articulation).toBeNull()
})
it('persists deleted generated rests and materializes edits with exact timing', () => {
  const p = fixture(),
    part = p.parts[0]!,
    s = part.staves![0]!,
    rest = {
      ...part.notes[0]!,
      id: 'generated',
      rest: true,
      beat: 4,
      duration: 2,
      notation: { ...part.notes[0]!.notation!, base: 2 },
    }
  const e: ScoreElement = { kind: 'rest', part: 'p', staff: s.id, rest }
  deleteElement(p, e)
  expect(part.staves![0]!.hidden_rests).toEqual([
    { beat: 4, duration: 2, voice: 1 },
  ])
  expect(part.notes).toHaveLength(2)
  const n = materializeRest(p, e)
  expect(n.id).not.toBe('generated')
  expect(part.notes[2]!.notation!.written_duration).toEqual({
    numerator: 2,
    denominator: 1,
  })
})
it('moves a repeat and its ending together, clamping to the score start', () => {
  const p = fixture()
  p.score = {
    version: 1,
    length: 16,
    loop_score: false,
    meters: [],
    keys: [],
    repeats: [{ start: 2, end: 8, times: 2, first_ending: 6 }],
  }
  moveElement(p, { kind: 'repeat', part: 'p', staff: 's', index: 0 }, -4, 0)
  expect(p.score.repeats[0]).toEqual({
    start: 0,
    end: 6,
    times: 2,
    first_ending: 4,
  })
})
