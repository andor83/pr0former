import { newId } from './id'
import type { Descriptor, GraphNode, GraphEdge, Signal } from './types'
export function nodeDescriptor(node: GraphNode, nodes: GraphNode[], catalog: Descriptor[]): Descriptor {
  const base = catalog.find(d => d.kind === node.kind)!
  if(node.kind==='pitch_tracker')return {...base,outputs:base.outputs.slice(0,node.parameters.slots??1)}
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
export function duplicateNodes(nodes: GraphNode[], edges: GraphEdge[], selection: string | string[]) {
  const roots = Array.isArray(selection) ? selection : [selection]
  const included = descendants(nodes, roots)
  const ids = new Map([...included].map(id => [id, newId()]))
  const copies: GraphNode[] = nodes.filter(n => included.has(n.id)).map(n => ({...JSON.parse(JSON.stringify(n)), id: ids.get(n.id)!, parent: n.parent ? ids.get(n.parent) || n.parent : null, x: n.x + (roots.includes(n.id) ? 40 : 0), y: n.y + (roots.includes(n.id) ? 40 : 0)}))
  const connections: GraphEdge[] = edges.filter(e => included.has(e.source) && included.has(e.target)).map(e => ({...e, id: newId(), source: ids.get(e.source)!, target: ids.get(e.target)!, source_port: nodes.find(n=>n.id===e.source)?.kind === 'subgraph' ? ids.get(e.source_port) || e.source_port : e.source_port, target_port: nodes.find(n=>n.id===e.target)?.kind === 'subgraph' ? ids.get(e.target_port) || e.target_port : e.target_port}))
  return {nodes: copies, edges: connections, id: ids.get(roots[0]!)!}
}

export function makeSubgraph(nodes: GraphNode[], edges: GraphEdge[], selection: string[], catalog: Descriptor[]) {
  const chosen = nodes.filter(n=>selection.includes(n.id))
  if (!chosen.length) throw new Error('Select nodes to group.')
  if (chosen.some(n=>(n.parent||null)!==(chosen[0]!.parent||null))) throw new Error('Select nodes in one graph.')
  const group: GraphNode = {id:newId(),kind:'subgraph',label:'Subgraph',parent:chosen[0]!.parent||null,x:Math.min(...chosen.map(n=>n.x)),y:Math.min(...chosen.map(n=>n.y)),channels:2,parameters:{}}
  const original = new Map(nodes.map(n=>[n.id, JSON.parse(JSON.stringify(n)) as GraphNode]))
  const added:GraphEdge[]=[], outputs=new Map<string,string>()
  // Preserve the containing graph's public ports when its own boundary nodes
  // are selected: leave equivalent proxies at that level and wire into the group.
  for(const n of chosen.filter(n=>n.kind.startsWith('subgraph_'))) {
    const input=n.kind.startsWith('subgraph_input_'), proxy={...JSON.parse(JSON.stringify(n)),id:newId(),x:group.x+(input?-240:300)} as GraphNode
    nodes.push(proxy)
    for(const e of edges){if(input&&e.target===n.parent&&e.target_port===n.id)e.target_port=proxy.id;if(!input&&e.source===n.parent&&e.source_port===n.id)e.source_port=proxy.id}
    added.push(input?{id:newId(),source:proxy.id,source_port:'out',target:group.id,target_port:n.id}:{id:newId(),source:group.id,source_port:n.id,target:proxy.id,target_port:'in'})
  }
  for (const n of chosen) {n.parent=group.id;n.x=n.x-group.x+240;n.y=n.y-group.y+100}
  let ins=0, outs=0
  const boundary=(nodeId:string,portId:string,direction:'input'|'output')=>{
    const n=original.get(nodeId)!, d=nodeDescriptor(n,nodes,catalog)
    const port=(direction==='input'?d.inputs:d.outputs).find(p=>p.id===portId)
    const parameter=direction==='input'?d.parameters.find(p=>p.id===portId):undefined
    if (!port&&!parameter) throw new Error('Missing port while grouping.')
    const signal=port?.signal||'control', contract=n.kind==='subgraph'?original.get(portId)!:n
    const boundary:GraphNode={id:newId(),kind:`subgraph_${direction}_${signal}`,label:`${n.label} · ${port?.label||parameter?.label}`,parent:group.id,channels:signal==='control'?1:port?.fixed_channels||contract.channels,x:direction==='input'?0:Math.max(...chosen.map(n=>n.x))+300,y:80+(direction==='input'?ins++:outs++)*190,parameters:signal==='spectral'?{size:contract.parameters.size??1024,overlap:contract.parameters.overlap??4}:{}}
    nodes.push(boundary);return boundary.id
  }
  for (const edge of edges) {
    const from=selection.includes(edge.source), to=selection.includes(edge.target)
    if (from===to) continue
    if (to) {
      const port=boundary(edge.target,edge.target_port,'input')
      added.push({id:newId(),source:port,source_port:'out',target:edge.target,target_port:edge.target_port})
      edge.target=group.id;edge.target_port=port
    } else {
      const key=JSON.stringify([edge.source,edge.source_port]);let port=outputs.get(key)
      if(!port){port=boundary(edge.source,edge.source_port,'output');outputs.set(key,port);added.push({id:newId(),source:edge.source,source_port:edge.source_port,target:port,target_port:'in'})}
      edge.source=group.id;edge.source_port=port
    }
  }
  nodes.push(group);edges.push(...added);return group.id
}
