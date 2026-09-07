<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import type { EdgeProps } from '@vue-flow/core'
import { BaseEdge, EdgeLabelRenderer, getBezierPath } from '@vue-flow/core'
const props = defineProps<EdgeProps<{ signal: string; channels: number; active: boolean; projectId:string }>>()
const path = computed(() => getBezierPath(props))
const anchor=ref({x:0,y:0})
const popupStyle=computed(()=>({left:`${Math.max(12,Math.min(window.innerWidth-276,anchor.value.x-126))}px`,top:`${Math.max(12,Math.min(window.innerHeight-(84+(props.data?.channels||1)*50)-12,anchor.value.y+16))}px`}))
const hovering=ref(false),channels=ref<number[][]>([]),error=ref(''),sampleRate=ref(48000)
let timer:ReturnType<typeof setTimeout>|undefined,controller:AbortController|undefined,generation=0
async function poll(token:number){
  if(!hovering.value||token!==generation)return
  if(!props.data?.active){channels.value=[];error.value='Play the active show to preview audio.';return}
  controller=new AbortController()
  try{const response=await fetch(`/api/projects/${props.data.projectId}/preview?edge=${encodeURIComponent(props.id)}`,{signal:controller.signal});const result=await response.json();if(!response.ok)throw new Error(result.error);if(token!==generation)return;channels.value=result.channels;sampleRate.value=result.sample_rate;error.value=''}catch(e){if(token===generation){channels.value=[];error.value=e instanceof Error?e.message:String(e)}}
  finally{if(hovering.value&&token===generation)timer=setTimeout(()=>void poll(token),200)}
}
function start(event:MouseEvent|FocusEvent){if(props.data?.signal!=='audio'||hovering.value)return;const rect=(event.currentTarget as Element).getBoundingClientRect();anchor.value={x:event instanceof MouseEvent?event.clientX:rect.left+rect.width/2,y:event instanceof MouseEvent?event.clientY:rect.top+rect.height/2};hovering.value=true;void poll(++generation)}
function stop(){hovering.value=false;generation++;clearTimeout(timer);controller?.abort();channels.value=[]}
function points(samples:number[]){const scale=Math.max(0.001,...samples.map(Math.abs));return samples.map((v,i)=>`${i*220/(samples.length-1)},${22-v/scale*19}`).join(' ')}
function peak(samples:number[]){return Math.max(...samples.map(Math.abs)).toFixed(3)}
watch(()=>props.data?.active,()=>{if(hovering.value){generation++;clearTimeout(timer);controller?.abort();void poll(generation)}})
onBeforeUnmount(stop)
</script>
<template>
  <g :tabindex="data?.signal==='audio'?0:undefined" :aria-label="data?.signal==='audio'?`Preview ${data.channels} audio channels`:undefined" @mouseenter="start" @mouseleave="stop" @focusin="start" @focusout="stop" @keydown.esc="stop">
    <BaseEdge :id="id" :path="path[0]" :class="['signal-edge', data?.signal, { flowing: data?.active, selected }]" :interaction-width="24" />
  </g>
  <EdgeLabelRenderer v-if="data?.signal === 'audio'">
    <span v-if="data.channels > 1" class="channel-badge" :style="{ transform: `translate(-50%, -50%) translate(${path[1]}px, ${path[2]}px)` }">{{ data.channels }} ch</span>
  </EdgeLabelRenderer>
  <foreignObject width="0" height="0"><Teleport to="body"><aside v-if="hovering" class="waveform-preview" role="tooltip" :style="popupStyle"><strong>Audio waveform · {{data.channels}} ch</strong><p v-if="error">{{error}}</p><p v-else-if="!channels.length">Reading engine audio…</p><div v-for="(samples,ch) in channels" :key="ch" class="waveform-channel"><small>CH {{ch+1}} <span>peak {{peak(samples)}}</span></small><svg viewBox="0 0 220 44" role="img" :aria-label="`Channel ${ch+1} waveform`"><path d="M0 22H220" stroke="#425358"/><polyline :points="points(samples)" fill="none" stroke="currentColor" stroke-width="1.3"/></svg></div><small v-if="channels.length">{{(256/sampleRate*1000).toFixed(1)}} ms snapshot · source · per-channel auto scale</small></aside></Teleport></foreignObject>
</template>
