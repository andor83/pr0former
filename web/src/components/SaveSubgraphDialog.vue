<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { api } from '../api'
import type { GraphNode } from '../types'
import type { LibraryEntry } from './SubgraphLibrary.vue'
const props=defineProps<{projectId:string;revision:number;node:GraphNode}>()
const emit=defineEmits<{close:[];saved:[reference:{id:string;version:number}]}>()
const dialog=ref<HTMLDialogElement>(), name=ref(props.node.label), destination=ref(''), publish=ref(false), entries=ref<LibraryEntry[]>([]), busy=ref(false),error=ref(''),loading=ref(true)
const entry=computed(()=>entries.value.find(e=>e.id===destination.value))
function destinationChanged(){if(entry.value){name.value=entry.value.name;if(entry.value.public)publish.value=true}}
async function save(){busy.value=true;error.value='';try{const result=await api<{id:string;version:number}>(`/projects/${props.projectId}/subgraphs`,'POST',{node:props.node.id,revision:props.revision,name:name.value,public:publish.value,library_id:destination.value||null});emit('saved',result)}catch(e){error.value=String(e)}finally{busy.value=false}}
onMounted(async()=>{dialog.value?.showModal();try{entries.value=(await api<LibraryEntry[]>('/subgraphs')).filter(e=>e.owned);if(props.node.library&&entries.value.some(e=>e.id===props.node.library?.id)){destination.value=props.node.library.id;destinationChanged()}}catch(e){error.value=String(e)}finally{loading.value=false}})
onBeforeUnmount(()=>dialog.value?.close())
</script>
<template>
  <dialog ref="dialog" class="parameter-modal" aria-labelledby="save-subgraph-title" @cancel.prevent="!busy&&emit('close')">
    <header class="modal-header"><h2 aria-label="Save subgraph to library" id="save-subgraph-title">Save subgraph to library<HelpNote label="Saving and sharing subgraphs">Public entries cannot become private again. Each save creates an immutable version. Existing projects keep their inserted version. Earlier private versions stay private. To make a private variant, choose a new library entry.<br /><br />Includes all nested nodes, connections, and uploaded WAV samples. Device routes are preserved.</HelpNote></h2><button class="icon-button" aria-label="Close subgraph save" :disabled="busy" @click="emit('close')">×</button></header>
    <div class="parameter-list"><label>Library name<input v-model="name" maxlength="256" :disabled="busy"></label><label>Save destination<select v-model="destination" :disabled="busy||loading" @change="destinationChanged"><option value="">New library entry (my own copy)</option><option v-for="item in entries" :key="item.id" :value="item.id">New version of {{item.name}}</option></select></label><label class="check-label"><input v-model="publish" type="checkbox" :disabled="busy||entry?.public">Make public for all users</label><p v-if="error" role="alert" class="field-error">{{error}}</p></div>
    <footer class="modal-footer"><button class="button primary" :disabled="busy||loading||!name.trim()" @click="save">{{busy?'Saving subgraph…':destination?'Save new version':'Save to library'}}</button></footer>
  </dialog>
</template>
