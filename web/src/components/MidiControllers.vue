<script setup lang="ts">
import {computed,ref,onMounted,onBeforeUnmount,watch} from 'vue'
import type {GraphNode} from '../types'
const props=defineProps<{node:GraphNode;values?:Record<string,number>;disabled:boolean;editable:boolean;connected:string[]}>()
const emit=defineEmits<{control:[index:number,value:number|null|'cancel'|'clear']}>()
const root=ref<HTMLElement>()
const learning=ref<number|null>(null)
function cancelLearn(){if(learning.value!==null){emit('control',learning.value,'cancel');learning.value=null}}
function learn(index:number){cancelLearn();learning.value=index;emit('control',index,null)}
function outside(e:PointerEvent){if(learning.value!==null&&!root.value?.querySelectorAll('.gesture')[learning.value]?.contains(e.target as Node))cancelLearn()}
function escape(e:KeyboardEvent){if(e.key==='Escape'&&learning.value!==null){e.preventDefault();e.stopImmediatePropagation();cancelLearn()}}
watch(()=>props.values?._learn_serial,()=>{learning.value=null})
watch(()=>props.disabled,disabled=>{if(disabled)cancelLearn()})
onMounted(()=>{window.addEventListener('pointerdown',outside,true);window.addEventListener('keydown',escape,true)})
onBeforeUnmount(()=>{cancelLearn();window.removeEventListener('pointerdown',outside,true);window.removeEventListener('keydown',escape,true)})
const knobs=computed(()=>props.node.kind==='knobs'), count=computed(()=>props.node.parameters.count??4)
const min=computed(()=>knobs.value?0:props.node.parameters.min??0), max=computed(()=>knobs.value?1:props.node.parameters.max??1)
const horizontal=computed(()=>!knobs.value&&props.node.parameters.orientation===1)
const drag=ref<{index:number;x:number;y:number;value:number;moved:boolean}|null>(null)
const local=ref<Record<number,number>>({})
const value=(i:number)=>local.value[i]??props.values?.[`_control_${i+1}`]??min.value
const unit=(i:number)=>max.value===min.value?0:(value(i)-min.value)/(max.value-min.value)
const assigned=(i:number)=>!knobs.value||(props.node.parameters[`channel_${i+1}`]??1)>0
function clear(i:number){if(!knobs.value||!props.editable)return;cancelLearn();drag.value=null;delete local.value[i];emit('control',i,'clear')}
const locked=(i:number)=>props.disabled||props.connected.includes(`slider_${i+1}`)
function set(i:number,v:number){
  if(!assigned(i))return
  cancelLearn()
  v=Math.max(min.value,Math.min(max.value,v));local.value[i]=v;emit('control',i,v)
}
function down(e:PointerEvent,i:number){if(locked(i))return;drag.value={index:i,x:e.clientX,y:e.clientY,value:value(i),moved:false};(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)}
function move(e:PointerEvent){const d=drag.value;if(!d)return;const delta=horizontal.value?e.clientX-d.x:d.y-e.clientY;if(Math.abs(delta)>3)d.moved=true;if(d.moved)set(d.index,d.value+delta/140*(max.value-min.value))}
function up(e:PointerEvent,cancel=false){const d=drag.value;if(!d)return;drag.value=null;if(!cancel&&(!d.moved||!assigned(d.index)))learn(d.index);delete local.value[d.index];if((e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId))(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId)}
function key(e:KeyboardEvent,i:number){if(locked(i))return;if(e.key==='Enter'||e.key===' '){e.preventDefault();learn(i);return}if(!['ArrowUp','ArrowRight','ArrowDown','ArrowLeft','Home','End'].includes(e.key))return;e.preventDefault();const delta=(props.node.parameters.step||((max.value-min.value)/127))*(e.shiftKey?10:1);set(i,e.key==='Home'?min.value:e.key==='End'?max.value:value(i)+(['ArrowUp','ArrowRight'].includes(e.key)?delta:-delta));delete local.value[i]}
</script>
<template>
  <div ref="root" class="controllers nodrag nopan" :class="{horizontal}" @dblclick.stop @click.stop @pointerdown.stop>
    <div v-for="i in count" :key="i" class="controller">
      <div class="gesture" :class="{learning:values?._learning===i,locked:locked(i-1),unassigned:!assigned(i-1)}" :title="!assigned(i-1)?'Unassigned · click to learn MIDI':knobs?'Drag to adjust · click to learn · double-click to unassign':'Drag to adjust · click to learn'" @dblclick.stop.prevent="clear(i-1)" role="slider" :tabindex="locked(i-1)?-1:0" :aria-label="`${knobs?'Knob':'Slider'} ${i}`" :aria-valuemin="min" :aria-valuemax="max" :aria-valuenow="value(i-1)" :aria-disabled="locked(i-1)" :aria-orientation="horizontal?'horizontal':'vertical'" @pointerdown="down($event,i-1)" @pointermove="move" @pointerup="up($event)" @pointercancel="up($event,true)" @keydown.stop="key($event,i-1)">
        <svg v-if="knobs" viewBox="0 0 72 72" aria-hidden="true"><path class="track" d="M 17 56 A 28 28 0 1 1 55 56" pathLength="100"/><path class="fill" d="M 17 56 A 28 28 0 1 1 55 56" pathLength="100" :stroke-dasharray="`${unit(i-1)*100} 100`"/><circle cx="36" cy="35" r="20"/><path class="pointer" d="M 36 35 L 36 20" :transform="`rotate(${-138+unit(i-1)*276} 36 35)`"/></svg>
        <div v-else class="fader"><i :style="horizontal?{width:`${unit(i-1)*100}%`}:{height:`${unit(i-1)*100}%`}"/><b :style="horizontal?{left:`${unit(i-1)*100}%`}:{bottom:`${unit(i-1)*100}%`}"/></div>
      </div>
      <output>{{value(i-1).toFixed(knobs?3:node.parameters.decimals??2)}}</output>
      <small>{{values?._learning===i?'Turn MIDI control… · Esc cancels':!assigned(i-1)?'Unassigned · click to learn':connected.includes(`slider_${i}`)?'Connected input':`${i} · Ch ${node.parameters[`channel_${i}`]??1} / CC ${node.parameters[`controller_${i}`]??i}`}}</small>
    </div>
  </div>
