<script setup lang="ts">
import { computed, inject } from 'vue'
import type { ShallowRef, ComputedRef } from 'vue'
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { Settings2 } from '@lucide/vue'
import type { Descriptor, GraphNode, Telemetry } from '../types'
import { formatValue } from '../api'
import GraphControl from './GraphControl.vue'
import PartPlayerReadout from './PartPlayerReadout.vue'
import SampleSelectorButtons from './SampleSelectorButtons.vue'
import PianoKeys from './PianoKeys.vue'
import DrumPads from './DrumPads.vue'
import MidiControllers from './MidiControllers.vue'
import EnvelopeGraph from './EnvelopeGraph.vue'
import InstrumentFace from './InstrumentFace.vue'
import GranularFieldFace from './GranularFieldFace.vue'
import { readEnvelope } from '../envelopeGeometry'
import PitchTrackerReadout from './PitchTrackerReadout.vue'
import NodeMeters from './NodeMeters.vue'
import OscMessageDebug from './OscMessageDebug.vue'
import MidiInputActivity from './MidiInputActivity.vue'
import TriggerButton from './TriggerButton.vue'
import ToggleButton from './ToggleButton.vue'
import LocalAudioButton from './LocalAudioButton.vue'
import {type LocalAudioAccess,remoteAudioInputs} from '../browserInputs'
import DataVisualizer from './DataVisualizer.vue'
const props = defineProps<NodeProps<{ previewPart?:(id:string)=>void;partName?:string;parameter?:(node:string,key:string,value:number)=>void;projectId:string;canMute:boolean;controller:(node:string,index:number,value:number|null|'cancel'|'clear')=>void;connected:string[];piano:(project:string,node:string,pitch:number,velocity:number,bend?:number)=>void;active:boolean;editable:boolean;driven:boolean;setControl:(id:string,value:number|string)=>void;previewControl:(id:string,value:number)=>void;bang:(id:string)=>void;node: GraphNode; descriptor: Descriptor; values?: Record<string, number>; edit: (id:string)=>void; open: (id: string) => void; connectPort: (id: string, port: string, direction: string) => void; toggleSelection:(id:string)=>void; contextMenu: (id: string, event: MouseEvent) => void }>>()
const telemetry = inject<ShallowRef<Telemetry | null>>('telemetry')
const telemetryStale=inject<ComputedRef<boolean>>('telemetryStale')
const values = computed(() => telemetry?.value?.values[props.id])
const visualizer=computed(()=>props.data.node.kind.endsWith('_visualizer'))
const visualization=computed(()=>telemetry?.value?.visualizations?.[props.id])
const isMath = computed(() => props.data.descriptor.category === 'Math')
// A renamed node keeps its type visible in small letters under the name.
const renamed = computed(() => props.data.node.label.trim().toLowerCase() !== props.data.descriptor.label.trim().toLowerCase())
const signal = computed(() => props.data.node.kind==='js_control'?'control':props.data.descriptor.outputs[0]?.signal || props.data.descriptor.inputs[0]?.signal || 'control')
const inputs = computed(() => [...props.data.descriptor.inputs, ...props.data.descriptor.parameters.filter(p => !p.structural).map(p => ({ id: p.id, label: p.label, signal: 'control' as const }))])
const midiInputs = computed(() => inputs.value.filter(port => port.signal === 'midi'))
const midiOutputs = computed(() => props.data.descriptor.outputs.filter(port => port.signal === 'midi'))
const regularInputs = computed(() => inputs.value.filter(port => port.signal !== 'midi'))
const regularOutputs = computed(() => props.data.descriptor.outputs.filter(port => port.signal !== 'midi'))
// The on-node envelope shows engine values while live, else the stored parameters.
// Polyphonic instruments get a full face: sample, per-voice envelope and recent notes. The ADSR node keeps its bare graph.
const instrument = computed(() => ['poly_sampler','synth','fm_synth','granular_synth','granular_cloud'].includes(props.data.node.kind))
const hasEnvelope = computed(() => props.data.node.kind === 'adsr')
const granular = computed(() => props.data.node.kind === 'granular_field')
const adsrEnvelope = computed(() => readEnvelope(props.data.node.parameters, props.data.descriptor.parameters,
  props.data.active && !(telemetryStale?.value ?? true) ? values.value : undefined, props.data.connected))
