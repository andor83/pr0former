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
  <button class="mic-button" :class="{muted}" :aria-label="muted?'Unmute local audio input':'Mute local audio input'" :aria-pressed="muted" :disabled="disabled||driven" :title="driven?'Mute is controlled by its connected source':muted?'Unmute input':'Mute input'" @click.stop="toggle"><MicOff v-if="muted" :size="25"/><Mic v-else :size="25"/></button>
  <span>{{driven?'CONTROLLED · ':''}}{{muted?'MUTED':'UNMUTED'}}</span>
  <small>{{source || (!active?'Engine off':state==='disconnected'?'Microphone access needed · tap to connect':state)}}</small>
</div>
</template>
<style scoped>
.local-audio-control{margin:44px 10px 12px;display:flex;flex-direction:column;align-items:center;gap:9px;text-align:center}.mic-button{width:58px;height:58px;min-width:44px;min-height:44px;display:grid;place-items:center;border-radius:50%;border:2px solid #eda952;color:#ffd092;background:#272b29;touch-action:manipulation}.mic-button:hover:not(:disabled){background:#32352f;border-color:#ffd092}.mic-button:active:not(:disabled){background:#3c382f}.mic-button.muted{color:#a38c6c;border-color:#62543c;background:#222624}.mic-button:focus-visible{outline:2px solid #edba77;outline-offset:5px}.local-audio-control>span{font:9px monospace;letter-spacing:1px;color:#edba77}.local-audio-control small{font-size:9px;color:#aaa096;overflow-wrap:anywhere;max-width:175px}
</style>
