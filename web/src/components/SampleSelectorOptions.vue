<script setup lang="ts">
import { computed, ref } from 'vue'
import type { SampleChoice } from '../types'
import type { SampleEntry } from '../samples'
import SuggestInput from './SuggestInput.vue'
const props = defineProps<{ choices: SampleChoice[]; samples: SampleEntry[]; library: SampleEntry[]; attach?: (id: string) => Promise<SampleEntry>; disabled: boolean }>()
const emit = defineEmits<{ change: [choices: SampleChoice[]] }>()
const search = ref('')
const adding = ref(false)
// Search everything the user can reach. A sample that is not in the project yet
// has no asset id until it is picked, and is then added to the project first.
const choosable = computed(() => (props.library.length ? props.library : props.samples))
const label = (s: SampleEntry) => (s.asset ? `${s.name} (#${s.asset})` : s.name)
const match = computed(() => choosable.value.find(s => label(s)===search.value || s.name===search.value || (!!s.asset && String(s.asset)===search.value)))
async function add() {
  const sample = match.value
  if (!sample || adding.value || props.disabled || props.choices.length>=64) return
  let asset = sample.asset
  if (!asset) {
    if (!props.attach) return
    adding.value = true
    try { asset = (await props.attach(sample.id)).asset } finally { adding.value = false }
    if (!asset) return
  }
  emit('change',[...props.choices,{asset,name:sample.name,nickname:''}])
  search.value=''
}
function rename(i: number, nickname: string) {
  emit('change',props.choices.map((s,j)=>j===i?{...s,nickname:nickname.trim()}:s))
}
function move(i: number, delta: number) {
  const next=[...props.choices], other=i+delta
  if(other<0||other>=next.length)return
  ;[next[i],next[other]]=[next[other]!,next[i]!]
  emit('change',next)
}
</script>
<template>
  <section class="parameter-row sample-selector-options">
    <h3>Sample shortlist<HelpNote label="Sample shortlist">Add samples in the order you want, numbered from 0. Optional nicknames appear on the node. Click an entry on the node or connect a number to Index; connect Sample ID to a sampler's Sample ID input. The search covers every sample you can reach, and one that is not in this project yet is added to it when you pick it. Selecting a sample does not play a note.</HelpNote></h3>
    <div class="sample-add"><label>Find a sample<SuggestInput label="Find shortlist sample" :value="search" :suggestions="choosable.map(s=>({value:label(s),detail:s.asset?`${s.channels} ch`:`${s.channels} ch · add to project`}))" placeholder="Search all your samples…" :disabled="disabled||adding||choices.length>=64" @input="text=>search=text" @enter="add" /></label><button class="button small" :disabled="disabled||adding||!match||choices.length>=64" @click="add">{{adding?'Adding…':'Add sample'}}</button></div>
    <ol start="0">
      <li v-for="(sample,i) in choices" :key="i"><div class="choice-name"><span>{{i}} · {{sample.name}}</span><small>Sample ID {{sample.asset}}</small></div><label>Nickname<input :aria-label="`Nickname for sample ${i}`" :value="sample.nickname" maxlength="80" :disabled="disabled" placeholder="Optional nickname" @change="rename(i,($event.target as HTMLInputElement).value)"></label><div class="choice-actions"><button class="button small" :aria-label="`Move sample ${i} up`" :disabled="disabled||i===0" @click="move(i,-1)">↑</button><button class="button small" :aria-label="`Move sample ${i} down`" :disabled="disabled||i===choices.length-1" @click="move(i,1)">↓</button><button class="button small" :aria-label="`Remove sample ${i}`" :disabled="disabled" @click="emit('change',choices.filter((_,j)=>j!==i))">Remove</button></div></li>
    </ol>
    <p v-if="!choices.length" class="feature-note">No samples yet. Add a sample to create entry 0.</p>
    <p v-if="choices.length===64" class="feature-note">Shortlist limit: 64 samples.</p>
  </section>
</template>
<style scoped>
h3{display:flex;gap:8px;align-items:center}.sample-add{display:flex;gap:10px;align-items:end}.sample-add label{flex:1;min-width:0}label{display:flex;flex-direction:column;gap:6px;font-size:12px}ol{list-style:none;padding:0;margin:12px 0 0}li{padding:12px 0;display:grid;grid-template-columns:1fr 1fr;gap:8px;border-bottom:1px solid var(--line)}.choice-name{overflow-wrap:anywhere;min-width:0}.choice-name small{display:block;color:var(--muted);margin-top:5px}.choice-actions{grid-column:1/-1;display:flex;gap:6px}input{width:100%}@media(max-width:500px){li{grid-template-columns:1fr}.sample-add{flex-wrap:wrap}}
</style>
