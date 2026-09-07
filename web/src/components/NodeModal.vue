<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { X, Link2, ArrowUpRight, Unplug, RotateCcw, Activity } from 'lucide-vue-next'
import type { Descriptor, GraphNode, GraphEdge, Parameter } from '../types'
import { finiteInput, formatValue } from '../api'
import DataVisualizer from './DataVisualizer.vue'
import SpectralCurveEditor from './SpectralCurveEditor.vue'
import type {Visualization} from '../types'
const props = defineProps<{ visualization?:Visualization; sampleRate?:number;blockSize?:number; interfaces?: {id:number;name:string}[]; node: GraphNode; descriptor: Descriptor; edges: GraphEdge[]; nodes: GraphNode[]; values?: Record<string, number>; stale: boolean; editable: boolean; active: boolean; saving: boolean }>()
const emit = defineEmits<{ close: []; change: [key: string, value: number]; disconnect: [edge: GraphEdge]; source: [id: string]; undo: []; remove: []; upload: [file: File]; control: [value:number|string]; curve: [parameters: Record<string, number>]; channels: [width: number] }>()
const dialog = ref<HTMLDialogElement>()
const history = ref<Record<string, number[]>>({})
const draft = ref<Record<string, number>>({})
const textDraft = ref<Record<string, string>>({})
const errors = ref<Record<string, string>>({})
let previousFocus: HTMLElement | null = null
const links = computed(() => Object.fromEntries(props.edges.filter(e => e.target === props.node.id).map(e => [e.target_port, e])))
watch(() => props.node.id, () => { history.value = {}; draft.value = {}; textDraft.value = {}; errors.value = {} })
watch(() => props.values, values => {
  if (!values || props.stale) return
  for (const p of props.descriptor.parameters) {
    const v = values[p.id]
    if (v !== undefined) history.value[p.id] = [...(history.value[p.id] || []), v].slice(-100)
  }
})
function trace(p: Parameter) {
  const values = history.value[p.id] || []
  const min = Math.min(...values), max = Math.max(...values), span = max - min || 1
  return values.map((v, i) => `${i * 240 / 99},${28 - (v - min) / span * 24}`).join(' ')
}
function value(p: Parameter) { return draft.value[p.id] ?? props.node.parameters[p.id] ?? p.default }
function edit(p: Parameter, text: string, commit: boolean) {
  const v = finiteInput(text, p.min, p.max)
  if (v === null) { errors.value[p.id] = `Enter ${p.min} to ${p.max}`; return }
  delete errors.value[p.id]
  delete textDraft.value[p.id]
  draft.value[p.id] = v
  if (commit || !p.structural) emit('change', p.id, v)
}
function curveValues(prefix: string, neutral: number) { return Array.from({ length: 33 }, (_, i) => props.node.parameters[`${prefix}_${i}`] ?? neutral) }
function curveChange(prefix: string, values: number[]) { emit('curve', Object.fromEntries(values.map((value, i) => [`${prefix}_${i}`, value]))) }
function sourceName(edge: GraphEdge) { return props.nodes.find(n => n.id === edge.source)?.label || edge.source }
function trap(event: KeyboardEvent) {
  if (event.key !== 'Tab') return
  const els = dialog.value?.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select:not(:disabled),[tabindex="0"]')
  if (!els?.length) return
  const first = els[0], last = els[els.length - 1]
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
}
onMounted(async () => { previousFocus = document.activeElement as HTMLElement; await nextTick(); dialog.value?.showModal() })
onBeforeUnmount(() => { dialog.value?.close(); previousFocus?.focus() })
</script>

