<script setup lang="ts">
import { computed, reactive, onBeforeUnmount, onMounted, watch } from 'vue'
const props = defineProps<{ octave:number; octaves:number; values?:Record<string,number>; disabled:boolean; stale:boolean }>()
const emit = defineEmits<{ note:[pitch:number,velocity:number] }>()
const names = ['C','C♯','D','D♯','E','F','F♯','G','G♯','A','A♯','B']
const positions = [0,.65,1,1.65,2,3,3.65,4,4.65,5,5.65,6]
const span=computed(()=>Math.min(props.octaves,10-props.octave))
const keys = computed(() => Array.from({length:span.value*12},(_,index)=>{const name=names[index%12]!,octave=props.octave+Math.floor(index/12);return {name,octave,pitch:(props.octave+1)*12+index,black:name.includes('♯'),left:(Math.floor(index/12)*7+positions[index%12]!)/(span.value*7)*100}}))
const held = reactive(new Map<string,number>())
function lit(pitch:number) { return [...held.values()].includes(pitch) || (!props.stale && !!props.values?.[`_key${pitch}`]) }
function press(token:string,pitch:number) { if (props.disabled || pitch>127 || held.has(token)) return; held.set(token,pitch); emit('note',pitch,100) }
function release(token:string) { const pitch=held.get(token); if(pitch===undefined)return; held.delete(token); emit('note',pitch,0) }
function down(event:PointerEvent,pitch:number) { if(event.button!==0)return; (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId); press(`p${event.pointerId}`,pitch) }
function clear() { for(const token of [...held.keys()])release(token) }
function visibility() { if(document.hidden)clear() }
watch(() => [props.octave,props.octaves,props.disabled],clear)
onMounted(() => { window.addEventListener('blur',clear); document.addEventListener('visibilitychange',visibility) })
onBeforeUnmount(() => { clear(); window.removeEventListener('blur',clear); document.removeEventListener('visibilitychange',visibility) })
</script>
<template>
  <div class="piano-keys nodrag nopan nowheel" role="group" aria-label="Piano keyboard" @dblclick.stop @pointerdown.stop @touchstart.stop.prevent @keydown.stop @keyup.stop>
    <button v-for="key in keys" :key="key.pitch" class="piano-key" :class="{black:key.black,lit:lit(key.pitch)}" :style="{left:`${key.left}%`,width:`${(key.black?10:100/7)/span}%`}" :disabled="disabled || key.pitch>127" :aria-label="`${key.name}${key.octave} MIDI ${key.pitch}`" :aria-pressed="lit(key.pitch)" @pointerdown.prevent.stop="down($event,key.pitch)" @pointerup.prevent.stop="release(`p${$event.pointerId}`)" @pointercancel.stop="release(`p${$event.pointerId}`)" @lostpointercapture="release(`p${$event.pointerId}`)" @keydown.space.prevent="press(`k${key.pitch}`,key.pitch)" @keydown.enter.prevent="press(`k${key.pitch}`,key.pitch)" @keyup.space.prevent="release(`k${key.pitch}`)" @keyup.enter.prevent="release(`k${key.pitch}`)" @blur="release(`k${key.pitch}`)"><span>{{key.name==='C'?`C${key.octave}`:key.name}}</span></button>
  </div>
</template>
<style scoped>
.piano-keys{position:absolute;left:12px;right:12px;top:211px;height:70px;touch-action:none;user-select:none}
.piano-key{position:absolute;top:0;width:14.2857%;height:70px;background:#d5dfdc;color:#293332;border:1px solid #121819;border-radius:0 0 4px 4px;touch-action:none;padding:0;opacity:1}
.piano-key span{position:absolute;bottom:5px;left:0;right:0;font-size:10px}.piano-key.black{width:10%;height:44px;background:#202829;color:#c2ceca;z-index:1}.piano-key.lit{background:#202829;color:#e7f5f1;box-shadow:inset 0 0 0 2px var(--cyan)}.piano-key.black.lit{background:#e7f5f1;color:#202829;box-shadow:inset 0 0 0 2px var(--amber)}.piano-key:disabled{cursor:default}.piano-key:focus-visible{outline:2px solid var(--violet);outline-offset:-3px}
</style>
