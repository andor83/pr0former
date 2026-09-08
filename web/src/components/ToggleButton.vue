<script setup lang="ts">
import {computed} from 'vue'
const props=defineProps<{label:string;checked:boolean;disabled:boolean}>()
const emit=defineEmits<{value:[value:number]}>()
const checked=computed(()=>props.checked)
</script>
<template>
<label class="toggle-button nodrag nopan" :class="{disabled}" @click.stop @dblclick.stop @keydown.stop @keyup.stop>
  <input type="checkbox" :aria-label="`Toggle ${label}`" :checked="checked" :disabled="disabled" @change="emit('value',($event.target as HTMLInputElement).checked?1:0)">
</label>
</template>
<style scoped>
.toggle-button{width:44px;height:44px;display:grid;place-items:center;cursor:pointer;touch-action:manipulation}
.toggle-button input{appearance:none;width:24px;height:24px;padding:0;margin:0;border:2px solid var(--cyan);border-radius:4px;background:#192426;cursor:pointer;display:grid;place-items:center}
.toggle-button input:checked{background:var(--cyan);color:#192426}
.toggle-button input:checked::after{content:'✓';font-size:20px;font-weight:700;line-height:1}
.toggle-button input:focus-visible{outline:2px solid var(--amber);outline-offset:4px}
.toggle-button.disabled{opacity:.45;cursor:default}.toggle-button input:disabled{cursor:default}
</style>
