<script setup lang="ts">
import {computed,nextTick,onBeforeUnmount,onMounted,ref,watch} from 'vue'
import {api} from '../api'
import {matchesSample,parseTags,tagCloud,toggleTag,type SampleEntry} from '../samples'
import SampleEditor from './SampleEditor.vue'
import TagChips from './TagChips.vue'
const props=defineProps<{projectId:string;open:boolean;editable:boolean}>()
const emit=defineEmits<{touch:[event:PointerEvent,sample:SampleEntry];toggle:[];use:[sample:SampleEntry];entries:[samples:SampleEntry[]]}>()
const entries=ref<SampleEntry[]>([]),catalog=ref<SampleEntry[]>([]),search=ref(''),fullSearch=ref(''),full=ref(false),dialog=ref<HTMLDialogElement>(),editing=ref<SampleEntry>(),busy=ref(false),error=ref(''),previewId=ref('')
// Tag filters: chips are clickable everywhere, and every active tag must match.
const projectTags=ref<string[]>([]),browserTags=ref<string[]>([])
const projectCloud=computed(()=>tagCloud(entries.value)),browserCloud=computed(()=>tagCloud(catalog.value))
const counts=(cloud:{tag:string;count:number}[])=>Object.fromEntries(cloud.map(c=>[c.tag.toLowerCase(),c.count]))
const visible=computed(()=>entries.value.filter(s=>matchesSample(s,search.value,projectTags.value))),available=computed(()=>catalog.value.filter(s=>matchesSample(s,fullSearch.value,browserTags.value)))
let preview:HTMLAudioElement|undefined,previewUrl='',timer:ReturnType<typeof setTimeout>|undefined;let generation=0,alive=true
function stop(){generation++;preview?.pause();preview=undefined;if(previewUrl)URL.revokeObjectURL(previewUrl);previewUrl='';previewId.value='';clearTimeout(timer)}
async function play(s:SampleEntry){if(previewId.value===s.id){stop();return}stop();const token=generation;try{const response=await fetch(`/api/projects/${props.projectId}/samples/${s.id}/audio`);if(!response.ok)throw new Error('Sample unavailable');const blob=await response.blob();if(token!==generation)return;previewUrl=URL.createObjectURL(blob);preview=new Audio(previewUrl);previewId.value=s.id;await preview.play();timer=setTimeout(stop,3000);preview.onended=stop}catch(e){stop();error.value=String(e)}}
async function refresh(){try{const project=props.projectId;const result=await api<SampleEntry[]>(`/projects/${project}/samples`);if(!alive||project!==props.projectId)return;entries.value=result;emit('entries',entries.value);if(full.value)catalog.value=await api<SampleEntry[]>(`/projects/${props.projectId}/sample-library`)}catch(e){error.value=String(e)}}
async function browse(){full.value=true;await nextTick();dialog.value?.showModal();await refresh()}
function close(){stop();dialog.value?.close();full.value=false}
async function upload(event:Event){const input=event.target as HTMLInputElement;busy.value=true;error.value='';try{for(const file of input.files||[]){const form=new FormData();form.append('sample',file);const response=await fetch(`/api/projects/${props.projectId}/samples`,{method:'POST',headers:{'X-Pr0former':'1'},body:form});const result=await response.json();if(!response.ok)throw new Error(result.error)}await refresh()}catch(e){error.value=String(e);await refresh()}finally{busy.value=false;input.value=''}}
async function add(s:SampleEntry){busy.value=true;try{await api(`/projects/${props.projectId}/samples/${s.id}/add`,'POST',{});await refresh()}catch(e){error.value=String(e)}finally{busy.value=false}}
function drag(event:DragEvent,s:SampleEntry){if(!props.editable||s.asset===null){event.preventDefault();return}event.dataTransfer?.setData('application/pr0former',JSON.stringify({sample:s.id}));if(event.dataTransfer)event.dataTransfer.effectAllowed='copy'}
function edit(s:SampleEntry){stop();editing.value=s}
onMounted(refresh);watch(()=>props.projectId,()=>{stop();close();editing.value=undefined;void refresh()});onBeforeUnmount(()=>{alive=false;stop()})
defineExpose({refresh,edit})
</script>
<template>
<section class="sample-library" :class="{expanded:open}" aria-label="Sample library">
  <div class="library-heading"><button class="library-accordion-title" :aria-expanded="open" aria-controls="sample-library-content" @click="emit('toggle')"><span aria-hidden="true">{{open?'⌄':'›'}}</span> Samples</button><button class="text-button" aria-label="Refresh samples" @click="refresh">↻</button></div>
  <div v-if="open" id="sample-library-content" class="sample-library-content">
    <input v-model="search" aria-label="Search project samples" placeholder="Find project samples…">
    <TagChips v-if="projectCloud.length" class="tag-filter" :tags="projectCloud.map(c=>c.tag)" :active="projectTags" :counts="counts(projectCloud)" interactive @toggle="tag=>projectTags=toggleTag(projectTags,tag)" />
    <div class="sample-library-actions"><label class="button small"><span class="field-title">Import audio<HelpNote label="Import audio">FFmpeg audio formats · up to 30 s / 8 channels / 64 MB. Converted at the current engine rate.</HelpNote></span><input type="file" multiple hidden :disabled="!editable||busy" aria-label="Import project audio" @change="upload"></label><button class="button small" @click="browse">Browse all</button></div>
    <p v-if="busy" role="status">Importing audio…</p><p v-if="error" role="alert" class="field-error">{{error}}</p>
    <div class="sample-rows"><article v-for="s in visible" :key="s.id" class="sample-row"><div class="sample-row-title"><button class="text-button" :aria-label="`Preview ${s.name}`" @click="play(s)">{{previewId===s.id?'■':'▶'}}</button><button class="sample-name" :draggable="editable&&!busy" style="touch-action:none" @dragstart="drag($event,s)" @pointerdown="editable&&!busy&&emit('touch',$event,s)" :aria-label="`Edit sample ${s.name}`" @click="edit(s)">{{s.name}}</button><button :disabled="!editable||busy" :aria-label="`Add sampler for ${s.name}`" @click="emit('use',s)">＋</button></div><small>{{s.duration.toFixed(1)}} s · {{s.channels}} ch · {{s.category||s.author}}</small><TagChips :tags="parseTags(s.tags)" :active="projectTags" interactive @toggle="tag=>projectTags=toggleTag(projectTags,tag)" /></article><p v-if="!visible.length" class="feature-note">No matching project samples. Import audio or browse your library.</p></div>

  </div>
