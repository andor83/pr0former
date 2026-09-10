import { describe, expect, it } from 'vitest'
import type { Part } from './types'
import { addCurve, dropCurveAnchors, slurPath, tieTargets, updateCurve } from './scoreCurves'
import { metadata } from './score'
describe('phrasing curves', () => {
  const part = (): Part => {
    const p = {
      id: 'p',
      name: 'P',
      performer: null,
      view: 'notation',
      clef: 'treble',
      staves: [{ id: 's', name: 'S', clef: 'treble', transpose: 0 }],
      notes: [
        { id: 'a', pitch: 60, beat: 0, duration: 1, velocity: 90, rest: false, tied: false },
        { id: 'b', pitch: 60, beat: 1, duration: 1, velocity: 90, rest: false, tied: false },
        { id: 'c', pitch: 64, beat: 2, duration: 1, velocity: 90, rest: false, tied: false },
      ],
      loop_beats: 4,
      instrument_node: null,
      osc_address: '/n',
    } as Part
    for (const n of p.notes) n.notation = metadata(n, p)
    return p
  }
  it('adds curves with note anchors or free beats, ordered by beat', () => {
    const p = addCurve(part(), 's', 'slur', { note: 'c', beat: 2 }, { note: 'a', beat: 0 })
    const c = p.staves![0]!.curves![0]!
    expect(c).toMatchObject({ kind: 'slur', start_note: 'a', start_beat: 0, end_note: 'c', end_beat: 2, height: -26 })
    const free = addCurve(p, 's', 'bracket', { beat: 4 }, { beat: 6.5 })
    expect(free.staves![0]!.curves![1]).toMatchObject({ kind: 'bracket', start_note: null, end_beat: 6.5, height: 12 })
    expect(() => addCurve(part(), 's', 'slur', { beat: 1 }, { beat: 1 })).toThrow()
  })
  it('updates shape within bounds, swaps reversed ends and drops anchors of removed notes', () => {
    let p = addCurve(part(), 's', 'slur', { note: 'a', beat: 0 }, { note: 'c', beat: 2 })
    const id = p.staves![0]!.curves![0]!.id
    p = updateCurve(p, 's', id, { height: -500, lift: 30 })
    expect(p.staves![0]!.curves![0]).toMatchObject({ height: -200, lift: 30 })
    p = updateCurve(p, 's', id, { end_lift: 12 })
    p = updateCurve(p, 's', id, { end_beat: -1, end_note: null })
    expect(p.staves![0]!.curves![0]).toMatchObject({ start_beat: -1, end_beat: 0, start_note: null, end_note: 'a', lift: 12, end_lift: 30 })
    const dropped = dropCurveAnchors(p, new Set(['a']))
    expect(dropped.staves![0]!.curves![0]!.end_note).toBeNull()
  })
  it('finds tie targets and draws slur paths', () => {
    const p = part()
    expect(tieTargets(p, p.notes[0]!).map((n) => n.id)).toEqual(['b'])
    expect(tieTargets(p, p.notes[1]!)).toEqual([])
    expect(slurPath(0, 100, 40, 100, -20)).toBe('M0 100 C10 80 30 80 40 100')
  })
})

import { addHairpin, retimeHairpin } from './scoreCurves'
import { moveElement, deleteElement } from './scoreElements'
import type { Project } from './types'
it('hairpin previews, whole moves and deletion carry only owned playback', () => {
  const initial = {id:'p',clef:'treble',notes:[],staves:[{id:'s',name:'S',clef:'treble',transpose:0}]} as unknown as Part
  const base = addHairpin(initial,'s','crescendo',1,4)
  const curve = base.staves![0]!.curves![0]!
  const intermediate = retimeHairpin(base,'s',curve,1,5)
  const final = retimeHairpin(base,'s',curve,1,6)
  expect(final.staves![0]!.dynamics!.events[0]!.duration).toBe(5)
  expect(base.staves![0]!.dynamics!.events[0]!.duration).toBe(3)
  expect(intermediate.staves![0]!.dynamics!.events[0]!.duration).toBe(4)
  const project = {parts:[final]} as Project
  const element = {kind:'curve',part:'p',staff:'s',curve:curve.id}
  moveElement(project,element,2,0)
  expect(project.parts[0]!.staves![0]!.dynamics!.events[0]).toMatchObject({beat:3,duration:5})
  deleteElement(project,element)
  expect(project.parts[0]!.staves![0]!.dynamics!.events).toEqual([])
})
it('deleting or moving a hairpin restores an authored starting dynamic', () => {
  const mark = {id:'piano',beat:1,duration:0,start:48,end:48,curve:'step' as const}
  const p = {id:'p',clef:'treble',notes:[],staves:[{id:'s',name:'S',clef:'treble',transpose:0,dynamics:{mode:'velocity',controller:11,events:[mark]}}]} as unknown as Part
  const withHairpin=addHairpin(p,'s','crescendo',1,4)
  const curve=withHairpin.staves![0]!.curves![0]!
  const moved=retimeHairpin(withHairpin,'s',curve,5,8)
  expect(moved.staves![0]!.dynamics!.events[0]).toEqual(mark)
  const project={parts:[withHairpin]} as Project
  deleteElement(project,{kind:'curve',part:'p',staff:'s',curve:curve.id})
  expect(project.parts[0]!.staves![0]!.dynamics!.events).toEqual([mark])
})
