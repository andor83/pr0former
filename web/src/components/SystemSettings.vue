<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { X, AudioLines, Plus, Trash2 } from '@lucide/vue'
import { api } from '../api'
import TaskProgress from './TaskProgress.vue'
import ExternalSettings from './ExternalSettings.vue'
import UserAdministration from './UserAdministration.vue'
import LinuxAudioDevices from './LinuxAudioDevices.vue'
import type {Project,Part} from '../types'
export interface AudioSettings {input_interfaces:{id:number;name:string;enabled:boolean}[];sample_rate:number;block_size:number;interfaces:{id:number;name:string;enabled:boolean;correct_latency:boolean;latency_ms:number}[]}
const props=defineProps<{projectId?:string;project?:Project;currentUserId:string;saving:boolean;editable:boolean;active:boolean}>()
const emit=defineEmits<{close:[];saved:[settings:AudioSettings];part:[part:Part]}>()
const dialog=ref<HTMLDialogElement>(),settings=ref<AudioSettings>({sample_rate:48000,block_size:128,interfaces:[],input_interfaces:[]}),devices=ref<any>(),error=ref(''),loading=ref(true),testingPending=ref(false),busy=ref(''),testing=ref(false),position=ref(0)
const tab=ref<'audio'|'midi'|'osc'|'users'>(props.projectId?'audio':'users'), titleDialog=ref<HTMLDialogElement>(), loginTitles=ref<{first:string;second:string}[]>([]), titleBusy=ref(false), titleError=ref('')
async function editLoginTitles(){titleError.value='';try{loginTitles.value=await api('/login-titles');titleDialog.value?.showModal()}catch(e){titleError.value=String(e)}}
async function addLoginTitle(){loginTitles.value.push({first:'New title',second:'Live Electroacoustic Performance Platform'});await nextTick();const input=titleDialog.value?.querySelector<HTMLInputElement>('.login-title-row:last-child input');input?.scrollIntoView({block:'center'});input?.focus();input?.select()}
function removeLoginTitle(index:number){loginTitles.value.splice(index,1)}
async function saveLoginTitles(){titleBusy.value=true;try{loginTitles.value=await api('/login-titles','PUT',loginTitles.value);titleDialog.value?.close()}catch(e){titleError.value=String(e)}finally{titleBusy.value=false}}
async function refreshDevices(){try{devices.value=await api('/devices')}catch(e){error.value=String(e)}}
function deviceInfo(id:number,input=false){return (input?devices.value?.input_interfaces:devices.value?.interfaces)?.find((d:any)=>d.id===id)}
let disposed=false
let stopTimer:ReturnType<typeof setTimeout>|undefined
let socket:WebSocket|undefined,frame=0,last:{sample:number;sample_rate:number;received:number}|undefined
async function refresh(){const [saved,found]=await Promise.all([api<AudioSettings>('/system/audio'),api<any>('/devices')]);devices.value=found;settings.value={...saved,input_interfaces:[...(found.input_interfaces||[]).map((d:any)=>saved.input_interfaces?.find(i=>i.id===d.id)||{...d,enabled:true}),...(saved.input_interfaces||[]).filter(i=>!found.input_interfaces?.some((d:any)=>d.id===i.id))],interfaces:[...found.interfaces.map((d:any)=>saved.interfaces.find(i=>i.id===d.id)||{...d,enabled:true,correct_latency:false,latency_ms:0}),...saved.interfaces.filter(i=>!found.interfaces.some((d:any)=>d.id===i.id))]}}
async function toggleDevice(direction:'input'|'output',device:{id:number;enabled:boolean}) {
  const enabled=device.enabled;busy.value='Saving device preference';error.value='';
  try {await stop();const saved=await api<AudioSettings>(`/projects/${props.projectId}/system/audio/device`,'PUT',{direction,id:device.id,enabled});emit('saved',saved)}
  catch(e){device.enabled=!enabled;error.value=String(e)}finally{busy.value=''}
}
async function save(){busy.value='Preparing audio clips';error.value='';try{await stop();const saved=await api<AudioSettings>(`/projects/${props.projectId}/system/audio`,'PUT',settings.value);settings.value=saved;emit('saved',saved)}catch(e){error.value=String(e)}finally{busy.value=''}}
async function stop(){clearTimeout(stopTimer);if(testing.value){await api(`/projects/${props.projectId}/latency-test`,'POST',{enabled:true});testing.value=false}}
async function test(){if(testingPending.value)return;testingPending.value=true;error.value='';try{if(testing.value){await stop();return}await api(`/projects/${props.projectId}/latency-test`,'POST',{enabled:true});if(disposed){await api(`/projects/${props.projectId}/latency-test`,'POST',{enabled:true});return}testing.value=true;stopTimer=setTimeout(()=>void stop().catch(()=>{}),60000)}catch(e){error.value=String(e)}finally{testingPending.value=false}}
async function close(){if(testingPending.value)return;try{await stop();emit('close')}catch(e){error.value=String(e)}}
onMounted(async()=>{dialog.value?.showModal();try{await refresh()}catch(e){error.value=String(e)}finally{loading.value=false}if(disposed||!props.projectId)return;socket=new WebSocket(`${location.protocol==='https:'?'wss:':'ws:'}//${location.host}/api/projects/${props.projectId}/events`);socket.onmessage=e=>{const m=JSON.parse(e.data);if(m.type==='latency_test')last={...m,received:performance.now()}};const tick=()=>{if(testing.value&&last){const t=last.sample/last.sample_rate+Math.min(0.5,(performance.now()-last.received)/1000);position.value=(1-Math.cos(t*Math.PI*2))/2*100}frame=requestAnimationFrame(tick)};frame=requestAnimationFrame(tick)})
onBeforeUnmount(()=>{disposed=true;clearTimeout(stopTimer);void stop().catch(()=>{});socket?.close();cancelAnimationFrame(frame)})
</script>
<template>
<dialog ref="dialog" class="parameter-modal system-settings" aria-labelledby="system-title" @cancel.prevent="close"><header class="modal-header"><div><div class="eyebrow">PERFORMANCE SERVER</div><h2 id="system-title" @dblclick="editLoginTitles">System settings</h2></div><button class="icon-button" aria-label="Close system settings" :disabled="!!busy" @click="close"><X :size="20"/></button></header>
<div class="settings-layout"><nav role="tablist" aria-label="System settings sections"><button v-for="t in (['audio','midi','osc','users'] as const)" :key="t" :id="`system-tab-${t}`" :class="{active:tab===t}" role="tab" :aria-selected="tab===t" :aria-controls="`system-panel-${t}`" @click="tab=t">{{t==='audio'?'System audio':t==='users'?'Users':t.toUpperCase()}}</button></nav><section v-if="tab==='users'" id="system-panel-users" role="tabpanel" aria-labelledby="system-tab-users"><UserAdministration :current-user-id="currentUserId"/></section><section v-else-if="tab!=='audio' && project" :id="`system-panel-${tab}`" role="tabpanel" :aria-labelledby="`system-tab-${tab}`"><ExternalSettings :tab="tab" :project="project" :devices="devices" :editable="editable" :active="active" :saving="saving" @part="emit('part',$event)" @refresh="refreshDevices"/></section><section v-show="tab==='audio'" id="system-panel-audio" role="tabpanel" aria-labelledby="system-tab-audio"><h3 aria-label="System audio">System audio<HelpNote label="System audio">These settings apply to every project on this server. Newly discovered devices are enabled by default, except explicit Linux routes, which require opt-in to avoid duplicate or exclusive streams. Device checkboxes save immediately; other settings use Save system audio.</HelpNote></h3><p v-if="active" class="field-error">Deactivate the show before changing audio settings.</p><p v-if="error" role="alert" class="field-error">{{error}}</p><p v-if="devices?.error" class="field-error">{{devices.error}}</p>
<fieldset :disabled="!editable||!projectId||active||loading||!!busy"><label><span class="field-title">Global sample rate (Hz)<HelpNote label="Global sample rate (Hz)">Clips are converted and cached at this rate. Originals are retained. Other projects prepare their clips when opened.</HelpNote></span><select aria-label="Global sample rate (Hz)" v-model.number="settings.sample_rate"><option :value="44100">44,100 Hz</option><option :value="48000">48,000 Hz</option><option :value="88200">88,200 Hz</option><option :value="96000">96,000 Hz</option></select></label>
<label><span class="field-title">DSP block size<HelpNote label="DSP block size">Smaller blocks increase scheduling overhead and reduce the target output queue. The device driver chooses its callback buffer separately.</HelpNote></span><select aria-label="DSP block size" v-model.number="settings.block_size"><option v-for="size in [32,64,128,256,512,1024]" :key="size" :value="size">{{size}} frames · {{(size/settings.sample_rate*1000).toFixed(2)}} ms</option></select></label><h3 aria-label="Active interfaces">Active interfaces<HelpNote label="Active interfaces">Checked outputs are available in Audio output node modals. With none selected, the engine serves browser monitors only.<br /><br />Correction delays faster checked interfaces to match the largest entered latency. Separate device clocks may drift; an OS aggregate device is preferable for synchronized multichannel output.</HelpNote></h3><p v-if="!settings.interfaces.length">No audio output interfaces detected.</p>
<article v-for="i in settings.interfaces" :key="i.id" class="interface-card"><label class="check-label"><input v-model="i.enabled" type="checkbox" @change="toggleDevice('output',i)">{{deviceInfo(i.id)?.label||i.name}}<span v-if="!devices?.interfaces.some((d:any)=>d.id===i.id)"> · unavailable</span></label><p v-if="deviceInfo(i.id)?.backend" class="feature-note">{{deviceInfo(i.id).backend}} · {{deviceInfo(i.id).channels??'?'}} channels</p><code v-if="deviceInfo(i.id)?.label && i.name!==deviceInfo(i.id).label">{{i.name}}</code><p v-if="deviceInfo(i.id)?.error" class="field-error">{{deviceInfo(i.id).error}}</p><div class="latency-controls"><label class="check-label"><input v-model="i.correct_latency" :disabled="!i.enabled" type="checkbox">Correct for latency</label><label>Latency (ms)<input v-model.number="i.latency_ms" type="number" min="0" max="1000" step="0.1" :disabled="!i.enabled||!i.correct_latency"></label></div></article>
<h3 aria-label="Enabled native inputs">Enabled native inputs<HelpNote label="Enabled native inputs">Checked inputs are selectable in Audio input nodes. Saved inputs start automatically with the audio engine. All physical channels are available in Monitor; node routes can select channels 1–64.</HelpNote></h3><p v-if="!settings.input_interfaces.length">No native audio inputs are available on the server.</p><article v-for="i in settings.input_interfaces" :key="i.id" class="interface-card"><label class="check-label"><input v-model="i.enabled" type="checkbox" @change="toggleDevice('input',i)">{{deviceInfo(i.id,true)?.label||i.name}}<span v-if="!devices?.input_interfaces?.some((d:any)=>d.id===i.id)"> · unavailable</span></label><p class="feature-note">{{deviceInfo(i.id,true)?.backend}} · {{deviceInfo(i.id,true)?.channels??'?'}} channels</p><p v-if="deviceInfo(i.id,true)?.error" class="field-error">{{deviceInfo(i.id,true).error}}</p></article><button class="button primary" @click="save">Save system audio</button></fieldset>
<section class="latency-test"><h3 aria-label="Latency test">Latency test<HelpNote label="Latency test">Save your interfaces, then compare the server metronome with the bouncing indicator. Enter measured output latencies above. This is manual calibration; browser and network delay affect the visual reference. The test stops automatically after 60 seconds.</HelpNote></h3><div class="metronome-track" aria-label="Metronome position"><span :style="{left:`${position}%`}" :class="{testing}"></span></div><button class="button" :disabled="!editable||!projectId||active||loading||testingPending||!!busy" @click="test">{{testing?'Stop latency test':'Start latency test'}}</button></section>
<LinuxAudioDevices />
</section></div></dialog>
<dialog ref="titleDialog" class="parameter-modal login-title-editor" aria-labelledby="login-title-heading">
  <header class="modal-header login-title-header">
    <div><div class="eyebrow">LOGIN SCREEN · EASTER EGG</div><h2 aria-label="Login Title Pairs" id="login-title-heading">Login Title Pairs<HelpNote label="Login title pairs">Each entry supplies the two paired lines shown on the login screen. One entry is chosen at random for each visit.</HelpNote></h2></div>
    <button class="icon-button modal-close" aria-label="Close title editor" :disabled="titleBusy" @click="titleDialog?.close()"><X :size="20"/></button>
  </header>
  <div class="login-title-body">
    <div class="login-title-toolbar"><button class="button" :disabled="titleBusy" @click="addLoginTitle"><Plus :size="15"/> Add Pair</button></div>
    <p v-if="titleError" class="field-error login-title-error" role="alert">{{titleError}}</p>
    <div v-if="loginTitles.length" class="login-title-columns" aria-hidden="true"><span></span><span>First line</span><span>Second line</span><span></span></div>
    <div class="login-title-list">
      <div v-for="(title,index) in loginTitles" :key="index" class="login-title-row">
        <span class="login-title-number">{{index + 1}}</span>
        <label><span>First line</span><input v-model="title.first" :aria-label="`Pair ${index + 1} first line`"></label>
        <label><span>Second line</span><input v-model="title.second" :aria-label="`Pair ${index + 1} second line`"></label>
        <button class="icon-button danger login-title-delete" :aria-label="`Delete title pair ${index + 1}`" title="Delete pair" @click="removeLoginTitle(index)"><Trash2 :size="16"/></button>
      </div>
      <p v-if="!loginTitles.length" class="login-title-empty">No title pairs yet. Add one to keep the login screen from using the built-in defaults.</p>
    </div>
  </div>
  <footer class="modal-footer login-title-footer">
    <div><button class="button" :disabled="titleBusy" @click="titleDialog?.close()">Cancel</button><button class="button primary" :disabled="titleBusy || !loginTitles.length" @click="saveLoginTitles">{{titleBusy ? 'Saving…' : 'Save Title Pairs'}}</button></div>
  </footer>
</dialog>
<TaskProgress v-if="busy" :title="busy" detail="Converting and caching this project’s clips. Original files are retained."/>
</template>
<style scoped>.interface-card code{display:block;overflow-wrap:anywhere;font-size:11px;color:var(--muted)}</style>
