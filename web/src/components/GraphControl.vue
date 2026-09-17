<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import FaderTrack from './FaderTrack.vue'
import {roundSlider,formatSlider} from '../sliderNumbers'
import type { GraphNode, Visualization } from '../types'
const props=defineProps<{node:GraphNode;connected:boolean;data?:Visualization;stale:boolean;active:boolean;editable:boolean}>()
const emit=defineEmits<{value:[value:number|string];preview:[value:number];bang:[]}>()
const mode=computed(()=>props.node.parameters.mode??2),min=computed(()=>props.node.parameters.min??-100000),max=computed(()=>props.node.parameters.max??100000)
// Step 0/absent keeps the mode defaults: integers and floats move by 1, sliders drag freely and nudge by 0.01.
const configuredStep=computed(()=>{const s=props.node.parameters.step;return typeof s==='number'&&Number.isFinite(s)&&s>0?s:0})
const step=computed(()=>configuredStep.value||(mode.value===3?0.01:1))
function snap(value:number){const s=configuredStep.value;if(!s)return value;const text=String(s),point=text.indexOf('.');const digits=text.includes('e')?10:point<0?0:Math.min(10,text.length-point-1);return Number((Math.round(value/s)*s).toFixed(digits))}
function display(value:number|string|null|undefined){return mode.value===3&&typeof value==='number'?formatSlider(value):String(value??'')}
const draft=ref(display(props.node.control_value)),focused=ref(false),error=ref('')
const pending=ref<number|string>()
const chrome=computed(()=>props.node.parameters.hide_chrome!==1)
const drag=ref<{x:number;value:number}|null>(null)
const faderUnit=computed(()=>max.value===min.value?0:Math.max(0,Math.min(1,(Number(draft.value||0)-min.value)/(max.value-min.value))))
// pointerdown is default-prevented (no text selection while dragging), so focus explicitly: arrow keys then nudge one step per press.
function slideDown(e:PointerEvent){if(!props.editable)return;focused.value=true;drag.value={x:e.clientX,value:Number(draft.value)||0};const target=e.currentTarget as HTMLElement;target.focus();target.setPointerCapture(e.pointerId)}
function slideMove(e:PointerEvent){if(!drag.value)return;const raw=Math.max(min.value,Math.min(max.value,drag.value.value+(e.clientX-drag.value.x)/140*(max.value-min.value)));const value=configuredStep.value?Math.max(min.value,Math.min(max.value,snap(raw))):roundSlider(raw);draft.value=formatSlider(value);if(props.active)emit('preview',value)}
function slideUp(e:PointerEvent){if(!drag.value)return;drag.value=null;edit(draft.value);focused.value=false;if((e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId))(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId)}
function bump(delta:number){const value=Number(draft.value||0);if(Number.isFinite(value))edit(String(Math.max(min.value,Math.min(max.value,snap(value+delta)))))}
watch(()=>props.node.control_value,value=>{if(value===pending.value)pending.value=undefined;if(!focused.value&&pending.value===undefined)draft.value=display(value)})
watch(mode,()=>{draft.value=display(props.active&&!props.stale?props.data?.value??props.node.control_value:props.node.control_value);error.value=''})
const incoming=computed(()=>props.data?.value)
watch(incoming,value=>{if(value===pending.value)pending.value=undefined;if(props.active&&!props.stale&&!focused.value&&pending.value===undefined&&value!==undefined)draft.value=display(value)},{immediate:true})
function edit(text:string){
  draft.value=text;error.value=''
  if(mode.value===4){if(new TextEncoder().encode(text).length>256){error.value='Use at most 256 UTF-8 bytes';return}emit('value',text);return}
  const n=Number(text)
  if(!text.trim()||!Number.isFinite(n)||n<min.value||n>max.value||(mode.value===1&&!Number.isInteger(n))){error.value=`Enter ${mode.value===1?'an integer':'a number'} from ${min.value} to ${max.value}`;return}
  const value=mode.value===3?Math.max(min.value,Math.min(max.value,roundSlider(n))):n
  if(mode.value===3)draft.value=formatSlider(value)
  pending.value=value;emit('value',value)
}
</script>
<template>
  <div class="graphical-control nodrag nopan nowheel" @dblclick.stop @keydown.stop @mousedown.stop @click.stop>
      <button v-if="mode===0" class="button primary" :aria-label="`Trigger ${node.label}`" :disabled="!editable||!active" @click="emit('bang')">Bang</button>
      <template v-else>
        <div v-if="mode===3" class="fader-gesture" role="slider" :tabindex="editable?0:-1" :step="step" :aria-label="`${node.label} slider`" :aria-valuemin="min" :aria-valuemax="max" :aria-valuenow="Number(draft)||0" :aria-disabled="!editable" aria-orientation="horizontal" @pointerdown.prevent.stop="slideDown" @pointermove="slideMove" @pointerup="slideUp" @pointercancel="slideUp" @lostpointercapture="slideUp" @keydown.right.prevent="editable&&bump(step)" @keydown.left.prevent="editable&&bump(-step)" @keydown.up.prevent="editable&&bump(step)" @keydown.down.prevent="editable&&bump(-step)"><FaderTrack :unit="faderUnit" horizontal /></div>
        <div class="input-box"><input :type="mode===4?'text':'number'" :aria-label="`${node.label} value`" :step="configuredStep||(mode===1?1:mode===3?0.01:'any')" :min="min" :max="max" :value="draft|| (mode===4?'':0)" :disabled="!editable" @focus="focused=true" @blur="focused=false" @input="draft=($event.target as HTMLInputElement).value" @change="edit(($event.target as HTMLInputElement).value)" @keydown.up="mode!==4&&($event.preventDefault(),bump(step))" @keydown.down="mode!==4&&($event.preventDefault(),bump(-step))"><div v-if="mode===1||mode===2" class="steppers"><button :aria-label="`Increase ${node.label} by ${step}`" :disabled="!editable||Number(draft)>=max" @click="bump(step)">▴</button><button :aria-label="`Decrease ${node.label} by ${step}`" :disabled="!editable||Number(draft)<=min" @click="bump(-step)">▾</button></div></div>
      </template>
      <small v-if="chrome">{{['One-sample trigger','Integer','Float','Slider','Text'][mode]}}<template v-if="mode>0&&node.parameters.changes_only===1"> · changes only</template><template v-if="connected"> · input + manual override</template></small>
      <p v-if="chrome&&mode===0&&!active" class="feature-note">Enable the engine to trigger.</p>
      <p v-if="error" role="alert" class="field-error">{{error}}</p>
  </div>
