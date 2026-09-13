import { describe, it, expect } from 'vitest'
import { graphViewport, readView, restoreScoreParts, writeView } from './viewMemory'
class MemoryStorage {
  data = new Map<string, string>()
  getItem(k: string) { return this.data.get(k) ?? null }
  setItem(k: string, v: string) { this.data.set(k, v) }
}
describe('view memory', () => {
  it('round-trips JSON and tolerates junk or missing storage', () => {
    const s = new MemoryStorage()
    writeView('k', { a: 1 }, s)
    expect(readView('k', s)).toEqual({ a: 1 })
    s.setItem('bad', '{oops')
    expect(readView('bad', s)).toBeNull()
    s.setItem('scalar', '3')
    expect(readView('scalar', s)).toBeNull()
    expect(readView('k', undefined)).toBeNull()
    expect(() => writeView('k', {}, undefined)).not.toThrow()
  })
  it('selects the viewport for the current subgraph and rejects malformed entries', () => {
    const saved = { root: { x: 1, y: 2, zoom: 0.5 }, sub: { x: 0, y: 0, zoom: 0 } }
    expect(graphViewport(saved, null)).toEqual({ x: 1, y: 2, zoom: 0.5 })
    expect(graphViewport(saved, 'sub')).toBeNull()
    expect(graphViewport(saved, 'other')).toBeNull()
    expect(graphViewport(null, null)).toBeNull()
  })
  it('restores focused and visible parts only when they still exist', () => {
    const defaults = { focused: 'a', visible: new Set(['a', 'b']) }
    expect(restoreScoreParts({ focused: 'b', visible: ['b'] }, ['a', 'b'], defaults)).toEqual({
      focused: 'b',
      visible: new Set(['b']),
    })
    expect(restoreScoreParts({ focused: 'gone', visible: ['gone'] }, ['a', 'b'], defaults)).toEqual(defaults)
    expect(restoreScoreParts(null, ['a', 'b'], defaults)).toEqual(defaults)
  })
})
