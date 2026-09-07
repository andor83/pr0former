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
