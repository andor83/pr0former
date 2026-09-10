import { describe, expect, it } from 'vitest'
import { PresentationClock } from './presentationClock'
const snapshot = { project_id: 'p', epoch: 'e', beat: 2, bpm: 120, server_time: 1000, running: true }
describe('presentation clock', () => {
  it('holds through late snapshots and resumes when the estimate catches up', () => {
    const clock = new PresentationClock()
    expect(clock.read(snapshot, true, false, 1100)).toBe(2.2)
    const next = { ...snapshot, beat: 2.1, server_time: 1100 }
    expect(clock.read(next, true, false, 1120)).toBe(2.2)
    expect(clock.read(next, true, false, 1250)).toBe(2.4)
  })
  it('freezes at the displayed position on loss instead of jumping back to the snapshot', () => {
    const clock = new PresentationClock()
    expect(clock.read(snapshot, true, false, 1200)).toBe(2.4)
    expect(clock.read(snapshot, true, true, 2000)).toBe(2.4)
    expect(clock.read(snapshot, true, true, 5000)).toBe(2.4)
  })
  it('honors explicit stop, restart, new project and epoch positions', () => {
    const clock = new PresentationClock()
    clock.read(snapshot, true, false, 1200)
    expect(clock.read({ ...snapshot, beat: 0, running: false }, true, false, 1250)).toBe(0)
    expect(clock.read({ ...snapshot, beat: .1, server_time: 1300 }, true, false, 1300)).toBe(.1)
    expect(clock.read({ ...snapshot, epoch: 'new', beat: 0, server_time: 1400 }, true, false, 1400)).toBe(0)
    expect(clock.read(snapshot, false, false, 1400)).toBe(0)
    expect(clock.read(null, true, true, 1400)).toBe(0)
  })
  it('uses changed tempo and offset without rewinding or unlimited extrapolation', () => {
    const clock = new PresentationClock()
    clock.read(snapshot, true, false, 1200)
    expect(clock.read({ ...snapshot, beat: 2.1, bpm: 60, server_time: 1200 }, true, false, 1190)).toBe(2.4)
    expect(clock.read({ ...snapshot, beat: 2.1, bpm: 60, server_time: 1200 }, true, false, 9999)).toBe(2.6)
  })
})
