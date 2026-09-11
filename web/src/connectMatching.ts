import type {Descriptor,GraphNode,GraphEdge} from './types'
import {nodeDescriptor} from './subgraphs'
export function matchingPorts(source:GraphNode,target:GraphNode,nodes:GraphNode[],edges:GraphEdge[],descriptors:Descriptor[]){
  const a=nodeDescriptor(source,nodes,descriptors),b=nodeDescriptor(target,nodes,descriptors)
  if(!a||!b)return []
  const inputs=[...b.inputs,...b.parameters.filter(p=>!p.structural).map(p=>({id:p.id,label:p.label,signal:'control' as const,fixed_channels:null}))]
  const pairs:{source_port:string;target_port:string}[]=[]
  // A complete MIDI cable is the compact default for note-oriented nodes.
  // Users can still wire the individual control ports explicitly afterward.
  const midiIn=b.inputs.find(p=>p.signal==='midi'), midiOut=a.outputs.find(p=>p.signal==='midi')
  if(midiIn&&midiOut&&!edges.some(e=>e.target===target.id&&e.target_port===midiIn.id)&&!edges.some(e=>e.source===source.id&&e.target===target.id&&e.source_port===midiOut.id&&e.target_port===midiIn.id))
    return [{source_port:midiOut.id,target_port:midiIn.id}]
  for(const input of inputs){
    if(input.signal!=='control'&&edges.some(e=>e.target===target.id&&e.target_port===input.id))continue
    const output=a.outputs.find(o=>o.signal===input.signal&&(o.id===input.id||o.label.trim().toLowerCase()===input.label.trim().toLowerCase())&&(o.signal==='control'||(o.fixed_channels??source.channels)===(input.fixed_channels??target.channels)))
    if(!output||edges.some(e=>e.source===source.id&&e.target===target.id&&e.source_port===output.id&&e.target_port===input.id))continue
    if(input.signal==='spectral'&&((source.parameters.size??1024)!==(target.parameters.size??1024)||(source.parameters.overlap??4)!==(target.parameters.overlap??4)))continue
    pairs.push({source_port:output.id,target_port:input.id})
  }
  return pairs
}
export function selectionOrder(previous:string[],selected:string[],nodes:GraphNode[]){
  const old=previous.filter(id=>selected.includes(id)),added=selected.filter(id=>!previous.includes(id))
  if(added.length>1)added.sort((a,b)=>{const x=nodes.find(n=>n.id===a),y=nodes.find(n=>n.id===b);return (x?.x??0)-(y?.x??0)||(x?.y??0)-(y?.y??0)||a.localeCompare(b)})
  return [...old,...added]
}
