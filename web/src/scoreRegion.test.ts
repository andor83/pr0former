import { describe, expect, it } from 'vitest'
import type { Project, Part } from './types'
import { metadata } from './score'
import {
  copyRegion,
  moveRegionToStaff,
  moveRegionToVoice,
  pasteRegion,
  regionNotes,
  scaleRegionDurations,
  transposeRegion,
} from './scoreRegion'
function fixture(): Project {
  const part: Part = {
    id: 'p',
    name: 'Piano',
    performer: null,
    view: 'notation',
    clef: 'treble',
    staves: [
      { id: 'upper', name: 'Upper', clef: 'treble', transpose: 0 },
      { id: 'lower', name: 'Lower', clef: 'bass', transpose: 0 },
    ],
    notes: [
      { id: 'a', pitch: 60, beat: 0, duration: 1, velocity: 90, rest: false, tied: false },
      { id: 'b', pitch: 62, beat: 1, duration: 1, velocity: 90, rest: false, tied: false },
      { id: 'c', pitch: 64, beat: 4, duration: 2, velocity: 90, rest: false, tied: false },
    ],
    loop_beats: 8,
    instrument_node: null,
    osc_address: '/note',
  }
  for (const n of part.notes) n.notation = { ...metadata(n, part), staff: 'upper' }
  part.notes[0]!.notation!.tie_to = null
  part.notes[0]!.notation!.slur_to = 'b'
  const horn: Part = {
    ...structuredClone(part),
    id: 'h',
    name: 'Horn',
    staves: [{ id: 'horn', name: 'Horn in F', clef: 'treble', transpose: -7 }],
    notes: [],
  }
  return {
    schema_version: 1,
    id: 't',
    name: 'T',
    mode: 'structured',
    revision: 0,
    bpm: 120,
    graph: { nodes: [], edges: [] },
    parts: [part, horn],
    beats_per_bar: 4,
    beat_unit: 4,
    score: { version: 1, length: 16, loop_score: false, meters: [], keys: [], repeats: [] },
  } as Project
}
const region = { part: 'p', staff: 'upper', start: 0, end: 4 }
describe('score regions', () => {
  it('finds notes in a bar range on one staff', () => {
    expect(regionNotes(fixture(), region)!.notes.map((n) => n.id)).toEqual(['a', 'b'])
  })
  it('transposes diatonically and chromatically without touching other bars', () => {
    const up = transposeRegion(fixture(), region, { steps: 1 })
    expect(up.parts[0]!.notes.map((n) => n.pitch)).toEqual([62, 64, 64])
    const octave = transposeRegion(fixture(), region, { steps: 7 })
    expect(octave.parts[0]!.notes.map((n) => n.pitch)).toEqual([72, 74, 64])
    const semis = transposeRegion(fixture(), region, { semitones: 3 })
    expect(semis.parts[0]!.notes.map((n) => n.pitch)).toEqual([63, 65, 64])
    expect(semis.parts[0]!.notes[0]!.notation).toMatchObject({ step: 29, alter: 1 })
  })
  it('scales durations from the region start and moves notes across voices and staves', () => {
    const doubled = scaleRegionDurations(fixture(), region, 2)
    expect(doubled.parts[0]!.notes.map((n) => [n.beat, n.duration])).toEqual([
      [0, 2],
      [2, 2],
      [4, 2],
    ])
    expect(() => scaleRegionDurations(fixture(), region, 3)).toThrow()
    const voiced = moveRegionToVoice(fixture(), region, 2)
    expect(voiced.parts[0]!.notes.map((n) => n.notation!.voice)).toEqual([2, 2, 1])
    const lower = moveRegionToStaff(fixture(), region, 'lower')
    expect(lower.parts[0]!.notes.map((n) => n.notation!.staff)).toEqual(['lower', 'lower', 'upper'])
    expect(lower.parts[0]!.notes[0]!.pitch).toBe(60)
  })
  it('copies relative to the region and pastes with remapped links, replacing the destination', () => {
    const p = fixture(),
      clip = copyRegion(p, region)!
    expect(clip.length).toBe(4)
    expect(clip.notes.map((n) => n.beat)).toEqual([0, 1])
    const { project: next, ids } = pasteRegion(p, clip, { part: 'p', staff: 'upper', beat: 4 })
    const pasted = next.parts[0]!.notes.filter((n) => ids.includes(n.id))
    expect(pasted.map((n) => [n.beat, n.pitch])).toEqual([
      [4, 60],
      [5, 62],
    ])
    // The original note at beat 4 was replaced; the slur points at the new copy.
    expect(next.parts[0]!.notes.some((n) => n.id === 'c')).toBe(false)
    expect(pasted[0]!.notation!.slur_to).toBe(pasted[1]!.id)
    // Cross-part paste onto a transposing staff keeps sounding pitch and respells.
    const horn = pasteRegion(p, clip, { part: 'h', staff: 'horn', beat: 0 }).project
    expect(horn.parts[1]!.notes.map((n) => n.pitch)).toEqual([60, 62])
    expect(horn.parts[1]!.notes[0]!.notation).toMatchObject({ step: 32, alter: 0 })
    expect(() => pasteRegion(p, clip, { part: 'p', staff: 'upper', beat: 14 })).toThrow()
  })
})
