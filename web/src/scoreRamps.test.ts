import { describe, expect, it } from 'vitest'
import {
  addLane,
  applyDynamicMark,
  eventsFromNodes,
  interpolate,
  nodesFromEvents,
  removeLane,
  removeNode,
  setNode,
  staffDynamicsEvents,
  updateLane,
  writeRamp,
} from './scoreRamps'
import { staves } from './score'
import type { Part } from './types'
describe('score ramps', () => {
  const events = [
    { id: 'a', beat: 1, duration: 1, start: 64, end: 96, curve: 'linear' as const },
    { id: 'b', beat: 4, duration: 0, start: 48, end: 48, curve: 'linear' as const },
  ]
  it('converts ramp events to breakpoints and back losslessly for linear ramps', () => {
    const nodes = nodesFromEvents(events)
    expect(nodes).toEqual([
      { beat: 1, value: 64 },
      { beat: 2, value: 96 },
      { beat: 4, value: 48 },
    ])
    const back = eventsFromNodes(nodes, events)
    expect(back.map((e) => [e.beat, e.duration, e.start, e.end])).toEqual([
      [1, 1, 64, 96],
      [2, 2, 96, 48],
      [4, 0, 48, 48],
    ])
    expect(back[0]!.id).toBe('a')
    expect(back[1]!.id).toBe('b')
    expect(typeof back[2]!.id).toBe('string')
  })
  it('interpolates the value a note would receive', () => {
    const nodes = nodesFromEvents(events)
    expect(interpolate([], 3)).toBe(90)
    expect(interpolate(nodes, 0)).toBe(64)
    expect(interpolate(nodes, 1.5)).toBe(80)
    expect(interpolate(nodes, 3)).toBe(72)
    expect(interpolate(nodes, 10)).toBe(48)
  })
  it('adds a dynamic mark as a rapid ramp from the current level', () => {
    const marked = applyDynamicMark([], 4, 80)
    expect(marked).toEqual([
      { beat: 3.875, value: 90 },
      { beat: 4, value: 80 },
    ])
    // Existing line: ramp starts from the interpolated level, existing nodes stay.
    const nodes = nodesFromEvents(events)
    const next = applyDynamicMark(nodes, 3, 112)
    expect(next.map((n) => [n.beat, n.value])).toEqual([
      [1, 64],
      [2, 96],
      [2.875, 75],
      [3, 112],
      [4, 48],
    ])
    // A mark right after another node does not insert an extra ramp start.
    expect(applyDynamicMark(nodes, 2.05, 16).length).toBe(4)
    expect(setNode(nodes, 2, 100).find((n) => n.beat === 2)!.value).toBe(100)
    expect(removeNode(nodes, 2).length).toBe(2)
  })
  it('writes velocity dynamics or a lane', () => {
    const part = {
      id: 'p',
      name: 'P',
      performer: null,
      view: 'notation',
      clef: 'treble',
      notes: [],
      loop_beats: 8,
      instrument_node: null,
      osc_address: '/n',
      automation: [
        { id: 'expr', name: 'Expression', channel: 1, message: 'cc', number: 11, events: [] },
      ],
    } as Part
    const velocity = writeRamp(part, 'velocity:p-staff', [{ beat: 0, value: 48 }])
    expect(velocity.staves![0]!.dynamics).toMatchObject({ mode: 'velocity', controller: 11 })
    expect(velocity.staves![0]!.dynamics!.events).toHaveLength(1)
    expect(velocity.dynamics).toBeNull()
    // Legacy part dynamics show on the first staff until that staff is written.
    const legacy = { ...part, dynamics: { mode: 'velocity' as const, controller: 11, events: [{ id: 'x', beat: 2, duration: 0, start: 96, end: 96, curve: 'linear' as const }] } }
    expect(staffDynamicsEvents(legacy, staves(legacy)[0]!).length).toBe(1)
    const withLanes = addLane(part, { name: 'Program', channel: 2, message: 'program', number: 0 })
    const programId = withLanes.automation!.at(-1)!.id
    const program = writeRamp(withLanes, programId, [{ beat: 0, value: 3 }, { beat: 4, value: 7 }])
    expect(program.automation!.at(-1)!.events.map((e) => [e.beat, e.duration, e.start, e.end])).toEqual([
      [0, 0, 3, 3],
      [4, 0, 7, 7],
    ])
    expect(removeLane(program, programId).automation).toHaveLength(1)
    expect(updateLane(program, 'expr', { number: 1 }).automation![0]!.number).toBe(1)
    const lane = writeRamp(part, 'expr', [
      { beat: 0, value: 0 },
      { beat: 4, value: 127 },
    ])
    expect(lane.automation![0]!.events.map((e) => [e.beat, e.duration, e.start, e.end])).toEqual([
      [0, 4, 0, 127],
      [4, 0, 127, 127],
    ])
  })
})
