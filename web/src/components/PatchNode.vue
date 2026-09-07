<script setup lang="ts">
import { computed, inject } from 'vue'
import type { ShallowRef } from 'vue'
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { Settings2 } from 'lucide-vue-next'
import type { Descriptor, GraphNode, Telemetry } from '../types'
import { formatValue } from '../api'
const props = defineProps<NodeProps<{ node: GraphNode; descriptor: Descriptor; values?: Record<string, number>; open: (id: string) => void; connectPort: (id: string, port: string, direction: string) => void }>>()
const telemetry = inject<ShallowRef<Telemetry | null>>('telemetry')
const values = computed(() => telemetry?.value?.values[props.id])
const isMath = computed(() => props.data.descriptor.category === 'Math')
const signal = computed(() => props.data.descriptor.outputs[0]?.signal || props.data.descriptor.inputs[0]?.signal || 'control')
const inputs = computed(() => [...props.data.descriptor.inputs, ...props.data.descriptor.parameters.filter(p => !p.structural).map(p => ({ id: p.id, label: p.label, signal: 'control' as const }))])
const height = computed(() => Math.max(isMath.value ? 124 : 156, 72 + Math.max(inputs.value.length, props.data.descriptor.outputs.length) * 30))
</script>

<template>
  <div class="patch-node" :class="[signal, { selected, 'math-node': isMath }]" :style="{ minHeight: `${height}px` }" tabindex="0" @keydown.enter.stop="data.open(id)" @dblclick.stop="data.open(id)">
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
    <div class="node-foot"><span>{{ signal === 'audio' ? `${data.node.channels} CH` : 'CONTROL' }}</span><span class="node-readout">{{ values ? formatValue(values._out) : '—' }}</span></div>
  </div>
</template>

