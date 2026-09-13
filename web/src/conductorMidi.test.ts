import {afterEach,expect,test,vi} from 'vitest'
import {createConductorMidi} from './conductorMidi'
afterEach(()=>vi.unstubAllGlobals())
function environment(){
  const input=Object.assign(new EventTarget(),{id:'keys',name:'Keyboard',state:'connected'})
  const access=Object.assign(new EventTarget(),{inputs:new Map([['keys',input]])})
  vi.stubGlobal('window',{isSecureContext:true})
  vi.stubGlobal('localStorage',{getItem:()=>null,setItem:vi.fn(),removeItem:vi.fn()})
  vi.stubGlobal('navigator',{requestMIDIAccess:vi.fn(async()=>access)})
  const signal=(data:number[])=>input.dispatchEvent(Object.assign(new Event('midimessage'),{data:new Uint8Array(data)}))
  return {input,access,signal}
}
test('only the local owner connects; mode changes preserve other MIDI consumers and filter clock',async()=>{
  const {input,signal}=environment(),send=vi.fn(()=>true),midi=createConductorMidi(send),other=vi.fn()
  input.addEventListener('midimessage',other)
  midi.context('project',false,true);await midi.enable()
  expect(navigator.requestMIDIAccess).not.toHaveBeenCalled()
  midi.context('project',true,true);await midi.enable()
  expect(send).toHaveBeenLastCalledWith({type:'conductor_midi_local_devices',devices:[{id:'keys',name:'Keyboard'}]})
  expect(midi.ready.value).toBe(false);expect(midi.connecting.value).toBe(true)
  midi.receive({type:'conductor_midi_local_status',connected:true})
  expect(midi.ready.value).toBe(true);expect(midi.connecting.value).toBe(false)
  send.mockClear();signal([0xf8]);signal([0xb0,12,255]);expect(send).not.toHaveBeenCalled()
  signal([0xb3,12,100]);expect(send).toHaveBeenLastCalledWith({type:'conductor_midi_data',device:'keys',data:[0xb3,12,100]})
  midi.context('project',false,true);send.mockClear();signal([0x90,60,90]);expect(send).not.toHaveBeenCalled();expect(other).toHaveBeenCalledTimes(4)
  midi.dispose()
})
test('permission granted after changing project cannot attach a stale local source',async()=>{
  const {access,signal}=environment(),send=vi.fn(()=>true),midi=createConductorMidi(send)
  let grant!:(value:any)=>void
  ;(navigator.requestMIDIAccess as any).mockImplementation(()=>new Promise(resolve=>grant=resolve))
  midi.context('first',true,true);const enabling=midi.enable()
  midi.context('second',false,true);grant(access);await enabling;send.mockClear()
  signal([0x90,60,90]);expect(send).not.toHaveBeenCalled();expect(midi.ready.value).toBe(false)
  midi.dispose()
})
test('disconnect removes owned listeners and announces release; telemetry never writes configuration',async()=>{
  const {signal}=environment(),send=vi.fn(()=>true),midi=createConductorMidi(send)
  midi.context('project',true,true);await midi.enable();midi.disable()
  expect(send).toHaveBeenLastCalledWith({type:'conductor_midi_release'})
  send.mockClear();signal([0xb0,1,100]);expect(send).not.toHaveBeenCalled()
  midi.receive({type:'conductor_midi_devices',local:[{id:'remote',name:'Conductor keyboard'}],server:['Server keys'],selected_set:'set-b'})
  expect(midi.selectedSet.value).toBe('set-b');expect(midi.local.value[0]?.name).toBe('Conductor keyboard')
  midi.receive({type:'conductor_midi_learned',token:'capture',error:'Access changed'})
  expect(midi.status.value).toBe('Access changed');expect(send).not.toHaveBeenCalled()
  midi.dispose()
})
