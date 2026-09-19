import { describe,expect,it } from 'vitest'
import { effectiveNode,RecallFeedback } from './states'
import { duplicateNodes } from './subgraphs'
import type { GraphNode } from './types'
const node:GraphNode={id:'v',kind:'value',label:'Value',channels:1,x:20,y:30,parameters:{value:1},parent:'root'}
describe('state runtime presentation',()=>{
 it('overlays effective options without changing authored settings or layout',()=>{
  const live={...node,label:'Recalled',parameters:{value:12}}
  expect(effectiveNode(node,{v:live}).parameters.value).toBe(12)
  expect(node.parameters.value).toBe(1)
  expect(effectiveNode(node,{v:{...live,kind:'other'}})).toBe(node)
 })
 it('deduplicates events, flashes unchanged restored nodes, and excludes hidden or new nodes',()=>{
  const feedback=new RecallFeedback()
  expect(feedback.accept('one',['v','hidden'],['v','new'])).toEqual(['v'])
  expect(feedback.accept('one',['hidden'],['hidden'])).toEqual([])
  expect(feedback.accept('two',['v'],['v','new'])).toEqual(['v'])
 })
 it('duplicates banks with remapped descendant IDs',()=>{
  const root:GraphNode={...node,id:'root',kind:'subgraph',parent:null,parameters:{},states:{next_slot:2,slots:[{slot:1,name:'State 1',nodes:[node]}]}}
  const copy=duplicateNodes([root,node],[],'root')
  expect(copy.nodes[0]!.states!.slots[0]!.nodes[0]!.id).toBe(copy.nodes[1]!.id)
  expect(root.states!.slots[0]!.nodes[0]!.id).toBe('v')
 })
})
