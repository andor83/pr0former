<script setup lang="ts">
import {computed,onBeforeUnmount,ref,watch} from 'vue'
const props=defineProps<{label:string;values?:Record<string,number>;disabled:boolean}>()
const emit=defineEmits<{trigger:[]}>()
const flash=ref(false)
let timer:ReturnType<typeof setTimeout>|undefined
function pulse(){clearTimeout(timer);flash.value=true;timer=setTimeout(()=>flash.value=false,80)}
watch(()=>props.values?._trigger_sequence,(value,old)=>{if(!props.disabled&&old!==undefined&&value!==undefined&&value>old)pulse()})
watch(()=>props.disabled,disabled=>{if(disabled){clearTimeout(timer);flash.value=false}})
onBeforeUnmount(()=>clearTimeout(timer))
const lit=computed(()=>!props.disabled&&(flash.value||(props.values?._out??0)!==0))
function click(){if(props.disabled)return;pulse();emit('trigger')}
</script>
<template>
<button class="trigger-button nodrag nopan" :class="{lit}" :disabled="disabled" :aria-label="`Trigger ${label}`" :aria-pressed="lit" :title="label" @click.stop="click" @dblclick.stop @keydown.stop @keyup.stop><span aria-hidden="true"></span></button>
</template>
<style scoped>
.trigger-button{width:44px;height:44px;padding:10px;display:grid;place-items:center;border:0;border-radius:50%;background:transparent;color:var(--cyan);cursor:pointer;touch-action:manipulation}
.trigger-button span{display:block;width:24px;height:24px;border:2px solid currentColor;border-radius:50%;background:var(--bg,#192426)}
.trigger-button.lit span{background:var(--cyan);box-shadow:inset 0 0 0 5px #213638}
.trigger-button:focus-visible{outline:2px solid var(--amber);outline-offset:1px}
.trigger-button:disabled{opacity:.45;cursor:default}
</style>
