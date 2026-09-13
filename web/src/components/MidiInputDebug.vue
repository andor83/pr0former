<script setup lang="ts">
import {computed,ref,watch} from 'vue'
import {decodeMidiDebug,type MidiDebugMessage} from '../midiDebug'
const props=defineProps<{values?:Record<string,number>;active:boolean;stale:boolean}>()
type Observation=MidiDebugMessage&{sequence:number;arrivals:number}
const history=ref<Observation[]>([])
let last=-1
const live=computed(()=>props.active&&!props.stale)
const current=computed(()=>!props.values?._midi_received?null:decodeMidiDebug(props.values._midi_status!,props.values._midi_data1!,props.values._midi_data2!))
watch(()=>[props.values,live.value],()=>{
  if(!live.value||!current.value)return
  const sequence=props.values!._midi_received!
  if(sequence===last)return
  if(sequence<last)history.value=[]
  const arrivals=last<0||sequence<last?1:sequence-last
  last=sequence
  history.value=[{...current.value,sequence,arrivals},...history.value].slice(0,64)
},{immediate:true})
const lastControls=computed(()=>['Control change','Pitch bend','Program change','Channel pressure','Poly pressure'].map(type=>({type,message:history.value.find(item=>item.type===type)})))
</script>
<template>
  <section class="midi-debug parameter-row" aria-label="MIDI message debug">
    <div class="debug-heading"><h3>MIDI messages</h3><span>{{active?stale?'Stale · history frozen':'Live':'Engine off · history frozen'}}</span></div>
    <p>Received: <output aria-label="MIDI received count">{{live?values?._midi_received??0:'—'}}</output> · Queue drops: <output aria-label="MIDI dropped count">{{live?values?._midi_event_dropped??0:'—'}}</output></p>
    <div v-if="live&&current" class="current-message" aria-label="Latest MIDI message">
      <strong>{{current.type}}</strong><span>Channel {{current.channel}} · {{current.detail}}</span><output>{{current.value}}</output><code>{{current.raw}}</code>
      <meter :value="current.value" :max="current.max" min="0" :aria-label="`${current.type} value`" />
    </div>
    <p v-else class="feature-note">{{live?'Waiting for MIDI channel messages…':'Enable the engine and wait for fresh telemetry to inspect incoming MIDI.'}}</p>
    <div class="control-readouts">
      <label v-for="item in lastControls" :key="item.type">Last {{item.type.toLowerCase()}}<output :aria-label="`Last MIDI ${item.type.toLowerCase()}`">{{live&&item.message?`Ch ${item.message.channel} · ${item.message.detail} · ${item.message.value}`:'—'}}</output></label>
    </div>
    <div class="debug-heading"><h4>Observed history</h4><button class="text-button" :disabled="!history.length" @click="history=[]">Clear observed MIDI history</button></div>
    <p class="feature-note">Sampled at 20 updates/second, after the input channel filter. Each row shows the last message in an update; +N is the number received since the previous observation. Fast messages can be skipped here. This is not a complete event log. System messages and SysEx are not forwarded.</p>
    <div v-if="history.length" class="midi-history" tabindex="0" aria-label="Observed MIDI messages">
      <table><thead><tr><th>Count</th><th>Type</th><th>Ch</th><th>Detail</th><th>Value</th><th>Hex bytes</th></tr></thead><tbody><tr v-for="item in history" :key="item.sequence"><td>{{item.sequence}} <small v-if="item.arrivals>1">(+{{item.arrivals}})</small></td><td>{{item.type}}</td><td>{{item.channel}}</td><td>{{item.detail}}</td><td>{{item.value}}</td><td><code>{{item.raw}}</code></td></tr></tbody></table>
    </div>
    <p v-else class="feature-note">No observations yet.</p>
  </section>
</template>
<style scoped>
.debug-heading{display:flex;align-items:center;justify-content:space-between;gap:12px}.debug-heading h3,.debug-heading h4{margin:0}.debug-heading span{color:var(--muted);font-size:11px}.midi-debug{display:grid;gap:12px}.midi-debug p{margin:0}.current-message{display:grid;grid-template-columns:1fr auto;gap:6px 14px;background:#152629;padding:12px;border:1px solid #355459;border-radius:6px}.current-message strong{color:var(--cyan)}.current-message span{grid-column:1}.current-message output{grid-column:2;grid-row:1/3;font-size:24px;color:var(--amber);font-variant-numeric:tabular-nums}.current-message meter{width:100%;grid-column:1/-1;accent-color:var(--cyan)}.control-readouts{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:10px}.control-readouts label{font-size:11px;color:var(--muted)}.control-readouts output{display:block;margin-top:5px;color:var(--amber);font-variant-numeric:tabular-nums}.midi-history{max-height:220px;overflow:auto;border:1px solid var(--border);border-radius:6px}.midi-history table{width:100%;font-size:11px;border-collapse:collapse;text-align:left;white-space:nowrap}.midi-history th,.midi-history td{padding:7px 9px;border-bottom:1px solid var(--border)}.midi-history th{position:sticky;top:0;background:#20262e}.midi-history small{color:var(--muted)}.midi-debug code{font-size:11px;color:var(--cyan)}
</style>
