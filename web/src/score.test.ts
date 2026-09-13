import { describe, it, expect } from 'vitest'
import {
  changeNote,
  entryBeat,
  fitEntry,
  insertEntry,
  metadata,
  performanceParts,
  pitchAt,
  durationKeys,
  keyAlter,
  staves,
} from './score'
import type { Part, Note } from './types'
const n: Note = {
  id: 'n',
  pitch: 60,
  beat: 0,
  duration: 1,
  velocity: 90,
  rest: false,
  tied: false,
}
const p: Part = {
  id: 'p',
  name: 'Piano',
  performer: 'me',
  view: 'notation',
  clef: 'treble',
  notes: [n],
  loop_beats: 8,
  instrument_node: null,
  osc_address: '/note',
}
describe('score commands', () => {
  it('uses the agreed duration mapping and preserves onset and dots', () => {
    expect(durationKeys[4]).toBe(1)
    let next = changeNote(n, p, { kind: 'dots' })
    expect(next.duration).toBe(1.5)
    next = changeNote(next, p, { kind: 'duration', value: 0.25 })
    expect(next.duration).toBe(0.375)
    expect(next.beat).toBe(0)
    expect(changeNote(next, p, { kind: 'dots' }).duration).toBe(0.4375)
  })
  it('moves diatonically in the key, alters without moving the staff step', () => {
    const g = { ...p, key_signature: 'G' }
    const e = { ...n, pitch: 64 }
    const f = changeNote(e, g, { kind: 'pitch', value: 1 })
    expect(f.pitch).toBe(66)
    const flat = changeNote(f, g, { kind: 'alter', value: -1 })
    expect(flat.pitch).toBe(65)
    expect(flat.notation?.step).toBe(f.notation?.step)
    expect(keyAlter(31, 'G')).toBe(1)
  })
  it('handles transposition, tuplets and pitch limits', () => {
    const part = { ...p, staves: [{ ...staves(p)[0]!, transpose: -2 }] }
    expect(metadata(n, part).step).toBe(29)
    expect(pitchAt(29, 0, -2)).toBe(60)
    expect(
      changeNote(n, p, { kind: 'tuplet', actual: 3, normal: 2 }).duration,
    ).toBeCloseTo(2 / 3)
    expect(() =>
      changeNote({ ...n, pitch: 127 }, p, {
        kind: 'pitch',
        value: 1,
        octave: true,
      }),
    ).toThrow()
  })
  it('groups all assigned parts first without mutating project order', () => {
    const parts = [
      { ...p, id: 'other', performer: 'them' },
      p,
      { ...p, id: 'mine2' },
      { ...p, id: 'unassigned', performer: null },
    ]
    expect(performanceParts(parts, 'me', false).map((p) => p.id)).toEqual([
      'p',
      'mine2',
    ])
    expect(performanceParts(parts, 'me', true).map((p) => p.id)).toEqual([
      'p',
      'mine2',
      'other',
      'unassigned',
    ])
    expect(performanceParts(parts, 'nobody', false)).toEqual([])
    expect(parts[0]!.id).toBe('other')
  })
})

