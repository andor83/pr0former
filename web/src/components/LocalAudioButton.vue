<script setup lang="ts">
import {computed,inject} from 'vue'
import {Mic,MicOff} from '@lucide/vue'
import {browserInputState} from '../browserInputs'
const props=defineProps<{projectId:string;node:string;muted:boolean;driven:boolean;disabled:boolean;active:boolean;source?:string}>()
const connect=inject<(node:string)=>Promise<void>>('connectBrowserInput')
const setParameter=inject<(node:string,key:string,value:number)=>void>('setNodeParameter')
const state=computed(()=>browserInputState[`${props.projectId}:${props.node}`]||'disconnected')
async function toggle(){
  if(props.disabled||props.driven)return
  if(props.active&&!props.source&&!['connected','connecting','new'].includes(state.value)){await connect?.(props.node);if(props.muted)setParameter?.(props.node,'mute',0);return}
  setParameter?.(props.node,'mute',props.muted?0:1)
}
</script>
<template>
<div class="local-audio-control nodrag nopan" @dblclick.stop @keydown.stop @keyup.stop>
  <button class="mic-dome" :class="{muted}" :aria-label="muted?'Unmute local audio input':'Mute local audio input'" :aria-pressed="muted" :disabled="disabled||driven" :title="driven?'Mute is controlled by its connected source':muted?'Unmute input':'Mute input'" @click.stop="toggle"><MicOff v-if="muted" :size="25"/><Mic v-else :size="25"/></button>
  <span>{{driven?'CONTROLLED · ':''}}{{muted?'MUTED':'UNMUTED'}}</span>
  <small>{{source || (!active?'Engine off':state==='disconnected'?'Microphone access needed · tap to connect':state)}}</small>
</div>
</template>
<style scoped>
.local-audio-control{margin:44px 10px 12px;display:flex;flex-direction:column;align-items:center;gap:9px;text-align:center}.mic-dome{width:58px;height:58px;min-width:44px;min-height:44px;display:grid;place-items:center;border-radius:50%;border:2px solid #161311;color:#211711;background:radial-gradient(ellipse at 40% 20%,#ffe0a7 0%,#e89c4e 35%,#b75b20 78%,#743a1c 100%);box-shadow:0 4px 0 #0b0e0e,0 6px 9px #0008,inset 0 2px 3px #fff8,inset 0 -3px 4px #612705;touch-action:manipulation}.mic-dome:active{transform:translateY(2px);box-shadow:0 2px 0 #0b0e0e,inset 0 2px 4px #0006}.mic-dome.muted{color:#dc9856;background:radial-gradient(ellipse at 40% 20%,#4c4036,#211c18 80%);box-shadow:0 3px 0 #090b0b,inset 0 1px 3px #b37b3e55}.mic-dome:focus-visible{outline:2px solid #efb765;outline-offset:5px}.local-audio-control>span{font:9px monospace;letter-spacing:1px;color:#e9ac70}.local-audio-control small{font-size:9px;color:#aaa096;overflow-wrap:anywhere;max-width:175px}
</style>
