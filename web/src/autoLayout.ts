import type { GraphEdge, GraphNode } from './types'

export interface Size { width: number; height: number }
export interface Point { x: number; y: number }
export interface LayoutOptions {
  /** Horizontal distance between the right edge of one column and the next. */
  columnGap?: number
  /** Vertical distance between nodes in one column. */
  rowGap?: number
  /** Vertical offset of a port's handle from the node top, for straight wires. */
  portOffset?: (node: GraphNode, port: string, direction: 'input' | 'output') => number
  /** Index of a port among a node's ports of the same direction, for crossing order. */
  portIndex?: (node: GraphNode, port: string, direction: 'input' | 'output') => number
  /** Number of ports of one direction on a node. */
  portCount?: (node: GraphNode, direction: 'input' | 'output') => number
}

interface Placed { node: GraphNode; size: Size; rank: number; order: number; x: number; y: number }

/**
 * Layered left-to-right layout of the selected nodes. Ranks follow the longest
 * path through the selection's internal connections; each column is ordered
 * to reduce wire crossings (barycenter sweeps with port-aware weights, then a
 * pairwise transpose pass); rows are then pulled toward their connected ports
 * so multi-input nodes receive straight wires. The selection keeps its
 * original top-left corner. Nodes outside the selection are untouched.
 */
