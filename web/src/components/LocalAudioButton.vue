<script setup lang="ts">
import {computed,inject} from 'vue'
import {Mic,MicOff} from '@lucide/vue'
import {browserInputState} from '../browserInputs'
const props=defineProps<{projectId:string;node:string;muted:boolean;driven:boolean;disabled:boolean;active:boolean;source?:string;assignedName?:string;sending:boolean}>()
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
  <button class="mic-button" :class="{muted,offline:!sending}" :aria-label="muted?'Unmute local audio input':'Mute local audio input'" :aria-pressed="muted" :disabled="disabled||driven" :title="driven?'Mute is controlled by its connected source':muted?'Unmute input':'Mute input'" @click.stop="toggle"><MicOff v-if="muted" :size="25"/><Mic v-else :size="25"/></button>
  <span>{{driven?'CONTROLLED · ':''}}{{muted?'MUTED':'UNMUTED'}}</span>
  <small>{{source || (!assignedName?'Unassigned':!active?'Engine off':state==='disconnected'?`${assignedName} · disconnected`:state)}}</small>
</div>
</template>
<style scoped>
.local-audio-control{margin:44px 10px 12px;display:flex;flex-direction:column;align-items:center;gap:9px;text-align:center}.mic-button{width:58px;height:58px;min-width:44px;min-height:44px;display:grid;place-items:center;border-radius:50%;border:2px solid #eda952;color:var(--control-ink);background:var(--shade);touch-action:manipulation}.mic-button:hover:not(:disabled){background:color-mix(in srgb,var(--amber) 12%,var(--shade));border-color:#ffd092}.mic-button:active:not(:disabled){background:color-mix(in srgb,var(--amber) 22%,var(--shade))}.mic-button.muted{color:color-mix(in srgb,var(--control-ink) 65%,transparent);border-color:color-mix(in srgb,var(--amber) 45%,transparent);background:var(--shade-deep)}.mic-button.offline{color:var(--ink-muted);border-color:color-mix(in srgb,var(--ink-muted) 60%,transparent);background:var(--shade)}.mic-button.offline:hover:not(:disabled){border-color:var(--ink-muted);background:var(--shade-deep)}.mic-button:focus-visible{outline:2px solid #edba77;outline-offset:5px}.local-audio-control>span{font:9px monospace;letter-spacing:1px;color:var(--control-ink)}.local-audio-control small{font-size:9px;color:var(--ink-muted);overflow-wrap:anywhere;max-width:175px}
</style>
