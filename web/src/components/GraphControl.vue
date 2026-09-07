<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { GraphNode, Visualization } from '../types'
const props=defineProps<{node:GraphNode;connected:boolean;data?:Visualization;stale:boolean;active:boolean;editable:boolean}>()
const emit=defineEmits<{value:[value:number|string];bang:[]}>()
const mode=computed(()=>props.node.parameters.mode??2),min=computed(()=>props.node.parameters.min??-100000),max=computed(()=>props.node.parameters.max??100000)
const draft=ref(String(props.node.control_value??'')),focused=ref(false),error=ref('')
watch(()=>props.node.control_value,value=>{if(!focused.value)draft.value=String(value??'')})
watch(mode,()=>{draft.value=String(props.node.control_value??'');error.value=''})
const incoming=computed(()=>props.data?.value)
function edit(text:string){
  draft.value=text;error.value=''
  if(mode.value===4){if(new TextEncoder().encode(text).length>256){error.value='Use at most 256 UTF-8 bytes';return}emit('value',text);return}
  const n=Number(text)
  if(!text.trim()||!Number.isFinite(n)||n<min.value||n>max.value||(mode.value===1&&!Number.isInteger(n))){error.value=`Enter ${mode.value===1?'an integer':'a number'} from ${min.value} to ${max.value}`;return}
  emit('value',n)
}
</script>
<template>
  <div class="graphical-control nodrag nopan nowheel" @dblclick.stop @keydown.stop @mousedown.stop @click.stop>
    <template v-if="connected"><span class="connected-label">CONNECTED · READ ONLY</span><output :class="{dim:stale}">{{stale?'Engine value stale':incoming??'Waiting for engine'}}</output></template>
    <template v-else>
      <button v-if="mode===0" class="button primary" :aria-label="`Trigger ${node.label}`" :disabled="!editable||!active" @click="emit('bang')">Bang</button>
      <template v-else>
        <input v-if="mode===3" type="range" :aria-label="`${node.label} slider`" :min="min" :max="max" step="any" :value="draft||0" :disabled="!editable" @focus="focused=true" @blur="focused=false" @input="edit(($event.target as HTMLInputElement).value)">
        <input :type="mode===4?'text':'number'" :aria-label="`${node.label} value`" :step="mode===1?1:'any'" :min="min" :max="max" :value="draft|| (mode===4?'':0)" :disabled="!editable" @focus="focused=true" @blur="focused=false" @input="draft=($event.target as HTMLInputElement).value" @change="edit(($event.target as HTMLInputElement).value)">
      </template>
      <small>{{['One-sample trigger','Integer','Float','Slider','Text'][mode]}}<template v-if="mode>0&&mode<4"> · {{min}} … {{max}}</template></small>
      <p v-if="mode===0&&!active" class="feature-note">Activate the show to trigger.</p>
      <p v-if="error" role="alert" class="field-error">{{error}}</p>
    </template>
  </div>
</template>
<style scoped>
.graphical-control{position:absolute;top:103px;left:12px;right:12px;display:flex;flex-direction:column;gap:8px;font-size:11px}.graphical-control input{width:100%;padding:8px}.graphical-control small{font-size:9px;color:var(--muted)}.graphical-control output{overflow-wrap:anywhere;max-height:90px;overflow:auto;font-size:15px}.graphical-control .connected-label{font-size:8px}.graphical-control .field-error{font-size:9px;line-height:1.2}
</style>
