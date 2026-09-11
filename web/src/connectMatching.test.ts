import {describe,it,expect} from 'vitest'
import {matchingPorts,selectionOrder} from './connectMatching'
import type {GraphNode,Descriptor} from './types'
const node=(id:string,x:number):GraphNode=>({id,kind:id,label:id,x,y:0,channels:2,parameters:{}})
describe('matching port connections',()=>{
  it('uses chronological selection and left-to-right simultaneous selection',()=>{
    const nodes=[node('left',0),node('right',400)]
    expect(selectionOrder([],['right','left'],nodes)).toEqual(['left','right'])
    expect(selectionOrder(['right'],['left','right'],nodes)).toEqual(['right','left'])
    expect(selectionOrder(['right','left'],['left'],nodes)).toEqual(['left'])
  })
  it('matches names and types, checks widths, and skips existing edges',()=>{
    const a=node('a',0),b=node('b',400)
    const base={label:'Node',symbol:'',category:'Control',description:'',aliases:[],parameters:[]}
    const descriptors:Descriptor[]=[{...base,kind:'a',inputs:[],outputs:[{id:'pitch',label:'Pitch',signal:'control'},{id:'audio',label:'Audio',signal:'audio'},{id:'bad',label:'Bad',signal:'spectral'}]},{...base,kind:'b',outputs:[],inputs:[{id:'pitch',label:'Pitch',signal:'control'},{id:'audio',label:'Audio',signal:'audio',fixed_channels:1},{id:'bad',label:'Bad',signal:'control'}]}]
    expect(matchingPorts(a,b,[a,b],[],descriptors)).toEqual([{source_port:'pitch',target_port:'pitch'}])
    expect(matchingPorts(a,b,[a,b],[{id:'e',source:'a',source_port:'pitch',target:'b',target_port:'pitch'}],descriptors)).toEqual([])
  })
  it('prefers one MIDI cable when note controls overlap',()=>{
    const a=node('a',0),b=node('b',400)
    const base={label:'Node',symbol:'',category:'Audio',description:'',aliases:[],parameters:[]}
    const descriptors:Descriptor[]=[
      {...base,kind:'a',inputs:[],outputs:[{id:'pitch',label:'Pitch',signal:'control'},{id:'midi',label:'MIDI',signal:'midi'}]},
      {...base,kind:'b',outputs:[],inputs:[{id:'pitch',label:'Pitch',signal:'control'},{id:'midi',label:'MIDI',signal:'midi'}]},
    ]
    expect(matchingPorts(a,b,[a,b],[],descriptors)).toEqual([{source_port:'midi',target_port:'midi'}])
  })
})
