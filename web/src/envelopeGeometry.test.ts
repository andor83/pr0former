import { describe, expect, it } from 'vitest'
import { dragHandle, envelopePoints, holdTime, scaleLabel, timeScale } from './envelopeGeometry'

const e = { attack: 100, decay: 200, sustain: 0.5, release: 300 }

describe('envelope geometry', () => {
  it('draws attack, decay, a sustain plateau and a release that ends at the right edge', () => {
    const points = envelopePoints(e)
    expect(points).toHaveLength(5)
    expect(points.map(p => p.y)).toEqual([0, 1, 0.5, 0.5, 0])
    for (let i = 1; i < points.length; i++) expect(points[i]!.x).toBeGreaterThan(points[i - 1]!.x)
    expect(points.at(-1)!.x).toBe(1)
    // 600 ms of timed stages need a 1 s axis (80% fill of 500 ms is too small).
    expect(timeScale(e)).toBe(1000)
    expect(holdTime(e)).toBe(400)
    expect(points[3]!.x).toBeCloseTo(0.7)
  })
  it('steps the time axis only when the envelope outgrows it', () => {
    expect(timeScale({ attack: 0, decay: 0, sustain: 1, release: 0 })).toBe(250)
    expect(timeScale({ attack: 100, decay: 50, sustain: 1, release: 50 })).toBe(250)
    expect(timeScale({ attack: 100, decay: 50, sustain: 1, release: 51 })).toBe(500)
    expect(timeScale({ attack: 10000, decay: 10000, sustain: 1, release: 10000 })).toBe(64000)
    expect(scaleLabel(500)).toBe('500 ms')
    expect(scaleLabel(2000)).toBe('2 s')
  })
  it('keeps a visible plateau for very short envelopes', () => {
    const short = { attack: 0, decay: 0, sustain: 1, release: 0 }
    expect(holdTime(short)).toBe(250)
    const points = envelopePoints(short)
    expect(points[3]!.x - points[2]!.x).toBeCloseTo(1)
  })
  it('maps handle drags back to parameters with a frozen time scale', () => {
    const total = timeScale(e)
    expect(dragHandle(e, 'attack', { x: 200 / total, y: 0.3 }, total)).toEqual({ attack: 200 })
    expect(dragHandle(e, 'decay', { x: 400 / total, y: 0.25 }, total)).toEqual({ decay: 300, sustain: 0.75 })
    // The release handle sits 300 ms before the right edge; dragging it left lengthens the release.
    expect(dragHandle(e, 'release', { x: 0.7, y: 0.5 }, total)).toEqual({ release: 300, sustain: 0.5 })
    expect(dragHandle(e, 'release', { x: 0.5, y: 0.9 }, total)).toEqual({ release: 500, sustain: 0.1 })
  })
  it('clamps drags to the parameter limits', () => {
    const total = timeScale(e)
    expect(dragHandle(e, 'attack', { x: -1, y: 0 }, total)).toEqual({ attack: 0 })
    expect(dragHandle(e, 'attack', { x: 50, y: 0 }, 64000)).toEqual({ attack: 10000 })
    expect(dragHandle(e, 'decay', { x: 0, y: -2 }, total)).toEqual({ decay: 0, sustain: 1 })
    expect(dragHandle(e, 'release', { x: 2, y: 5 }, total)).toEqual({ release: 0, sustain: 0 })
    expect(dragHandle(e, 'release', { x: -1, y: 0.5 }, 64000)).toEqual({ release: 10000, sustain: 0.5 })
  })
})
