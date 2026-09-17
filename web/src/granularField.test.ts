import { describe, expect, it } from 'vitest'
import type { Descriptor, GraphNode } from './types'
import { controlPoint, describePoint, fieldDefault, fieldSources, fromPixels, granularFieldDescriptor, nudge, pruneGranularEdges, removeSlotParameters, sourceWeights, swapSlotParameters, toPixels } from './granularField'

const node = (choices = 3, parameters: Record<string, number> = {}): GraphNode => ({
  id: 'field', kind: 'granular_field', label: 'Field', x: 0, y: 0, channels: 2, parameters,
  sample_choices: Array.from({ length: choices }, (_, i) => ({ asset: 10 + i, name: `Sample ${i + 1}`, nickname: i === 0 ? 'Kick' : '' })),
})

describe('granular field sources', () => {
  it('lists sample slots and only the connected live inputs', () => {
    const sources = fieldSources(node(3), ['live_2'])
    expect(sources.map(s => s.index)).toEqual([1, 2, 3, 10])
    expect(sources[0]).toMatchObject({ kind: 'sample', slot: 1, label: 'Kick', asset: 10, port: 'sample_1', weightKey: '_source_1_weight', missingKey: '_sample_1_missing' })
    expect(sources[1].label).toBe('Sample 2')
    expect(sources[3]).toMatchObject({ kind: 'live', slot: 2, port: 'live_2', keys: { x: 'live_2_x', y: 'live_2_y', tune: 'live_2_tune', gain: 'live_2_gain', buffer: 'live_2_buffer_ms' } })
    expect(fieldSources(node(9)).length).toBe(8)
    expect(fieldSources(node(0)).length).toBe(0)
  })
  it('seeds fresh slots on a ring and other defaults from the descriptor', () => {
    expect(fieldDefault('source_1_x')).toBe(0.7)
    expect(fieldDefault('source_5_x')).toBe(-0.7)
    expect(fieldDefault('live_2_x')).toBe(0.3)
    expect(fieldDefault('source_3_gain')).toBe(1)
    expect(fieldDefault('live_1_buffer_ms')).toBe(500)
    expect(fieldDefault('source_3_tune')).toBe(0)
  })
})

describe('field geometry', () => {
  it('maps corners and the centre and inverts the y axis', () => {
    expect(toPixels({ x: 0, y: 0 }, 240, 10)).toEqual({ x: 120, y: 120 })
    expect(toPixels({ x: -1, y: 1 }, 240, 10)).toEqual({ x: 10, y: 10 })
    expect(toPixels({ x: 1, y: -1 }, 240, 10)).toEqual({ x: 230, y: 230 })
    for (const p of [{ x: -1, y: -1 }, { x: 0.25, y: -0.5 }, { x: 1, y: 1 }]) {
      const back = fromPixels(toPixels(p, 240, 10), 240, 10)
      expect(back.x).toBeCloseTo(p.x); expect(back.y).toBeCloseTo(p.y)
    }
    expect(fromPixels({ x: -50, y: 900 }, 240, 10)).toEqual({ x: -1, y: -1 })
  })
  it('weights sources by a gaussian of distance and normalises them', () => {
    const points = [{ x: -0.5, y: 0 }, { x: 0.5, y: 0 }]
    const even = sourceWeights({ x: 0, y: 0 }, points, 0.5)
    expect(even[0]).toBeCloseTo(0.5); expect(even[1]).toBeCloseTo(0.5)
    const near = sourceWeights({ x: -0.5, y: 0 }, points, 0.05)
    expect(near[0]).toBeGreaterThan(0.999)
    const blended = sourceWeights({ x: -0.5, y: 0 }, points, 2)
    expect(blended[1]).toBeGreaterThan(0.3)
    expect(sourceWeights({ x: 0, y: 0 }, [], 0.5)).toEqual([])
    const far = sourceWeights({ x: 1, y: 1 }, points, 0.05)
    expect(far[0] + far[1]).toBeCloseTo(1)
  })
  it('describes and nudges points', () => {
    expect(describePoint({ x: 0.25, y: -0.5 })).toBe('x 0.25, y −0.50')
    expect(nudge({ x: 0, y: 0 }, 'ArrowRight', false)).toEqual({ x: 0.01, y: 0 })
    expect(nudge({ x: 0, y: 0 }, 'ArrowUp', true)).toEqual({ x: 0, y: 0.1 })
    expect(nudge({ x: 1, y: 0 }, 'ArrowRight', true)).toEqual({ x: 1, y: 0 })
    expect(nudge({ x: 0, y: 0 }, 'Enter', false)).toBeNull()
  })
  it('never invents a control point for a cabled axis', () => {
    expect(controlPoint(node(1, { x: 0.3, y: -0.2 }))).toEqual({ x: 0.3, y: -0.2 })
    expect(controlPoint(node(1, { x: 0.3 }), undefined, ['x'])).toBeNull()
    expect(controlPoint(node(1), { _x: 0.9, _y: 0.1 }, ['x'])).toEqual({ x: 0.9, y: 0.1 })
  })
})

