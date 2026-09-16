<script setup lang="ts">
import {computed,onBeforeUnmount,ref,watch} from 'vue'
import {useMidiHistory} from '../midiHistory'
const props=defineProps<{values?:Record<string,number>;active:boolean;stale:boolean}>()
const lit=ref(false)
let timer:ReturnType<typeof setTimeout>|undefined
const live=computed(()=>props.active&&!props.stale)
const {history}=useMidiHistory(()=>props.values,live,5)
watch(()=>[props.values?._midi_received,live.value] as const,([count,enabled],[previous,wasEnabled])=>{
  if(count===previous&&enabled===wasEnabled)return
  clearTimeout(timer)
  lit.value=!!enabled&&!!wasEnabled&&!!count&&previous!==undefined&&count>previous
  if(lit.value)timer=setTimeout(()=>{lit.value=false},200)
})
onBeforeUnmount(()=>clearTimeout(timer))
</script>
<template>
  <section class="midi-activity nodrag nopan" :class="{lit}" aria-label="MIDI input activity">
    <header><span class="activity-light" aria-hidden="true" /><span>Recent MIDI</span><small v-if="!live">{{active?'Stale':'Engine off'}}</small></header>
    <table aria-label="Recent MIDI messages"><thead><tr><th>Ch</th><th>Detail</th><th>Value</th></tr></thead><tbody>
      <tr v-for="item in history" :key="item.sequence"><td>{{item.channel}}</td><td>{{item.detail}}</td><td>{{item.value}}</td></tr>
    </tbody></table>
    <p v-if="!history.length">Waiting for MIDI…</p>
  </section>
</template>
<style scoped>
.midi-activity{position:relative;margin:55px 72px 34px;padding:10px;min-height:176px;border:1px solid #edaa55;border-radius:7px;background:var(--shade-deep);color:var(--control-ink);box-shadow:0 0 10px #edaa5526,inset 0 0 12px #edaa550a;font-variant-numeric:tabular-nums;font-size:11px}.midi-activity header{display:flex;align-items:center;gap:8px;margin-bottom:8px;font-size:11px}.midi-activity small{margin-left:auto;font-size:9px}.activity-light{width:18px;height:18px;flex-shrink:0;border:1px solid #edaa55;border-radius:50%;background:var(--control-well)}.lit .activity-light{background:#ffc078;box-shadow:0 0 10px #edaa5580}.midi-activity table{width:100%;border-collapse:collapse;text-align:left;table-layout:fixed}.midi-activity th{font-size:9px;font-weight:500;border-bottom:1px solid #edaa5540;padding-bottom:5px}.midi-activity td{padding:4px 0;overflow-wrap:anywhere;vertical-align:top}.midi-activity th:first-child{width:26px}.midi-activity th:last-child{width:40px;text-align:right}.midi-activity td:last-child{text-align:right}.midi-activity p{margin-top:12px;font-size:11px}
</style>
