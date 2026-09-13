import {afterEach,expect,test,vi} from 'vitest'
import {createLocalMidiInput} from './localMidiInput'
afterEach(()=>vi.unstubAllGlobals())
function environment(){
  const input=Object.assign(new EventTarget(),{id:'twister',name:'Twister',state:'connected',open:async()=>input})
  const access=Object.assign(new EventTarget(),{inputs:new Map([['twister',input]])})
  vi.stubGlobal('window',{isSecureContext:true})
  vi.stubGlobal('navigator',{requestMIDIAccess:vi.fn(async()=>access)})
  return {input,access}
}
test('cancellation during device permission does not attach or forward later',async()=>{
  const {access,input}=environment()
  let grant!:(value:unknown)=>void
  ;(navigator.requestMIDIAccess as any).mockImplementation(()=>new Promise(resolve=>{grant=resolve}))
  const send=vi.fn(()=>true),midi=createLocalMidiInput(send)
  midi.state('node').device='twister'
  const connecting=midi.connect('node')
  midi.disconnect('node');grant(access);await connecting
  input.dispatchEvent(Object.assign(new Event('midimessage'),{data:new Uint8Array([0xb0,74,90])}))
  expect(send.mock.calls).toEqual([[{type:'local_midi_reset',node:'node'}]])
  expect(midi.state('node').connected).toBe(false)
  midi.dispose()
})
test('reconnect ignores old reset ack and detaches only its own MIDI listener',async()=>{
  const {input}=environment(),send=vi.fn(()=>true),midi=createLocalMidiInput(send),other=vi.fn()
  input.addEventListener('midimessage',other)
  midi.state('node').device='twister';await midi.connect('node')
  midi.receive({node:'node',connected:true})
  await midi.connect('node')
  midi.receive({node:'node',connected:false})
  expect(midi.state('node').connecting).toBe(true)
  midi.receive({node:'node',connected:true})
  input.dispatchEvent(Object.assign(new Event('midimessage'),{data:new Uint8Array([0xc3,12])}))
  expect(send).toHaveBeenLastCalledWith({type:'local_midi',node:'node',data:[0xc3,12]})
  midi.disconnect('node');send.mockClear()
  input.dispatchEvent(Object.assign(new Event('midimessage'),{data:new Uint8Array([0xb0,74,90])}))
  expect(send).not.toHaveBeenCalled();expect(other).toHaveBeenCalledTimes(2)
  midi.dispose()
})
