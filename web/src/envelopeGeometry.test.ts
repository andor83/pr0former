import { describe, expect, it } from 'vitest'
import { MAX_SCALE, MIN_HOLD, autoScale, dragHandle, envelopePoints, fitScale, logTime, logWidth, minScale, timeLabel } from './envelopeGeometry'

const e = { attack: 10, decay: 100, sustain: 0.5, release: 300 }

describe('envelope geometry', () => {
  it('draws attack, decay, a sustain plateau and a release that ends at the right edge', () => {
    const points = envelopePoints(e)
    expect(points).toHaveLength(5)
    expect(points.map(p => p.y)).toEqual([0, 1, 0.5, 0.5, 0])
    for (let i = 1; i < points.length; i++) expect(points[i]!.x).toBeGreaterThan(points[i - 1]!.x)
    expect(points[0]!.x).toBe(0)
    expect(points.at(-1)!.x).toBe(1)
    // The automatic zoom leaves a fifth of the graph to the plateau.
    expect(points[3]!.x - points[2]!.x).toBeCloseTo(0.2)
  })
  it('gives short stages visible width on the log axis', () => {
    const points = envelopePoints(e)
    // 10 ms of attack is not 30× narrower than the 300 ms release.
    const attack = points[1]!.x, release = 1 - points[3]!.x
    expect(attack).toBeGreaterThan(release / 3)
    expect(attack).toBeLessThan(release)
    expect(logTime(logWidth(123.4))).toBeCloseTo(123.4)
  })
  it('keeps everything visible at any zoom by growing the plateau or the scale', () => {
    // Zooming out only widens the plateau.
    const wide = envelopePoints(e, MAX_SCALE)
    expect(wide.at(-1)!.x).toBe(1)
    expect(wide[3]!.x - wide[2]!.x).toBeGreaterThan(0.5)
    // A zoom tighter than the envelope needs is raised to the minimum plateau.
    const long = { attack: 100, decay: 1000, sustain: 0.5, release: 3000 }
    const tight = envelopePoints(long, 1)
    expect(tight[3]!.x - tight[2]!.x).toBeCloseTo(MIN_HOLD)
    expect(fitScale(long, 1)).toBe(minScale(long))
    expect(envelopePoints(e, 1)[3]!.x - envelopePoints(e, 1)[2]!.x).toBeGreaterThanOrEqual(MIN_HOLD)
    expect(autoScale(e)).toBeGreaterThan(minScale(e))
    expect(minScale({ attack: 10000, decay: 10000, sustain: 1, release: 10000 })).toBeLessThan(MAX_SCALE)
  })
  it('maps handle drags back to parameters with a frozen zoom', () => {
    const scale = autoScale(e), points = envelopePoints(e, scale)
    expect(dragHandle(e, 'attack', { x: points[1]!.x, y: 0.3 }, scale)).toEqual({ attack: 10 })
    expect(dragHandle(e, 'attack', { x: logWidth(50) / scale, y: 0.3 }, scale)).toEqual({ attack: 50 })
    expect(dragHandle(e, 'decay', { x: points[2]!.x, y: 0.25 }, scale)).toEqual({ decay: 100, sustain: 0.75 })
    expect(dragHandle(e, 'release', { x: points[3]!.x, y: 0.9 }, scale)).toEqual({ release: 300, sustain: 0.1 })
    expect(dragHandle(e, 'release', { x: 1 - logWidth(1000) / scale, y: 0.5 }, scale)).toEqual({ release: 1000, sustain: 0.5 })
  })
  it('clamps drags so the plateau never shrinks below its minimum', () => {
    const scale = autoScale(e)
    const room = scale * (1 - MIN_HOLD)
    const longest = dragHandle(e, 'attack', { x: 1, y: 0 }, scale).attack!
    expect(logWidth(longest) + logWidth(e.decay) + logWidth(e.release)).toBeLessThanOrEqual(room + 1e-6)
    const points = envelopePoints({ ...e, attack: longest }, scale)
    expect(points[3]!.x - points[2]!.x).toBeCloseTo(MIN_HOLD, 2)
    expect(dragHandle(e, 'attack', { x: -1, y: 0 }, scale)).toEqual({ attack: 0 })
    expect(dragHandle(e, 'decay', { x: 0, y: -2 }, scale)).toEqual({ decay: 0, sustain: 1 })
    expect(dragHandle(e, 'release', { x: 2, y: 5 }, scale)).toEqual({ release: 0, sustain: 0 })
    expect(dragHandle({ ...e, attack: 0, decay: 0 }, 'release', { x: -1, y: 0.5 }, MAX_SCALE)).toEqual({ release: 10000, sustain: 0.5 })
  })
  it('labels durations compactly', () => {
    expect(timeLabel(50)).toBe('50 ms')
    expect(timeLabel(2371)).toBe('2.4 s')
    expect(timeLabel(10000)).toBe('10.0 s')
  })
})
