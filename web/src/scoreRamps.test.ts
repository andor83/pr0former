import { describe, expect, it } from 'vitest'
import { eventsFromNodes, nodesFromEvents, interpolate, setNode, moveNode, writeRamp, staffDynamicsEvents, addLane } from './scoreRamps'
import type { Part, AutomationEvent, MidiCurve } from './types'
const part = (): Part => ({ id:'p', name:'P', clef:'treble', notes:[], staves:[{ id:'upper', name:'Upper', clef:'treble', transpose:0 },{ id:'lower', name:'Lower', clef:'bass', transpose:0 }], dynamics:{ mode:'velocity', controller:11, events:[{id:'legacy', beat:0,duration:0,start:48,end:48,curve:'step'}] } } as unknown as Part)
describe('ramp musical semantics', () => {
  it('preserves holds, gaps, identities and every interpolation shape', () => {
    for (const curve of ['linear','step','ease_in','ease_out','s_curve'] as MidiCurve[]) {
      const events: AutomationEvent[] = [{id:'a',beat:1,duration:1,start:64,end:96,curve},{id:'b',beat:4,duration:0,start:48,end:48,curve:'step'}]
      const nodes = nodesFromEvents(events)
      expect(eventsFromNodes(nodes)).toEqual(events)
      expect(interpolate(nodes,0)).toBe(90)
      expect(interpolate(nodes,3)).toBe(96)
      expect(interpolate(nodes,4)).toBe(48)
      expect(eventsFromNodes(setNode(nodes,4,80))[0]).toEqual(events[0])
    }
  })
  it('moves a ramp endpoint without flattening its curve', () => {
    const nodes = nodesFromEvents([{id:'a',beat:1,duration:1,start:64,end:96,curve:'ease_in'}])
    expect(eventsFromNodes(moveNode(nodes,2,3,112))).toEqual([{id:'a',beat:1,duration:2,start:64,end:112,curve:'ease_in'}])
  })
  it('overrides one staff without removing another staff’s inherited dynamics', () => {
    const p=part(), next=writeRamp(p,'velocity:upper',[{beat:0,value:80}])
    expect(staffDynamicsEvents(next,next.staves![0]!)[0]!.start).toBe(80)
    expect(staffDynamicsEvents(next,next.staves![1]!)[0]!.start).toBe(48)
    expect(next.dynamics).toEqual(p.dynamics)
  })
  it('retains program steps and bend initial values', () => {
    const p=addLane(part(),{name:'Program',channel:2,message:'program',number:0})
    const id=p.automation!.at(-1)!.id
    expect(writeRamp(p,id,[{beat:0,value:3},{beat:4,value:7}]).automation!.at(-1)!.events.map(e=>e.duration)).toEqual([0,0])
    expect(interpolate([],0,8192)).toBe(8192)
  })
})
it('preserves a discontinuity at a shared endpoint and instantaneous end values', () => {
  const events: AutomationEvent[] = [
    {id:'a',beat:0,duration:2,start:32,end:96,curve:'linear'},
    {id:'b',beat:2,duration:0,start:64,end:48,curve:'step'},
  ]
  const nodes = nodesFromEvents(events)
  expect(eventsFromNodes(nodes)).toEqual(events)
  expect(interpolate(nodes,1)).toBe(64)
  expect(interpolate(nodes,2)).toBe(48)
  expect(eventsFromNodes(setNode(nodes,2,80))[0]).toEqual(events[0])
})
