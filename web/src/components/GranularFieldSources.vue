<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId } from 'vue'
import type { GraphEdge, GraphNode, SampleChoice } from '../types'
import { midiNoteLabel, type SampleEntry } from '../samples'
import { FIELD_MAX_LIVE, FIELD_MAX_SAMPLES, fieldDefault, fieldValue, removeSlotParameters, slotKeys, swapSlotParameters } from '../granularField'
// Second dialog of a Granular Field node: the ordered sample slots and the two live
// inputs. It stacks above the node modal in the browser's top layer.
const props = defineProps<{ node: GraphNode; samples: SampleEntry[]; edges: GraphEdge[]; nodes: GraphNode[]; values?: Record<string, number>; active: boolean; stale: boolean; editable: boolean; saving: boolean }>()
const emit = defineEmits<{ close: []; sources: [choices: SampleChoice[], parameters: Record<string, number>]; change: [key: string, value: number] }>()
const dialog = ref<HTMLDialogElement>()
const uid = useId()
const search = ref('')
const live = computed(() => props.active && !props.stale)
const choices = computed(() => (props.node.sample_choices ?? []).slice(0, FIELD_MAX_SAMPLES))
const busy = computed(() => !props.editable || props.saving)
const label = (s: SampleEntry) => `${s.name} (#${s.asset})`
const match = computed(() => props.samples.find(s => s.asset && (label(s) === search.value || s.name === search.value || String(s.asset) === search.value)))
const driver = (port: string) => props.edges.find(e => e.target === props.node.id && e.target_port === port)
const sourceName = (edge: GraphEdge) => `${props.nodes.find(n => n.id === edge.source)?.label || edge.source} / ${edge.source_port}`
const valueOf = (key: string) => fieldValue(props.node, key, live.value ? props.values : undefined)
function add() {
  const sample = match.value
  if (!sample?.asset || busy.value || choices.value.length >= FIELD_MAX_SAMPLES) return
  const slot = choices.value.length + 1, keys = slotKeys(slot)
  emit('sources', [...choices.value, { asset: sample.asset, name: sample.name, nickname: '' }], { [keys.x]: fieldDefault(keys.x), [keys.y]: fieldDefault(keys.y), [keys.tune]: 0, [keys.gain]: 1, [keys.sample]: 0 })
  search.value = ''
}
function rename(i: number, nickname: string) { emit('sources', choices.value.map((s, j) => j === i ? { ...s, nickname: nickname.trim() } : s), {}) }
function move(i: number, delta: number) {
  const next = [...choices.value], other = i + delta
  if (other < 0 || other >= next.length) return
  ;[next[i], next[other]] = [next[other]!, next[i]!]
  emit('sources', next, swapSlotParameters(props.node.parameters, i + 1, other + 1))
}
function remove(i: number) { emit('sources', choices.value.filter((_, j) => j !== i), removeSlotParameters(props.node.parameters, i + 1, choices.value.length)) }
function number(key: string, text: string, min: number, max: number, integer = false) {
  const v = Number(text)
  if (!Number.isFinite(v)) return
  const value = Math.max(min, Math.min(max, integer ? Math.round(v) : Math.round(v * 100) / 100))
  emit('change', key, value)
}
function trap(event: KeyboardEvent) {
  if (event.key !== 'Tab') return
  const els = dialog.value?.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select:not(:disabled),[tabindex="0"]')
  if (!els?.length) return
  const first = els[0]!, last = els[els.length - 1]!
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
}
let previousFocus: HTMLElement | null = null
onMounted(async () => { previousFocus = document.activeElement as HTMLElement; await nextTick(); dialog.value?.showModal() })
onBeforeUnmount(() => { dialog.value?.close(); if (previousFocus?.isConnected) previousFocus.focus() })
</script>
<template>
  <dialog ref="dialog" class="parameter-modal granular-sources-modal" :aria-labelledby="`sources-title-${uid}`" @cancel.prevent="emit('close')" @keydown.stop="trap" @click.stop @pointerdown.stop>
    <header class="modal-header"><div class="modal-icon">⁘</div><div><div class="eyebrow">Granular Field</div><h2 :id="`sources-title-${uid}`">Sources</h2></div><button class="icon-button" aria-label="Close sources" @click="emit('close')">✕</button></header>
    <div class="parameter-list">
      <section class="parameter-row">
        <h3>Sample slots<HelpNote label="Sample slots">Up to eight project samples, numbered from 1. Each slot has a position on the field, a tune offset in semitones and a gain; drag the circles on the field to place them. Slot N also exposes a Sample N ID input on the node, so a Sample selector or number can swap that slot's sample while playing. Moving a slot carries its settings with it.</HelpNote></h3>
        <div class="sample-add"><label>Find a sample<input v-model="search" aria-label="Find field sample" :list="`field-samples-${uid}`" autocomplete="off" placeholder="Search project samples…" :disabled="busy || choices.length >= FIELD_MAX_SAMPLES" @keydown.enter.prevent="add"><datalist :id="`field-samples-${uid}`"><option v-for="sample in samples" :key="sample.id" :value="label(sample)">{{ sample.channels }} ch</option></datalist></label><button class="button small" :disabled="busy || !match || choices.length >= FIELD_MAX_SAMPLES" @click="add">Add sample</button></div>
        <ol>
          <li v-for="(sample, i) in choices" :key="i" class="slot-row">
            <div class="slot-name"><strong>{{ i + 1 }} · {{ sample.name }}</strong><small>Sample ID {{ sample.asset }}<template v-if="live && values?.[`_sample_${i + 1}_missing`]"> · <span class="missing">audio missing</span></template><template v-if="driver(`sample_${i + 1}`)"> · setter cabled from {{ sourceName(driver(`sample_${i + 1}`)!) }}</template></small></div>
            <label>Nickname<input :aria-label="`Nickname for slot ${i + 1}`" :value="sample.nickname" maxlength="80" :disabled="busy" placeholder="Optional" @change="rename(i, ($event.target as HTMLInputElement).value)"></label>
            <label>Tune<template v-if="driver(slotKeys(i + 1).tune)"><output>{{ valueOf(slotKeys(i + 1).tune).toFixed(2) }} st · cabled</output></template><input v-else type="number" step="0.5" min="-24" max="24" :aria-label="`Tune for slot ${i + 1}`" :value="valueOf(slotKeys(i + 1).tune)" :disabled="busy" @change="number(slotKeys(i + 1).tune, ($event.target as HTMLInputElement).value, -24, 24)"></label>
            <label>Gain<template v-if="driver(slotKeys(i + 1).gain)"><output>{{ valueOf(slotKeys(i + 1).gain).toFixed(2) }} · cabled</output></template><input v-else type="number" step="0.05" min="0" max="1" :aria-label="`Gain for slot ${i + 1}`" :value="valueOf(slotKeys(i + 1).gain)" :disabled="busy" @change="number(slotKeys(i + 1).gain, ($event.target as HTMLInputElement).value, 0, 1)"></label>
            <div class="slot-actions"><button class="button small" :aria-label="`Move slot ${i + 1} up`" :disabled="busy || i === 0" @click="move(i, -1)">↑</button><button class="button small" :aria-label="`Move slot ${i + 1} down`" :disabled="busy || i === choices.length - 1" @click="move(i, 1)">↓</button><button class="button small" :aria-label="`Remove slot ${i + 1}`" :disabled="busy" @click="remove(i)">Remove</button></div>
          </li>
        </ol>
        <p v-if="!choices.length" class="feature-note">No samples yet. Add a project sample to create slot 1.</p>
        <p v-if="choices.length >= FIELD_MAX_SAMPLES" class="feature-note">A field holds at most 8 samples.</p>
      </section>
      <section class="parameter-row">
        <h3>Live inputs<HelpNote label="Live inputs">Each live input joins the field only while a cable feeds its audio port, so nothing else needs enabling. Its buffer holds the most recent audio, from 100 ms to 10 s, and Position 0 reads the oldest of it while 1 reads the newest. The buffer length can change while the engine runs.</HelpNote></h3>
        <div v-for="n in FIELD_MAX_LIVE" :key="n" class="live-row">
          <div class="slot-name"><strong>Live {{ n }}</strong><small v-if="driver(`live_${n}`)">Fed by {{ sourceName(driver(`live_${n}`)!) }}<template v-if="live"> · buffer {{ Math.round((values?.[`_live_${n}_fill`] ?? 0) * 100) }}% full</template></small><small v-else>Connect audio to the Live {{ n }} input to add it to the field.</small></div>
          <template v-if="driver(`live_${n}`)">
            <label>Buffer<input type="number" step="50" min="100" max="10000" :aria-label="`Live ${n} buffer`" :value="valueOf(`live_${n}_buffer_ms`)" :disabled="busy || !!driver(`live_${n}_buffer_ms`)" @change="number(`live_${n}_buffer_ms`, ($event.target as HTMLInputElement).value, 100, 10000, true)"><span>ms</span></label>
            <label>Tune<input type="number" step="0.5" min="-24" max="24" :aria-label="`Live ${n} tune`" :value="valueOf(`live_${n}_tune`)" :disabled="busy || !!driver(`live_${n}_tune`)" @change="number(`live_${n}_tune`, ($event.target as HTMLInputElement).value, -24, 24)"></label>
            <label>Gain<input type="number" step="0.05" min="0" max="1" :aria-label="`Live ${n} gain`" :value="valueOf(`live_${n}_gain`)" :disabled="busy || !!driver(`live_${n}_gain`)" @change="number(`live_${n}_gain`, ($event.target as HTMLInputElement).value, 0, 1)"></label>
          </template>
        </div>
      </section>
    </div>
    <footer class="modal-footer"><span>Edits apply immediately.</span><button class="button" @click="emit('close')">Done</button></footer>
  </dialog>
