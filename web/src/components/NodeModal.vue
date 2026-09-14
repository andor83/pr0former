<script setup lang="ts">
import { defineAsyncComponent, computed, inject, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { X, Link2, ArrowUpRight, Unplug, RotateCcw, Activity } from '@lucide/vue'
import type { Descriptor, GraphNode, GraphEdge, Parameter, Part, IoConfig } from '../types'
import { finiteInput, formatValue } from '../api'
import { roundSlider, formatSlider } from '../sliderNumbers'
import OscMessageDebug from './OscMessageDebug.vue'
import MidiInputDebug from './MidiInputDebug.vue'
import NodeIoSettings from './NodeIoSettings.vue'
import type {SampleEntry} from '../samples'
import LocalMidiInputPicker from './LocalMidiInputPicker.vue'
import LocalAudioSettings from './LocalAudioSettings.vue'
import NativeInputPicker from './NativeInputPicker.vue'
import DeviceChannelRouting from './DeviceChannelRouting.vue'
import DataVisualizer from './DataVisualizer.vue'
import SpectralCurveEditor from './SpectralCurveEditor.vue'
import LooperStatus from './LooperStatus.vue'
import ToggleButton from './ToggleButton.vue'
import EnvelopeGraph from './EnvelopeGraph.vue'
import { readEnvelope } from '../envelopeGeometry'
import PartPlayerReadout from './PartPlayerReadout.vue'
import SampleSelectorOptions from './SampleSelectorOptions.vue'
import type {Visualization} from '../types'
const ScriptEditor=defineAsyncComponent(()=>import('./ScriptEditor.vue'))
const props = defineProps<{ applyScript:(script:import('../types').ScriptConfig,nodeId:string,projectId:string)=>Promise<void>; scriptStatus?:import('../types').ScriptStatus; oscMessage?:[number,number|string]; routeTargets?:Record<string,string>;routeTarget?:string;samples?:SampleEntry[]; inputError?:string|null; ioStatus?:{error:string|null;dropped:number}; parts?: Part[]; projectId?:string; visualization?:Visualization; sampleRate?:number;blockSize?:number; interfaces?: {id:number;name:string}[]; node: GraphNode; descriptor: Descriptor; edges: GraphEdge[]; nodes: GraphNode[]; values?: Record<string, number>; stale: boolean; editable: boolean; active: boolean; saving: boolean }>()
const emit = defineEmits<{ sampleChoices:[choices:import('../types').SampleChoice[]]; sample:[sample:SampleEntry]; rename: [label:string]; part: [id:string]; io: [value:IoConfig]; expand: []; close: []; change: [key: string, value: number]; disconnect: [edge: GraphEdge]; source: [id: string]; undo: []; remove: []; upload: [file: File]; control: [value:number|string]; curve: [parameters: Record<string, number>]; channels: [width: number] }>()
const localAudioAccess=inject<import('vue').ComputedRef<import('../browserInputs').LocalAudioAccess>>('localAudioAccess')
const localMuteReadOnly=computed(()=>props.node.kind==='browser_input'&&localAudioAccess?.value.assignments[props.node.id]!==localAudioAccess?.value.userId)
const nameDraft = ref(props.node.label)
watch(() => [props.node.id, props.node.label], () => { nameDraft.value = props.node.label })
const dialog = ref<HTMLDialogElement>()
const history = ref<Record<string, number[]>>({})
const draft = ref<Record<string, number>>({})
const textDraft = ref<Record<string, string>>({})
const errors = ref<Record<string, string>>({})
const sampleBacked = computed(() => ['sample', 'poly_sampler', 'granular_synth', 'granular_cloud', 'convolution_reverb'].includes(props.node.kind))
const sampleValue = computed(() => {
  const asset = links.value.sample_id ? (props.active && !props.stale ? props.values?._sample_id ?? 0 : 0) : props.node.parameters.asset ?? 0
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
const adsrEnvelope=computed(()=>readEnvelope(props.node.parameters,props.descriptor.parameters,
  props.active&&!props.stale?props.values:undefined,Object.keys(links.value)))
const targetSuggestions=computed(()=>{
  const signal=props.node.kind.replace(/^(send|receive)_/,'')
  const connected=new Set(props.edges.filter(e=>e.target_port==='target').map(e=>e.target))
  const names=props.nodes.filter(n=>n.id!==props.node.id&&/^(send|receive)_(control|audio|spectral)$/.test(n.kind)&&n.kind.endsWith(`_${signal}`)).map(n=>connected.has(n.id)?(props.active&&!props.stale?props.routeTargets?.[n.id]:undefined):n.control_value)
  return [...new Set(names.filter((name):name is string=>typeof name==='string'&&name.length>0))].sort((a,b)=>a.localeCompare(b))
})
const controlDraft=ref<number|string>(props.node.control_value??0)
const controlFocused=ref(false)
watch(()=>[props.node.id,props.node.control_value,props.visualization?.value],()=>{if(!controlFocused.value)controlDraft.value=props.node.kind==='control_input'&&props.active&&!props.stale?props.visualization?.value??props.node.control_value??0:props.node.control_value??0},{immediate:true})
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
function integerParameter(p:Parameter){return (p.structural && !(nodeIsSliderRange(p))) || p.id==='root_note'}
function nodeIsSliderRange(p:Parameter){return props.node.kind==='sliders'&&['min','max','step'].includes(p.id)}
function editEnvelope(key:string,value:number) {
  const parameter=props.descriptor.parameters.find(p=>p.id===key)
  if(parameter) edit(parameter,String(value),true)
}
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
// A click on the backdrop closes the modal. Both the press and the release
// must land outside the dialog box, so a drag that starts on a slider or an
// envelope handle and ends over the backdrop does not dismiss it.
let pressedOutside = false
function outside(event: MouseEvent) {
  const r = dialog.value?.getBoundingClientRect()
  return !!r && event.target === dialog.value && (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom)
}
function backdropDown(event: PointerEvent) { pressedOutside = outside(event) }
function backdropClick(event: MouseEvent) { if (pressedOutside && outside(event)) emit('close'); pressedOutside = false }
</script>

<template>
  <dialog ref="dialog" class="parameter-modal node-parameter-modal" :class="{'script-modal':node.kind==='js_control'}" @cancel.prevent="emit('close')" @keydown="trap" @pointerdown="backdropDown" @click="backdropClick" aria-labelledby="node-modal-title">
    <header class="modal-header"><div class="modal-icon">{{ descriptor.symbol }}</div><div><div class="eyebrow">{{ descriptor.category }} / {{ ['record','pitch_tracker'].includes(node.kind)?'AUTO':node.channels }} CHANNELS</div><h2 :aria-label="node.label" id="node-modal-title">{{ node.label }}<HelpNote :label="descriptor.label">{{ descriptor.description }}<template v-if="node.kind==='pitch_tracker'"><br /><br />All incoming audio channels are averaged to mono. Pitches rank strongest first; empty slots output −1. Analysis window: {{((node.parameters.fft_size??8192)/(sampleRate||48000)*1000).toFixed(1)}} ms, updated every {{((node.parameters.fft_size??8192)/(sampleRate||48000)*250).toFixed(1)}} ms.</template><template v-if="node.kind==='meter'"><br /><br />The node draws one dBFS meter per channel, matching the monitor tab. Each channel's Level output carries its amplitude as a control value (0–1 peak, instant rise, falling over the fall time); reducing the channel count removes the upper Level outputs and their cables.</template><template v-if="node.library"><br /><br />Library version {{node.library.version}} · embedded copy; local edits do not change the library.</template></HelpNote></h2><small v-if="node.label.trim().toLowerCase()!==descriptor.label.trim().toLowerCase()" class="node-kind">{{ descriptor.label }}</small></div><button class="icon-button modal-close" aria-label="Close parameters" @click="emit('close')"><X :size="20" /></button></header>

    <div class="modal-status"><span class="status-dot" :class="{ live: active && !stale }"></span>{{ active ? stale ? 'Engine values stale' : 'Live engine · 20 updates / second' : 'Project inactive · stored values' }}<span v-if="saving" class="saving">Applying…</span></div>
    <div class="parameter-list"><section class="parameter-row"><label for="node-name">Node name</label><input id="node-name" v-model="nameDraft" maxlength="256" :disabled="!editable || saving" @change="emit('rename',($event.target as HTMLInputElement).value)"><button v-if="node.kind==='subgraph'" class="button" @click="emit('expand')">Open subgraph</button></section>
      <ScriptEditor v-if="node.kind==='js_control'&&projectId" :key="`${projectId}/${node.id}`" :project-id="projectId" :node="node" :edges="edges" :editable="editable" :saving="saving" :active="active" :stale="stale" :status="scriptStatus" :values="values" :apply="applyScript" />
      <section v-if="/^(send|receive)_(control|audio|spectral)$/.test(node.kind)" class="parameter-row"><label><span class="field-title">Target name<HelpNote label="Target name"><template v-if="links.target">Target is controlled by a connection. Disconnect it to enter a target manually.</template><template v-else>Type or choose a target name. Names match exactly, including case.</template><br /><br />Matching names connect within this project, with one sample of routing delay. An empty name disconnects.</HelpNote></span><input aria-label="Target name" placeholder="e.g. main" maxlength="256" list="route-target-suggestions" autocomplete="off" :value="links.target?(active&&!stale?routeTarget??'':''):typeof controlDraft==='string'?controlDraft:''" :disabled="!editable||saving||!!links.target" @input="controlDraft=($event.target as HTMLInputElement).value" @change="!links.target&&emit('control',controlDraft)"></label><datalist id="route-target-suggestions"><option v-for="target in targetSuggestions" :key="target" :value="target"/></datalist><template v-if="links.target"><div v-for="edge in groupedLinks.target" :key="edge.id" class="driver-source"><button @click="emit('source',edge.source)"><ArrowUpRight :size="14"/>{{sourceName(edge)}} / {{edge.source_port}}<small v-if="groupedLinks.target.length>1&&links.target.id===edge.id"> · current</small></button><button :disabled="!editable||saving" :aria-label="`Disconnect ${sourceName(edge)} from Target name`" @click="emit('disconnect',edge)"><Unplug :size="14"/> Disconnect</button></div></template><p v-if="active&&!stale&&values?._route_error" class="field-error" role="alert">Route target or audio/spectral format is incompatible.</p><p v-if="active&&!stale&&node.kind.startsWith('receive_')" class="feature-note">{{values?._route_connected||0}} matching sender(s)</p></section>


      <section v-if="node.kind==='control_input' && node.parameters.mode!==0" class="parameter-row"><label>Current value<input aria-label="Graphical control value" :type="node.parameters.mode===4?'text':'number'" v-model="controlDraft" :disabled="!editable||saving" @focus="controlFocused=true" @blur="controlFocused=false" @change="emit('control',node.parameters.mode===4?String(controlDraft):Number(controlDraft))"></label><p v-if="links.in" class="feature-note">Input: {{sourceName(links.in)}} / {{links.in.source_port}}. Manual changes hold until this input changes or sends an event.</p></section>
      <section v-if="node.kind==='toggle'" class="parameter-row"><ToggleButton :label="node.label" :checked="active&&!stale?values?._checked===1:node.control_value===1" :disabled="!editable||saving||(active&&stale)" @value="value=>emit('control',value)" /><p v-if="links.in" class="feature-note">Input: {{sourceName(links.in)}} / {{links.in.source_port}}. Manual changes hold until this input changes.</p></section>
      <section v-if="node.kind==='record'" class="parameter-row" aria-label="Recording status">
        <h3 :aria-label="!active?'Engine off':stale?'Stale':values?._record_overflow?'Recording failed: buffer overflow':values?._recording?'Recording':'Stopped'">{{!active?'Engine off':stale?'Stale':values?._record_overflow?'Recording failed: buffer overflow':values?._recording?'Recording':'Stopped'}}<HelpNote>The node name above is the recording filename prefix. Each take gets a timestamp and is saved in this server’s recordings folder, owned by this project. Channels follow the audio connection automatically.<br /><br />Send a positive pulse to Start or Stop; return to zero to rearm. Send Stop or disable the engine to finalize the WAV. Stopping the show leaves recording running. Pausing the show leaves recording running. Large takes split into 1 GiB segments. Download browsing will be added later.</HelpNote></h3>
        <p v-if="active&&!stale">{{values?._record_channels||0}} input channels · {{(sampleRate||48000)/1000}} kHz · 32-bit float · {{(values?._record_seconds||0).toFixed(2)}} s</p>


      </section>
      <LooperStatus :project-id="projectId" :node-id="node.id" :editable="editable" v-if="node.kind==='looper'" :values="values" :active="active" :stale="stale" />
      <section v-if="node.kind==='part_player'" class="parameter-row"><p class="feature-note">Send a rising trigger to Play for one pass, or Play &amp; repeat to loop. Starts and restarts wait for the next metronome beat, with no count-in. Stop releases notes immediately. The engine must be enabled; show playback is optional.</p><PartPlayerReadout :part-name="parts?.find(p=>p.id===node.part_id)?.name" :values="values" :stale="!active||stale" /></section>
      <section v-if="['part_midi','part_player'].includes(node.kind)" class="parameter-row"><label for="source-part"><span class="field-title">Source part<HelpNote label="Source part">Trigger marks note-on; note_off marks note-off. Pitch identifies each event. Chords are emitted one event per two engine samples. Use Counter nodes to observe pulses.</HelpNote></span></label><select aria-label="Source part" id="source-part" :value="node.part_id || ''" :disabled="!editable || saving" @change="emit('part',($event.target as HTMLSelectElement).value)"><option value="">Unassigned — silent</option><option v-for="part in parts || []" :key="part.id" :value="part.id">{{ part.name }}</option></select><p v-if="!parts?.length" class="feature-note">Add a score part to assign this node.</p><div class="part-midi-values"><label v-for="name in ['pitch','velocity','gate','trigger','note_off']" :key="name">{{ name.replace('_',' ') }}<output :aria-label="`Part MIDI ${name}`">{{ active && !stale ? formatValue(values?.[name]) : '—' }}</output></label></div><p v-if="active && !stale && values?._dropped" class="field-error">MIDI event queue overloaded: {{ values._dropped }} events dropped; held notes released.</p></section>
      <section v-if="node.kind==='monitor_output'" class="parameter-row"><label for="monitor-part"><span class="field-title">Performer cue association<HelpNote label="Performer cue association">Associate any part for a performer to send that performer's conducted count-in only to this dedicated monitor feed.</HelpNote></span></label><select aria-label="Performer cue association" id="monitor-part" :value="node.part_id || ''" :disabled="!editable || saving" @change="emit('part',($event.target as HTMLSelectElement).value)"><option value="">Shared feed — no conducted count-in</option><option v-for="part in parts || []" :key="part.id" :value="part.id">{{ part.name }}</option></select></section>
      <NodeIoSettings v-if="['midi_input','midi_output','midi_to_osc','osc_to_midi','osc_input','osc_output'].includes(node.kind)" :node="node" :disabled="!editable || saving" @change="value=>emit('io',value)" />
      <section v-if="['midi_input','osc_to_midi'].includes(node.kind)" class="parameter-row"><div class="part-midi-values"><label v-for="name in ['pitch','velocity','gate','trigger','note_off']" :key="name">{{ name.replace('_',' ') }}<output :aria-label="`MIDI ${name}`">{{ active && !stale ? formatValue(values?.[name]) : '—' }}</output></label></div><p v-if="node.kind==='midi_input' && active && inputError" class="field-error">{{inputError}}</p><p v-if="active && !stale && values?._dropped" class="field-error">MIDI event queue overloaded: {{values._dropped}} events dropped.</p></section>
      <p v-if="active && ['midi_output','midi_to_osc'].includes(node.kind) && ioStatus?.error" class="field-error">{{ioStatus.error}}</p>
      <OscMessageDebug v-if="['osc_input','osc_output'].includes(node.kind)" :key="node.id" :message="oscMessage" :address="node.io?.address" :output="node.kind==='osc_output'" :active="active" :stale="stale" />
      <MidiInputDebug v-if="['midi_input','local_midi_input','midi_to_osc','osc_to_midi'].includes(node.kind)" :key="node.id" :values="values" :active="active" :stale="stale" />
      <LocalMidiInputPicker v-if="node.kind==='local_midi_input'" :node="node.id" :active="active" :editable="editable" :values="values" />
      <section v-if="node.kind==='browser_input'" class="parameter-row"><LocalAudioSettings :project-id="projectId||''" :node="node.id" :active="active" :saving="saving" /></section>
      <section v-if="node.kind==='clock'" class="parameter-row"><label>Project tempo input<HelpNote label="Project tempo input">Connect a control signal to tempo to change global BPM programmatically. With no connection, use the project transport tempo.</HelpNote></label><template v-if="links.tempo"><p class="connected-label">CONNECTED · {{active ? formatValue(values?.tempo, 'BPM') : 'Project inactive'}}</p><button class="text-button" @click="emit('source',links.tempo.source)">{{sourceName(links.tempo)}} / {{links.tempo.source_port}}</button><button class="text-button" :disabled="!editable || saving" @click="emit('disconnect',links.tempo)">Disconnect tempo</button></template></section>
      <section v-if="node.kind.endsWith('_visualizer')" class="parameter-row"><DataVisualizer :kind="node.kind" :data="visualization" :sample-rate="sampleRate||48000" :block-size="blockSize||128" :stale="stale" /></section>
      <section v-if="node.kind==='control_visualizer'" class="parameter-row"><template v-if="links.in"><p class="connected-label">CONNECTED · input is read-only</p><button class="text-button" @click="emit('source',links.in.source)">{{sourceName(links.in)}} / {{links.in.source_port}}</button></template><template v-else><label>Disconnected input type<select aria-label="Disconnected input type" :value="typeof node.control_value==='string'?'text':'number'" :disabled="!editable||saving" @change="emit('control',($event.target as HTMLSelectElement).value==='text'?'':0)"><option value="number">Number</option><option value="text">String</option></select></label><label><span class="field-title">Disconnected input value<HelpNote label="Disconnected input value">Used only with no input connection. Strings support up to 256 UTF-8 bytes.</HelpNote></span><input aria-label="Disconnected input value" :type="typeof node.control_value==='string'?'text':'number'" v-model="controlDraft" :disabled="!editable||saving" @change="emit('control',controlDraft)"></label></template></section>
      <section v-if="['adsr','poly_sampler'].includes(node.kind)" class="parameter-row"><div class="parameter-heading"><label><span class="field-title">Envelope<HelpNote label="Envelope">Drag the attack, decay/sustain and release handles, or focus a handle and use the arrow keys (Shift for larger steps). The time axis is logarithmic per stage, so short attacks and long releases are both visible, and the sustain plateau absorbs whatever width is left; the Zoom slider stretches or compresses the axis while everything stays in view. The release ends at the right edge, so drag its handle left to lengthen it. Drags stop where the plateau would vanish; zoom out to go further. Connected parameters are locked. For the sampler, the dotted line shows the highest active voice envelope; each note has its own attack, decay, sustain and release. Stage times latch on entry; sustain changes are smoothed. One-shot samples still end when the sample runs out; enable Loop while held for longer notes.<br /><br /><template v-if="node.kind==='adsr'">Every input is edge triggered: a rising gate or retrigger pulse attacks, a falling gate or rising note_off pulse releases (even with Gate left at 1). For pulse sources (Piano, Part MIDI, MIDI to control) wire trigger to retrigger and note_off to note_off; for held sources wire gate to Gate. The dotted line is the live output level.</template></HelpNote></span></label><span class="small-tag">DRAG THE HANDLES</span></div><EnvelopeGraph v-if="adsrEnvelope" :envelope="adsrEnvelope" :level="active&&!stale?values?.[node.kind==='poly_sampler'?'_envelope':'_out']:undefined" :gate="active&&!stale&&(values?.[node.kind==='poly_sampler'?'_held':'gate']??0)>0" :editable="editable" :locked="Object.keys(links)" @change="editEnvelope" /><p v-else class="feature-note">Envelope waiting for live values from its connected parameters.</p></section>
      <section v-if="['input','output'].includes(node.kind)" class="parameter-row"><div class="parameter-heading"><label for="node-interface">{{ node.kind==='input' ? 'Input interface' : 'Audio interface' }}</label><span class="small-tag">STRUCTURAL</span></div><NativeInputPicker v-if="node.kind==='input'" :value="node.parameters.interface || 0" :disabled="!editable || saving" @change="emit('change','interface',$event)" /><select v-else id="node-interface" aria-label="Audio interface" :value="node.parameters.interface || 0" :disabled="!editable||saving" @change="emit('change','interface',+($event.target as HTMLSelectElement).value)"><option :value="0">All enabled interfaces</option><option v-for="i in interfaces" :key="i.id" :value="i.id">{{i.name}}</option><option v-if="node.parameters.interface && !interfaces?.some(i=>i.id===node.parameters.interface)" :value="node.parameters.interface" disabled>Unavailable interface — select another</option></select></section>
      <section v-if="node.kind !== 'record' && node.kind !== 'pitch_tracker' && node.kind !== 'subgraph' && !node.kind.endsWith('_control') && descriptor.category !== 'Math' && descriptor.category !== 'Control' && descriptor.category !== 'Timing'" class="parameter-row"><div class="parameter-heading"><label for="node-channels">Audio channels</label><span class="small-tag">STRUCTURAL</span></div><select id="node-channels" :value="node.channels" :disabled="!editable || saving" @change="emit('channels', +($event.target as HTMLSelectElement).value)"><option v-for="width in 8" :key="width" :value="width">{{ width }} channel{{ width === 1 ? '' : 's' }}</option></select></section>
      <DeviceChannelRouting v-if="['input','output'].includes(node.kind)" :key="node.id" :node="node" :sample-rate="sampleRate || 48000" :disabled="!editable || saving" @change="emit('curve', $event)" />
      <SampleSelectorOptions v-if="node.kind==='sample_selector'" :choices="node.sample_choices||[]" :samples="samples||[]" :disabled="!editable||saving" @change="emit('sampleChoices',$event)" />
      <section v-if="sampleBacked" class="parameter-row"><label>Project sample<input aria-label="Project sample" list="project-sample-options" :value="sampleValue" :disabled="!editable||saving||!!links.sample_id" placeholder="Search samples or enter numeric ID" @change="chooseSample(($event.target as HTMLInputElement).value)"><datalist id="project-sample-options"><option v-for="sample in samples||[]" :key="sample.id" :value="sample.name">{{sample.asset}} · {{sample.channels}} ch</option></datalist></label><label class="button"><span class="field-title">Import audio sample<HelpNote label="Import audio sample">Search by sample name or enter a numeric sample ID. Imported audio is converted to WAV at the current engine rate.</HelpNote></span><input aria-label="Import audio sample" type="file" style="display:none" :disabled="!editable || saving || !!links.sample_id" @change="($event.target as HTMLInputElement).files?.[0] && emit('upload', ($event.target as HTMLInputElement).files![0])"></label></section>
      <section v-if="node.kind === 'spectral_curve'" class="parameter-row">
        <SpectralCurveEditor label="Magnitude curve" :values="curveValues('magnitude_curve', 1)" :min="0" :max="2" :neutral="1" :disabled="!editable || saving" :sample-rate="sampleRate || 48000" @change="curveChange('magnitude_curve', $event)" />
        <SpectralCurveEditor label="Phase curve" :values="curveValues('phase_curve', 0)" :min="-Math.PI" :max="Math.PI" :neutral="0" :disabled="!editable || saving" :sample-rate="sampleRate || 48000" @change="curveChange('phase_curve', $event)" />
      </section>
      <section v-if="sampleBacked && links.sample_id" class="parameter-row driven">
        <p class="connected-label">Sample ID is connected · {{active&&!stale ? values?._sample_id || 'No sample' : 'Engine value unavailable'}}</p>
        <div v-for="edge in groupedLinks.sample_id" :key="edge.id" class="driver-source"><button @click="emit('source',edge.source)">{{sourceName(edge)}} / {{edge.source_port}}</button><button :disabled="!editable||saving" @click="emit('disconnect',edge)">Disconnect Sample ID</button></div>
        <p v-if="active&&!stale&&values?._sample_missing" class="field-error">This Sample ID is not prepared for this sampler. Add it to a Sample selector list and match the sample's channel count.</p>
      </section>
      <section v-for="port in descriptor.inputs.filter(p=>(groupedLinks[p.id]?.length??0)>1)" :key="`sources-${port.id}`" class="parameter-row"><h3 :aria-label="`${port.label} sources`">{{port.label}} sources<HelpNote :label="`${port.label} sources`">Changes arrive in sample order. The uppermost source wins simultaneous changes.</HelpNote></h3><div v-for="edge in groupedLinks[port.id]" :key="edge.id" class="driver-source"><button @click="emit('source',edge.source)">{{sourceName(edge)}} / {{edge.source_port}}<small v-if="active&&!stale&&links[port.id].id===edge.id"> · current</small></button><button :disabled="!editable||saving" :aria-label="`Disconnect ${sourceName(edge)} from ${port.label}`" @click="emit('disconnect',edge)">Disconnect</button></div></section>
      <section v-for="p in descriptor.parameters.filter(p => !(['knobs','sliders'].includes(node.kind) && /^(channel|controller)_/.test(p.id) && Number(p.id.split('_').at(-1))>(node.parameters.count??4)) && !(sampleBacked && p.id === 'asset') && !p.id.includes('_curve_') && !p.id.startsWith('route_') && !(['input','output'].includes(node.kind) && p.id === 'interface') && !(node.kind==='control_input' && ['min','max'].includes(p.id) && [0,4].includes(node.parameters.mode??2)))" :key="p.id" class="parameter-row" :class="{ driven: links[p.id] }">
        <div class="parameter-heading"><label :for="`param-${p.id}`">{{ p.label }}</label><HelpNote v-if="node.kind==='pitch_tracker' && p.id==='slots'" label="Pitch slots">Reducing slots disconnects removed outputs. Empty slots output −1.</HelpNote><span v-if="p.structural" class="small-tag">STRUCTURAL</span><span v-else-if="links[p.id]" class="connected-label"><Link2 :size="12" /> CONNECTED</span></div>
        <template v-if="node.kind==='sample_selector' && p.id==='index' && !links.index"><select aria-label="Index" :value="node.parameters.index??0" :disabled="!editable||saving" @change="emit('change','index',Number(($event.target as HTMLSelectElement).value))"><option :value="-1">No sample</option><option v-for="(sample,index) in node.sample_choices||[]" :key="index" :value="index">{{index}} · {{sample.nickname||sample.name}}</option></select></template>
        <template v-else-if="node.kind==='knobs' && p.id.startsWith('channel_')"><select :id="`param-${p.id}`" :aria-label="p.label" :value="node.parameters[p.id]??1" :disabled="!editable||saving" @change="emit('change',p.id,Number(($event.target as HTMLSelectElement).value))"><option :value="0">Unassigned</option><option v-for="channel in 16" :key="channel" :value="channel">Channel {{channel}}</option></select></template>
        <template v-else-if="['convolution','convolution_reverb'].includes(node.kind) && p.id==='normalize' && !links[p.id]"><select aria-label="Normalize response" :value="(node.parameters.normalize??1)>0?1:0" :disabled="!editable||saving" @change="emit('change','normalize',Number(($event.target as HTMLSelectElement).value))"><option :value="1">On</option><option :value="0">Off</option></select></template><template v-else-if="['convolution','convolution_reverb'].includes(node.kind) && p.id==='window'"><select aria-label="Window size" :value="node.parameters.window??256" :disabled="!editable||saving" @change="emit('change','window',Number(($event.target as HTMLSelectElement).value))"><option v-for="n in [128,256,512,1024,2048]" :key="n" :value="n">{{n}} samples</option></select></template><template v-else-if="node.kind==='adsr' && p.id==='reset'"><select aria-label="Retrigger from zero" :value="(node.parameters.reset??0)>0?1:0" :disabled="!editable||saving" @change="emit('change','reset',Number(($event.target as HTMLSelectElement).value))"><option :value="0">Off · attack ramps from the current level</option><option :value="1">On · every trigger drops to 0 and ramps the full attack</option></select></template><template v-else-if="node.kind==='pitch_tracker' && ['slots','fft_size'].includes(p.id)"><select :aria-label="p.label" :value="node.parameters[p.id]??p.default" :disabled="!editable||saving" @change="emit('change',p.id,Number(($event.target as HTMLSelectElement).value))"><option v-for="n in p.id==='slots'?[1,2,3,4]:[2048,4096,8192]" :key="n" :value="n">{{n}}</option></select></template>
        <template v-else-if="node.kind==='piano' && p.id==='octaves'"><select aria-label="Octave span" :value="node.parameters.octaves??1" :disabled="!editable||saving" @change="emit('change','octaves',Number(($event.target as HTMLSelectElement).value))"><option v-for="count in 8" :key="count" :value="count">{{count}} {{count===1?'octave':'octaves'}}</option></select></template><template v-else-if="node.kind==='piano' && p.id==='octave'"><select aria-label="Octave" :value="node.parameters.octave??4" :disabled="!editable||saving" @change="emit('change','octave',Number(($event.target as HTMLSelectElement).value))"><option v-for="octave in Array.from({length:11},(_,i)=>i-1)" :key="octave" :value="octave">{{octave}} · C{{octave}}–{{octave===9?'G':'B'}}{{octave}}</option></select></template>
        <template v-else-if="node.kind==='sliders' && p.id==='orientation'"><select aria-label="Slider orientation" :value="node.parameters.orientation??0" :disabled="!editable||saving" @change="emit('change','orientation',Number(($event.target as HTMLSelectElement).value))"><option :value="0">Vertical</option><option :value="1">Horizontal</option></select></template>
        <template v-else-if="node.kind==='control_input' && ['hide_chrome','changes_only'].includes(p.id)"><input type="checkbox" :aria-label="p.label" :checked="node.parameters[p.id]===1" :disabled="!editable||saving" @change="emit('change',p.id,($event.target as HTMLInputElement).checked?1:0)"></template>
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
          <div class="parameter-controls"><input type="range" :min="p.min" :max="p.max" :step="integerParameter(p) ? 1 : 0.01" :value="value(p)" :disabled="(node.kind==='browser_input'&&p.id==='mute'?localMuteReadOnly:!editable) || (saving && p.structural)" :aria-label="p.label" @input="edit(p, ($event.target as HTMLInputElement).value, false)"><div class="number-field"><input :id="`param-${p.id}`" type="number" @input="textDraft[p.id] = ($event.target as HTMLInputElement).value" :min="p.min" :max="p.max" :step="integerParameter(p) ? 1 : 0.01" :value="textDraft[p.id] ?? formatSlider(value(p),integerParameter(p))" :disabled="(node.kind==='browser_input'&&p.id==='mute'?localMuteReadOnly:!editable) || (saving && p.structural)" @change="edit(p, ($event.target as HTMLInputElement).value, !p.structural)"><span>{{ p.unit }}</span></div><button v-if="p.structural" class="button small" :disabled="!editable || saving" @click="emit('change', p.id, value(p))">Apply</button></div>
          <div class="parameter-range"><span>{{ p.min }} {{ p.unit }}</span><span>{{ p.max }} {{ p.unit }}</span></div>
          <p v-if="errors[p.id]" class="field-error">{{ errors[p.id] }}</p>
        </template>
      </section>
      <div v-if="!descriptor.parameters.length && !['js_control','toggle','record','browser_input','local_midi_input','clock','subgraph','part_midi','part_player','midi_input','midi_output','midi_to_osc','osc_to_midi'].includes(node.kind) && !node.kind.startsWith('subgraph_') && !/^(send|receive)_/.test(node.kind) && !node.kind.endsWith('_visualizer')" class="empty-parameters"><Activity :size="24" /><p>This node has no editable parameters.</p><p>Output: {{ formatValue(values?._out) }}</p></div>
    </div>
    <section v-if="node.kind==='sliders'" class="parameter-row"><template v-for="i in node.parameters.count??4" :key="i"><div v-if="links[`slider_${i}`]" class="driver-source"><span>Slider {{i}} · {{active&&!stale?formatValue(values?.[`_control_${i}`]):'—'}} · read-only</span><button @click="emit('source',links[`slider_${i}`].source)">{{sourceName(links[`slider_${i}`])}} / {{links[`slider_${i}`].source_port}}</button></div></template></section>
    <footer class="modal-footer"><button class="text-button" :disabled="!editable || saving" @click="emit('undo')"><RotateCcw :size="14" /> Undo last edit</button><span>Live edits apply immediately. Revisions autosave each minute.</span><button class="text-button danger" :disabled="!editable || saving" @click="emit('remove')">Delete node</button></footer>
  </dialog>
</template>
