import { describe, expect, it } from 'vitest'
import type { Part, Project } from './types'
import { buildConductedLane, engineToLocal, localToEngine } from './conductedLane'

const part = (id: string, startPitch: number): Part => ({
  id,
  name: id,
  performer: 'player',
  view: 'notation',
  clef: 'treble',
  notes: [{ id: 'n', pitch: startPitch, beat: 1, duration: 1, velocity: 90, rest: false, tied: false }],
  loop_beats: 4,
  instrument_node: null,
  midi_channel: 1,
  osc_address: '/note',
  performance_meters: [{ beat: 0, beats: 2, unit: 4 }, { beat: 2, beats: 4, unit: 8 }],
})
const project = {
  id: 'p', name: 'p', mode: 'conducted', revision: 1, schema_version: 1, bpm: 120,
  beats_per_bar: 4, beat_unit: 4, graph: { nodes: [], edges: [] },
  conducted: { count_in_pulses: 2, pulse_unit: 4, sets: [] }, parts: [part('a', 60), part('b', 67)],
} as Project

describe('conducted performer lane', () => {
  it('maps every local meter denominator beat onto the global pulse', () => {
    expect(localToEngine(project, project.parts[0]!, 2)).toBe(2)
    expect(localToEngine(project, project.parts[0]!, 4)).toBe(6)
    expect(engineToLocal(project, project.parts[0]!, 4)).toBe(3)
  })

  it('concatenates current and queued notation while retaining an idle rest gap', () => {
    const lane = buildConductedLane(project, 'player', [
      { id: 'a', playing: true, start: 1, position: 1, pending: null },
      { id: 'b', playing: false, start: 0, position: 0, pending: null, queue_position: 1, scheduled_start: 9 },
    ], 3, [])
    const live = lane.project.parts[0]!
    expect(live.notes.map(note => [note.pitch, note.beat])).toEqual([[60, 2], [67, 8]])
    expect(lane.beat).toBe(3)
    expect(lane.upcoming).toBe('b')
    expect(live.performance_meters).toContainEqual({ beat: 9, beats: 4, unit: 8 })
  })

  it('keeps repeated notation visible through the announced successor boundary', () => {
    const repeatedProject = structuredClone(project)
    for (const item of repeatedProject.parts) item.performance_meters = [{beat:0,beats:4,unit:4}]
    const lane = buildConductedLane(repeatedProject, 'player', [
      {id:'a',playing:true,start:0,position:1,pending:[8,false],repeating:false},
      {id:'b',playing:false,start:0,position:0,pending:null,scheduled_start:8,queue_position:1},
    ], 6, [])
    expect(lane.project.parts[0]!.notes.map(note=>[note.pitch,note.beat])).toEqual([[60,1],[60,5],[67,9]])
    expect(lane.upcoming).toBe('b')
  })
})
