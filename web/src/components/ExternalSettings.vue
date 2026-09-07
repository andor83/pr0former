<script setup lang="ts">
import {computed,onMounted,onBeforeUnmount,ref,toRaw} from 'vue'
import {api} from '../api'
import type {Part,Project} from '../types'
const props=defineProps<{tab:'midi'|'osc';project:Project;devices:any;editable:boolean;active:boolean;saving:boolean}>()
const emit=defineEmits<{part:[part:Part];refresh:[]}>()
interface OscSettings {receive_enabled:boolean;bind_addresses:string[];receive_port:number;send_enabled:boolean;send_address:string;send_port:number}
const osc=ref<OscSettings>(),status=ref<any>(),error=ref(''),busy=ref(false),selected=ref(props.project.parts[0]?.id||'')
const part=computed(()=>props.project.parts.find(p=>p.id===selected.value)||props.project.parts[0])
const interfaces=computed(()=>[{name:'All IPv4 interfaces',address:'0.0.0.0'},...(status.value?.interfaces||[]),...(osc.value?.bind_addresses||[]).filter(ip=>ip!=='0.0.0.0'&&!status.value?.interfaces?.some((i:any)=>i.address===ip)).map(address=>({name:'Unavailable saved interface',address}))])
function update(key:keyof Part,value:unknown){if(part.value)emit('part',{...part.value,[key]:value})}
function binding(ip:string,enabled:boolean){if(!osc.value)return;osc.value.bind_addresses=enabled?(ip==='0.0.0.0'?[ip]:[...osc.value.bind_addresses.filter(x=>x!=='0.0.0.0'&&x!==ip),ip]):osc.value.bind_addresses.filter(x=>x!==ip)}
async function refresh(){try{status.value=await api('/system/osc');if(!osc.value)osc.value=structuredClone(toRaw(status.value.settings))}catch(e){error.value=String(e)}}
async function save(){if(!osc.value)return;busy.value=true;error.value='';try{status.value=await api(`/projects/${props.project.id}/system/osc`,'PUT',osc.value);osc.value=structuredClone(toRaw(status.value.settings))}catch(e){error.value=String(e)}finally{busy.value=false}}
let timer:ReturnType<typeof setInterval>|undefined
onMounted(()=>{void refresh();timer=setInterval(()=>void refresh(),2000)})
onBeforeUnmount(()=>clearInterval(timer))
</script>
<template>
<section class="external-settings">
<h3>{{tab==='midi'?'MIDI':'OSC'}}</h3>
<p v-if="error" role="alert" class="field-error">{{error}}</p>
<template v-if="tab==='midi'">
<p>Devices available to this performance server, including OS virtual MIDI ports.</p>
<button class="button small" @click="emit('refresh')">Refresh MIDI devices</button>
<p v-if="devices?.midi_error" role="alert" class="field-error">{{devices.midi_error}}</p>
<h4>MIDI inputs</h4><p v-for="name in devices?.midi_inputs" :key="name">{{name}}</p><p v-if="!devices?.midi_inputs?.length">No MIDI inputs detected on the server.</p>
<p class="feature-note">Input discovery is available; MIDI input routing is not implemented yet.</p>
<h4>MIDI outputs</h4><p v-for="name in devices?.midi_outputs" :key="name">{{name}}</p><p v-if="!devices?.midi_outputs?.length">No MIDI outputs detected on the server.</p>
<p class="feature-note">Browser MIDI devices are not connected. A future browser bridge can announce them after login and permission, and mark routes unavailable when the browser disconnects.</p>
</template>
<template v-else>
<p>Network bindings apply to every project on this server. Incoming messages affect only the active show.</p>
<p v-if="status?.error" role="alert" class="field-error">{{status.error}}</p><p v-if="status?.interface_error" role="alert" class="field-error">Interface discovery failed: {{status.interface_error}}</p>
<fieldset v-if="osc" :disabled="!editable||active||busy">
<label class="check-label"><input v-model="osc.receive_enabled" type="checkbox">Receive OSC</label>
<p class="feature-note">Enabling reception allows devices on the selected networks to control the active show without a browser login.</p>
<h4>Receive interfaces (IPv4)</h4>
<label v-for="i in interfaces" :key="i.address" class="check-label"><input type="checkbox" :checked="osc.bind_addresses.includes(i.address)" @change="binding(i.address,($event.target as HTMLInputElement).checked)">{{i.name}} · {{i.address}}</label>
<p v-if="!status?.interfaces?.length">No individual IPv4 interfaces available.</p>
<label>OSC receive port<input v-model.number="osc.receive_port" type="number" min="1" max="65535" step="1"></label>
<label class="check-label"><input v-model="osc.send_enabled" type="checkbox">Send OSC</label>
<label>OSC send interface<select v-model="osc.send_address"><option v-for="i in interfaces" :key="i.address" :value="i.address">{{i.name}} · {{i.address}}</option><option v-if="!interfaces.some(i=>i.address===osc!.send_address)" :value="osc.send_address">{{osc.send_address}} · unavailable</option></select></label>
<label>OSC source port<input v-model.number="osc.send_port" type="number" min="0" max="65535" step="1"></label><p class="feature-note">Source port 0 lets the OS choose a port. The destination port belongs to each part below.</p>
<button class="button primary" @click="save">Save OSC settings</button>
</fieldset>
<p v-if="active" class="feature-note">Deactivate the show to change network bindings or part routes.</p>
<div v-if="status" class="feature-note" aria-label="OSC status">Receiving: {{status.receiving.join(', ')||'disabled'}}<br>Sending from: {{status.sending_from||'disabled'}}<br>Received {{status.received}} · Rejected {{status.rejected}} · Sent {{status.sent}}</div>
<details><summary>Incoming OSC addresses</summary><p>Send individual UDP OSC messages; bundles and timetags are not supported.</p><ul><li><code>/pr0former/play</code>, <code>/pr0former/pause</code>, <code>/pr0former/stop</code>: no arguments.</li><li><code>/pr0former/tempo</code>: one number, 1–400 BPM. Running changes wait for the next beat. A connected global clock tempo input takes precedence.</li><li><code>/pr0former/node/{id}/control</code>: one number or string matching an unconnected graphical control’s mode and limits.</li><li><code>/pr0former/node/{id}/bang</code>: no arguments; triggers an unconnected Bang control.</li></ul><p>Use the node ID listed below or in exported project JSON. Incoming values are transient and do not change saved literals or undo history. UDP delivery is best effort.</p><p v-for="n in project.graph.nodes.filter(n=>n.kind==='control_input')" :key="n.id">{{n.label}}: <code>{{n.id}}</code></p></details>
</template>
<h3>Project part routing</h3>
<label v-if="project.parts.length">Part<select v-model="selected"><option v-for="p in project.parts" :key="p.id" :value="p.id">{{p.name}}</option></select></label>
<p v-else>No score parts in this project.</p>
<fieldset v-if="part" :disabled="!editable||active||saving">
<template v-if="tab==='midi'">
<label>MIDI port name<select :value="part.midi_port||''" @change="update('midi_port',($event.target as HTMLSelectElement).value||null)"><option value="">No MIDI output</option><option v-for="name in devices?.midi_outputs" :key="name" :value="name">{{name}}</option><option v-if="part.midi_port&&!devices?.midi_outputs?.includes(part.midi_port)" :value="part.midi_port">{{part.midi_port}} · unavailable</option></select></label>
<label>MIDI channel<select aria-label="MIDI channel" :value="part.midi_channel||1" @change="update('midi_channel',Number(($event.target as HTMLSelectElement).value))"><option v-for="channel in 16" :key="channel" :value="channel">{{channel}}</option></select></label>
</template>
<template v-else>
<label>OSC IP:port<input :value="part.osc_destination||''" placeholder="192.168.1.10:9000" @change="update('osc_destination',($event.target as HTMLInputElement).value||null)"></label>
<label>OSC address<input :value="part.osc_address" placeholder="/pr0former/note" @change="update('osc_address',($event.target as HTMLInputElement).value)"></label>
<p class="feature-note">Score output sends two integers: pitch (0–127), velocity (0–127; zero releases the note). Changes save to this project.</p>
</template>
</fieldset>
</section>
</template>
<style scoped>
.external-settings label{display:flex;flex-direction:column;gap:6px;margin:12px 0}.external-settings .check-label{flex-direction:row;align-items:center}.external-settings h4{margin:18px 0 8px}.external-settings details{margin:18px 0}.external-settings code{overflow-wrap:anywhere;font-size:11px}.external-settings li{margin:10px 0}.external-settings fieldset{min-width:0}
</style>