<template>
  <dialog ref="dialog" class="parameter-modal" @cancel.prevent="emit('close')" @keydown="trap" aria-labelledby="node-modal-title">
    <header class="modal-header"><div class="modal-icon">{{ descriptor.symbol }}</div><div><div class="eyebrow">{{ descriptor.category }} / {{ node.channels }} CHANNELS</div><h2 id="node-modal-title">{{ node.label }}</h2></div><button class="icon-button modal-close" aria-label="Close parameters" @click="emit('close')"><X :size="20" /></button></header>
    <p class="modal-description">{{ descriptor.description }}</p>
    <div class="modal-status"><span class="status-dot" :class="{ live: active && !stale }"></span>{{ active ? stale ? 'Engine values stale' : 'Live engine · 20 updates / second' : 'Project inactive · stored values' }}<span v-if="saving" class="saving">Saving…</span></div>
    <div class="parameter-list">
      <section v-if="node.kind.endsWith('_visualizer')" class="parameter-row"><DataVisualizer :kind="node.kind" :data="visualization" :sample-rate="sampleRate||48000" :block-size="blockSize||128" :stale="stale" /></section>
      <section v-if="node.kind==='control_visualizer'" class="parameter-row"><template v-if="links.in"><p class="connected-label">CONNECTED · input is read-only</p><button class="text-button" @click="emit('source',links.in.source)">{{sourceName(links.in)}} / {{links.in.source_port}}</button></template><template v-else><label>Disconnected input type<select aria-label="Disconnected input type" :value="typeof node.control_value==='string'?'text':'number'" :disabled="!editable||saving" @change="emit('control',($event.target as HTMLSelectElement).value==='text'?'':0)"><option value="number">Number</option><option value="text">String</option></select></label><label>Disconnected input value<input aria-label="Disconnected input value" :type="typeof node.control_value==='string'?'text':'number'" :value="node.control_value??0" :disabled="!editable||saving" @change="emit('control',typeof node.control_value==='string'?($event.target as HTMLInputElement).value:Number(($event.target as HTMLInputElement).value))"></label><p class="feature-note">Used only with no input connection. Strings support up to 256 UTF-8 bytes.</p></template></section>
      <section v-if="descriptor.category !== 'Math' && descriptor.category !== 'Control' && descriptor.category !== 'Timing'" class="parameter-row"><div class="parameter-heading"><label for="node-channels">Audio channels</label><span class="small-tag">STRUCTURAL</span></div><select id="node-channels" :value="node.channels" :disabled="!editable || saving" @change="emit('channels', +($event.target as HTMLSelectElement).value)"><option v-for="width in 8" :key="width" :value="width">{{ width }} channel{{ width === 1 ? '' : 's' }}</option></select></section>
      <section v-if="['sample', 'phase_vocoder'].includes(node.kind)" class="parameter-row"><label class="button">Upload WAV sample<input type="file" accept=".wav,audio/wav" style="display:none" :disabled="!editable || saving" @change="($event.target as HTMLInputElement).files?.[0] && emit('upload', ($event.target as HTMLInputElement).files![0])"></label><p class="feature-note">Up to 30 seconds. Match the node’s channel width to the WAV file.</p></section>
      <section v-if="node.kind === 'spectral_curve'" class="parameter-row">
        <SpectralCurveEditor label="Magnitude curve" :values="curveValues('magnitude_curve', 1)" :min="0" :max="2" :neutral="1" :disabled="!editable || saving" :sample-rate="sampleRate || 48000" @change="curveChange('magnitude_curve', $event)" />
        <SpectralCurveEditor label="Phase curve" :values="curveValues('phase_curve', 0)" :min="-Math.PI" :max="Math.PI" :neutral="0" :disabled="!editable || saving" :sample-rate="sampleRate || 48000" @change="curveChange('phase_curve', $event)" />
      </section>
      <section v-for="p in descriptor.parameters.filter(p => !p.id.includes('_curve_'))" :key="p.id" class="parameter-row" :class="{ driven: links[p.id] }">
        <div class="parameter-heading"><label :for="`param-${p.id}`">{{ p.label }}</label><span v-if="p.structural" class="small-tag">STRUCTURAL</span><span v-else-if="links[p.id]" class="connected-label"><Link2 :size="12" /> CONNECTED</span></div>
        <template v-if="['size','overlap'].includes(p.id)"><select :aria-label="p.label" :value="node.parameters[p.id]??p.default" :disabled="!editable||saving" @change="emit('change',p.id,+($event.target as HTMLSelectElement).value)"><option v-for="n in p.id==='size'?[256,512,1024,2048,4096,8192]:[2,4]" :key="n" :value="n">{{n}}</option></select></template>
        <template v-else-if="p.id==='interface' && !links[p.id]"><select aria-label="Audio interface" :value="node.parameters.interface || 0" :disabled="!editable||saving" @change="emit('change','interface',+($event.target as HTMLSelectElement).value)"><option :value="0">All enabled interfaces</option><option v-for="i in interfaces" :key="i.id" :value="i.id">{{i.name}}</option><option v-if="node.parameters.interface && !interfaces?.some(i=>i.id===node.parameters.interface)" :value="node.parameters.interface" disabled>Unavailable interface — select another</option></select></template>
        <template v-else-if="p.id==='waveform' && !links[p.id]"><select aria-label="Waveform" :value="node.parameters.waveform || 0" :disabled="!editable||saving" @change="emit('change','waveform',+($event.target as HTMLSelectElement).value)"><option v-for="(name,index) in ['Sine','Triangle','Sawtooth','Square','Noise']" :key="index" :value="index">{{name}}</option></select></template>
        <template v-else-if="links[p.id]">
          <div class="driven-value"><output :class="{ dim: stale }">{{ active ? formatValue(values?.[p.id], p.unit) : '—' }}</output><svg viewBox="0 0 240 34" role="img" :aria-label="`${p.label} recent history`"><polyline :points="trace(p)" fill="none" stroke="currentColor" stroke-width="1.5" /></svg></div>
          <div class="driver-source"><button @click="emit('source', links[p.id].source)"><ArrowUpRight :size="14" />{{ sourceName(links[p.id]) }} <span>/ {{ links[p.id].source_port }}</span></button><button :disabled="!editable || saving" title="Disconnect this parameter" @click="emit('disconnect', links[p.id])"><Unplug :size="14" /> Disconnect</button></div>
        </template>
        <template v-else>
          <div class="parameter-controls"><input type="range" :min="p.min" :max="p.max" :step="p.structural ? 1 : 'any'" :value="value(p)" :disabled="!editable || (saving && p.structural)" :aria-label="p.label" @input="edit(p, ($event.target as HTMLInputElement).value, false)"><div class="number-field"><input :id="`param-${p.id}`" type="number" @input="textDraft[p.id] = ($event.target as HTMLInputElement).value" :min="p.min" :max="p.max" :step="p.structural ? 1 : 'any'" :value="textDraft[p.id] ?? value(p)" :disabled="!editable || (saving && p.structural)" @change="edit(p, ($event.target as HTMLInputElement).value, !p.structural)"><span>{{ p.unit }}</span></div><button v-if="p.structural" class="button small" :disabled="!editable || saving" @click="emit('change', p.id, value(p))">Apply</button></div>
          <div class="parameter-range"><span>{{ p.min }} {{ p.unit }}</span><span>{{ p.max }} {{ p.unit }}</span></div>
          <p v-if="errors[p.id]" class="field-error">{{ errors[p.id] }}</p>
        </template>
      </section>
      <div v-if="!descriptor.parameters.length && !node.kind.endsWith('_visualizer')" class="empty-parameters"><Activity :size="24" /><p>This node has no editable parameters.</p><p>Output: {{ formatValue(values?._out) }}</p></div>
    </div>
    <footer class="modal-footer"><button class="text-button" :disabled="!editable || saving" @click="emit('undo')"><RotateCcw :size="14" /> Undo last edit</button><span>Live edits are kept when you close.</span><button class="text-button danger" :disabled="!editable || saving" @click="emit('remove')">Delete node</button></footer>
  </dialog>
</template>