import {
  scoreAnchors,
  scoreBeat,
  scoreX,
  scoreMeasures,
  rational,
} from './score'
describe('shared score geometry', () => {
  it('retains exact fractional tuplets and onsets', () => {
    expect(rational(1 / 7)).toEqual({ numerator: 1, denominator: 7 })
    expect(rational(3 + 2 / 3)).toEqual({ numerator: 11, denominator: 3 })
    expect(rational(0.8)).toEqual({ numerator: 4, denominator: 5 })
  })
  it('rebars meter changes without moving musical positions', () => {
    expect(
      scoreMeasures(10, 4, 4, [{ beat: 4, beats: 3, unit: 8 }]).map((m) => [
        m.start,
        m.end,
      ]),
    ).toEqual([
      [0, 4],
      [4, 5.5],
      [5.5, 7],
      [7, 8.5],
      [8.5, 10],
    ])
  })
  it('expands dense onsets and preserves the inverse mapping for pointer entry', () => {
    const dense = {
      ...p,
      notes: Array.from({ length: 64 }, (_, i) => ({
        ...n,
        id: String(i),
        beat: i / 16,
        duration: 1 / 16,
      })),
    }
    const anchors = scoreAnchors([dense], 4, 90, 210, scoreMeasures(4, 4, 4))
    expect(
      scoreX(1 / 16, anchors, 90) - scoreX(0, anchors, 90),
    ).toBeGreaterThanOrEqual(24)
    expect(scoreBeat(0, anchors, 90)).toBeCloseTo(-210 / 90)
    expect(scoreX(-1, anchors, 90)).toBe(120)
    for (let beat = 0; beat < 4; beat += 1 / 37)
      expect(scoreBeat(scoreX(beat, anchors, 90), anchors, 90)).toBeCloseTo(
        beat,
        10,
      )
  })
})
describe('entryBeat', () => {
  const at = (beat: number, duration: number, voice = 1, extra: Partial<Note> = {}): Note => ({
    ...n,
    id: `${beat}-${voice}`,
    beat,
    duration,
    notation: {
      staff: 'main',
      step: 34,
      alter: 0,
      voice,
      base: duration,
      dots: 0,
      tuplet_actual: 1,
      tuplet_normal: 1,
    },
    ...extra,
  })
  const part = { ...p, staves: [{ id: 'main', name: 'Main', clef: 'treble', transpose: 0 }] }
  it('uses the earliest open position instead of the clicked gap', () => {
    expect(entryBeat([at(0, 1), at(1, 0.5)], part, 'main', 1, undefined, 6.25)).toBe(1.5)
    expect(entryBeat([at(0, 1), at(4, 1)], part, 'main', 1, undefined, 2)).toBe(1)
  })
  it('starts at the selected bar and fills after entries already at its left edge', () => {
    expect(entryBeat([at(0, 1), at(4, 1)], part, 'main', 1, undefined, 6, 4, 8)).toBe(5)
    expect(entryBeat([at(5, 1)], part, 'main', 1, undefined, 7, 4, 8)).toBe(6)
  })
  it('starts at the bar boundary on an empty staff or voice', () => {
    expect(entryBeat([], part, 'main', 1, undefined, 3)).toBe(0)
    expect(entryBeat([at(0, 1, 2)], part, 'main', 1, undefined, 3)).toBe(0)
    expect(entryBeat([at(0, 1)], part, 'other', 1, undefined, 3)).toBe(0)
  })
  it('counts hidden rests as entries and ignores grace notes', () => {
    expect(entryBeat([at(0, 1)], part, 'main', 1, [{ beat: 1, duration: 1, voice: 1 }], 7)).toBe(2)
    const grace = at(3, 0.25, 1)
    grace.notation = { ...grace.notation!, grace_to: '0-1' }
    expect(entryBeat([at(0, 1), grace], part, 'main', 1, undefined, 7)).toBe(1)
  })
  it('places a click inside a note at its release and a click before a note at the preceding release', () => {
    expect(entryBeat([at(0,2),at(3,1)],part,'main',1,undefined,1,0,4)).toBe(2)
    expect(entryBeat([at(0,1),at(2,1)],part,'main',1,undefined,1.75,0,4)).toBe(1)
    expect(entryBeat([at(1,1)],part,'main',1,undefined,0.5,0,4)).toBe(0)
  })
  it('shifts later chords together, consumes gaps, and rejects overflow without mutation', () => {
    const source={...part,notes:[at(0,1),at(1,1),{...at(1,1),id:'chord',pitch:67},at(3,1)]}
    const result=insertEntry(source,'main',1,1,0.5,4)
    expect(typeof result).toBe('object')
    if(typeof result!=='string')expect(result.notes.map(n=>n.beat)).toEqual([0,1.5,1.5,3])
    expect(insertEntry(source,'main',1,1,2,4)).toMatch(/overflow/)
    expect(source.notes.map(n=>n.beat)).toEqual([0,1,1,3])
    expect(insertEntry({...part,notes:[]},'main',1,3.5,1,4)).toMatch(/overflow/)
  })
})
describe('fitEntry', () => {
  const at = (beat: number, duration: number, rest = false, voice = 1): Note => ({
    ...n,
    id: `${beat}-${rest ? 'r' : 'n'}-${voice}`,
    beat,
    duration,
    rest,
    notation: {
      staff: 'main',
      step: 34,
      alter: 0,
      voice,
      base: duration,
      dots: 0,
      tuplet_actual: 1,
      tuplet_normal: 1,
    },
  })
  const part = { ...p, staves: [{ id: 'main', name: 'Main', clef: 'treble', transpose: 0 }] }
  it('leaves a fitting entry alone', () => {
    expect(fitEntry([at(0, 1), at(3, 1)], part, 'main', 1, 1, 1)).toEqual({ remove: [], shorten: null })
    expect(fitEntry([at(0, 1)], part, 'main', 1, 1, 4)).toEqual({ remove: [], shorten: null })
  })
  it('replaces rests the entry covers', () => {
    expect(fitEntry([at(0, 1), at(1, 1, true), at(2, 2)], part, 'main', 1, 1, 1)).toEqual({
      remove: ['1-r-1'],
      shorten: null,
    })
    // A longer rest under the onset goes too; the remainder becomes an automatic rest.
    expect(fitEntry([at(0, 2, true)], part, 'main', 1, 1, 0.5)).toEqual({ remove: ['0-r-1'], shorten: null })
  })
  it('shortens to the room before the next pitched onset', () => {
    expect(fitEntry([at(3, 0.5), at(3.75, 0.25), at(4, 1)], part, 'main', 1, 3.5, 1)).toEqual({
      remove: [],
      shorten: { base: 0.25, dots: 0 },
    })
    expect(fitEntry([at(0, 1), at(2.5, 0.5)], part, 'main', 1, 1, 2)).toEqual({
      remove: [],
      shorten: { base: 1, dots: 1 },
    })
  })
  it('reports onsets inside another note or gaps nothing fits', () => {
    expect(fitEntry([at(0, 2)], part, 'main', 1, 1, 1)).toMatch(/inside another note/)
    expect(fitEntry([at(0, 1), at(1 + 1 / 3, 1)], part, 'main', 1, 1, 1)).toMatch(/Nothing fits/)
  })
  it('ignores other voices and staves', () => {
    expect(fitEntry([at(0, 4, false, 2)], part, 'main', 1, 1, 1)).toEqual({ remove: [], shorten: null })
  })
})
