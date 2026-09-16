<script setup lang="ts">
import {computed,ref,watch} from 'vue'
const props=defineProps<{message?:[number,number|string];active:boolean;stale:boolean;compact?:boolean;address?:string;output?:boolean}>()
const live=computed(()=>props.active&&!props.stale)
const history=ref<{count:number;arrivals:number;value:number|string;address:string}[]>([])
let last=-1
watch(()=>[props.message,live.value],()=>{
  if(!live.value||!props.message)return
  const [count,value]=props.message
  if(count===last)return
  if(count<last)history.value=[]
  const arrivals=last<0||count<last?1:count-last
  last=count;history.value=[{count,arrivals,value,address:props.address||'/pr0/note'},...history.value].slice(0,props.compact?5:20)
},{immediate:true})
</script>
<template>
<section class="osc-debug nodrag nopan" :class="{compact}" :aria-label="compact?'OSC node messages':'OSC message debug'">
  <header><strong>{{output?'Prepared OSC':'Received OSC'}}</strong><small>{{live?'Live':active?'Stale':'Engine off'}}</small></header>
  <div class="osc-history" tabindex="0" :aria-label="compact?'Recent OSC messages':'Observed OSC messages'"><table><thead><tr><th>Count</th><th>Address</th><th>Value</th></tr></thead><tbody><tr v-for="row in history" :key="row.count"><td>{{row.count}}<small v-if="row.arrivals>1"> (+{{row.arrivals}})</small></td><td>{{row.address}}</td><td>{{typeof row.value==='string'?JSON.stringify(row.value):row.value}}</td></tr></tbody></table></div>
  <p v-if="!history.length">Waiting for OSC values…</p><button v-if="!compact" class="text-button" :disabled="!history.length" @click="history=[]">Clear observed OSC history</button>
</section>
</template>
<style scoped>
.osc-debug{padding:14px;background:var(--shade-deep);color:var(--control-ink);border:1px solid #edaa55;border-radius:7px;font-size:12px}.osc-debug.compact{margin:55px 72px 34px;min-height:176px;padding:10px;font-size:11px}.osc-debug header{display:flex;justify-content:space-between;gap:10px;margin-bottom:10px}.osc-debug small{font-size:10px}.osc-debug p{font-size:11px;line-height:1.5;margin:12px 0}.osc-history{max-height:220px;overflow:auto}.osc-history table{border-collapse:collapse;width:100%;table-layout:fixed}.osc-history th,.osc-history td{text-align:left;padding:7px 4px;border-bottom:1px solid #edaa5540;overflow-wrap:anywhere;vertical-align:top}.osc-history th{position:sticky;top:0;background:var(--shade-soft);backdrop-filter:blur(8px);font-size:10px}.osc-history th:first-child{width:40px}.osc-debug button{min-height:44px;color:var(--control-ink)}
</style>
