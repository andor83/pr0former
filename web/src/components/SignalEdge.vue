<script setup lang="ts">
import { computed, inject, onBeforeUnmount, ref, watch } from 'vue'
import type { ComputedRef, Ref, ShallowRef } from 'vue'
import type { EdgeProps } from '@vue-flow/core'
import { BaseEdge, EdgeLabelRenderer, getBezierPath } from '@vue-flow/core'
import type { Telemetry } from '../types'
const props = defineProps<EdgeProps<{ signal: string; channels: number; projectId:string; preview?:boolean }>>()
// Live state comes from the shared telemetry reference, like nodes, so the
// edge list itself never changes on a telemetry tick.
const telemetry=inject<ShallowRef<Telemetry|null>>('telemetry')
const telemetryStale=inject<ComputedRef<boolean>>('telemetryStale')
const graphActive=inject<Ref<boolean>>('graphActive')
const active=computed(()=>!!graphActive?.value&&!(telemetryStale?.value??true))
const values=computed(()=>telemetry?.value?.values?.[props.source])
// Wires that close a feedback loop are read one sample late by the engine.
const feedback=computed(()=>!!telemetry?.value?.feedback_edges?.includes(props.id))
const path = computed(() => getBezierPath(props))
const anchor=ref({x:0,y:0})
const popupStyle=computed(()=>({left:`${Math.max(12,Math.min(window.innerWidth-276,anchor.value.x-126))}px`,top:`${Math.max(12,Math.min(window.innerHeight-(84+(props.data?.channels||1)*50)-12,anchor.value.y+16))}px`}))
const hovering=ref(false),channels=ref<number[][]>([]),error=ref(''),sampleRate=ref(48000)
const midiRows = computed(() => Object.entries(values.value || {})
  .filter(([key]) => key === 'gate' || key.startsWith('_key'))
  .map(([key, value]) => ({ key: key === 'gate' ? 'gate' : key.slice(1), value: value.toFixed(3) })))
let timer:ReturnType<typeof setTimeout>|undefined,controller:AbortController|undefined,generation=0
async function poll(token:number){
  if(!hovering.value||token!==generation)return
  if(!active.value){channels.value=[];error.value='Enable the audio engine to visualize this signal.';return}
  controller=new AbortController()
  try{const response=await fetch(`/api/projects/${props.data.projectId}/preview?edge=${encodeURIComponent(props.id)}`,{signal:controller.signal});const result=await response.json();if(!response.ok)throw new Error(result.error);if(token!==generation)return;channels.value=result.channels;sampleRate.value=result.sample_rate;error.value=''}catch(e){if(token===generation){channels.value=[];error.value=e instanceof Error?e.message:String(e)}}
  finally{if(hovering.value&&token===generation)timer=setTimeout(()=>void poll(token),200)}
}
function start(event:MouseEvent|FocusEvent){if(props.data?.preview || hovering.value)return;const rect=(event.currentTarget as Element).getBoundingClientRect();anchor.value={x:event instanceof MouseEvent?event.clientX:rect.left+rect.width/2,y:event instanceof MouseEvent?event.clientY:rect.top+rect.height/2};hovering.value=true;if(props.data?.signal==='audio')void poll(++generation)}
function stop(){hovering.value=false;generation++;clearTimeout(timer);controller?.abort();channels.value=[]}
function points(samples:number[]){const scale=Math.max(0.001,...samples.map(Math.abs));return samples.map((v,i)=>`${i*220/(samples.length-1)},${22-v/scale*19}`).join(' ')}
function peak(samples:number[]){return Math.max(...samples.map(Math.abs)).toFixed(3)}
watch(active,()=>{if(hovering.value){generation++;clearTimeout(timer);controller?.abort();void poll(generation)}})
onBeforeUnmount(stop)
</script>
<template>
  <g tabindex="0" :aria-label="`Preview ${data?.signal || 'signal'} connection`" @mouseenter="start" @mouseleave="stop" @focusin="start" @focusout="stop" @keydown.esc="stop">
    <BaseEdge :id="id" :path="path[0]" :class="['signal-edge', data?.signal, { flowing: active, selected, feedback }]" :interaction-width="24" />
    <title v-if="feedback">Feedback connection · one sample delay</title>
  </g>
  <EdgeLabelRenderer v-if="data?.signal === 'audio'">
    <span v-if="data.channels > 1" class="channel-badge" :style="{ transform: `translate(-50%, -50%) translate(${path[1]}px, ${path[2]}px)` }">{{ data.channels }} ch</span>
  </EdgeLabelRenderer>
  <foreignObject width="0" height="0"><Teleport to="body"><aside v-if="hovering" class="waveform-preview" role="tooltip" :style="popupStyle"><strong v-if="data?.signal==='audio'">Audio waveform · {{data.channels}} ch</strong><strong v-else>{{data?.signal?.toUpperCase()}} signal</strong><p v-if="error">{{error}}</p><template v-else-if="data?.signal==='midi'"><p v-if="!midiRows.length">No MIDI messages in the last block.</p><table v-else class="midi-preview-table"><thead><tr><th>DATA</th><th>VALUE</th></tr></thead><tbody><tr v-for="row in midiRows" :key="row.key"><td>{{row.key}}</td><td>{{row.value}}</td></tr></tbody></table><small>Latest MIDI block · telemetry snapshot</small></template><p v-else-if="data?.signal==='spectral'">Spectral frames are passing through this connection.</p><p v-else-if="!channels.length">Reading engine audio…</p><div v-for="(samples,ch) in channels" :key="ch" class="waveform-channel"><small>CH {{ch+1}} <span>peak {{peak(samples)}}</span></small><svg viewBox="0 0 220 44" role="img" :aria-label="`Channel ${ch+1} waveform`"><path d="M0 22H220" stroke="#425358"/><polyline :points="points(samples)" fill="none" stroke="currentColor" stroke-width="1.3"/></svg></div><small v-if="channels.length">{{(256/sampleRate*1000).toFixed(1)}} ms snapshot · source · per-channel auto scale</small></aside></Teleport></foreignObject>
</template>
