import { describe, expect, it } from 'vitest'
import { CONTAINER, containerFor, containerRect, grownSize, snapInside } from './containers'
import type { GraphNode } from './types'
const node = (id: string, kind: string, x: number, y: number, parameters: Record<string, number> = {}, parent: string | null = null): GraphNode => ({ id, kind, label: id, x, y, channels: 1, parameters, parent })
describe('container frames', () => {
  const big = node('big', 'container', 0, 0, { width: 1000, height: 800 })
  const small = node('small', 'container', 100, 100, { width: 400, height: 300 })
  it('uses defaults and minimums for the frame size', () => {
    expect(containerRect(node('c', 'container', 5, 6))).toEqual({ x: 5, y: 6, width: CONTAINER.defaultWidth, height: CONTAINER.defaultHeight })
    expect(containerRect(node('c', 'container', 0, 0, { width: 10, height: 10 }))).toMatchObject({ width: CONTAINER.minWidth, height: CONTAINER.minHeight })
  })
  it('assigns a node to the smallest frame around it, never a frame to a frame, and only within the same graph level', () => {
    const nodes = [big, small, node('inner', 'oscillator', 150, 150), node('outer', 'oscillator', 700, 600), node('away', 'oscillator', 2000, 20), node('nested', 'oscillator', 150, 150, {}, 'sub')]
    expect(containerFor(nodes[2]!, nodes)?.id).toBe('small')
    expect(containerFor(nodes[3]!, nodes)?.id).toBe('big')
    expect(containerFor(nodes[4]!, nodes)).toBeUndefined()
    expect(containerFor(nodes[5]!, nodes)).toBeUndefined()
    expect(containerFor(small, nodes)).toBeUndefined()
  })
  it('snaps members to the frame grid below the header', () => {
    expect(snapInside(small, 100, 100)).toEqual({ x: 116, y: 160 })
    expect(snapInside(small, 116 + 228 + 100, 160 + 24 * 3 + 11)).toEqual({ x: 116 + 228, y: 160 + 24 * 3 })
    expect(snapInside(small, 116 + 228 + 115, 160 + 24 * 3 + 13)).toEqual({ x: 116 + 228 * 2, y: 160 + 24 * 4 })
    expect(snapInside(small, 0, 0)).toEqual({ x: 116, y: 160 })
  })
  it('grows the frame to fit a member and never shrinks it', () => {
    expect(grownSize(small, 116, 160, 204, 120)).toEqual({ width: 400, height: 300 })
    expect(grownSize(small, 116 + 228 * 2, 160, 204, 120)).toEqual({ width: 116 + 228 * 2 - 100 + 204 + 16, height: 300 })
    expect(grownSize(small, 116, 160 + 24 * 10, 204, 200)).toEqual({ width: 400, height: 160 + 240 - 100 + 200 + 16 })
  })
})
