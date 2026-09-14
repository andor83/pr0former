import {describe,it,expect} from 'vitest'
import {nodeDescriptor,duplicateNodes} from './subgraphs'
import {removedScriptEdges,completions} from './scripting'
import type {GraphNode,Descriptor,ScriptConfig,GraphEdge} from './types'
const script:ScriptConfig={source:'define_input("a");define_output("b");',inputs:[{name:'a',initial:0}],outputs:[{name:'b',initial:0}],bindings:[]}
const node:GraphNode={id:'s',kind:'js_control',label:'JS',x:0,y:0,channels:1,parameters:{},script}
const base={kind:'js_control',inputs:[],outputs:[],parameters:[]} as unknown as Descriptor
describe('script graph contracts',()=>{
 it('resolves named ports and keeps MIDI separate',()=>{const d=nodeDescriptor(node,[node],[base]);expect(d.inputs.map(p=>[p.id,p.signal])).toEqual([['a','control'],['midi','midi']]);expect(d.outputs.map(p=>p.id)).toEqual(['b','midi'])})
 it('preserves source and independently clones configuration on duplicate',()=>{const copy=duplicateNodes([node],[],['s']).nodes[0]!;expect(copy.script).toEqual(script);copy.script!.inputs[0]!.name='different';expect(script.inputs[0]!.name).toBe('a')})
 it('removes only attached ports that no longer exist',()=>{
   const edges:GraphEdge[]=[{id:'in',source:'v',source_port:'out',target:'s',target_port:'a'},{id:'old',source:'s',source_port:'old',target:'v',target_port:'a'},{id:'midi',source:'s',source_port:'midi',target:'synth',target_port:'midi'}]
   expect(removedScriptEdges(edges,'s',script).map(e=>e.id)).toEqual(['old'])
 })
 it('offers input/output, engine timing and GUI console helpers',()=>{for(const helper of ['define_input','define_output','on_tick','metronome','osc','midi','console','bind_receive'])expect(completions.some(c=>c.label===helper)).toBe(true)})
})
