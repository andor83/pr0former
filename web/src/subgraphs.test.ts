import { expect, it } from 'vitest'
import { duplicateNodes, descendants } from './subgraphs'
import type { GraphNode, GraphEdge } from './types'
it('duplicates a nested graph with independent IDs, ports, and settings', () => {
  const node = (id:string,parent:string|null,kind='subgraph'):GraphNode => ({id,parent,kind,label:id,x:10,y:20,channels:2,parameters:{gain:-6}})
  const nodes = [node('a',null),node('nested','a'),node('port','nested','subgraph_output_audio'),node('source','nested','input'),node('outside',null,'gain')]
  const edges:GraphEdge[] = [{id:'inner',source:'source',source_port:'out',target:'port',target_port:'in'},{id:'outer',source:'nested',source_port:'port',target:'outside',target_port:'in'}]
  expect([...descendants(nodes,['a'])]).toEqual(['a','nested','port','source'])
  const copy = duplicateNodes(nodes,edges,'a')
  expect(copy.nodes).toHaveLength(4)
  expect(copy.edges).toHaveLength(1)
  expect(copy.nodes[0].x).toBe(50)
  expect(copy.nodes[1].x).toBe(10)
  expect(copy.nodes[1].parent).toBe(copy.id)
  expect(copy.nodes[2].parent).toBe(copy.nodes[1].id)
  expect(copy.edges[0].source).toBe(copy.nodes[3].id)
  copy.nodes[0].parameters.gain=0
  expect(nodes[0].parameters.gain).toBe(-6)
})

it('duplicates all selected roots and their connecting cables',()=>{
  const node=(id:string):GraphNode=>({id,kind:'add',label:id,x:0,y:0,channels:2,parameters:{a:2,b:3}})
  const nodes=[node('a'),node('b'),node('outside')],edges:GraphEdge[]=[{id:'ab',source:'a',source_port:'out',target:'b',target_port:'a'},{id:'bo',source:'b',source_port:'out',target:'outside',target_port:'a'}]
  const copy=duplicateNodes(nodes,edges,['a','b'])
  expect(copy.nodes).toHaveLength(2);expect(copy.edges).toHaveLength(1)
  expect(copy.nodes.every(n=>n.x===40&&n.y===40)).toBe(true)
  expect(copy.edges[0].source).toBe(copy.nodes[0].id);expect(copy.edges[0].target).toBe(copy.nodes[1].id)
})

import { makeSubgraph } from './subgraphs'
import type { Descriptor } from './types'
it('grouping keeps existing parent ports through matching boundary proxies',()=>{
  const n=(id:string,kind:string,parent:string|null):GraphNode=>({id,kind,parent,label:id,x:0,y:0,channels:2,parameters:{}})
  const nodes=[n('parent','subgraph',null),n('in','subgraph_input_audio','parent'),n('out','subgraph_output_audio','parent'),n('source','input',null),n('sink','output',null)]
  const edges:GraphEdge[]=[{id:'enter',source:'source',source_port:'out',target:'parent',target_port:'in'},{id:'inner',source:'in',source_port:'out',target:'out',target_port:'in'},{id:'leave',source:'parent',source_port:'out',target:'sink',target_port:'in'}]
  const id=makeSubgraph(nodes,edges,['in','out'],[])
  expect(nodes.find(n=>n.id==='in')!.parent).toBe(id)
  const inlet=nodes.find(n=>n.parent==='parent'&&n.kind==='subgraph_input_audio')!
  const outlet=nodes.find(n=>n.parent==='parent'&&n.kind==='subgraph_output_audio')!
  expect(edges.find(e=>e.id==='enter')!.target_port).toBe(inlet.id)
  expect(edges.find(e=>e.id==='leave')!.source_port).toBe(outlet.id)
  expect(edges.some(e=>e.source===inlet.id&&e.target===id&&e.target_port==='in')).toBe(true)
  expect(edges.some(e=>e.source===id&&e.source_port==='out'&&e.target===outlet.id)).toBe(true)
})
it('grouping derives audio widths and spectral formats for crossing connections',()=>{
  const descriptors:Descriptor[]=['audio','spectral'].map(signal=>({kind:signal,label:signal,symbol:'',category:'',description:'',aliases:[],parameters:[],inputs:[{id:'in',label:'Input',signal:signal as 'audio'|'spectral'}],outputs:[{id:'out',label:'Output',signal:signal as 'audio'|'spectral'}]}))
  for(const kind of ['audio','spectral']){
    const nodes:GraphNode[]=['a','b','c'].map((id):GraphNode=>({id,kind,label:id,x:0,y:0,channels:8,parameters:kind==='spectral'?{size:256,overlap:2}:{}}))
    const edges:GraphEdge[]=[{id:'ab',source:'a',source_port:'out',target:'b',target_port:'in'},{id:'bc',source:'b',source_port:'out',target:'c',target_port:'in'}]
    const id=makeSubgraph(nodes,edges,['b'],descriptors),ports=nodes.filter(n=>n.kind.startsWith('subgraph_'))
    expect(ports.map(n=>n.channels)).toEqual([8,8])
    if(kind==='spectral')expect(ports.map(n=>n.parameters)).toEqual([{size:256,overlap:2},{size:256,overlap:2}])
    expect(edges.find(e=>e.id==='ab')!.target).toBe(id);expect(edges.find(e=>e.id==='bc')!.source).toBe(id)
  }
})
