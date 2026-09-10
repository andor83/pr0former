import { it, expect } from 'vitest'
import type { Project, Part } from './types'
import { editBars, setMeter, setClef, measures } from './scoreBars'
import { metadata } from './score'
function fixture(): Project {
  const part: Part = {
    id: 'p',
    name: 'Piano',
    performer: null,
    view: 'notation',
    clef: 'treble',
    staves: [
      {
        id: 's',
        name: 'Upper',
        clef: 'treble',
        transpose: 0,
        clef_changes: [{ beat: 8, clef: 'bass' }],
        hidden_rests: [{ beat: 8, duration: 2, voice: 1 }],
      },
    ],
    notes: [
      {
        id: 'a',
        pitch: 60,
        beat: 3,
        duration: 2,
        velocity: 90,
        rest: false,
        tied: false,
      },
      {
        id: 'b',
        pitch: 62,
        beat: 8,
        duration: 1.5,
        velocity: 90,
        rest: false,
        tied: false,
      },
    ],
    loop_beats: 12,
    instrument_node: null,
    osc_address: '/note',
    automation: [
      {
        id: 'cc',
        name: 'CC11',
        channel: 1,
        message: 'cc',
        number: 11,
        events: [
          {
            id: 'e',
            beat: 8,
            duration: 1,
            start: 0,
            end: 100,
            curve: 'linear',
          },
        ],
      },
    ],
  }
  for (const n of part.notes) n.notation = metadata(n, part)
  part.notes[1]!.notation!.base = 1
  part.notes[1]!.notation!.dots = 1
  return {
    schema_version: 1,
    id: 'test',
    name: 'Test',
    mode: 'structured',
    revision: 0,
    bpm: 120,
    graph: { nodes: [], edges: [] },
    parts: [part, { ...structuredClone(part), id: 'q' }],
    beats_per_bar: 4,
    beat_unit: 4,
    score: {
      version: 1,
      length: 12,
      loop_score: false,
      meters: [],
      keys: [{ beat: 8, key: 'G' }],
      repeats: [{ start: 4, end: 12, times: 2 }],
    },
  } as Project
}
it('inserts blank bars across all parts and preserves untouched notation and events', () => {
  const original = fixture(),
    p = editBars(original, 'insert', 2, 2)
  expect(p.score!.length).toBe(20)
  expect(original.score!.length).toBe(12)
  for (const part of p.parts) {
    expect(part.notes.map((n) => [n.beat, n.duration])).toEqual([
      [3, 1],
      [12, 1],
      [16, 1.5],
    ])
    expect(
      part.notes.every((n) => n.beat >= 12 || n.beat + n.duration <= 4),
    ).toBe(true)
    expect(part.notes[2]!.notation!.dots).toBe(1)
    expect(part.notes[2]!.notation!.onset).toEqual({
      numerator: 16,
      denominator: 1,
    })
    expect(part.automation![0]!.events[0]!.beat).toBe(16)
    expect(part.staves![0]!.clef_changes![0]!.beat).toBe(16)
    expect(part.loop_beats).toBe(20)
  }
  expect(p.score!.repeats[0]).toMatchObject({ start: 12, end: 20 })
  expect(p.score!.keys[0]!.beat).toBe(16)
})
it('deletes bars, removes their contents and restores the active signature at the cut', () => {
  const p = fixture()
  p.score!.keys = [
    { beat: 4, key: 'F' },
    { beat: 5, key: 'D' },
    { beat: 8, key: 'G' },
  ]
  const result = editBars(p, 'delete', 2, 1)
  expect(result.parts[0]!.notes.map((n) => [n.beat, n.duration])).toEqual([
    [3, 1],
    [4, 1.5],
  ])
  expect(result.score!.keys).toEqual([{ beat: 4, key: 'G' }])
  expect(result.score!.repeats[0]).toMatchObject({ start: 4, end: 8 })
  expect(result.parts[0]!.automation![0]!.events[0]!.beat).toBe(4)
  expect(() => editBars(p, 'delete', 1, 3)).toThrow('Keep at least one bar')
})
it('uses the selected meter for inserted bars and keeps a later meter change', () => {
  let p = setMeter(fixture(), 4, 3, 8)
  p = editBars(p, 'insert', 2, 2)
  expect(p.score!.length).toBe(15)
  expect(p.score!.meters).toEqual([{ beat: 4, beats: 3, unit: 8 }])
  expect(
    measures(p)
      .slice(0, 3)
      .map((m) => m.end - m.start),
  ).toEqual([4, 1.5, 1.5])
})
it('appends whole blank bars after a partial last bar and accepts a clef between bar lines', () => {
  const p = fixture()
  p.score!.length = 10
  const q = editBars(p, 'append', 1, 2)
  expect(q.score!.length).toBe(20)
  const r = setClef(q, 'p', 's', 2.5, 'alto')
  expect(r.parts[0]!.staves![0]!.clef_changes).toEqual([
    { beat: 2.5, clef: 'alto' },
    { beat: 8, clef: 'bass' },
  ])
  expect(r.parts[0]!.notes).toEqual(q.parts[0]!.notes)
  expect(setClef(q, 'p', 's', 0, 'bass').parts[0]!.clef).toBe('bass')
  expect(() => setMeter(q, 0, 0, 4)).toThrow()
  expect(() => editBars(q, 'insert', 1, 1024)).toThrow('4096')
})

