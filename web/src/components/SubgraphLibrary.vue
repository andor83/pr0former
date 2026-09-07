<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api } from '../api'
export interface LibraryEntry { id:string; name:string; owned:boolean; public:boolean; owner:string; versions:{version:number;name:string;public:boolean}[] }
const props=defineProps<{editable:boolean}>()
const emit=defineEmits<{insert:[id:string,version:number]}>()
const entries=ref<LibraryEntry[]>([]), selected=ref<Record<string,number>>({}), error=ref('')
async function refresh(){try{entries.value=await api('/subgraphs');error.value='';for(const entry of entries.value)if(!entry.versions.some(v=>v.version===selected.value[entry.id]))selected.value[entry.id]=entry.versions[0]!.version}catch(e){error.value=String(e)}}
function drag(event:DragEvent,entry:LibraryEntry){if(!props.editable)return;event.dataTransfer?.setData('application/pr0former',JSON.stringify({library:entry.id,version:selected.value[entry.id]}));if(event.dataTransfer)event.dataTransfer.effectAllowed='copy'}
defineExpose({refresh})
onMounted(refresh)
</script>
<template>
  <section class="subgraph-library" aria-label="Subgraph library">
    <div class="library-heading"><h3>Subgraph library</h3><button class="text-button" @click="refresh" aria-label="Refresh subgraph library">↻</button></div>
    <p v-if="error" role="alert" class="field-error">{{error}}</p>
    <p v-if="!entries.length" class="feature-note">Save a subgraph from its right-click menu. Private entries are yours; public versions can be used by everyone on this server.</p>
    <article v-for="entry in entries" :key="entry.id" class="subgraph-library-entry">
      <button class="library-node" :disabled="!editable" :draggable="editable" @dragstart="drag($event,entry)" @click="emit('insert',entry.id,selected[entry.id]!)" :aria-label="`Insert ${entry.name}`"><span class="library-glyph">▣</span><span>{{entry.name}}<small>{{entry.owner}} · {{entry.public?'Public':'Private'}}</small></span></button>
      <select v-model.number="selected[entry.id]" :aria-label="`Version of ${entry.name}`"><option v-for="version in entry.versions" :key="version.version" :value="version.version">v{{version.version}} · {{version.name}}{{version.public?'':' · private'}}</option></select>
    </article>
  </section>
</template>
