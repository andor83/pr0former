<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { X, ExternalLink } from 'lucide-vue-next'
import { api } from '../api'
const props=defineProps<{ projectId:string; standalone?:boolean }>()
const emit=defineEmits<{close:[]}>()
const dialog=ref<HTMLDialogElement>(),viewport=ref<HTMLElement>()
const rows=ref<{sequence:number;time:number;level:string;message:string}[]>([]),error=ref(''),follow=ref(true)
let timer:ReturnType<typeof setTimeout>,disposed=false
async function poll(){try{rows.value=await api(`/projects/${props.projectId}/logs`);error.value='';await nextTick();if(follow.value&&viewport.value)viewport.value.scrollTop=viewport.value.scrollHeight}catch(e){error.value=String(e)}finally{if(!disposed)timer=setTimeout(poll,1000)}}
function popout(){window.open(`/?console=${encodeURIComponent(props.projectId)}`,'_blank','noopener')}
onMounted(()=>{if(!props.standalone)dialog.value?.showModal();void poll()})
onBeforeUnmount(()=>{disposed=true;clearTimeout(timer)})
</script>
<template>
<component :is="standalone?'section':'dialog'" ref="dialog" class="parameter-modal engine-console" :class="{'console-standalone':standalone}" aria-labelledby="console-title" @cancel.prevent="emit('close')">
<header class="modal-header"><div><div class="eyebrow">PROJECT ENGINE</div><h2 id="console-title">Console</h2></div><button v-if="!standalone" class="button small" @click="popout"><ExternalLink :size="16"/> Open in new tab</button><button v-if="!standalone" class="icon-button" aria-label="Close console" @click="emit('close')"><X :size="20"/></button></header>
<div class="console-controls"><label><input v-model="follow" type="checkbox"> Autoscroll</label><span>Recent server activity · up to 2,000 entries</span></div>
<p v-if="error" role="alert">{{error}}</p><div ref="viewport" class="console-lines" role="log" aria-live="off"><p v-if="!rows.length">Waiting for engine activity…</p><div v-for="row in rows" :key="row.sequence" :class="row.level"><time>{{new Date(row.time).toLocaleTimeString()}}</time><b>{{row.level.toUpperCase()}}</b><span>{{row.message}}</span></div></div>
</component>
</template>
