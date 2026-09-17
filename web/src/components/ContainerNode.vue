<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { NodeProps } from '@vue-flow/core'
import { useVueFlow } from '@vue-flow/core'
import { Settings2 } from '@lucide/vue'
import type { Descriptor, GraphNode } from '../types'
import { containerColor } from '../containerPalette'
import { CONTAINER, containerRect } from '../containers'
const props = defineProps<NodeProps<{ node: GraphNode; descriptor: Descriptor; editable: boolean; edit: (id: string) => void; resize?: (id: string, width: number, height: number) => void; toggleSelection: (id: string) => void; contextMenu: (id: string, event: MouseEvent) => void }>>()
const { viewport } = useVueFlow()
const rect = computed(() => containerRect(props.data.node))
// While the corner is dragged the frame follows the pointer; the saved size takes over once it lands.
const live = ref<{ width: number; height: number } | null>(null)
const size = computed(() => live.value ?? { width: rect.value.width, height: rect.value.height })
const color = computed(() => containerColor(props.data.node.parameters.color))
const explanation = computed(() => typeof props.data.node.control_value === 'string' ? props.data.node.control_value.trim() : '')
let drag: { x: number; y: number; width: number; height: number } | null = null
watch(rect, () => { if (!drag) live.value = null })
function resizeStart(event: PointerEvent) {
  if (!props.data.editable || event.button !== 0) return
  event.preventDefault(); event.stopPropagation()
  drag = { x: event.clientX, y: event.clientY, width: size.value.width, height: size.value.height }
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
}
function resizeMove(event: PointerEvent) {
  if (!drag) return
  const zoom = viewport.value.zoom || 1
  live.value = { width: Math.max(CONTAINER.minWidth, Math.round(drag.width + (event.clientX - drag.x) / zoom)), height: Math.max(CONTAINER.minHeight, Math.round(drag.height + (event.clientY - drag.y) / zoom)) }
}
function resizeEnd(event: PointerEvent) {
  if (!drag) return
  drag = null
  const target = event.currentTarget as HTMLElement
  if (target.hasPointerCapture(event.pointerId)) target.releasePointerCapture(event.pointerId)
  const final = live.value
  if (final && (final.width !== rect.value.width || final.height !== rect.value.height)) props.data.resize?.(props.id, final.width, final.height)
  else live.value = null
}
function controlSelect(event: MouseEvent) {
  if (event.ctrlKey && event.button === 0 && !(event.target instanceof Element && event.target.closest('button'))) { event.preventDefault(); event.stopPropagation(); props.data.toggleSelection(props.id) }
}
</script>
<template>
  <div class="container-node" :class="{ selected }" :style="{ width: `${size.width}px`, height: `${size.height}px`, '--container': color.hex }" tabindex="0" :aria-label="`${data.node.label} container`" @mousedown="controlSelect" @contextmenu.prevent.stop="!$event.ctrlKey && data.contextMenu(id, $event)" @keydown.enter.stop.prevent="data.edit(id)">
    <header class="container-head"><span class="container-title">{{ data.node.label }}<HelpNote v-if="explanation" :label="data.node.label">{{ explanation }}</HelpNote></span><button class="node-settings nodrag nopan" :aria-label="`Edit ${data.node.label}`" @click.stop="data.edit(id)"><Settings2 :size="14" /></button></header>
    <div v-if="data.editable" class="container-resize nodrag nopan" role="button" tabindex="-1" :aria-label="`Resize ${data.node.label}`" title="Drag to resize" @pointerdown="resizeStart" @pointermove="resizeMove" @pointerup="resizeEnd" @pointercancel="resizeEnd" @lostpointercapture="resizeEnd" @mousedown.stop @click.stop></div>
  </div>
</template>
<style scoped>
.container-node{position:relative;border:1px solid var(--container);border-radius:10px;background:color-mix(in srgb,var(--container) 16%,#1b2224);outline:none;box-sizing:border-box}
.container-node.selected{box-shadow:0 0 0 2px var(--cyan)}
.container-head{height:44px;display:flex;align-items:center;justify-content:space-between;padding:0 8px 0 14px;border-bottom:1px solid color-mix(in srgb,var(--container) 60%,transparent);background:color-mix(in srgb,var(--container) 28%,transparent);border-radius:9px 9px 0 0}
.container-title{display:flex;align-items:center;gap:6px;min-width:0;font:600 13px 'Space Grotesk',system-ui,sans-serif;letter-spacing:-.2px;color:var(--white);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.container-node :deep(.help-inline-container){padding:0 14px}
.container-resize{position:absolute;right:3px;bottom:3px;width:18px;height:18px;cursor:nwse-resize;border-right:2px solid var(--container);border-bottom:2px solid var(--container);border-radius:0 0 7px 0;opacity:.85;touch-action:none}
.container-resize:hover{opacity:1}
:global(.vue-flow__node-container){z-index:-1!important}
:global(.graph-light) .container-node{background:color-mix(in srgb,var(--container) 16%,#f4f8f8);color:#1b3035}
:global(.graph-light) .container-title{color:#1b3035}
</style>