const audioAccess=inject<ComputedRef<LocalAudioAccess>>('localAudioAccess')
const assignedName=computed(()=>audioAccess?.value.members.find(m=>m.id===audioAccess.value.assignments[props.id])?.username)
const audioSource=computed(()=>remoteAudioInputs.value.find(i=>i.project_id===props.data.projectId&&i.node===props.id))
const audioName=computed(()=>audioSource.value?`${audioSource.value.user_name} (${audioSource.value.machine_name})`:undefined)
const height = computed(() => props.data.node.kind==='part_player'?380 : granular.value ? Math.max(360 + 16 * ((props.data.node.sample_choices?.length ?? 0) + props.data.connected.filter(p => /^live_\d$/.test(p)).length), 100 + Math.max(inputs.value.length, props.data.descriptor.outputs.length) * 30) : instrument.value ? Math.max(380, 100 + Math.max(inputs.value.length, props.data.descriptor.outputs.length) * 30) : props.data.node.kind==='sample_selector'?132+Math.max(1,Math.min(8,props.data.node.sample_choices?.length??0))*36 : props.data.node.kind==='browser_input'?235 : ['midi_input','local_midi_input','midi_to_osc','osc_to_midi','osc_input','osc_output'].includes(props.data.node.kind) ? Math.max(320,100+Math.max(inputs.value.length,props.data.descriptor.outputs.length)*30) : ['knobs','sliders'].includes(props.data.node.kind) ? Math.max(300,130+Math.max(props.data.node.parameters.count??4,5)*30) : props.data.node.kind === 'pitch_tracker' ? Math.max(210,172+30*(props.data.node.parameters.slots??1)) : props.data.node.kind === 'piano' ? 280 : props.data.node.kind === 'drum_pads' ? 300 : props.data.node.kind === 'control_input' ? 260 : visualizer.value ? props.data.node.kind==='control_visualizer'?190:150+Math.ceil(props.data.node.channels/2)*(props.data.node.kind==='spectral_visualizer'?148:104) : Math.max(isMath.value ? 124 : 156, (props.data.node.kind === 'subgraph' ? 180 : 100) + Math.max(inputs.value.length, props.data.descriptor.outputs.length) * 30))
function controlSelect(event: MouseEvent) {
  if (event.ctrlKey && event.button === 0 && !(event.target instanceof Element && event.target.closest('button,.vue-flow__handle'))) {
    event.preventDefault();event.stopPropagation();props.data.toggleSelection(props.id)
  }
}
</script>

