<script setup lang="ts">
import { computed, markRaw, nextTick, onBeforeUnmount, onMounted, provide, ref, shallowRef, useId, watch } from 'vue'
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { Background } from '@vue-flow/background'
import { Controls } from '@vue-flow/controls'
import type { Descriptor, GraphNode, NodeDocumentation } from '../types'
import { autoSpace } from '../autoLayout'
import { nodeDescriptor } from '../subgraphs'
import PatchNode from './PatchNode.vue'
import SignalEdge from './SignalEdge.vue'
const props = defineProps<{ documentation: NodeDocumentation; descriptors: Descriptor[]; focusKind?: string }>()
const id = `documentation-${useId()}`
const { getNodes, setNodes, fitView, onNodesInitialized } = useVueFlow({ id })
const types = { instrument: markRaw(PatchNode) }, edgeTypes = { signal: markRaw(SignalEdge) }
const selected = ref<GraphNode>()
const root = ref<HTMLElement>()
const previewNodes = computed(() => props.documentation.graph.nodes.filter(n => n.kind !== 'subgraph'))
const descriptor = (kind: string) => props.descriptors.find(d => d.kind === kind)!
const noop = () => {}
// Explicitly shadow the live workspace's injections. These graphs never subscribe,
// render sound, write project state or request signal previews.
provide('telemetry', shallowRef(null))
provide('telemetryStale', computed(() => true))
provide('graphActive', ref(false))
provide('localAudioAccess', undefined)
const nodes = computed(() => previewNodes.value.map(node => ({
  id: node.id, type: 'instrument', position: { x: node.x, y: node.y },
  class: node.kind === props.focusKind ? 'documented-node' : undefined,
  data: { node, descriptor: nodeDescriptor(node, props.documentation.graph.nodes, props.descriptors), projectId: '', active: false, editable: false, canMute: false,
    connected: props.documentation.graph.edges.filter(e => e.target === node.id).map(e => e.target_port), driven: false,
    controller: noop, piano: noop, setControl: noop, bang: noop, connectPort: noop, toggleSelection: noop, contextMenu: noop,
    edit: () => { selected.value = node }, open: () => { selected.value = node } },
})))
const edges = computed(() => props.documentation.graph.edges.map(edge => {
  const source = props.documentation.graph.nodes.find(n => n.id === edge.source)!
  const port = descriptor(source.kind).outputs.find(p => p.id === edge.source_port)!
  return { id: edge.id, type: 'signal', source: edge.source, target: edge.target, sourceHandle: edge.source_port, targetHandle: edge.target_port,
    data: { signal: port.signal, channels: port.fixed_channels ?? source.channels, projectId: '', preview: true } }
}))
const settings = computed(() => selected.value ? descriptor(selected.value.kind).parameters.map(p => ({ ...p, value: selected.value!.parameters[p.id] ?? p.default })) : [])
async function layout() {
  const positions = autoSpace(previewNodes.value, props.documentation.graph.edges, previewNodes.value.map(n => n.id), node => {
    const measured = getNodes.value.find(n => n.id === node.id)?.dimensions
    return { width: measured?.width || 260, height: measured?.height || 280 }
  }, { columnGap: 100, rowGap: 80 })
  setNodes(getNodes.value.map(n => ({ ...n, position: positions.get(n.id) ?? n.position })))
  await nextTick()
  void fitView({ padding: .15, maxZoom: .9, duration: 0 })
}
onNodesInitialized(layout)
let observer: ResizeObserver | undefined
onMounted(() => { observer = new ResizeObserver(() => void fitView({ padding: .15, maxZoom: .9, duration: 0 })); if (root.value) observer.observe(root.value) })
onBeforeUnmount(() => observer?.disconnect())
watch(() => props.documentation, () => { selected.value = undefined; void nextTick(layout) })
</script>
<template>
  <div class="documentation-patch">
    <div ref="root" class="documentation-canvas" role="region" :aria-label="documentation.title + ' graph'">
      <VueFlow @node-click="({ node }) => selected = node.data.node" :id="id" :nodes="nodes" :edges="edges" :node-types="types" :edge-types="edgeTypes" :nodes-draggable="false" :nodes-connectable="false" :elements-selectable="false" :delete-key-code="null" :selection-key-code="null" :min-zoom=".1" :max-zoom="2" :fit-view-on-init="true" :fit-view-options="{ padding: .15, maxZoom: .9 }" :zoom-on-scroll="false" :prevent-scrolling="false">
        <Background :gap="24" :size="1" pattern-color="#394043" />
        <Controls position="bottom-left" :show-interactive="false" />
      </VueFlow>
    </div>
    <p class="preview-caption">Read-only example · Pan or use + / − to inspect · Open a node’s gear for its example settings</p>
    <section v-if="selected" class="example-settings" :aria-label="selected.label + ' example settings'">
      <header><h3>{{ selected.label }} · {{ selected.channels }} channels</h3><button class="text-button" @click="selected = undefined">Close settings</button></header>
      <p>{{ descriptor(selected.kind).description }}</p>
      <p v-if="selected.control_value != null">{{ selected.kind.startsWith('send_') || selected.kind.startsWith('receive_') ? 'Target name' : 'Stored value' }}: {{ selected.control_value }}</p>
      <dl><template v-for="p in settings" :key="p.id"><dt>{{ p.label }}</dt><dd>{{ p.value }} {{ p.unit }}<span v-if="documentation.graph.edges.some(e => e.target === selected!.id && e.target_port === p.id)"> · connected in this example</span></dd></template></dl>
    </section>
    <details class="example-connections"><summary>Connections and settings to recreate this patch</summary>
      <ul><li v-for="node in previewNodes" :key="node.id"><b>{{ node.label }}</b> ({{ descriptor(node.kind).label }}, {{ node.channels }} ch)<template v-for="(value, key) in node.parameters" :key="key"> · {{ descriptor(node.kind).parameters.find(p => p.id === key)?.label }}: {{ value }}</template><template v-if="node.control_value != null"> · {{ node.kind.startsWith('send_') || node.kind.startsWith('receive_') ? 'Target' : 'Value' }}: {{ node.control_value }}</template></li></ul>
      <ul><li v-for="edge in documentation.graph.edges" :key="edge.id">{{ previewNodes.find(n => n.id === edge.source)?.label }} / {{ edge.source_port }} → {{ previewNodes.find(n => n.id === edge.target)?.label }} / {{ edge.target_port }}</li></ul>
    </details>
  </div>
</template>
<style scoped>
.documentation-canvas{height:440px;min-height:280px;border:1px solid #3b484b;border-radius:8px;background:#171e20;overflow:hidden;margin-top:18px}
.documentation-canvas :deep(.vue-flow__handle){pointer-events:none}
.documentation-canvas :deep(.documented-node .patch-node){box-shadow:0 0 0 2px var(--cyan),0 8px 24px #0005}
.preview-caption,.example-connections{font-size:11px;color:#9bafb1;line-height:1.6;margin:10px 0}.example-connections summary{cursor:pointer;padding:8px 0}.example-connections ul{padding-left:20px}
.example-settings{padding:16px;border:1px solid var(--line);border-radius:6px}.example-settings header{display:flex;justify-content:space-between;align-items:center}.example-settings p{font-size:12px;line-height:1.6;color:#a9babc}.example-settings dl{display:grid;grid-template-columns:1fr 1fr;gap:7px;font-size:12px}.example-settings dd{margin:0;color:var(--amber)}
@media(max-width:600px){.documentation-canvas{height:340px}}
</style>