export function autoSpace(nodes: GraphNode[], edges: GraphEdge[], ids: string[], size: (node: GraphNode) => Size, options: LayoutOptions = {}): Map<string, Point> {
  const chosen = new Set(ids)
  const members = nodes.filter(n => chosen.has(n.id))
  const result = new Map<string, Point>()
  if (members.length < 2) return result
  const columnGap = options.columnGap ?? 90, rowGap = options.rowGap ?? 40
  const portOffset = options.portOffset ?? ((node: GraphNode) => size(node).height / 2)
  const portIndex = options.portIndex ?? (() => 0)
  const portCount = options.portCount ?? (() => 1)
  const byId = new Map(members.map(n => [n.id, n]))
  const internal = edges.filter(e => chosen.has(e.source) && chosen.has(e.target) && e.source !== e.target)

  // Longest-path ranking over an acyclic selection; any unexpected cycle is
  // broken by ignoring the edge that would close it.
  const rank = new Map<string, number>()
  const preds = new Map<string, GraphEdge[]>(), succs = new Map<string, GraphEdge[]>()
  for (const n of members) { preds.set(n.id, []); succs.set(n.id, []) }
  for (const e of internal) { preds.get(e.target)!.push(e); succs.get(e.source)!.push(e) }
  const visiting = new Set<string>()
  const rankOf = (id: string): number => {
    const known = rank.get(id)
    if (known !== undefined) return known
    if (visiting.has(id)) return 0
    visiting.add(id)
    let r = 0
    for (const e of preds.get(id)!) r = Math.max(r, rankOf(e.source) + 1)
    visiting.delete(id)
    rank.set(id, r)
    return r
  }
  for (const n of members) rankOf(n.id)

  const layers: Placed[][] = []
  for (const node of members) {
    const r = rank.get(node.id)!
    ;(layers[r] ??= []).push({ node, size: size(node), rank: r, order: 0, x: 0, y: 0 })
  }
  const placed = new Map<string, Placed>()
  for (const layer of layers) {
    layer.sort((a, b) => a.node.y - b.node.y || a.node.x - b.node.x || a.node.id.localeCompare(b.node.id))
    layer.forEach((p, i) => { p.order = i; placed.set(p.node.id, p) })
  }

  // Position of a wire end within a column: the node's order plus a small
  // port term so a source feeding an upper input sorts above one feeding a
  // lower input of the same node.
  const slot = (id: string, port: string, direction: 'input' | 'output') => {
    const p = placed.get(id)!
    const count = Math.max(1, portCount(p.node, direction))
    return p.order + (portIndex(p.node, port, direction) + 0.5) / count - 0.5
  }
  const reorder = (layer: Placed[], weight: (p: Placed) => number | null) => {
    const keyed = layer.map(p => ({ p, w: weight(p) ?? p.order }))
    keyed.sort((a, b) => a.w - b.w || a.p.order - b.p.order)
    keyed.forEach(({ p }, i) => { p.order = i })
    layer.sort((a, b) => a.order - b.order)
  }
  const mean = (values: number[]) => values.length ? values.reduce((a, b) => a + b, 0) / values.length : null
  for (let sweep = 0; sweep < 12; sweep++) {
    const down = sweep % 2 === 0
    const sequence = down ? layers.slice(1) : layers.slice(0, -1).reverse()
    for (const layer of sequence) {
      reorder(layer, p => down
        ? mean(preds.get(p.node.id)!.map(e => slot(e.source, e.source_port, 'output')))
        : mean(succs.get(p.node.id)!.map(e => slot(e.target, e.target_port, 'input'))))
    }
  }
  // Pairwise transpose: swap neighbours whenever that removes crossings with
  // the adjacent columns.
  const crossings = (layer: Placed[]) => {
    let total = 0
    const count = (list: GraphEdge[], end: (e: GraphEdge) => number, other: (e: GraphEdge) => number) => {
      for (let i = 0; i < list.length; i++) for (let j = i + 1; j < list.length; j++) {
        const a = list[i]!, b = list[j]!
        if ((end(a) - end(b)) * (other(a) - other(b)) < 0) total++
      }
    }
    const incoming = layer.flatMap(p => preds.get(p.node.id)!), outgoing = layer.flatMap(p => succs.get(p.node.id)!)
    count(incoming, e => slot(e.target, e.target_port, 'input'), e => slot(e.source, e.source_port, 'output'))
    count(outgoing, e => slot(e.source, e.source_port, 'output'), e => slot(e.target, e.target_port, 'input'))
    return total
  }
  for (let pass = 0; pass < 4; pass++) {
    let improved = false
    for (const layer of layers) {
      for (let i = 0; i + 1 < layer.length; i++) {
        const before = crossings(layer)
        const a = layer[i]!, b = layer[i + 1]!
        ;[a.order, b.order] = [b.order, a.order]
        layer[i] = b; layer[i + 1] = a
        if (crossings(layer) >= before) {
          ;[a.order, b.order] = [b.order, a.order]
          layer[i] = a; layer[i + 1] = b
        } else improved = true
      }
    }
    if (!improved) break
  }

  // Columns: widest node in each rank sets the column width.
  let x = 0
  for (const layer of layers) {
    for (const p of layer) p.x = x
    x += Math.max(...layer.map(p => p.size.width)) + columnGap
  }
  // Rows: stack, then pull each node toward the wires it connects to and
  // resolve overlaps while keeping the column order.
  for (const layer of layers) {
    let y = 0
    for (const p of layer) { p.y = y; y += p.size.height + rowGap }
  }
  const settle = (layer: Placed[], desired: (p: Placed) => number | null) => {
    const wanted = layer.map(p => desired(p) ?? p.y)
    let cursor = -Infinity
    layer.forEach((p, i) => { p.y = Math.max(wanted[i]!, cursor); cursor = p.y + p.size.height + rowGap })
    // Centre the packed run on where it wanted to be.
    const shift = mean(layer.map((p, i) => wanted[i]! - p.y)) ?? 0
    for (const p of layer) p.y += shift
  }
  const wireTarget = (p: Placed, list: GraphEdge[], mine: (e: GraphEdge) => [string, 'input' | 'output'], theirs: (e: GraphEdge) => [string, 'input' | 'output']) =>
    mean(list.map(e => {
      const [otherId, otherDirection] = theirs(e), [_, myDirection] = mine(e)
      const other = placed.get(otherId)!
      const otherPort = otherDirection === 'output' ? e.source_port : e.target_port
      const myPort = myDirection === 'output' ? e.source_port : e.target_port
      return other.y + portOffset(other.node, otherPort, otherDirection) - portOffset(p.node, myPort, myDirection)
    }))
  for (let iteration = 0; iteration < 6; iteration++) {
    for (const layer of layers.slice(1)) settle(layer, p => wireTarget(p, preds.get(p.node.id)!, e => [e.target, 'input'], e => [e.source, 'output']))
    for (const layer of layers.slice(0, -1).reverse()) settle(layer, p => wireTarget(p, succs.get(p.node.id)!, e => [e.source, 'output'], e => [e.target, 'input']))
  }
  const originX = Math.min(...members.map(n => n.x)), originY = Math.min(...members.map(n => n.y))
  const minX = Math.min(...layers.flat().map(p => p.x)), minY = Math.min(...layers.flat().map(p => p.y))
  for (const p of layers.flat()) result.set(p.node.id, { x: Math.round(p.x - minX + originX), y: Math.round(p.y - minY + originY) })
  void byId
  return result
}