it('applies a meter to a bar region and restores the previous meter afterwards', async () => {
  const { setMeterRange, meterAt, setKeyRange, keyAt, setRepeat, setBarline, clearRange } =
    await import('./scoreBars')
  const p = fixture()
  const next = setMeterRange(p, 4, 8, 3, 4)
  expect(next.score!.meters).toEqual([
    { beat: 4, beats: 3, unit: 4 },
    { beat: 8, beats: 4, unit: 4 },
  ])
  expect(meterAt(next, 5)).toEqual({ beats: 3, unit: 4 })
  expect(meterAt(next, 9)).toEqual({ beats: 4, unit: 4 })
  // To the end of the piece: no restore.
  expect(setMeterRange(p, 4, null, 6, 8).score!.meters).toEqual([
    { beat: 4, beats: 6, unit: 8 },
  ])
  // Existing key change at beat 8 (G) is retained; the region restores C at its end.
  const keyed = setKeyRange(p, 0, 4, 'G', 'major', 'hold')
  expect(keyed.score!.keys.map((k) => [k.beat, k.key])).toEqual([
    [0, 'G'],
    [4, 'C'],
    [8, 'G'],
  ])
  expect(keyed.parts.every((part) => part.key_signature === 'G')).toBe(true)
  expect(keyAt(keyed, 2).key).toBe('G')
  expect(keyAt(keyed, 6).key).toBe('C')
  expect(keyed.parts[0]!.notes.find((n) => n.id === 'b')!.pitch).toBe(62)
  const up = setKeyRange(p, 4, null, 'D', 'major', 1)
  expect(up.parts[0]!.notes.find((n) => n.id === 'b')!.pitch).toBe(64)
  expect(up.parts[0]!.notes.find((n) => n.id === 'a')!.pitch).toBe(60)
  const down = setKeyRange(p, 0, null, 'Bb', 'major', -1)
  expect(down.parts[0]!.notes.find((n) => n.id === 'a')!.pitch).toBe(58)
  const repeated = setRepeat(p, 0, 8, 3, 4)
  expect(repeated.score!.repeats).toEqual([
    { start: 0, end: 8, times: 3, first_ending: 4 },
  ])
  expect(() => setRepeat(p, 4, 4)).toThrow()
  expect(setBarline(p, 8, 'double').score!.barlines).toEqual([
    { beat: 8, style: 'double' },
  ])
  expect(setBarline(setBarline(p, 8, 'double'), 8, null).score!.barlines).toEqual([])
  const cleared = clearRange(p, 'p', 's', 8, 12)
  expect(cleared.parts[0]!.notes.map((n) => n.id)).toEqual(['a'])
  expect(cleared.parts[0]!.staves![0]!.hidden_rests).toEqual([])
})