</template>
<style scoped>
.graphical-control{position:relative;display:flex;flex-direction:column;gap:5px;font-size:11px}.graphical-control input{width:100%;min-width:0;padding:8px}.graphical-control small{font-size:9px;color:var(--ink-muted)}.graphical-control output{overflow-wrap:anywhere;max-height:90px;overflow:auto;font-size:15px}.graphical-control .connected-label{font-size:8px}.graphical-control .field-error{font-size:9px;line-height:1.2}
.input-box{display:flex;border:1px solid var(--amber);border-radius:5px;background:var(--control-well);overflow:hidden}.input-box input{border:0;background:transparent;color:var(--well-control-ink);appearance:textfield;-moz-appearance:textfield}.input-box input::-webkit-inner-spin-button,.input-box input::-webkit-outer-spin-button{appearance:none;margin:0}.steppers{display:flex;flex-direction:column;width:28px;flex-shrink:0}.steppers button{padding:0;min-height:24px;border:0;border-radius:0;background:color-mix(in srgb,var(--amber) 22%,var(--shade));color:var(--well-control-ink);line-height:1}.steppers button:hover{background:color-mix(in srgb,var(--amber) 36%,var(--shade))}
.fader-gesture{min-height:36px;padding:2px 0;touch-action:none;cursor:ew-resize;border-radius:5px}.fader-gesture:focus-visible{outline:2px solid var(--cyan)}
</style>
