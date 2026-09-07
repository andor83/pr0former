<script setup lang="ts">
import { computed, inject } from 'vue'
import type { ShallowRef, ComputedRef } from 'vue'
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { Settings2 } from 'lucide-vue-next'
import type { Descriptor, GraphNode, Telemetry } from '../types'
import { formatValue } from '../api'
import DataVisualizer from './DataVisualizer.vue'
const props = defineProps<NodeProps<{ node: GraphNode; descriptor: Descriptor; values?: Record<string, number>; open: (id: string) => void; connectPort: (id: string, port: string, direction: string) => void }>>()
const telemetry = inject<ShallowRef<Telemetry | null>>('telemetry')
const telemetryStale=inject<ComputedRef<boolean>>('telemetryStale')
const values = computed(() => telemetry?.value?.values[props.id])
const visualizer=computed(()=>props.data.node.kind.endsWith('_visualizer'))
const visualization=computed(()=>telemetry?.value?.visualizations?.[props.id])
const isMath = computed(() => props.data.descriptor.category === 'Math')
const signal = computed(() => props.data.descriptor.outputs[0]?.signal || props.data.descriptor.inputs[0]?.signal || 'control')
const inputs = computed(() => [...props.data.descriptor.inputs, ...props.data.descriptor.parameters.filter(p => !p.structural).map(p => ({ id: p.id, label: p.label, signal: 'control' as const }))])
const height = computed(() => visualizer.value ? props.data.node.kind==='control_visualizer'?190:150+Math.ceil(props.data.node.channels/2)*(props.data.node.kind==='spectral_visualizer'?148:104) : Math.max(isMath.value ? 124 : 156, 72 + Math.max(inputs.value.length, props.data.descriptor.outputs.length) * 30))
</script>

<template>
  <div class="patch-node" :class="[signal, { selected, 'math-node': isMath, 'visualizer-node':visualizer }]" :style="{ minHeight: `${height}px` }" tabindex="0" @keydown.enter.stop="data.open(id)" @dblclick.stop="data.open(id)">
    <div class="node-cap"><span>{{ data.descriptor.category }}</span><button class="node-settings nodrag nopan" :aria-label="`Edit ${data.node.label}`" @click.stop="data.open(id)"><Settings2 :size="14" /></button></div>
    <div v-if="isMath" class="math-symbol">{{ data.descriptor.symbol }}</div>
    <div v-else class="node-title"><span class="node-glyph">{{ data.descriptor.symbol }}</span>{{ data.node.label }}</div>
    <div v-if="isMath" class="math-label">{{ data.node.label }}</div>
    <div v-for="(port, index) in inputs" :key="`in-${port.id}`" class="port-row input-port" :style="{ top: `${65 + index * 30}px` }">
      <Handle :id="port.id" type="target" :position="Position.Left" :class="port.signal" @click.stop="data.connectPort(id, port.id, 'target')" />
      <span>{{ port.label }}</span>
    </div>
    <div v-for="(port, index) in data.descriptor.outputs" :key="`out-${port.id}`" class="port-row output-port" :style="{ top: `${65 + index * 30}px` }">
      <span>{{ port.label }}</span><Handle :id="port.id" type="source" :position="Position.Right" :class="port.signal" @click.stop="data.connectPort(id, port.id, 'source')" />
    </div>
    <DataVisualizer v-if="visualizer" :kind="data.node.kind" :data="visualization" :sample-rate="telemetry?.sample_rate || 48000" :block-size="telemetry?.block_size || 128" :stale="telemetryStale??true" compact />
    <div class="node-foot"><span>{{ signal === 'control' ? 'CONTROL' : `${data.node.channels} CH` }}</span><span class="node-readout">{{visualizer ? 'PASS THROUGH' : values ? formatValue(values._out) : '—'}}</span></div>
  </div>
</template>

