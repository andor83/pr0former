<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { X, Link2, ArrowUpRight, Unplug, RotateCcw, Activity } from '@lucide/vue'
import type { Descriptor, GraphNode, GraphEdge, Parameter, Part, IoConfig } from '../types'
import { finiteInput, formatValue } from '../api'
import { roundSlider, formatSlider } from '../sliderNumbers'
import NodeIoSettings from './NodeIoSettings.vue'
import type {SampleEntry} from '../samples'
import BrowserInputPicker from './BrowserInputPicker.vue'
import NativeInputPicker from './NativeInputPicker.vue'
import DeviceChannelRouting from './DeviceChannelRouting.vue'
import DataVisualizer from './DataVisualizer.vue'
import SpectralCurveEditor from './SpectralCurveEditor.vue'
import LooperStatus from './LooperStatus.vue'
import ToggleButton from './ToggleButton.vue'
import type {Visualization} from '../types'
const props = defineProps<{ routeTargets?:Record<string,string>;routeTarget?:string;samples?:SampleEntry[]; inputError?:string|null; ioStatus?:{error:string|null;dropped:number}; parts?: Part[]; projectId?:string; visualization?:Visualization; sampleRate?:number;blockSize?:number; interfaces?: {id:number;name:string}[]; node: GraphNode; descriptor: Descriptor; edges: GraphEdge[]; nodes: GraphNode[]; values?: Record<string, number>; stale: boolean; editable: boolean; active: boolean; saving: boolean }>()
const emit = defineEmits<{ sample:[sample:SampleEntry]; rename: [label:string]; part: [id:string]; io: [value:IoConfig]; expand: []; close: []; change: [key: string, value: number]; disconnect: [edge: GraphEdge]; source: [id: string]; undo: []; remove: []; upload: [file: File]; control: [value:number|string]; curve: [parameters: Record<string, number>]; channels: [width: number] }>()
const nameDraft = ref(props.node.label)
watch(() => [props.node.id, props.node.label], () => { nameDraft.value = props.node.label })
const dialog = ref<HTMLDialogElement>()
const history = ref<Record<string, number[]>>({})
const draft = ref<Record<string, number>>({})
const textDraft = ref<Record<string, string>>({})
const errors = ref<Record<string, string>>({})
const sampleBacked = computed(() => ['sample', 'poly_sampler', 'granular_synth', 'convolution_reverb'].includes(props.node.kind))
const sampleValue = computed(() => {
  const asset = props.node.parameters.asset ?? 0
  return props.samples?.find(s => s.asset === asset)?.name || (asset ? String(asset) : '')
})
function chooseSample(value: string) {
  const match = props.samples?.find(s => s.name === value || String(s.asset) === value)
  if (match) emit('sample', match)
  else if (/^\d+$/.test(value)) emit('change', 'asset', Number(value))
}
let previousFocus: HTMLElement | null = null
const groupedLinks=computed(()=>{
  const groups:Record<string,GraphEdge[]>={}
  for(const edge of props.edges.filter(e=>e.target===props.node.id))(groups[edge.target_port]??=[]).push(edge)
  for(const edges of Object.values(groups))edges.sort((a,b)=>(props.nodes.find(n=>n.id===a.source)?.y??0)-(props.nodes.find(n=>n.id===b.source)?.y??0)||(props.nodes.find(n=>n.id===a.source)?.x??0)-(props.nodes.find(n=>n.id===b.source)?.x??0)||a.source.localeCompare(b.source))
  return groups
})
const links=computed(()=>Object.fromEntries(Object.entries(groupedLinks.value).map(([port,edges])=>{
  const winner=props.active&&!props.stale?props.edges[props.values?.[`_driver_${port}`]??-1]:undefined
  return [port,edges.find(e=>e.id===winner?.id)??edges[0]]
})))
const targetSuggestions=computed(()=>{
  const signal=props.node.kind.replace(/^(send|receive)_/,'')
  const connected=new Set(props.edges.filter(e=>e.target_port==='target').map(e=>e.target))
  const names=props.nodes.filter(n=>n.id!==props.node.id&&/^(send|receive)_(control|audio|spectral)$/.test(n.kind)&&n.kind.endsWith(`_${signal}`)).map(n=>connected.has(n.id)?(props.active&&!props.stale?props.routeTargets?.[n.id]:undefined):n.control_value)
  return [...new Set(names.filter((name):name is string=>typeof name==='string'&&name.length>0))].sort((a,b)=>a.localeCompare(b))
})
const controlDraft=ref<number|string>(props.node.control_value??0)
watch(()=>[props.node.id,props.node.control_value],()=>{controlDraft.value=props.node.control_value??0})
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
function integerParameter(p:Parameter){return p.structural || p.id==='root_note'}
function edit(p: Parameter, text: string, commit: boolean) {
  let v = finiteInput(text, p.min, p.max)
  if (p.id==='root_note' && v!==null && !Number.isInteger(v)) { errors.value[p.id]='Enter a whole MIDI note from 0 to 127'; return }
  if (v === null) { errors.value[p.id] = `Enter ${p.min} to ${p.max}`; return }
  v=Math.max(p.min,Math.min(p.max,roundSlider(v,integerParameter(p))))
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
  <dialog ref="dialog" class="parameter-modal node-parameter-modal" @cancel.prevent="emit('close')" @keydown="trap" aria-labelledby="node-modal-title">
    <header class="modal-header"><div class="modal-icon">{{ descriptor.symbol }}</div><div><div class="eyebrow">{{ descriptor.category }} / {{ ['record','pitch_tracker'].includes(node.kind)?'AUTO':node.channels }} CHANNELS</div><h2 id="node-modal-title">{{ node.label }}</h2></div><button class="icon-button modal-close" aria-label="Close parameters" @click="emit('close')"><X :size="20" /></button></header>
    <p v-if="node.library" class="feature-note">Library version {{node.library.version}} · embedded copy; local edits do not change the library.</p><p class="modal-description">{{ descriptor.description }}</p>
    <div class="modal-status"><span class="status-dot" :class="{ live: active && !stale }"></span>{{ active ? stale ? 'Engine values stale' : 'Live engine · 20 updates / second' : 'Project inactive · stored values' }}<span v-if="saving" class="saving">Applying…</span></div>
    <div class="parameter-list"><section class="parameter-row"><label for="node-name">Node name</label><input id="node-name" v-model="nameDraft" maxlength="256" :disabled="!editable || saving" @change="emit('rename',($event.target as HTMLInputElement).value)"><button v-if="node.kind==='subgraph'" class="button" @click="emit('expand')">Open subgraph</button></section>
      <section v-if="/^(send|receive)_(control|audio|spectral)$/.test(node.kind)" class="parameter-row"><label>Target name<input aria-label="Target name" placeholder="e.g. main" maxlength="256" list="route-target-suggestions" autocomplete="off" :value="links.target?(active&&!stale?routeTarget??'':''):typeof controlDraft==='string'?controlDraft:''" :disabled="!editable||saving||!!links.target" @input="controlDraft=($event.target as HTMLInputElement).value" @change="!links.target&&emit('control',controlDraft)"></label><datalist id="route-target-suggestions"><option v-for="target in targetSuggestions" :key="target" :value="target"/></datalist><template v-if="links.target"><p class="feature-note">Target is controlled by a connection. Disconnect it to enter a target manually.</p><div v-for="edge in groupedLinks.target" :key="edge.id" class="driver-source"><button @click="emit('source',edge.source)"><ArrowUpRight :size="14"/>{{sourceName(edge)}} / {{edge.source_port}}<small v-if="groupedLinks.target.length>1&&links.target.id===edge.id"> · current</small></button><button :disabled="!editable||saving" :aria-label="`Disconnect ${sourceName(edge)} from Target name`" @click="emit('disconnect',edge)"><Unplug :size="14"/> Disconnect</button></div></template><p v-else class="feature-note">Type a new target or choose an existing {{node.kind.split('_')[1]}} target in this project. Names match exactly, including case.</p><p v-if="active&&!stale&&values?._route_error" class="field-error" role="alert">Route target or audio/spectral format is incompatible.</p><p v-if="active&&!stale&&node.kind.startsWith('receive_')" class="feature-note">{{values?._route_connected||0}} matching sender(s)</p><p class="feature-note">Matching names connect within this project, with one sample of routing delay. An empty name disconnects.</p></section>
      <section v-if="node.kind==='pitch_tracker'" class="parameter-row"><p class="feature-note">All incoming audio channels are averaged to mono. Pitches rank strongest first; empty slots output −1. Analysis window: {{((node.parameters.fft_size??8192)/(sampleRate||48000)*1000).toFixed(1)}} ms, updated every {{((node.parameters.fft_size??8192)/(sampleRate||48000)*250).toFixed(1)}} ms.</p></section>
      <section v-if="node.kind==='toggle'" class="parameter-row"><ToggleButton :label="node.label" :checked="active&&!stale?values?._checked===1:node.control_value===1" :disabled="!editable||saving||(active&&stale)" @value="value=>emit('control',value)" /><p v-if="links.in" class="feature-note">Input: {{sourceName(links.in)}} / {{links.in.source_port}}. Manual changes hold until this input changes.</p></section>
      <section v-if="node.kind==='record'" class="parameter-row" aria-label="Recording status">
        <h3>{{!active?'Engine off':stale?'Stale':values?._record_overflow?'Recording failed: buffer overflow':values?._recording?'Recording':'Stopped'}}</h3>
        <p v-if="active&&!stale">{{values?._record_channels||0}} input channels · {{(sampleRate||48000)/1000}} kHz · 32-bit float · {{(values?._record_seconds||0).toFixed(2)}} s</p>
        <p class="feature-note">The node name above is the recording filename prefix. Each take gets a timestamp and is saved in this server’s recordings folder, owned by this project. Channels follow the audio connection automatically.</p>
        <p class="feature-note">Send a positive pulse to Start or Stop; return to zero to rearm. Stop or disable the engine to finalize the WAV. Pausing the show leaves recording running. Large takes split into 1 GiB segments. Download browsing will be added later.</p>
      </section>
      <LooperStatus :project-id="projectId" :node-id="node.id" :editable="editable" v-if="node.kind==='looper'" :values="values" :active="active" :stale="stale" />
      <section v-if="node.kind==='part_midi'" class="parameter-row"><label for="source-part">Source part</label><select id="source-part" :value="node.part_id || ''" :disabled="!editable || saving" @change="emit('part',($event.target as HTMLSelectElement).value)"><option value="">Unassigned — silent</option><option v-for="part in parts || []" :key="part.id" :value="part.id">{{ part.name }}</option></select><p v-if="!parts?.length" class="feature-note">Add a score part to assign this node.</p><div class="part-midi-values"><label v-for="name in ['pitch','velocity','gate','trigger','note_off']" :key="name">{{ name.replace('_',' ') }}<output :aria-label="`Part MIDI ${name}`">{{ active && !stale ? formatValue(values?.[name]) : '—' }}</output></label></div><p v-if="active && !stale && values?._dropped" class="field-error">MIDI event queue overloaded: {{ values._dropped }} events dropped; held notes released.</p><p class="feature-note">Trigger marks note-on; note_off marks note-off. Pitch identifies each event. Chords are emitted one event per two engine samples. Use Counter nodes to observe pulses.</p></section>
      <NodeIoSettings v-if="['midi_input','midi_output','midi_to_osc','osc_to_midi'].includes(node.kind)" :node="node" :disabled="!editable || saving" @change="value=>emit('io',value)" />
      <section v-if="['midi_input','osc_to_midi'].includes(node.kind)" class="parameter-row"><div class="part-midi-values"><label v-for="name in ['pitch','velocity','gate','trigger','note_off']" :key="name">{{ name.replace('_',' ') }}<output :aria-label="`MIDI ${name}`">{{ active && !stale ? formatValue(values?.[name]) : '—' }}</output></label></div><p v-if="node.kind==='midi_input' && active && inputError" class="field-error">{{inputError}}</p><p v-if="active && !stale && values?._dropped" class="field-error">MIDI event queue overloaded: {{values._dropped}} events dropped.</p></section>
      <p v-if="active && ['midi_output','midi_to_osc'].includes(node.kind) && ioStatus?.error" class="field-error">{{ioStatus.error}}</p>
      <section v-if="node.kind==='browser_input'" class="parameter-row"><BrowserInputPicker :input-key="`${projectId}:${node.id}`" /></section>
      <section v-if="node.kind==='clock'" class="parameter-row"><label>Project tempo input</label><template v-if="links.tempo"><p class="connected-label">CONNECTED · {{active ? formatValue(values?.tempo, 'BPM') : 'Project inactive'}}</p><button class="text-button" @click="emit('source',links.tempo.source)">{{sourceName(links.tempo)}} / {{links.tempo.source_port}}</button><button class="text-button" :disabled="!editable || saving" @click="emit('disconnect',links.tempo)">Disconnect tempo</button></template><p v-else class="feature-note">Connect a control signal to tempo to change global BPM programmatically. With no connection, use the project transport tempo.</p></section>
      <section v-if="node.kind.endsWith('_visualizer')" class="parameter-row"><DataVisualizer :kind="node.kind" :data="visualization" :sample-rate="sampleRate||48000" :block-size="blockSize||128" :stale="stale" /></section>
      <section v-if="node.kind==='control_visualizer'" class="parameter-row"><template v-if="links.in"><p class="connected-label">CONNECTED · input is read-only</p><button class="text-button" @click="emit('source',links.in.source)">{{sourceName(links.in)}} / {{links.in.source_port}}</button></template><template v-else><label>Disconnected input type<select aria-label="Disconnected input type" :value="typeof node.control_value==='string'?'text':'number'" :disabled="!editable||saving" @change="emit('control',($event.target as HTMLSelectElement).value==='text'?'':0)"><option value="number">Number</option><option value="text">String</option></select></label><label>Disconnected input value<input aria-label="Disconnected input value" :type="typeof node.control_value==='string'?'text':'number'" v-model="controlDraft" :disabled="!editable||saving" @change="emit('control',controlDraft)"></label><p class="feature-note">Used only with no input connection. Strings support up to 256 UTF-8 bytes.</p></template></section>
      <section v-if="node.kind !== 'record' && node.kind !== 'pitch_tracker' && node.kind !== 'subgraph' && !node.kind.endsWith('_control') && descriptor.category !== 'Math' && descriptor.category !== 'Control' && descriptor.category !== 'Timing'" class="parameter-row"><div class="parameter-heading"><label for="node-channels">Audio channels</label><span class="small-tag">STRUCTURAL</span></div><select id="node-channels" :value="node.channels" :disabled="!editable || saving" @change="emit('channels', +($event.target as HTMLSelectElement).value)"><option v-for="width in 8" :key="width" :value="width">{{ width }} channel{{ width === 1 ? '' : 's' }}</option></select></section>
      <DeviceChannelRouting v-if="['input','output'].includes(node.kind)" :key="node.id" :node="node" :sample-rate="sampleRate || 48000" :disabled="!editable || saving" @change="emit('curve', $event)" />
      <section v-if="sampleBacked" class="parameter-row"><label>Project sample<input aria-label="Project sample" list="project-sample-options" :value="sampleValue" :disabled="!editable||saving" placeholder="Search samples or enter numeric ID" @change="chooseSample(($event.target as HTMLInputElement).value)"><datalist id="project-sample-options"><option v-for="sample in samples||[]" :key="sample.id" :value="sample.name">{{sample.asset}} · {{sample.channels}} ch</option></datalist></label><label class="button">Import audio sample<input type="file" style="display:none" :disabled="!editable || saving" @change="($event.target as HTMLInputElement).files?.[0] && emit('upload', ($event.target as HTMLInputElement).files![0])"></label><p class="feature-note">Search by sample name or enter a numeric sample ID. Imported audio is converted to WAV at the current engine rate.</p></section>
      <section v-if="node.kind === 'spectral_curve'" class="parameter-row">
        <SpectralCurveEditor label="Magnitude curve" :values="curveValues('magnitude_curve', 1)" :min="0" :max="2" :neutral="1" :disabled="!editable || saving" :sample-rate="sampleRate || 48000" @change="curveChange('magnitude_curve', $event)" />
        <SpectralCurveEditor label="Phase curve" :values="curveValues('phase_curve', 0)" :min="-Math.PI" :max="Math.PI" :neutral="0" :disabled="!editable || saving" :sample-rate="sampleRate || 48000" @change="curveChange('phase_curve', $event)" />
      </section>
      <section v-for="port in descriptor.inputs.filter(p=>(groupedLinks[p.id]?.length??0)>1)" :key="`sources-${port.id}`" class="parameter-row"><h3>{{port.label}} sources</h3><p class="feature-note">Changes arrive in sample order. The uppermost source wins simultaneous changes.</p><div v-for="edge in groupedLinks[port.id]" :key="edge.id" class="driver-source"><button @click="emit('source',edge.source)">{{sourceName(edge)}} / {{edge.source_port}}<small v-if="active&&!stale&&links[port.id].id===edge.id"> · current</small></button><button :disabled="!editable||saving" :aria-label="`Disconnect ${sourceName(edge)} from ${port.label}`" @click="emit('disconnect',edge)">Disconnect</button></div></section>
      <section v-for="p in descriptor.parameters.filter(p => !(sampleBacked && p.id === 'asset') && !p.id.includes('_curve_') && !p.id.startsWith('route_') && !(node.kind==='control_input' && ['min','max'].includes(p.id) && [0,4].includes(node.parameters.mode??2)))" :key="p.id" class="parameter-row" :class="{ driven: links[p.id] }">
        <div class="parameter-heading"><label :for="`param-${p.id}`">{{ p.label }}</label><span v-if="p.structural" class="small-tag">STRUCTURAL</span><span v-else-if="links[p.id]" class="connected-label"><Link2 :size="12" /> CONNECTED</span></div>
        <template v-if="['convolution','convolution_reverb'].includes(node.kind) && p.id==='normalize' && !links[p.id]"><select aria-label="Normalize response" :value="(node.parameters.normalize??1)>0?1:0" :disabled="!editable||saving" @change="emit('change','normalize',Number(($event.target as HTMLSelectElement).value))"><option :value="1">On</option><option :value="0">Off</option></select></template><template v-else-if="['convolution','convolution_reverb'].includes(node.kind) && p.id==='window'"><select aria-label="Window size" :value="node.parameters.window??256" :disabled="!editable||saving" @change="emit('change','window',Number(($event.target as HTMLSelectElement).value))"><option v-for="n in [128,256,512,1024,2048]" :key="n" :value="n">{{n}} samples</option></select></template><template v-else-if="node.kind==='pitch_tracker' && ['slots','fft_size'].includes(p.id)"><select :aria-label="p.label" :value="node.parameters[p.id]??p.default" :disabled="!editable||saving" @change="emit('change',p.id,Number(($event.target as HTMLSelectElement).value))"><option v-for="n in p.id==='slots'?[1,2,3,4]:[2048,4096,8192]" :key="n" :value="n">{{n}}</option></select><p v-if="p.id==='slots'" class="feature-note">Reducing slots disconnects removed outputs. Empty slots output −1.</p></template>
        <template v-else-if="node.kind==='piano' && p.id==='octaves'"><select aria-label="Octave span" :value="node.parameters.octaves??1" :disabled="!editable||saving" @change="emit('change','octaves',Number(($event.target as HTMLSelectElement).value))"><option v-for="count in 8" :key="count" :value="count">{{count}} {{count===1?'octave':'octaves'}}</option></select></template><template v-else-if="node.kind==='piano' && p.id==='octave'"><select aria-label="Octave" :value="node.parameters.octave??4" :disabled="!editable||saving" @change="emit('change','octave',Number(($event.target as HTMLSelectElement).value))"><option v-for="octave in Array.from({length:11},(_,i)=>i-1)" :key="octave" :value="octave">{{octave}} · C{{octave}}–{{octave===9?'G':'B'}}{{octave}}</option></select></template>
        <template v-else-if="node.kind==='control_input' && p.id==='mode'"><select aria-label="Control type" :value="node.parameters.mode??2" :disabled="!editable||saving" @change="emit('change','mode',Number(($event.target as HTMLSelectElement).value))"><option v-for="(label,index) in ['Bang','Integer','Float','Slider','Text']" :key="label" :value="index">{{label}}</option></select></template>
        <template v-else-if="['size','overlap'].includes(p.id)"><select :aria-label="p.label" :value="node.parameters[p.id]??p.default" :disabled="!editable||saving" @change="emit('change',p.id,+($event.target as HTMLSelectElement).value)"><option v-for="n in p.id==='size'?[256,512,1024,2048,4096,8192]:[2,4]" :key="n" :value="n">{{n}}</option></select></template>
        <template v-else-if="p.id==='interface' && node.kind==='input'"><NativeInputPicker :value="node.parameters.interface || 0" :disabled="!editable || saving" @change="emit('change','interface',$event)" /></template>
        <template v-else-if="p.id==='interface' && !links[p.id]"><select aria-label="Audio interface" :value="node.parameters.interface || 0" :disabled="!editable||saving" @change="emit('change','interface',+($event.target as HTMLSelectElement).value)"><option :value="0">All enabled interfaces</option><option v-for="i in interfaces" :key="i.id" :value="i.id">{{i.name}}</option><option v-if="node.parameters.interface && !interfaces?.some(i=>i.id===node.parameters.interface)" :value="node.parameters.interface" disabled>Unavailable interface — select another</option></select></template>
        <template v-else-if="['waveform','carrier_waveform','modulator_waveform'].includes(p.id) && !links[p.id]"><select :aria-label="p.label" :value="node.parameters[p.id] || 0" :disabled="!editable||saving" @change="emit('change',p.id,+($event.target as HTMLSelectElement).value)"><option v-for="(name,index) in ['Sine','Triangle','Sawtooth','Square','Noise']" :key="index" :value="index">{{name}}</option></select></template>
        <template v-else-if="links[p.id]">
          <div class="driven-value"><output :class="{ dim: stale }">{{ active ? values?.[p.id]===undefined?'—':`${formatSlider(values[p.id]!,integerParameter(p))}${p.unit?' '+p.unit:''}` : '—' }}</output><svg viewBox="0 0 240 34" role="img" :aria-label="`${p.label} recent history`"><polyline :points="trace(p)" fill="none" stroke="currentColor" stroke-width="1.5" /></svg></div>
          <div v-for="edge in groupedLinks[p.id]" :key="edge.id" class="driver-source"><button @click="emit('source', edge.source)"><ArrowUpRight :size="14" />{{ sourceName(edge) }} <span>/ {{ edge.source_port }}</span><small v-if="groupedLinks[p.id].length>1&&active&&!stale&&links[p.id].id===edge.id"> · current</small></button><button :disabled="!editable || saving" :aria-label="`Disconnect ${sourceName(edge)} from ${p.label}`" title="Disconnect this parameter" @click="emit('disconnect', edge)"><Unplug :size="14" /> Disconnect</button></div>
        </template>
        <template v-else>
          <div class="parameter-controls"><input type="range" :min="p.min" :max="p.max" :step="integerParameter(p) ? 1 : 0.01" :value="value(p)" :disabled="!editable || (saving && p.structural)" :aria-label="p.label" @input="edit(p, ($event.target as HTMLInputElement).value, false)"><div class="number-field"><input :id="`param-${p.id}`" type="number" @input="textDraft[p.id] = ($event.target as HTMLInputElement).value" :min="p.min" :max="p.max" :step="integerParameter(p) ? 1 : 0.01" :value="textDraft[p.id] ?? formatSlider(value(p),integerParameter(p))" :disabled="!editable || (saving && p.structural)" @change="edit(p, ($event.target as HTMLInputElement).value, !p.structural)"><span>{{ p.unit }}</span></div><button v-if="p.structural" class="button small" :disabled="!editable || saving" @click="emit('change', p.id, value(p))">Apply</button></div>
          <div class="parameter-range"><span>{{ p.min }} {{ p.unit }}</span><span>{{ p.max }} {{ p.unit }}</span></div>
          <p v-if="errors[p.id]" class="field-error">{{ errors[p.id] }}</p>
        </template>
      </section>
      <div v-if="!descriptor.parameters.length && !['toggle','record','browser_input','clock','subgraph','part_midi','midi_input','midi_output','midi_to_osc','osc_to_midi'].includes(node.kind) && !node.kind.startsWith('subgraph_') && !/^(send|receive)_/.test(node.kind) && !node.kind.endsWith('_visualizer')" class="empty-parameters"><Activity :size="24" /><p>This node has no editable parameters.</p><p>Output: {{ formatValue(values?._out) }}</p></div>
    </div>
    <footer class="modal-footer"><button class="text-button" :disabled="!editable || saving" @click="emit('undo')"><RotateCcw :size="14" /> Undo last edit</button><span>Live edits apply immediately. Revisions autosave each minute.</span><button class="text-button danger" :disabled="!editable || saving" @click="emit('remove')">Delete node</button></footer>
  </dialog>
</template>
