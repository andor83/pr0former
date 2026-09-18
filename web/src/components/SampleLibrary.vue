<script setup lang="ts">
import {computed,nextTick,onBeforeUnmount,onMounted,ref,watch} from 'vue'
import {api} from '../api'
import {parseSampleQuery,parseTags,searchSamples,tagCloud,toggleQueryTag,toggleTag,type SampleEntry} from '../samples'
import SampleEditor from './SampleEditor.vue'
import TagChips from './TagChips.vue'
const props=defineProps<{projectId:string;open:boolean;editable:boolean}>()
const emit=defineEmits<{touch:[event:PointerEvent,sample:SampleEntry];toggle:[];use:[sample:SampleEntry];entries:[samples:SampleEntry[]]}>()
const entries=ref<SampleEntry[]>([]),catalog=ref<SampleEntry[]>([]),search=ref(''),fullSearch=ref(''),full=ref(false),dialog=ref<HTMLDialogElement>(),editing=ref<SampleEntry>(),busy=ref(false),error=ref(''),previewId=ref('')
// Sidebar tag chips are an AND filter; the full browser keeps its tags in the query as `tag:a,b` (any of them, most hits first).
const projectTags=ref<string[]>([])
const projectCloud=computed(()=>tagCloud(entries.value)),browserCloud=computed(()=>tagCloud(catalog.value).slice(0,8))
const counts=(cloud:{tag:string;count:number}[])=>Object.fromEntries(cloud.map(c=>[c.tag.toLowerCase(),c.count]))
const visible=computed(()=>searchSamples(entries.value,search.value,projectTags.value)),available=computed(()=>searchSamples(catalog.value,fullSearch.value))
const queryTags=computed(()=>parseSampleQuery(fullSearch.value).tags)
function queryTag(tag:string){fullSearch.value=toggleQueryTag(fullSearch.value,tag)}
const page=ref(1),pageSize=ref(25)
const pageCount=computed(()=>Math.max(1,Math.ceil(available.value.length/pageSize.value)))
const pageItems=computed(()=>available.value.slice((page.value-1)*pageSize.value,page.value*pageSize.value))
const pageRange=computed(()=>available.value.length?`${(page.value-1)*pageSize.value+1}–${Math.min(page.value*pageSize.value,available.value.length)} of ${available.value.length} samples`:'0 samples')
watch([fullSearch,pageSize],()=>{page.value=1});watch(pageCount,count=>{if(page.value>count)page.value=count})
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
<div class="sample-browser-tools"><input v-model="fullSearch" aria-label="Search all samples" placeholder="Search name, tags, category, key, BPM or author · tag:acoustic,electric matches either tag"><TagChips v-if="browserCloud.length" class="tag-filter" :tags="browserCloud.map(c=>c.tag)" :active="queryTags" :counts="counts(browserCloud)" interactive @toggle="queryTag" /><p v-if="error" role="alert" class="field-error">{{error}}</p></div>
<div class="sample-browser-list" role="region" aria-label="Sample results"><div class="sample-browser-head" aria-hidden="true"><span></span><span>Sample</span><span>Details</span><span>Tags</span><span></span></div>
<article v-for="s in pageItems" :key="s.id" class="sample-browser-row"><button class="button small sb-play" :aria-label="`Preview ${s.name}`" @click="play(s)">{{previewId===s.id?'Stop':'Preview'}}</button><button class="sample-name" :aria-label="`Edit sample ${s.name}`" :title="s.name" @click="edit(s)">{{s.name}}</button><small class="sb-details" :title="`${s.author} · ${s.global?'Global':'Private'} · ${s.channels} ch · ${s.duration.toFixed(1)} s${s.category?` · ${s.category}`:''}`">{{s.author}} · {{s.global?'Global':'Private'}} · {{s.channels}} ch · {{s.duration.toFixed(1)}} s{{s.category?` · ${s.category}`:''}}</small><div class="sb-tags"><TagChips :tags="parseTags(s.tags)" :active="queryTags" interactive @toggle="queryTag" /></div><button class="button small sb-add" :disabled="!editable||busy||s.asset!==null" @click="add(s)">{{s.asset!==null?'In project':'Add to project'}}</button></article><p v-if="!available.length" class="sample-browser-empty">No matching samples.</p></div>
<footer class="sample-browser-pager"><span role="status" aria-live="polite">{{pageRange}}</span><label class="page-size">Per page<select v-model.number="pageSize" aria-label="Samples per page"><option v-for="n in [10,25,50,100]" :key="n" :value="n">{{n}}</option></select></label><div class="pager-buttons"><button class="button small" :disabled="page<=1" aria-label="Previous page" @click="page--">‹ Prev</button><span>Page {{page}} of {{pageCount}}</span><button class="button small" :disabled="page>=pageCount" aria-label="Next page" @click="page++">Next ›</button></div></footer>
</dialog>
<SampleEditor v-if="editing" :key="editing.id" :project-id="projectId" :sample="editing" @close="editing=undefined" @saved="refresh" />
</template>
<style scoped>
.modal-header{display:flex;align-items:center;justify-content:space-between}
.sample-library{border-top:1px solid var(--line);padding:8px 0;display:flex;flex:0 0 auto;flex-direction:column;min-height:0}.sample-library.expanded{flex:1 1 0%}.sample-library-content{display:flex;flex-direction:column;min-height:0;gap:10px;overflow:auto}.sample-library-content>input{width:100%;font-size:11px}.sample-library-actions{display:flex;gap:5px;flex-wrap:wrap}.sample-rows{min-height:0;overflow:auto}.sample-row{padding:8px 0;border-bottom:1px solid var(--line)}.sample-row-title{display:flex;gap:6px;align-items:center}.sample-name{flex:1;min-width:0;text-align:left;overflow-wrap:anywhere}.sample-row small{display:block;color:var(--ink-muted);font-size:10px;margin-top:4px}
/* Full browser: pinned header/tools and pager, a scrolling list with a sticky column header, one row per sample. */
.sample-browser{width:min(1000px,96vw);max-width:1000px}
.sample-browser[open]{display:flex;flex-direction:column;max-height:calc(100dvh - 64px)}
.sample-browser>.modal-header,.sample-browser-tools,.sample-browser-pager{flex-shrink:0}
.sample-browser-tools{display:flex;flex-direction:column;gap:8px;padding:0 27px 10px}
.sample-browser-tools>input{display:block;width:100%}
.sample-browser-tools :deep(.tag-chips){margin:0}
.sample-browser-list{flex:1 1 auto;min-height:0;overflow:auto;overscroll-behavior:contain;padding:0 27px}
.sample-browser-head,.sample-browser-row{display:grid;grid-template-columns:76px minmax(150px,1.1fr) minmax(180px,1.4fr) minmax(140px,2fr) 118px;gap:12px;align-items:center}
.sample-browser-head{position:sticky;top:0;z-index:1;padding:8px 0;background:var(--modal-face);border-bottom:1px solid var(--rail-line);font-size:10px;letter-spacing:1.2px;text-transform:uppercase;color:var(--ink-muted)}
.sample-browser-row{padding:7px 0;border-bottom:1px solid var(--line)}
.sample-browser-row .sample-name{flex:none;min-width:0;font-size:13px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;overflow-wrap:normal}
.sample-browser-row .sb-details{display:block;min-width:0;margin:0;color:var(--ink-muted);font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.sb-tags{min-width:0;overflow:hidden}.sb-tags :deep(.tag-chips){margin:0;flex-wrap:nowrap;overflow:hidden}
.sb-add{justify-self:end}
.sample-browser-empty{padding:16px 0;color:var(--ink-muted)}
.sample-browser-pager{display:flex;align-items:center;justify-content:space-between;gap:12px;flex-wrap:wrap;padding:10px 27px;border-top:1px solid var(--rail-line);background:var(--rail-face);font-size:12px;color:var(--ink-muted)}
.sample-browser-pager .page-size{display:flex;align-items:center;gap:8px}.sample-browser-pager select{padding:6px 26px 6px 8px;font-size:12px}
.pager-buttons{display:flex;align-items:center;gap:8px}
@media(max-width:740px){
  .sample-browser-tools,.sample-browser-list,.sample-browser-pager{padding-left:12px;padding-right:12px}
  .sample-browser-head{display:none}
  .sample-browser-row{grid-template-columns:auto 1fr auto;grid-template-areas:"play name add" "play details add" "play tags add";row-gap:4px;padding:10px 0}
  .sb-play{grid-area:play}.sample-browser-row .sample-name{grid-area:name;white-space:normal;overflow-wrap:anywhere}.sample-browser-row .sb-details{grid-area:details;white-space:normal}.sb-tags{grid-area:tags}.sb-tags :deep(.tag-chips){flex-wrap:wrap;overflow:visible}.sb-add{grid-area:add}
}
</style>
