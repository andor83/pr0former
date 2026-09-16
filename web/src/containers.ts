// Container frames are UI-only grouping nodes. Membership is geometric: a node
// whose anchor lies inside a frame belongs to it, so nothing extra is persisted.
import type { GraphNode } from './types'
export const CONTAINER = { minWidth: 280, minHeight: 160, defaultWidth: 520, defaultHeight: 320, header: 44, pad: 16, column: 228, row: 24, nodeWidth: 204, nodeHeight: 120 }
export interface Rect { x: number; y: number; width: number; height: number }
export function isContainer(node: { kind: string }): boolean { return node.kind === 'container' }
export function containerRect(node: GraphNode): Rect {
  return { x: node.x, y: node.y, width: Math.max(CONTAINER.minWidth, node.parameters.width ?? CONTAINER.defaultWidth), height: Math.max(CONTAINER.minHeight, node.parameters.height ?? CONTAINER.defaultHeight) }
}
/** The smallest frame (in the same graph level) holding the node's anchor point. Frames never nest. */
export function containerFor(node: { id: string; kind: string; x: number; y: number; parent?: string | null }, nodes: GraphNode[]): GraphNode | undefined {
  if (isContainer(node)) return undefined
  const px = node.x + 24, py = node.y + 16
  let best: GraphNode | undefined, area = Infinity
  for (const frame of nodes) {
    if (!isContainer(frame) || frame.id === node.id || (frame.parent ?? null) !== (node.parent ?? null)) continue
    const r = containerRect(frame)
    if (px < r.x || px > r.x + r.width || py < r.y || py > r.y + r.height) continue
    if (r.width * r.height < area) { area = r.width * r.height; best = frame }
  }
  return best
}
/** Snap an absolute position to the frame's column/row grid, inside its header and padding. */
export function snapInside(frame: GraphNode, x: number, y: number): { x: number; y: number } {
  const r = containerRect(frame)
  const column = Math.max(0, Math.round((x - r.x - CONTAINER.pad) / CONTAINER.column))
  const row = Math.max(0, Math.round((y - r.y - CONTAINER.header - CONTAINER.pad) / CONTAINER.row))
  return { x: r.x + CONTAINER.pad + column * CONTAINER.column, y: r.y + CONTAINER.header + CONTAINER.pad + row * CONTAINER.row }
}
/** The frame size that keeps a member at (x, y) with the given rendered size inside its padding. Frames only grow. */
export function grownSize(frame: GraphNode, x: number, y: number, width = CONTAINER.nodeWidth, height = CONTAINER.nodeHeight): { width: number; height: number } {
  const r = containerRect(frame)
  return { width: Math.max(r.width, Math.ceil(x - r.x + width + CONTAINER.pad)), height: Math.max(r.height, Math.ceil(y - r.y + height + CONTAINER.pad)) }
}