<template>
  <div v-if="['trigger','toggle'].includes(data.node.kind)" class="patch-node control trigger-node" :class="{selected}" tabindex="0" :aria-label="`${data.node.label} node`" @mousedown="controlSelect" @contextmenu.prevent.stop="!$event.ctrlKey && data.contextMenu(id,$event)" @keydown.enter.stop.prevent="data.open(id)" @dblclick.stop="data.open(id)">
    <Handle id="in" type="target" :position="Position.Left" class="control" aria-label="Trigger input" @click.stop="data.connectPort(id,'in','target')" />
    <ToggleButton v-if="data.node.kind==='toggle'" :label="data.node.label" :sequence="telemetry?.sequence" :checked="data.active&&!(telemetryStale??true)?values?._checked===1:data.node.control_value===1" :disabled="!data.editable||(data.active&&(telemetryStale??true))" @value="value=>data.setControl(id,value)" />
    <TriggerButton v-else :label="data.node.label" :values="values" :disabled="!data.active||!data.editable||(telemetryStale??true)" @trigger="data.bang(id)" />
    <Handle id="out" type="source" :position="Position.Right" class="control" aria-label="Trigger output" @click.stop="data.connectPort(id,'out','source')" />
  </div>
  <div v-else-if="data.node.kind==='control_input'" class="patch-node control compact-control" :class="{selected, 'contents-only':data.node.parameters.hide_chrome===1}" tabindex="0" :aria-label="`${data.node.label} node`" @mousedown="controlSelect" @contextmenu.prevent.stop="!$event.ctrlKey&&data.contextMenu(id,$event)" @keydown.enter.stop.prevent="data.open(id)" @dblclick.stop="data.open(id)">
    <header v-if="data.node.parameters.hide_chrome!==1"><span>{{data.node.label}}</span><button class="node-settings nodrag nopan" :aria-label="`Edit ${data.node.label}`" @click.stop="data.open(id)"><Settings2 :size="14" /></button></header>
    <Handle id="in" type="target" :position="Position.Left" class="control" aria-label="Control input" @click.stop="data.connectPort(id,'in','target')" />
    <GraphControl :node="data.node" :connected="data.driven" :data="visualization" :stale="telemetryStale??true" :active="data.active" :editable="data.editable" @preview="value=>data.previewControl(id,value)" @value="value=>data.setControl(id,value)" @bang="data.bang(id)" />
    <Handle id="out" type="source" :position="Position.Right" class="control" aria-label="Control output" @click.stop="data.connectPort(id,'out','source')" />
  </div>
  <div v-else-if="data.node.kind==='value'" class="patch-node control value-node" :class="{selected}" tabindex="0" :aria-label="`${data.node.label} node`" @mousedown="controlSelect" @contextmenu.prevent.stop="!$event.ctrlKey&&data.contextMenu(id,$event)" @keydown.enter.stop.prevent="data.open(id)" @dblclick.stop="data.open(id)">
    <Handle id="value" type="target" :position="Position.Left" class="control" style="top:52px" title="Set value" aria-label="Set value input" @click.stop="data.connectPort(id,'value','target')" />
    <Handle id="trigger" type="target" :position="Position.Left" class="control" style="top:20px" title="Trigger" aria-label="Value trigger input" @click.stop="data.connectPort(id,'trigger','target')" />
    <span class="value-input-label" style="top:14px">Trigger</span><span class="value-input-label" style="top:46px">Set value</span>
    <output aria-label="Stored value">{{formatValue(data.active&&!(telemetryStale??true)?values?.value:data.node.parameters.value??0)}}</output>
    <Handle id="out" type="source" :position="Position.Right" class="control" title="Value output" aria-label="Value output" @click.stop="data.connectPort(id,'out','source')" />
  </div>
  <div v-else class="patch-node" :class="[signal, { selected, 'math-node': isMath, 'visualizer-node':visualizer, 'subgraph-node':data.node.kind==='subgraph' }]" :style="{ minHeight: `${height}px`, width: ['midi_input','local_midi_input','midi_to_osc','osc_to_midi','osc_input','osc_output'].includes(data.node.kind) ? '420px' : ['knobs','sliders'].includes(data.node.kind) ? `${data.node.kind==='sliders'&&data.node.parameters.orientation===1?350:160+88*(data.node.parameters.count??4)}px` : data.node.kind==='pitch_tracker' ? `${Math.max(240,24+92*(data.node.parameters.slots??1))}px` : data.node.kind==='piano' ? `${160+256*Math.min(data.node.parameters.octaves??1,10-(data.node.parameters.octave??4))}px` : data.node.kind==='drum_pads' ? '470px' : instrument || granular ? '400px' : data.node.kind==='adsr' ? '300px' : data.node.kind==='meter' ? `${Math.max(204,150+16*Math.min(8,data.node.channels))}px` : undefined }" tabindex="0" @mousedown="controlSelect" @click="event => { if(event.ctrlKey) event.stopPropagation() }" @contextmenu.prevent.stop="!$event.ctrlKey && data.contextMenu(id, $event)" @keydown.enter.stop.prevent="data.open(id)" @dblclick.stop="data.open(id)">
    <div class="node-body" :style="{ minHeight: `${height - 2}px`, paddingBottom: data.node.kind==='sample_selector' ? '36px' : data.node.kind==='sliders' ? '40px' : undefined }">
    <div class="node-cap"><span>{{ data.descriptor.category }}</span><button class="node-settings nodrag nopan" :aria-label="`Edit ${data.node.label}`" @click.stop="data.edit(id)"><Settings2 :size="14" /></button></div>
    <div v-if="isMath" class="math-symbol">{{ data.descriptor.symbol }}</div>
    <div v-else class="node-title"><span class="node-glyph">{{ data.descriptor.symbol }}</span><span class="node-name">{{ audioName || data.node.label }}<small v-if="renamed" class="node-kind">{{ data.descriptor.label }}</small></span></div>
    <div v-if="isMath" class="math-label">{{ data.node.label }}<small v-if="renamed" class="node-kind">{{ data.descriptor.label }}</small></div>
    <div v-for="port in midiInputs" :key="`in-${port.id}`" class="port-row input-port midi-port" style="top: 65px">
      <Handle :id="port.id" type="target" :position="Position.Left" :class="port.signal" @click.stop="data.connectPort(id, port.id, 'target')" />
      <span>{{ port.label }}</span>
    </div>
    <div v-for="(port, index) in regularInputs" :key="`in-${port.id}`" class="port-row input-port" :style="{ top: `${65 + (index + midiInputs.length) * 30}px` }">
      <Handle :id="port.id" type="target" :position="Position.Left" :class="port.signal" @click.stop="data.connectPort(id, port.id, 'target')" />
      <span>{{ port.label }}</span>
    </div>
    <div v-for="port in midiOutputs" :key="`out-${port.id}`" class="port-row output-port midi-port" style="top: 65px">
      <span>{{ port.label }}</span><Handle :id="port.id" type="source" :position="Position.Right" :class="port.signal" @click.stop="data.connectPort(id, port.id, 'source')" />
    </div>
    <div v-for="(port, index) in regularOutputs" :key="`out-${port.id}`" class="port-row output-port" :style="{ top: `${65 + (index + midiOutputs.length) * 30}px` }">
      <span>{{ port.label }}</span><Handle :id="port.id" type="source" :position="Position.Right" :class="port.signal" @click.stop="data.connectPort(id, port.id, 'source')" />
    </div>
    <MidiControllers v-if="['knobs','sliders'].includes(data.node.kind)" :node="data.node" :values="values" :connected="data.connected" :editable="data.editable" :disabled="!data.active||!data.editable||(telemetryStale??true)" @control="(index,value)=>data.controller(id,index,value)" />
    <PartPlayerReadout :interactive="!!data.previewPart && !!data.partName" @open="data.previewPart?.(id)" v-if="data.node.kind==='part_player'" class="node-part-player" :part-name="data.partName" :values="values" :stale="!data.active||(telemetryStale??true)" />
    <NodeMeters v-if="data.node.kind==='meter'" :channels="data.node.channels" :values="values" :stale="!data.active || (telemetryStale??true)" />
    <LocalAudioButton v-if="data.node.kind==='browser_input'" :project-id="data.projectId" :node="id" :muted="((data.active&&!(telemetryStale??true)?values?.mute:data.node.parameters.mute)??0)>0" :driven="data.connected.includes('mute')" :disabled="!data.canMute||(data.active&&(telemetryStale??true))" :active="data.active" :source="audioName" :assigned-name="assignedName" :sending="data.active && !(telemetryStale??true) && !!audioSource?.sending && audioSource?.state==='connected'" />
    <PitchTrackerReadout v-if="data.node.kind==='pitch_tracker'" :slots="data.node.parameters.slots??1" :values="values" :stale="!data.active||(telemetryStale??true)" />
    <template v-if="hasEnvelope"><EnvelopeGraph v-if="adsrEnvelope" class="node-envelope" compact :envelope="adsrEnvelope" :level="data.active && !(telemetryStale??true) ? values?._out : undefined" :gate="data.active && !(telemetryStale??true) && (values?.gate ?? 0) > 0" /><span v-else class="node-envelope envelope-unavailable">Envelope waiting for live values</span></template>
    <GranularFieldFace v-if="granular" :node="data.node" :values="values" :connected="data.connected" :active="data.active" :stale="telemetryStale??true" :editable="data.editable" :parameter="data.parameter" />
    <InstrumentFace v-if="instrument" :node="data.node" :descriptor="data.descriptor" :values="values" :connected="data.connected" :active="data.active" :stale="telemetryStale??true" />
    <DrumPads v-if="data.node.kind==='drum_pads'" :notes="[36,38,45,50,42,49].map((fallback,i)=>data.node.parameters[`note_${i+1}`]??fallback)" :values="values" :stale="!data.active || (telemetryStale??true)" :disabled="!data.editable || !data.active || (telemetryStale??true)" @note="(pitch,velocity)=>data.piano(data.projectId,id,pitch,velocity)" />
    <OscMessageDebug v-if="['osc_input','osc_output'].includes(data.node.kind)" compact :message="telemetry?.osc_messages?.[id]" :address="data.node.io?.address" :output="data.node.kind==='osc_output'" :active="data.active" :stale="telemetryStale??true" />
    <MidiInputActivity v-if="['midi_input','local_midi_input','midi_to_osc','osc_to_midi'].includes(data.node.kind)" :values="values" :active="data.active" :stale="telemetryStale??true" />
    <PianoKeys @bend="value=>data.piano(data.projectId,id,0,0,value)" v-if="data.node.kind==='piano'" :octave="data.node.parameters.octave??4" :octaves="data.node.parameters.octaves??1" :values="values" :stale="!data.active || (telemetryStale??true)" :disabled="!data.editable || !data.active || (telemetryStale??true)" @note="(pitch,velocity)=>data.piano(data.projectId,id,pitch,velocity)" />
    <SampleSelectorButtons v-if="data.node.kind==='sample_selector'" :choices="data.node.sample_choices||[]" :index="data.active ? (telemetryStale??true) ? -1 : values?.index??-1 : data.connected.includes('index') ? -1 : data.node.parameters.index??0" :disabled="!data.editable||data.connected.includes('index')||(data.active&&(telemetryStale??true))" :connected="data.connected.includes('index')" @select="index=>data.parameter?.(id,'index',index)" />
    <GraphControl v-if="data.node.kind==='control_input'" :node="data.node" :connected="data.driven" :data="visualization" :stale="telemetryStale??true" :active="data.active" :editable="data.editable" @preview="value=>data.previewControl(id,value)" @value="value=>data.setControl(id,value)" @bang="data.bang(id)" />
    <DataVisualizer v-if="visualizer" :kind="data.node.kind" :data="visualization" :sample-rate="telemetry?.sample_rate || 48000" :block-size="telemetry?.block_size || 128" :stale="telemetryStale??true" compact />
    <button v-if="data.node.kind==='subgraph'" class="button small nodrag nopan subgraph-open" :aria-label="`Open ${data.node.label}`" @click.stop="data.open(id)">Open subgraph</button><div v-if="data.node.kind!=='pitch_tracker'" class="node-foot"><span>{{ signal === 'control' ? 'CONTROL' : `${data.node.channels} CH` }}</span><span class="node-readout">{{data.node.kind==='js_control' ? !data.active ? 'SCRIPT' : telemetry?.scripts?.[id]?.faulted ? 'SCRIPT STOPPED' : 'RUNNING' : visualizer ? 'PASS THROUGH' : values ? formatValue(values._out) : '—'}}</span></div>
    </div>
  </div>