</section>
<dialog v-if="full" ref="dialog" class="parameter-modal sample-browser" aria-label="Full sample browser" @cancel.prevent="close">
<header class="modal-header"><h2>Sample browser</h2><button class="icon-button" aria-label="Close sample browser" @click="close">×</button></header>
<div class="parameter-list"><input v-model="fullSearch" aria-label="Search all samples" placeholder="Search name, tags, category, key, BPM or author…"><TagChips v-if="browserCloud.length" class="tag-filter" :tags="browserCloud.map(c=>c.tag)" :active="browserTags" :counts="counts(browserCloud)" interactive @toggle="tag=>browserTags=toggleTag(browserTags,tag)" /><div class="help-section-title">Browse samples<HelpNote label="Browse samples">Your samples and samples marked Global. Project-only samples from other authors stay available in the project sidebar. Click tags to filter; every selected tag must match.</HelpNote></div><p v-if="error" role="alert" class="field-error">{{error}}</p>
<article v-for="s in available" :key="s.id" class="sample-browser-row"><button class="button small" :aria-label="`Preview ${s.name}`" @click="play(s)">{{previewId===s.id?'Stop':'Preview'}}</button><div class="sample-browser-info"><button class="sample-name" :aria-label="`Edit sample ${s.name}`" @click="edit(s)">{{s.name}}<small>{{s.author}} · {{s.global?'Global':'Private'}} · {{s.channels}} ch · {{s.duration.toFixed(1)}} s{{s.category?` · ${s.category}`:''}}</small></button><TagChips :tags="parseTags(s.tags)" :active="browserTags" interactive @toggle="tag=>browserTags=toggleTag(browserTags,tag)" /></div><button class="button small" :disabled="!editable||busy||s.asset!==null" @click="add(s)">{{s.asset!==null?'In project':'Add to project'}}</button></article><p v-if="!available.length">No matching samples.</p>
</div></dialog>
<SampleEditor v-if="editing" :key="editing.id" :project-id="projectId" :sample="editing" @close="editing=undefined" @saved="refresh" />
</template>
<style scoped>
.modal-header{display:flex;align-items:center;justify-content:space-between}
.sample-library{border-top:1px solid var(--line);padding:8px 0;display:flex;flex:0 0 auto;flex-direction:column;min-height:0}.sample-library.expanded{flex:1 1 0%}.sample-library-content{display:flex;flex-direction:column;min-height:0;gap:10px;overflow:auto}.sample-library-content>input{width:100%;font-size:11px}.sample-library-actions{display:flex;gap:5px;flex-wrap:wrap}.sample-rows{min-height:0;overflow:auto}.sample-row{padding:8px 0;border-bottom:1px solid var(--line)}.sample-row-title{display:flex;gap:6px;align-items:center}.sample-name{flex:1;min-width:0;text-align:left;overflow-wrap:anywhere}.sample-row small,.sample-browser-row small{display:block;color:var(--muted);font-size:10px;margin-top:4px}.sample-browser{width:min(850px,95vw)}.sample-browser-row{display:flex;gap:14px;align-items:center;padding:14px 0;border-bottom:1px solid var(--line)}.sample-browser-info{flex:1;min-width:0}.sample-browser-info .sample-name{width:100%}
</style>
