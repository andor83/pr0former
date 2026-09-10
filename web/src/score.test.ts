import { describe, it, expect } from 'vitest'
import {
  changeNote,
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
