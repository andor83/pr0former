import { ref, type InjectionKey } from 'vue'
export interface MidiDevice { id:string; name:string }
export type ConductorMidi = ReturnType<typeof createConductorMidi>
export const conductorMidiKey:InjectionKey<ConductorMidi> = Symbol('conductorMidi')

/** App-lifetime connection: switching between editor and stage keeps MIDI alive. */
export function createConductorMidi(transmit:(message:Record<string,unknown>)=>boolean) {
  const local=ref<MidiDevice[]>([]), server=ref<string[]>([]), selectedSet=ref('')
  const status=ref(''), ready=ref(false), connecting=ref(false), lastEvent=ref<Record<string,any>>({})
  let access:any=null, project='', allowed=false, online=false, generation=0
  const listeners=new Map<any,(event:any)=>void>()
  function send(message:Record<string,unknown>) {
    const sent=transmit(message)
    if(!sent) status.value='Connection lost. Reconnect before binding MIDI.'
    return sent
  }
  function detach() {
    generation++
    for(const [input,listener] of listeners) input.removeEventListener('midimessage',listener)
    listeners.clear()
    if(access) access.removeEventListener('statechange',announce)
    access=null;ready.value=false;connecting.value=false
  }
  function announce() {
    if(!access||!allowed||!online)return
    const inputs=[...access.inputs.values()].filter((i:any)=>i.state!=='disconnected').slice(0,32)
    for(const [input,listener] of listeners) if(!inputs.includes(input)) {input.removeEventListener('midimessage',listener);listeners.delete(input)}
    for(const input of inputs as any[]) if(!listeners.has(input)) {
      const listener=(event:any)=>{
        const data=Array.from(event.data) as number[]
        const s=data[0]??0, length=[12,13].includes(s>>4)?2:3
        if(!allowed||!online||s<0x80||s>=0xf0||data.length!==length||data.slice(1).some(v=>v<0||v>127))return
        send({type:'conductor_midi_data',device:input.id,data})
      }
      input.addEventListener('midimessage',listener);listeners.set(input,listener)
    }
    send({type:'conductor_midi_local_devices',devices:inputs.map((i:any)=>({id:i.id,name:i.name||i.id}))})
  }
  async function enable() {
    if(!allowed||!online)return
    const attempt=++generation;connecting.value=true
    try {
      if(!window.isSecureContext)throw new Error('Local MIDI needs HTTPS or localhost.')
      const request=(navigator as any).requestMIDIAccess
      if(!request)throw new Error('This browser does not support local MIDI.')
      const result=await request.call(navigator)
      if(attempt!==generation||!allowed||!online)return
      if(access)access.removeEventListener('statechange',announce)
      access=result;access.addEventListener('statechange',announce);status.value='Connecting local MIDI…'
      try{localStorage.setItem(`pr0former.conductor-midi.enabled.${project}`,'1')}catch{}
      announce()
    }catch(error){connecting.value=false;status.value=error instanceof Error?error.message:String(error)}
  }
  function context(id:string, canSupply:boolean, connected:boolean) {
    const changed=project!==id||allowed!==canSupply||online!==connected
    if(!changed)return
    detach();project=id;allowed=canSupply;online=connected
    local.value=[];server.value=[];selectedSet.value='';lastEvent.value={}
    if(id&&connected){
      send({type:'conductor_midi_inventory'})
      let enabled=false;try{enabled=localStorage.getItem(`pr0former.conductor-midi.enabled.${id}`)==='1'}catch{}
      if(enabled&&canSupply)void enable()
    }
  }
  function receive(message:Record<string,any>) {
    if(!message.type?.startsWith('conductor_midi_'))return
    lastEvent.value=message
    if(message.type==='conductor_midi_devices') {local.value=message.local||[];server.value=message.server||[]}
    if(message.selected_set)selectedSet.value=message.selected_set
    if(message.type==='conductor_midi_local_status'){connecting.value=false;ready.value=!!message.connected;if(!message.connected)detach();else status.value='Local MIDI connected'}
    if(message.error){if(connecting.value)detach();status.value=String(message.error)}
    if(message.type==='conductor_midi_learned'&&!message.error)status.value='MIDI binding saved'
  }
  function disable(){send({type:'conductor_midi_release'});try{localStorage.removeItem(`pr0former.conductor-midi.enabled.${project}`)}catch{}detach();status.value='Local MIDI disconnected'}
  return {local,server,selectedSet,status,ready,connecting,lastEvent,send,enable,disable,context,receive,dispose:detach}
}
