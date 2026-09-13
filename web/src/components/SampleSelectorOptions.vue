<script setup lang="ts">
import { computed, ref } from 'vue'
import type { SampleChoice } from '../types'
import type { SampleEntry } from '../samples'
const props = defineProps<{ choices: SampleChoice[]; samples: SampleEntry[]; disabled: boolean }>()
const emit = defineEmits<{ change: [choices: SampleChoice[]] }>()
const search = ref('')
const label = (s: SampleEntry) => `${s.name} (#${s.asset})`
const match = computed(() => props.samples.find(s => s.asset && (label(s)===search.value || s.name===search.value || String(s.asset)===search.value)))
function add() {
  const sample = match.value
  if (!sample?.asset || props.disabled || props.choices.length>=64) return
  emit('change',[...props.choices,{asset:sample.asset,name:sample.name,nickname:''}])
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
    <h3>Sample shortlist<HelpNote label="Sample shortlist">Add project samples in the order you want, numbered from 0. Optional nicknames appear on the node. Click an entry on the node or connect a number to Index; connect Sample ID to a sampler's Sample ID input. Add samples to the project through the Samples library first. Selecting a sample does not play a note.</HelpNote></h3>
    <div class="sample-add"><label>Find a sample<input v-model="search" aria-label="Find shortlist sample" list="shortlist-suggestions" autocomplete="off" placeholder="Search project samples…" :disabled="disabled||choices.length>=64" @keydown.enter.prevent="add"><datalist id="shortlist-suggestions"><option v-for="sample in samples" :key="sample.id" :value="label(sample)">{{sample.channels}} ch</option></datalist></label><button class="button small" :disabled="disabled||!match||choices.length>=64" @click="add">Add sample</button></div>
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
