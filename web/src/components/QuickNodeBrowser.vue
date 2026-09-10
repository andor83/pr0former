<script setup lang="ts">
import {computed,nextTick,onBeforeUnmount,onMounted,ref,watch} from 'vue'
import {Search,Box,Boxes,AudioLines,X} from '@lucide/vue'
import {api} from '../api'
import {matchesMetadata} from '../projectSearch'
import type {Descriptor} from '../types'
import type {SampleEntry} from '../samples'
import type {LibraryItem} from '../libraryTouch'
import type {LibraryEntry} from './SubgraphLibrary.vue'
const props=defineProps<{descriptors:Descriptor[];projectId:string;samples:SampleEntry[]}>()
const emit=defineEmits<{close:[];insert:[item:LibraryItem,point?:{x:number;y:number}]}>()
const query=ref(''),selected=ref(0),input=ref<HTMLInputElement>(),list=ref<HTMLElement>(),groups=ref<LibraryEntry[]>([]),catalog=ref<SampleEntry[]>([]),error=ref(''),loading=ref(true),dragging=ref(false)
type Result={id:string;name:string;detail:string;type:'node'|'group'|'sample';item:LibraryItem;search:unknown}
const results=computed(()=>{
 const nodes:Result[]=props.descriptors.filter(d=>d.kind!=='subgraph').map(d=>({id:`node-${d.kind}`,name:d.label,detail:d.category,type:'node',item:{kind:d.kind},search:[d.label,d.kind,d.category,d.description,d.aliases]}))
 const saved:Result[]=groups.value.flatMap(g=>{const v=g.versions[0];return v?[{id:`group-${g.id}`,name:v.name||g.name,detail:`Node group · ${g.owner} · v${v.version}`,type:'group' as const,item:{library:g.id,version:v.version},search:g}]:[]})
 const samples:Result[]=[...new Map([...catalog.value,...props.samples].map(s=>[s.id,s])).values()].map(s=>({id:`sample-${s.id}`,name:s.name,detail:`Sample · ${s.channels} ch · ${s.duration.toFixed(1)} s`,type:'sample',item:{sample:s.id},search:s}))
 const q=query.value.trim().toLowerCase()
 const rank=(r:Result)=>r.name.toLowerCase()===q?0:r.name.toLowerCase().startsWith(q)?1:r.name.toLowerCase().includes(q)?2:3
 return [...nodes,...saved,...samples].filter(r=>matchesMetadata(r.search,query.value)).sort((a,b)=>rank(a)-rank(b))
})
watch(query,()=>selected.value=0)
watch(results,()=>selected.value=Math.min(selected.value,Math.max(0,results.value.length-1)))
watch(selected,()=>nextTick(()=>list.value?.querySelector('[aria-selected="true"]')?.scrollIntoView({block:'nearest'})))
function choose(){const r=results.value[selected.value];if(r)emit('insert',r.item)}
function keys(e:KeyboardEvent){
 if(e.isComposing)return
 if(e.key==='Escape'){e.preventDefault();e.stopPropagation();emit('close');return}
 if(e.key==='Enter'&&e.target===input.value){e.preventDefault();choose();return}
 const moves:Record<string,number>={ArrowDown:1,ArrowUp:-1,PageDown:8,PageUp:-8}
 if(e.key in moves){e.preventDefault();selected.value=Math.max(0,Math.min(results.value.length-1,selected.value+moves[e.key]!))}
 if(e.key==='Home'&&e.ctrlKey){e.preventDefault();selected.value=0}
 if(e.key==='End'&&e.ctrlKey){e.preventDefault();selected.value=Math.max(0,results.value.length-1)}
 if(e.key==='Tab'){e.preventDefault();const close=document.getElementById('quick-close');if(document.activeElement===input.value)close?.focus();else input.value?.focus()}
}
let cleanup=()=>{}
function startPointer(e:PointerEvent,r:Result){
 if(!e.isPrimary||e.button!==0)return
 const target=e.currentTarget as HTMLElement;const origin={x:e.clientX,y:e.clientY};target.setPointerCapture(e.pointerId);e.preventDefault()
 const move=(event:PointerEvent)=>{if(event.pointerId!==e.pointerId)return;if(Math.hypot(event.clientX-origin.x,event.clientY-origin.y)>8)dragging.value=true}
 const end=(event:PointerEvent)=>{if(event.pointerId!==e.pointerId)return;const point={x:event.clientX,y:event.clientY};const valid=event.type==='pointerup'&&(!dragging.value||!!document.elementFromPoint(point.x,point.y)?.closest('.graph-canvas .vue-flow'));const moved=dragging.value;cleanup();if(valid)emit('insert',r.item,moved?point:undefined);else emit('close')}
 window.addEventListener('pointermove',move);window.addEventListener('pointerup',end);window.addEventListener('pointercancel',end)
 cleanup=()=>{window.removeEventListener('pointermove',move);window.removeEventListener('pointerup',end);window.removeEventListener('pointercancel',end);if(target.hasPointerCapture(e.pointerId))target.releasePointerCapture(e.pointerId)}
}
const previous=document.activeElement as HTMLElement|null
let alive=true
onMounted(async()=>{await nextTick();if(!matchMedia('(prefers-reduced-motion: reduce)').matches)await new Promise(resolve=>setTimeout(resolve,100));if(!alive)return;input.value?.focus();const fetched=await Promise.allSettled([api<LibraryEntry[]>('/subgraphs'),api<SampleEntry[]>(`/projects/${props.projectId}/sample-library`)]);if(!alive)return;const [g,s]=fetched;if(g.status==='fulfilled')groups.value=g.value;else error.value='Node groups could not be loaded.';if(s.status==='fulfilled')catalog.value=s.value;else error.value+=' Samples could not be loaded.';loading.value=false})
onBeforeUnmount(()=>{alive=false;cleanup();if(previous?.isConnected)previous.focus({preventScroll:true})})
</script>
<template>
<div class="quick-node-overlay" :class="{dragging}" @pointerdown.self="emit('close')"><section class="quick-node-panel" role="dialog" aria-modal="true" aria-label="Quick node browser" @keydown="keys"><header><Search :size="21"/><input ref="input" v-model="query" role="combobox" aria-label="Search nodes, node groups and samples" aria-controls="quick-results" aria-autocomplete="list" aria-expanded="true" :aria-activedescendant="results[selected]?.id" placeholder="Search nodes, node groups and samples…"><button id="quick-close" class="icon-button" aria-label="Close quick node browser" @click="emit('close')"><X :size="18"/></button></header><p v-if="error" class="field-error" role="alert">{{error}}</p><div id="quick-results" ref="list" class="quick-results" role="listbox" aria-label="Node browser results"><div v-for="(r,i) in results" :id="r.id" :key="r.id" class="quick-result" role="option" :aria-selected="i===selected" :aria-label="`${r.name} · ${r.type==='group'?'Node group':r.type}`" draggable="false" @mousemove="selected=i" @pointerdown="startPointer($event,r)"><Box v-if="r.type==='node'" :size="21"/><Boxes v-else-if="r.type==='group'" :size="21"/><AudioLines v-else :size="21"/><div><strong>{{r.name}}</strong><small>{{r.detail}}</small></div><span class="quick-kind">{{r.type==='group'?'Group':r.type}}</span></div><p v-if="!results.length">{{loading?'Loading libraries…':'No matching nodes, groups or samples.'}}</p></div><footer><span>↑ ↓ Navigate · Enter Insert · Esc Close</span><span>Drag to place · {{results.length}} results</span></footer></section></div>
</template>
<style scoped>
.quick-node-overlay{position:fixed;inset:0;z-index:200;background:#08101077;display:flex;align-items:flex-start;justify-content:center;padding:14dvh 16px 16px}.quick-node-panel{width:min(640px,100%);max-height:72dvh;background:#1b2425;border:1px solid #4c7772;border-radius:12px;box-shadow:0 22px 90px #0009;overflow:hidden;display:flex;flex-direction:column;animation:quick-appear 100ms ease-out}.quick-node-panel header{display:flex;align-items:center;gap:12px;padding:17px;border-bottom:1px solid var(--line);color:var(--cyan)}.quick-node-panel input{border:0;background:transparent;min-width:0;flex:1;outline:none;font-size:16px}.quick-results{min-height:0;overflow:auto;padding:6px;overscroll-behavior:contain}.quick-result{display:flex;align-items:center;gap:13px;padding:12px;border-radius:7px;cursor:grab;touch-action:none}.quick-result[aria-selected=true]{background:#29423f;outline:1px solid #5b8d83}.quick-result svg{color:var(--cyan);flex-shrink:0}.quick-result>div{flex:1;min-width:0}.quick-result strong{font-size:13px}.quick-result small{display:block;font-size:10px;color:var(--muted);margin-top:4px}.quick-kind{text-transform:uppercase;font-size:9px;color:var(--amber)}footer{display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap;padding:12px 18px;color:var(--muted);font-size:10px}.quick-node-overlay.dragging{pointer-events:none;background:transparent}.dragging .quick-node-panel{opacity:.15;pointer-events:none}@keyframes quick-appear{from{opacity:0;transform:translateY(-5px)}to{opacity:1;transform:translateY(0)}}@media(prefers-reduced-motion:reduce){.quick-node-panel{animation:none}}
</style>
