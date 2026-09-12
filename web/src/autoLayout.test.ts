import { describe, expect, it } from 'vitest'
import { autoSpace } from './autoLayout'
import type { GraphEdge, GraphNode } from './types'

const node = (id: string, x = 0, y = 0): GraphNode => ({ id, kind: 'gain', label: id, x, y, channels: 2, parameters: {} })
const edge = (source: string, target: string, target_port = 'in', source_port = 'out'): GraphEdge => ({ id: `${source}-${target}-${target_port}`, source, source_port, target, target_port })
const size = () => ({ width: 200, height: 150 })
const inputs = ['a', 'b']
const options = {
  portIndex: (_: GraphNode, port: string, direction: 'input' | 'output') => direction === 'input' ? Math.max(0, inputs.indexOf(port)) : 0,
  portCount: (_: GraphNode, direction: 'input' | 'output') => direction === 'input' ? inputs.length : 1,
  portOffset: (_: GraphNode, port: string, direction: 'input' | 'output') => direction === 'input' ? 65 + Math.max(0, inputs.indexOf(port)) * 30 : 75,
}

describe('autoSpace', () => {
  it('orders a chain left to right and keeps the selection origin', () => {
    const nodes = [node('c', 500, 500), node('a', 120, 300), node('b', 130, 900)]
    const placed = autoSpace(nodes, [edge('a', 'b'), edge('b', 'c')], ['a', 'b', 'c'], size)
    const a = placed.get('a')!, b = placed.get('b')!, c = placed.get('c')!
    expect(a.x).toBeLessThan(b.x)
    expect(b.x).toBeLessThan(c.x)
    expect(Math.min(a.x, b.x, c.x)).toBe(120)
    expect(Math.min(a.y, b.y, c.y)).toBe(300)
    expect(Math.abs(a.y - b.y)).toBeLessThan(1)
    expect(Math.abs(b.y - c.y)).toBeLessThan(1)
  })
  it('places sources so their wires reach a multi-input node without crossing', () => {
    // s1 feeds the lower input and s2 the upper one; s2 must end up above s1.
    const nodes = [node('s1', 0, 0), node('s2', 0, 200), node('t', 400, 100)]
    const edges = [edge('s1', 't', 'b'), edge('s2', 't', 'a')]
    const placed = autoSpace(nodes, edges, ['s1', 's2', 't'], size, options)
    expect(placed.get('s2')!.y).toBeLessThan(placed.get('s1')!.y)
    expect(placed.get('s1')!.x).toBe(placed.get('s2')!.x)
    expect(placed.get('t')!.x).toBeGreaterThan(placed.get('s1')!.x + 200)
    // No overlap in the source column.
    expect(Math.abs(placed.get('s1')!.y - placed.get('s2')!.y)).toBeGreaterThanOrEqual(150)
  })
  it('untangles a crossed pair of parallel chains', () => {
    const nodes = [node('a1', 0, 0), node('a2', 0, 200), node('b1', 300, 200), node('b2', 300, 0)]
    const edges = [edge('a1', 'b1'), edge('a2', 'b2')]
    const placed = autoSpace(nodes, edges, nodes.map(n => n.id), size)
    const above = (p: string, q: string) => placed.get(p)!.y < placed.get(q)!.y
    expect(above('a1', 'a2')).toBe(above('b1', 'b2'))
  })
  it('stacks unconnected nodes without overlap and ignores nodes outside the selection', () => {
    const nodes = [node('x', 10, 10), node('y', 12, 14), node('z', 11, 12), node('other', 0, 0)]
    const placed = autoSpace(nodes, [edge('other', 'x')], ['x', 'y', 'z'], size)
    expect(placed.has('other')).toBe(false)
    const ys = ['x', 'y', 'z'].map(id => placed.get(id)!.y).sort((p, q) => p - q)
    expect(ys[1]! - ys[0]!).toBeGreaterThanOrEqual(150)
    expect(ys[2]! - ys[1]!).toBeGreaterThanOrEqual(150)
    expect(new Set(['x', 'y', 'z'].map(id => placed.get(id)!.x)).size).toBe(1)
  })
  it('does nothing for fewer than two nodes', () => {
    expect(autoSpace([node('a')], [], ['a'], size).size).toBe(0)
  })
})
