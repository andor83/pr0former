<script setup lang="ts">
import {computed,onBeforeUnmount,ref,watch} from 'vue'
import {decodeMidiDebug} from '../midiDebug'
const props=defineProps<{values?:Record<string,number>;active:boolean;stale:boolean}>()
const lit=ref(false)
let timer:ReturnType<typeof setTimeout>|undefined
const live=computed(()=>props.active&&!props.stale)
const description=computed(()=>{
  const v=props.values
  const message=v?._midi_received?decodeMidiDebug(v._midi_status!,v._midi_data1!,v._midi_data2!):null
  const text=message?`${message.type} · Ch ${message.channel} · ${message.detail} · ${message.value} · ${message.raw}`:'No MIDI messages received'
  return `${!live.value?'Inactive · last received: ':''}${text}`
})
watch(()=>[props.values?._midi_received,live.value] as const,([count,enabled],[previous,wasEnabled])=>{
  if(count===previous&&enabled===wasEnabled)return
  clearTimeout(timer)
  lit.value=!!enabled&&!!wasEnabled&&!!count&&previous!==undefined&&count>previous
  if(lit.value)timer=setTimeout(()=>{lit.value=false},200)
})
onBeforeUnmount(()=>clearTimeout(timer))
</script>
<template>
  <span class="midi-activity nodrag nopan" :class="{lit}" tabindex="0" role="img" aria-label="MIDI input activity" :title="description" :aria-description="description"><span /></span>
</template>
<style scoped>
.midi-activity{position:absolute;left:50%;top:86px;display:grid;place-items:center;width:28px;height:28px;transform:translateX(-50%);border-radius:50%}.midi-activity>span{width:10px;height:10px;border:1px solid #647078;background:#283138;border-radius:50%}.midi-activity.lit>span{background:var(--amber);border-color:var(--amber);box-shadow:0 0 8px #e8b45f80}.midi-activity:focus-visible{outline:2px solid var(--cyan);outline-offset:2px}
</style>
