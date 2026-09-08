<script setup lang="ts">
import {nextTick,onBeforeUnmount,onMounted,ref} from 'vue'
import {api} from '../api'
import {midiNoteLabel,type SampleEntry} from '../samples'
const props=defineProps<{projectId:string;sample:SampleEntry}>()
const emit=defineEmits<{close:[];saved:[]}>()
const dialog=ref<HTMLDialogElement>(),canvas=ref<HTMLCanvasElement>(),audio=ref<HTMLAudioElement>()
const draft=ref({...props.sample}),busy=ref(false),error=ref(''),url=ref(''),position=ref(0),playing=ref(false)
const controller=new AbortController();let context:AudioContext|undefined
async function save(){busy.value=true;try{await api(`/projects/${props.projectId}/samples/${props.sample.id}`,'PUT',draft.value);emit('saved');emit('close')}catch(e){error.value=String(e)}finally{busy.value=false}}
function seek(value:number){position.value=Math.max(0,Math.min(props.sample.duration,value));if(audio.value)audio.value.currentTime=position.value}
function scrub(e:PointerEvent){const target=e.currentTarget as HTMLElement;target.setPointerCapture(e.pointerId);seekAt(e,target)}
function seekAt(e:PointerEvent,target:HTMLElement){const box=target.getBoundingClientRect();seek((e.clientX-box.left)/box.width*props.sample.duration)}
async function play(){try{if(!audio.value)return;if(playing.value)audio.value.pause();else await audio.value.play()}catch(e){error.value=String(e)}}
onMounted(async()=>{
  dialog.value?.showModal()
  try{
    const response=await fetch(`/api/projects/${props.projectId}/samples/${props.sample.id}/audio`,{signal:controller.signal});if(!response.ok)throw new Error('Sample audio unavailable')
    const bytes=await response.arrayBuffer();if(controller.signal.aborted)return
    context=new AudioContext();const buffer=await context.decodeAudioData(bytes.slice(0));await context.close();context=undefined
    if(controller.signal.aborted)return
    url.value=URL.createObjectURL(new Blob([bytes],{type:'audio/wav'}));await nextTick()
    const el=canvas.value;if(!el)return;el.width=1000;el.height=buffer.numberOfChannels*60
    const c=el.getContext('2d')!;c.fillStyle='#121819';c.fillRect(0,0,el.width,el.height)
    for(let ch=0;ch<buffer.numberOfChannels;ch++){const values=buffer.getChannelData(ch);c.strokeStyle=ch%2?'#d9ac67':'#75cec5';c.beginPath();for(let x=0;x<el.width;x++){const start=Math.floor(x*values.length/el.width),end=Math.max(start+1,Math.floor((x+1)*values.length/el.width));let lo=0,hi=0;for(let i=start;i<end;i++){lo=Math.min(lo,values[i]??0);hi=Math.max(hi,values[i]??0)}c.moveTo(x,ch*60+30-hi*27);c.lineTo(x,ch*60+30-lo*27)}c.stroke()}
  }catch(e){if(!controller.signal.aborted)error.value=String(e)}
})
onBeforeUnmount(()=>{controller.abort();audio.value?.pause();if(context)void context.close();if(url.value)URL.revokeObjectURL(url.value);dialog.value?.close()})
</script>
<template>
<dialog ref="dialog" class="parameter-modal sample-editor" aria-label="Sample metadata" @cancel.prevent="!busy && emit('close')">
<header class="modal-header"><h2>Sample metadata</h2><button aria-label="Close sample metadata" class="icon-button" :disabled="busy" @click="emit('close')">×</button></header>
<div class="parameter-list">
  <p>{{sample.channels}} channels · {{sample.sample_rate.toLocaleString()}} Hz · {{sample.duration.toFixed(2)}} s · {{sample.author}}</p>
  <div class="sample-waveform" @pointerdown.prevent="scrub" @pointermove.prevent="e=>{if((e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId))seekAt(e,e.currentTarget as HTMLElement)}"><canvas ref="canvas" aria-label="Full sample waveform"></canvas><i :style="{left:`${position/Math.max(.001,sample.duration)*100}%`}"></i></div>
  <input aria-label="Sample position" type="range" min="0" :max="sample.duration" step="0.001" :value="position" :disabled="!url" @input="seek(Number(($event.target as HTMLInputElement).value))">
  <div class="sample-playback"><button class="button" :disabled="!url" @click="play">{{playing?'Pause sample':'Play sample'}}</button><output>{{position.toFixed(2)}} / {{sample.duration.toFixed(2)}} s</output></div>
  <audio ref="audio" :src="url||undefined" @timeupdate="position=audio?.currentTime||0" @play="playing=true" @pause="playing=false" @ended="playing=false"></audio>
  <label>Name<input v-model="draft.name" maxlength="256" :disabled="!sample.can_edit||busy"></label>
  <label>Category<input v-model="draft.category" maxlength="128" :disabled="!sample.can_edit||busy"></label>
  <label>Tags<input v-model="draft.tags" placeholder="piano, soft, loop…" maxlength="1024" :disabled="!sample.can_edit||busy"></label>
  <label>Description<textarea v-model="draft.description" maxlength="4096" :disabled="!sample.can_edit||busy"></textarea></label>
  <div class="sample-musical"><label>BPM<input v-model.number="draft.bpm" type="number" min="1" max="400" :disabled="!sample.can_edit||busy" @change="draft.bpm=draft.bpm||null"></label><label>Musical key<input v-model="draft.musical_key" maxlength="64" placeholder="C minor" :disabled="!sample.can_edit||busy"></label></div>
  <label>Root pitch<select aria-label="Root pitch" :value="draft.root_note??''" :disabled="!sample.can_edit||busy" @change="draft.root_note=($event.target as HTMLSelectElement).value===''?null:Number(($event.target as HTMLSelectElement).value)"><option value="">Not set</option><option v-for="note in 128" :key="note-1" :value="note-1">{{midiNoteLabel(note-1)}}</option></select></label>
  <p class="feature-note">Sets the root MIDI note when this sample is assigned to a pitched sampler. Existing sampler settings and connected root-note controls are preserved.</p>
  <label class="check-label"><input v-model="draft.global" type="checkbox" :disabled="!sample.can_publish||busy">Global — available to every user and project</label>
  <p class="feature-note">Project members can access samples already added to their project. Turning Global off hides this sample from future browsing; existing project copies remain available.</p>
  <p v-if="error" class="field-error" role="alert">{{error}}</p>
</div>
<footer class="modal-footer"><button class="button primary" :disabled="!sample.can_edit||busy||!draft.name.trim()" @click="save">Save metadata</button></footer>
</dialog>
</template>
<style scoped>
.modal-header{display:flex;align-items:center;justify-content:space-between}.sample-editor input[type="range"]{width:100%;accent-color:var(--cyan)}
.sample-editor{width:min(800px,95vw)}.sample-editor label{display:flex;flex-direction:column;gap:7px;margin:14px 0}.sample-editor .check-label{flex-direction:row}.sample-editor textarea{min-height:90px;background:#121819;color:var(--white);border:1px solid var(--line);padding:10px}.sample-waveform{position:relative;touch-action:none;cursor:crosshair}.sample-waveform canvas{display:block;width:100%;min-height:60px}.sample-waveform i{position:absolute;top:0;bottom:0;width:2px;background:var(--white);pointer-events:none}.sample-musical,.sample-playback{display:flex;gap:16px;align-items:center}.sample-musical label{flex:1;min-width:0}
</style>