describe('slot parameters follow the sample list', () => {
  it('swaps every setting of two slots, using defaults for unset keys', () => {
    const next = swapSlotParameters({ source_1_x: -0.2, source_1_tune: 5, sample_1: 77 }, 1, 2)
    expect(next).toMatchObject({ source_2_x: -0.2, source_2_tune: 5, sample_2: 77, source_1_x: 0.49, source_1_y: 0.49, source_1_tune: 0, sample_1: 0, source_1_gain: 1 })
  })
  it('shifts higher slots down when one is removed and clears the last slot', () => {
    const next = removeSlotParameters({ source_1_x: 0.1, source_2_x: 0.2, source_2_gain: 0.5, source_3_x: 0.3, sample_3: 9 }, 1, 3)
    expect(next.source_1_x).toBe(0.2); expect(next.source_1_gain).toBe(0.5)
    expect(next.source_2_x).toBe(0.3); expect(next.sample_2).toBe(9)
    expect(next.source_3_x).toBeUndefined(); expect(next.sample_3).toBeUndefined()
  })
  it('prunes cables into sample setters beyond the list', () => {
    const edges = [
      { id: 'a', source: 'v', source_port: 'out', target: 'field', target_port: 'sample_1' },
      { id: 'b', source: 'v', source_port: 'out', target: 'field', target_port: 'sample_3' },
      { id: 'c', source: 'v', source_port: 'out', target: 'field', target_port: 'live_2' },
      { id: 'd', source: 'v', source_port: 'out', target: 'other', target_port: 'sample_3' },
    ]
    expect(pruneGranularEdges(edges, 'field', 2).map(e => e.id)).toEqual(['a', 'c', 'd'])
  })
})

describe('descriptor filtering', () => {
  const base: Descriptor = {
    kind: 'granular_field', label: 'Granular Field', symbol: '⁘', category: 'Audio', description: '', aliases: [],
    inputs: [{ id: 'live_1', label: 'live 1', signal: 'audio' }, { id: 'live_2', label: 'live 2', signal: 'audio' }],
    outputs: [{ id: 'out', label: 'out', signal: 'audio' }],
    parameters: ['x', 'y', 'focus', 'pitch', 'sample_1', 'sample_2', 'sample_3', 'source_1_x', 'source_3_gain', 'live_1_x', 'live_2_buffer_ms', 'live_1_tune'].map(id => ({ id, label: id, unit: '', min: 0, max: 1, default: 0, logarithmic: false, structural: false })),
  }
  it('keeps both live ports, hides per-source settings and sample setters beyond the list', () => {
    const d = granularFieldDescriptor(base, node(2))
    expect(d.inputs.map(p => p.id)).toEqual(['live_1', 'live_2'])
    expect(d.parameters.map(p => p.id)).toEqual(['x', 'y', 'focus', 'pitch', 'sample_1', 'sample_2'])
    expect(granularFieldDescriptor(base, node(0)).parameters.map(p => p.id)).toEqual(['x', 'y', 'focus', 'pitch'])
  })
})
