import { describe, expect, it } from 'vitest'
import { durationGlyphs } from './notation'
describe('quarter-beat notation durations', () => {
  it('renders dotted values and short notes exactly', () => {
    expect(durationGlyphs(0.75)).toEqual([{ duration: '8', beats: 0.75, dots: 1 }])
    expect(durationGlyphs(1.75)).toEqual([{ duration: 'q', beats: 1.75, dots: 2 }])
    expect(durationGlyphs(0.0625)).toEqual([{ duration: '64', beats: 0.0625, dots: 0 }])
  })
  it('preserves duration in tied decompositions across the supported grid', () => {
    for (let units = 1; units <= 4096; units++) {
      const glyphs = durationGlyphs(units / 16)!
      expect(glyphs).not.toBeNull()
      expect(glyphs.reduce((sum, g) => sum + g.beats, 0)).toBe(units / 16)
      expect(glyphs.length).toBeLessThanOrEqual(64)
    }
    expect(durationGlyphs(1.25)?.map(g => g.beats)).toEqual([1, 0.25])
  })
  it('reports unsupported timing instead of silently quantizing', () => {
    for (const beats of [0, -1, NaN, Infinity, 0.8, 1 / 3, 1e100]) expect(durationGlyphs(beats)).toBeNull()
  })
})