</template>
<style scoped>
.controllers{position:relative;margin:55px 72px 34px;display:flex;gap:12px;align-items:flex-start}.controller{width:76px;display:flex;flex-direction:column;align-items:center;gap:4px}.gesture{width:72px;min-height:72px;touch-action:none;cursor:ns-resize;border-radius:6px}.gesture.learning{outline:1px solid var(--amber);background:#dfb87916}.gesture.unassigned{opacity:.5;filter:grayscale(1);cursor:pointer}.gesture.unassigned.learning{opacity:1;filter:none}.gesture.locked{opacity:.55;cursor:default}svg{width:72px;height:72px}.track,.fill{fill:none;stroke:#4d4232;stroke-width:5;stroke-linecap:round}.fill{stroke:#eda952}circle{fill:#272b29;stroke:#62543c}.pointer{stroke:#ffd092;stroke-width:3;stroke-linecap:round}.controller output{color:#edba77;font-variant-numeric:tabular-nums;font-size:12px}.controller small{font-size:8px;color:var(--muted);text-align:center}.fader{position:relative;width:7px;height:130px;margin:10px auto;background:#463d30;border-radius:5px}.fader i{position:absolute;bottom:0;width:100%;background:#eda952;border-radius:5px}.fader b{position:absolute;left:50%;width:30px;height:15px;transform:translate(-50%,50%);border:1px solid #edba77;border-radius:3px;background:#69523b}.horizontal{flex-direction:column}.horizontal .controller{width:180px;display:grid;grid-template-columns:140px 40px}.horizontal .gesture{width:140px;min-height:32px;cursor:ew-resize}.horizontal .fader{width:120px;height:7px;margin:12px 10px}.horizontal .fader i{height:100%;left:0}.horizontal .fader b{top:50%;width:15px;height:28px;transform:translate(-50%,-50%)}.horizontal small{grid-column:1 / -1}
</style>
