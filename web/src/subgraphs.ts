import type { Descriptor, GraphNode, GraphEdge, Signal } from './types'
export function nodeDescriptor(node: GraphNode, nodes: GraphNode[], catalog: Descriptor[]): Descriptor {
  const base = catalog.find(d => d.kind === node.kind)!
  if (node.kind === 'subgraph') {
    const ports = (direction: string) => nodes.filter(n => n.parent === node.id && n.kind.startsWith(`subgraph_${direction}_`)).map(n => ({ id: n.id, label: n.label, signal: n.kind.split('_').at(-1) as Signal, fixed_channels: n.channels }))
    return {...base, inputs: ports('input'), outputs: ports('output')}
  }
  if (node.kind.startsWith('subgraph_input_')) return {...base, inputs: []}
  if (node.kind.startsWith('subgraph_output_')) return {...base, outputs: []}
  return base
}
export function descendants(nodes: GraphNode[], ids: string[]): Set<string> {
  const found = new Set(ids)
  let changed = true
  while (changed) {
    changed = false
    for (const node of nodes) if (node.parent && found.has(node.parent) && !found.has(node.id)) { found.add(node.id); changed = true }
  }
  return found
}
export function duplicateNodes(nodes: GraphNode[], edges: GraphEdge[], id: string) {
  const included = descendants(nodes, [id])
  const ids = new Map([...included].map(id => [id, crypto.randomUUID()]))
  const copies: GraphNode[] = nodes.filter(n => included.has(n.id)).map(n => ({...JSON.parse(JSON.stringify(n)), id: ids.get(n.id)!, parent: n.parent ? ids.get(n.parent) || n.parent : null, x: n.x + (n.id === id ? 40 : 0), y: n.y + (n.id === id ? 40 : 0)}))
  const connections: GraphEdge[] = edges.filter(e => included.has(e.source) && included.has(e.target)).map(e => ({...e, id: crypto.randomUUID(), source: ids.get(e.source)!, target: ids.get(e.target)!, source_port: nodes.find(n=>n.id===e.source)?.kind === 'subgraph' ? ids.get(e.source_port) || e.source_port : e.source_port, target_port: nodes.find(n=>n.id===e.target)?.kind === 'subgraph' ? ids.get(e.target_port) || e.target_port : e.target_port}))
  return {nodes: copies, edges: connections, id: ids.get(id)!}
}