</template>


<style scoped>
.node-body{position:relative}
.node-part-player{position:absolute;top:250px;left:12px;right:12px}


.patch-node.trigger-node{width:80px;min-width:80px;min-height:64px;height:64px;padding:8px;display:flex;align-items:center;justify-content:center}
.patch-node.compact-control{width:190px;min-width:150px;min-height:0;padding:10px}
.compact-control header{display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:6px;font-size:12px;color:var(--control-ink)}
.compact-control header span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.patch-node.compact-control.contents-only{width:150px;padding:6px}
.compact-control>.vue-flow__handle-left{left:0}.compact-control>.vue-flow__handle-right{right:0}
.patch-node.value-node{width:148px;min-width:148px;min-height:72px;height:72px;padding:10px;display:flex;align-items:center;justify-content:center}.value-input-label{position:absolute;left:10px;font-size:9px;color:var(--ink-muted)}.value-node output{margin-left:50px;font-size:18px;font-variant-numeric:tabular-nums;color:var(--control-ink)}
.patch-node .node-envelope{position:absolute;left:74px;top:66px;width:180px;height:90px;max-width:none}
.envelope-unavailable{color:var(--ink-muted);font-size:11px;padding:12px}
.node-name{display:flex;flex-direction:column;min-width:0;line-height:1.15;overflow-wrap:anywhere}.node-title .node-kind{margin-top:2px}.math-label .node-kind{display:block;margin-top:1px}
.trigger-node>.vue-flow__handle-left,.value-node>.vue-flow__handle-left{left:0}.trigger-node>.vue-flow__handle-right,.value-node>.vue-flow__handle-right{right:0}
</style>
