<script setup lang="ts">
import {ref,watch} from 'vue'
const props=defineProps<{label:string;checked:boolean;disabled:boolean;sequence?:number}>()
const emit=defineEmits<{value:[value:number]}>()
// Optimistic local state: a click shows immediately, then every telemetry snapshot
// re-asserts the engine's value. Re-syncing on the snapshot sequence (not only on a
// value change) matters when the engine flips 0→1→0 between two 20 Hz snapshots:
// the bound value never changes, so without this the DOM would keep the clicked state.
const checked=ref(props.checked)
watch(()=>[props.checked,props.sequence] as const,()=>{checked.value=props.checked})
function change(event:Event){const next=(event.target as HTMLInputElement).checked;checked.value=next;emit('value',next?1:0)}
</script>
<template>
<label class="toggle-button nodrag nopan" :class="{disabled}" @click.stop @dblclick.stop @keydown.stop @keyup.stop>
  <input type="checkbox" :aria-label="`Toggle ${label}`" :checked="checked" :disabled="disabled" @change="change">
</label>
</template>
<style scoped>
.toggle-button{width:44px;height:44px;display:grid;place-items:center;cursor:pointer;touch-action:manipulation}
.toggle-button input{appearance:none;width:24px;height:24px;padding:0;margin:0;border:2px solid var(--cyan);border-radius:4px;background:var(--shade);cursor:pointer;display:grid;place-items:center}
.toggle-button input:checked{background:var(--cyan);color:var(--bg)}
.toggle-button input:checked::after{content:'✓';font-size:20px;font-weight:700;line-height:1}
.toggle-button input:focus-visible{outline:2px solid var(--amber);outline-offset:4px}
.toggle-button.disabled{opacity:.45;cursor:default}.toggle-button input:disabled{cursor:default}
</style>