it('keeps tempo marks and staff marks aligned through bar edits and edits them', async () => {
  const { setTempo, removeTempo, tempoAt, addMark, removeMark, editBars } =
    await import('./scoreBars')
  let p = fixture()
  p = setTempo(p, 0, 90)
  p = setTempo(p, 8, 132)
  expect(p.bpm).toBe(90)
  expect(tempoAt(p, 7.9)).toBe(90)
  expect(tempoAt(p, 8)).toBe(132)
  expect(() => setTempo(p, 4, 0)).toThrow()
  p = addMark(p, 'p', 's', { beat: 8, kind: 'cue', text: ' start granular ' })
  expect(p.parts[0]!.staves![0]!.marks).toEqual([
    { id: expect.any(String), beat: 8, kind: 'cue', text: 'start granular' },
  ])
  expect(() => addMark(p, 'p', 's', { beat: 1, kind: 'text', text: '  ' })).toThrow()
  const inserted = editBars(p, 'insert', 2, 1)
  expect(inserted.score!.tempos).toEqual([
    { beat: 0, bpm: 90 },
    { beat: 12, bpm: 132 },
  ])
  expect(inserted.parts[0]!.staves![0]!.marks![0]!.beat).toBe(12)
  // Deleting bar 2 shifts the bar-3 tempo and mark to beat 4.
  const deleted = editBars(p, 'delete', 2, 1)
  expect(deleted.score!.tempos).toEqual([
    { beat: 0, bpm: 90 },
    { beat: 4, bpm: 132 },
  ])
  expect(deleted.parts[0]!.staves![0]!.marks![0]!.beat).toBe(4)
  // Deleting the bar that holds them removes the mark; the tempo has no later bar to occupy.
  const trimmed = editBars(p, 'delete', 3, 1)
  expect(trimmed.score!.tempos).toEqual([{ beat: 0, bpm: 90 }])
  expect(trimmed.parts[0]!.staves![0]!.marks).toEqual([])
  const id = p.parts[0]!.staves![0]!.marks![0]!.id
  expect(removeMark(p, 'p', 's', id).parts[0]!.staves![0]!.marks).toEqual([])
  expect(removeTempo(p, 8).score!.tempos).toEqual([{ beat: 0, bpm: 90 }])
})

it('places forward and backward repeat bars like Finale', async () => {
  const { placeRepeatBegin, placeRepeatEnd } = await import('./scoreBars')
  const p = fixture()
  p.score!.repeats = []
  // End with no begin: repeat from the top of the score.
  let next = placeRepeatEnd(p, 5)
  expect(next.score!.repeats).toEqual([{ start: 0, end: 8, times: 2, first_ending: null }])
  // A later end with no begin starts where the previous repeat ended (no nesting).
  next = placeRepeatEnd(next, 9)
  expect(next.score!.repeats.map((r) => [r.start, r.end])).toEqual([
    [0, 8],
    [8, 12],
  ])
  // Begin with no end runs to the end of the score; an end then closes it.
  let open = placeRepeatBegin(p, 4)
  expect(open.score!.repeats).toEqual([{ start: 4, end: 12, times: 2, first_ending: null }])
  open = placeRepeatEnd(open, 6)
  expect(open.score!.repeats).toEqual([{ start: 4, end: 8, times: 2, first_ending: null }])
  // A begin inside an existing repeat moves its start; repeated placement is idempotent.
  expect(placeRepeatBegin(open, 6).score!.repeats[0]).toMatchObject({ start: 4, end: 8 })
  const moved = placeRepeatBegin(placeRepeatEnd(p, 9), 4)
  expect(moved.score!.repeats).toEqual([{ start: 4, end: 12, times: 2, first_ending: null }])
  expect(() => placeRepeatEnd(p, 50)).toThrow()
})
