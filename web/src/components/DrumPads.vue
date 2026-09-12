<script setup lang="ts">
import { computed, reactive, onBeforeUnmount, onMounted, watch } from 'vue'
const props = defineProps<{ notes:number[]; values?:Record<string,number>; disabled:boolean; stale:boolean }>()
const emit = defineEmits<{ note:[pitch:number,velocity:number] }>()
const names = ['Bass drum','Snare','Tom 1','Tom 2','Hi hat','Cymbal']
const pads = computed(() => names.map((name,index)=>({name,index,pitch:props.notes[index] ?? 0})))
const held = reactive(new Map<string,number>())
function lit(index:number) { return [...held.values()].includes(index) || (!props.stale && !!props.values?.[`_pad${index+1}`]) }
function press(token:string,index:number) { if (props.disabled || held.has(token)) return; held.set(token,index); emit('note',pads.value[index]!.pitch,100) }
function release(token:string) { const index=held.get(token); if(index===undefined)return; held.delete(token); emit('note',pads.value[index]!.pitch,0) }
function down(event:PointerEvent,index:number) { if(event.button!==0)return; (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId); press(`p${event.pointerId}`,index) }
function clear() { for(const token of [...held.keys()])release(token) }
function visibility() { if(document.hidden)clear() }
watch(() => [props.disabled, ...props.notes],clear)
onMounted(() => { window.addEventListener('blur',clear); document.addEventListener('visibilitychange',visibility) })
onBeforeUnmount(() => { clear(); window.removeEventListener('blur',clear); document.removeEventListener('visibilitychange',visibility) })
</script>
<template>
  <div class="drum-pads nodrag nopan nowheel" role="group" aria-label="Drum pads" @dblclick.stop @pointerdown.stop @touchstart.stop.prevent @keydown.stop @keyup.stop>
    <button v-for="pad in pads" :key="pad.index" class="drum-pad" :class="{lit:lit(pad.index)}" :disabled="disabled" :aria-label="`${pad.name} MIDI ${pad.pitch}`" :aria-pressed="lit(pad.index)" @pointerdown.prevent.stop="down($event,pad.index)" @pointerup.prevent.stop="release(`p${$event.pointerId}`)" @pointercancel.stop="release(`p${$event.pointerId}`)" @lostpointercapture="release(`p${$event.pointerId}`)" @keydown.space.prevent="press(`k${pad.index}`,pad.index)" @keydown.enter.prevent="press(`k${pad.index}`,pad.index)" @keyup.space.prevent="release(`k${pad.index}`)" @keyup.enter.prevent="release(`k${pad.index}`)" @blur="release(`k${pad.index}`)"><span>{{pad.name}}</span><small>{{pad.pitch}}</small></button>
  </div>
</template>
<style scoped>
.drum-pads{position:absolute;left:14px;right:136px;top:94px;display:grid;grid-template-columns:repeat(3,1fr);gap:10px 8px;touch-action:none;user-select:none}
.drum-pad{aspect-ratio:1;border-radius:50%;background:radial-gradient(circle at 40% 35%,#3a4547,#232b2c 70%);border:1px solid #4a5a5c;color:#c8d4d2;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:2px;padding:0;touch-action:none;opacity:1;min-width:44px}
.drum-pad span{font-size:8px;letter-spacing:.2px;line-height:1.1;text-align:center}.drum-pad small{font-size:8px;color:#7f9294;font-family:monospace}
.drum-pad.lit{background:radial-gradient(circle at 40% 35%,#8fe3dc,#3f8f89 75%);color:#0f2624;border-color:var(--cyan);box-shadow:0 0 12px #79d5ce55}.drum-pad.lit small{color:#173b38}
.drum-pad:disabled{cursor:default}.drum-pad:focus-visible{outline:2px solid var(--violet);outline-offset:2px}
</style>