</template>
<style scoped>
h3{display:flex;gap:8px;align-items:center;margin-bottom:12px}.sample-add{display:flex;gap:10px;align-items:end}.sample-add label{flex:1;min-width:0}
label{display:flex;flex-direction:column;gap:6px;font-size:12px}label output{font-size:12px;color:var(--muted)}label span{font-size:10px;color:var(--muted)}
ol{list-style:none;padding:0;margin:12px 0 0}
.slot-row,.live-row{display:grid;grid-template-columns:minmax(0,1.6fr) 1fr 90px 90px;gap:10px 12px;align-items:end;padding:12px 0;border-bottom:1px solid var(--line)}
.live-row{grid-template-columns:minmax(0,1.6fr) 110px 90px 90px}
.slot-name{min-width:0;overflow-wrap:anywhere}.slot-name strong{font-weight:500;font-size:13px;display:block}.slot-name small{display:block;color:var(--muted);margin-top:4px;font-size:10px}.missing{color:var(--red)}
.slot-actions{grid-column:1/-1;display:flex;gap:6px}
input{width:100%}
.icon-button{margin-left:auto;align-self:flex-start;font-size:14px;padding:6px 10px}
@media(max-width:600px){.slot-row,.live-row{grid-template-columns:1fr 1fr}.sample-add{flex-direction:column;align-items:stretch}}
</style>
