import { reactive, ref } from 'vue'
export type LocalMidi = ReturnType<typeof createLocalMidiInput>
export function createLocalMidiInput(send: (message: object) => boolean) {
  const devices=ref<{id:string;name:string}[]>([]), error=ref('')
  const states=reactive<Record<string,{device:string;connected:boolean;connecting:boolean;error:string}>>({})
  let access:MIDIAccess|null=null
  const bindings=new Map<string,{input:MIDIInput;listener:(event:MIDIMessageEvent)=>void}>()
  const generations=new Map<string,number>()
  function state(node:string){return states[node]??(states[node]={device:'',connected:false,connecting:false,error:''})}
  function disconnect(node:string, notify=true){
    generations.set(node,(generations.get(node)??0)+1)
    const binding=bindings.get(node)
    if(binding){binding.input.removeEventListener('midimessage',binding.listener);bindings.delete(node)}
    const current=state(node)
    if(notify&&(current.connected||current.connecting))send({type:'local_midi_reset',node})
    current.connected=current.connecting=false
  }
  function update(){
    devices.value=[...(access?.inputs.values()??[])].filter(input=>input.state==='connected').map(input=>({id:input.id,name:input.name||input.id}))
    for(const [node,binding] of bindings)if(binding.input.state!=='connected'){
      disconnect(node);state(node).error='MIDI device disconnected. Reconnect it and connect this input again.'
    }
  }
  async function refresh(){
    error.value=''
    if(!window.isSecureContext){error.value='Local MIDI requires trusted HTTPS or localhost.';return}
    if(!navigator.requestMIDIAccess){error.value='Web MIDI is unavailable in this browser or app session. Use a Web MIDI-capable browser, or the server MIDI Input node.';return}
    try {if(!access){access=await navigator.requestMIDIAccess({sysex:false});access.addEventListener('statechange',update)}update()}
    catch(e){error.value=`MIDI access failed: ${e instanceof Error?e.message:String(e)}`}
  }
  async function connect(node:string){
    disconnect(node)
    const current=state(node), generation=generations.get(node)
    current.error='';current.connecting=true
    await refresh()
    if(generations.get(node)!==generation)return
    const input=access?.inputs.get(current.device)
    if(!input||input.state!=='connected'){current.connecting=false;current.error=error.value||'Select a connected MIDI device first.';return}
    try {await input.open()}catch(e){current.connecting=false;current.error=String(e);return}
    if(generations.get(node)!==generation)return
    const listener=(event:MIDIMessageEvent)=>{
      if(!current.connected||!event.data)return
      const data=Array.from(event.data),status=data[0]??0
      if(status<0x80||status>0xef||data.length!==([12,13].includes(status>>4)?2:3)||data.slice(1).some(v=>v>127))return
      if(!send({type:'local_midi',node,data})){
        disconnect(node);current.error='MIDI connection is unavailable or congested. Connect again.'
      }
    }
    bindings.set(node,{input,listener});input.addEventListener('midimessage',listener)
    if(!send({type:'local_midi_connect',node})){disconnect(node);current.error='Connect to the project server first.'}
  }
  function receive(message:{node:string;connected:boolean;error?:string}){
    const current=state(message.node)
    if(!current.connecting&&!current.connected)return
    if(message.error){disconnect(message.node);current.error=message.error;return}
    if(message.connected){current.connected=true;current.connecting=false}
  }
  function stop(){for(const node of Object.keys(states))disconnect(node)}
  function dispose(){stop();access?.removeEventListener('statechange',update)}
  return {devices,error,states,state,refresh,connect,disconnect,receive,stop,dispose}
}
