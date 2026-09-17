<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { describePoint, fromPixels, nudge, roundField, toPixels, type Point } from '../granularField'
// The two-dimensional field of a Granular Field node: one circle per source, sized
// by how much of the cloud it currently supplies, and the roaming control point.
// Modelled on EnvelopeGraph: a draft overlays the props during a drag, every handle
// is a keyboard slider, and cabled axes are locked.
export interface PadSource { key: string; label: string; kind: 'sample' | 'live'; point: Point; weight: number; missing?: boolean; keys: { x: string; y: string } }
const props = defineProps<{ sources: PadSource[]; control: Point | null; focus: number; locked?: string[]; editable?: boolean; compact?: boolean; live?: boolean }>()
const emit = defineEmits<{ change: [key: string, value: number] }>()
const SIZE = 240, PAD = 14
const uid = useId()
const svg = ref<SVGSVGElement>()
const draft = ref<Record<string, number>>({})
const dragging = ref<string | null>(null)
const CONTROL = { x: 'x', y: 'y' }
watch(() => [props.sources, props.control] as const, () => {
  for (const key of Object.keys(draft.value)) {
    const incoming = key === 'x' ? props.control?.x : key === 'y' ? props.control?.y : props.sources.find(s => s.keys.x === key)?.point.x ?? props.sources.find(s => s.keys.y === key)?.point.y
    if (incoming === draft.value[key]) delete draft.value[key]
  }
}, { deep: true })
const locked = (key: string) => !!props.locked?.includes(key)
const pointOf = (keys: { x: string; y: string }, base: Point): Point => ({ x: draft.value[keys.x] ?? base.x, y: draft.value[keys.y] ?? base.y })
const control = computed(() => props.control ? pointOf(CONTROL, props.control) : null)
const handles = computed(() => props.sources.map(s => ({ ...s, point: pointOf(s.keys, s.point), pixels: toPixels(pointOf(s.keys, s.point), SIZE, PAD), radius: (props.compact ? 4 : 5) + (props.compact ? 5 : 8) * Math.min(1, Math.max(0, s.weight)), locked: locked(s.keys.x) && locked(s.keys.y) })))
const controlPixels = computed(() => control.value ? toPixels(control.value, SIZE, PAD) : null)
const controlLocked = computed(() => locked('x') && locked('y'))
const focusRadius = computed(() => Math.max(0.05, props.focus) * (SIZE - 2 * PAD) / 2)
const pointer = ref<Point | null>(null)
const preview = computed(() => {
  if (props.compact || !dragging.value || !pointer.value) return null
  const point = dragging.value === 'control' ? control.value : handles.value.find(h => h.key === dragging.value)?.point
  if (!point) return null
  const text = describePoint(point), width = text.length * 4.6 + 10, height = 14
  const x = Math.min(SIZE - width / 2 - 2, Math.max(width / 2 + 2, pointer.value.x))
  const y = pointer.value.y - 18 - height < 1 ? pointer.value.y + 20 : pointer.value.y - 18
  return { text, width, height, x, y }
})
const summary = computed(() => {
  const samples = props.sources.filter(s => s.kind === 'sample').length, live = props.sources.length - samples
  const at = control.value ? `control at ${describePoint(control.value)}` : 'control point driven by cables'
  return `Granular field: ${samples} sample${samples === 1 ? '' : 's'}, ${live} live input${live === 1 ? '' : 's'}, ${at}`
})
function unit(event: PointerEvent): Point {
  const rect = svg.value!.getBoundingClientRect()
  const px = { x: (event.clientX - rect.left) / rect.width * SIZE, y: (event.clientY - rect.top) / rect.height * SIZE }
  pointer.value = px
  return fromPixels(px, SIZE, PAD)
}
function apply(keys: { x: string; y: string }, current: Point, next: Point) {
  for (const [axis, key] of [['x', keys.x], ['y', keys.y]] as const) {
    if (locked(key)) continue
    const value = roundField(next[axis])
    if (value === current[axis] && draft.value[key] === undefined) continue
    draft.value[key] = value
    emit('change', key, value)
  }
}
function down(event: PointerEvent, handle: string, keys: { x: string; y: string }, current: Point | null, isLocked: boolean) {
  if (!props.editable || isLocked || event.button !== 0 || !current) return
  event.preventDefault(); event.stopPropagation()
  ;(event.currentTarget as Element).setPointerCapture(event.pointerId)
  dragging.value = handle
  apply(keys, current, unit(event))
}
function move(event: PointerEvent, handle: string, keys: { x: string; y: string }, current: Point | null) {
  if (dragging.value === handle && current) apply(keys, current, unit(event))
}
function up(handle: string, keys: { x: string; y: string }, current: Point | null) {
  if (dragging.value !== handle) return
  dragging.value = null; pointer.value = null
  // Re-emit the resting position so the last movement lands even if a write was skipped mid-drag.
  if (current) for (const key of [keys.x, keys.y]) if (!locked(key) && draft.value[key] !== undefined) emit('change', key, draft.value[key]!)
}
function keys(event: KeyboardEvent, keys: { x: string; y: string }, current: Point | null, isLocked: boolean) {
  if (!props.editable || isLocked || !current) return
  const next = nudge(current, event.key, event.shiftKey)
  if (!next) return
  apply(keys, current, next)
  event.preventDefault(); event.stopPropagation()
}
// Pressing on the empty field places the control point there and starts dragging it.
function panelDown(event: PointerEvent) {
  if (!props.editable || controlLocked.value || event.button !== 0 || props.compact && !props.editable) return
  if (!control.value) return
  down(event, 'control', CONTROL, control.value, false)
}
</script>
<template>
  <div class="field-pad-wrap nodrag nopan" :class="{ compact, editable }">
    <svg ref="svg" class="field-pad" :class="{ compact, live }" :viewBox="`0 0 ${SIZE} ${SIZE}`" role="img" :aria-label="summary" @dblclick.stop>
      <defs>
        <pattern :id="`grid-${uid}`" :width="(SIZE - 2 * PAD) / 8" :height="(SIZE - 2 * PAD) / 8" patternUnits="userSpaceOnUse" :x="PAD" :y="PAD"><path :d="`M${(SIZE - 2 * PAD) / 8} 0H0V${(SIZE - 2 * PAD) / 8}`" fill="none" stroke="#f2953a" stroke-opacity=".18" stroke-width="1" /></pattern>
        <filter :id="`glow-${uid}`" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="2.6" result="blur" /><feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge></filter>
      </defs>
      <rect x="0" y="0" :width="SIZE" :height="SIZE" rx="12" class="panel" @pointerdown="panelDown" @pointermove="move($event, 'control', CONTROL, control)" @pointerup="up('control', CONTROL, control)" @pointercancel="up('control', CONTROL, control)" @lostpointercapture="up('control', CONTROL, control)" />
      <rect :x="PAD" :y="PAD" :width="SIZE - 2 * PAD" :height="SIZE - 2 * PAD" rx="6" :fill="`url(#grid-${uid})`" pointer-events="none" />
      <line :x1="PAD" :x2="SIZE - PAD" :y1="SIZE / 2" :y2="SIZE / 2" class="axis" /><line :x1="SIZE / 2" :x2="SIZE / 2" :y1="PAD" :y2="SIZE - PAD" class="axis" />
      <template v-if="!compact">
        <text :x="SIZE - PAD - 2" :y="SIZE / 2 - 4" class="tick" text-anchor="end">x 1</text><text :x="PAD + 2" :y="SIZE / 2 - 4" class="tick">x −1</text>
        <text :x="SIZE / 2 + 4" :y="PAD + 8" class="tick">y 1</text><text :x="SIZE / 2 + 4" :y="SIZE - PAD - 3" class="tick">y −1</text>
      </template>
      <circle v-if="controlPixels" :cx="controlPixels.x" :cy="controlPixels.y" :r="focusRadius" class="focus-ring" pointer-events="none" />
      <g v-for="h in handles" :key="h.key" class="source" :class="[h.kind, { missing: h.missing, locked: h.locked, active: dragging === h.key }]">
        <circle :cx="h.pixels.x" :cy="h.pixels.y" :r="h.radius + 6" class="halo" :style="{ opacity: 0.15 + 0.6 * Math.min(1, h.weight) }" pointer-events="none" :filter="`url(#glow-${uid})`" />
        <circle :cx="h.pixels.x" :cy="h.pixels.y" :r="h.radius" class="handle nodrag nopan" :tabindex="compact ? -1 : 0" role="slider" :aria-label="`${h.label} source handle`" :aria-valuenow="h.point.x" :aria-valuetext="describePoint(h.point)" :aria-disabled="h.locked || !editable" :style="{ pointerEvents: compact ? 'none' : undefined }" @pointerdown="down($event, h.key, h.keys, h.point, h.locked)" @pointermove="move($event, h.key, h.keys, h.point)" @pointerup="up(h.key, h.keys, h.point)" @pointercancel="up(h.key, h.keys, h.point)" @lostpointercapture="up(h.key, h.keys, h.point)" @keydown="keys($event, h.keys, h.point, h.locked)"><title>{{ h.label }} · {{ describePoint(h.point) }}{{ h.missing ? ' · sample missing' : '' }}</title></circle>
        <text v-if="!compact" :x="h.pixels.x" :y="h.pixels.y + h.radius + 9" class="label" text-anchor="middle">{{ h.label }}</text>
      </g>
      <g v-if="controlPixels" class="control-point" :class="{ locked: controlLocked, active: dragging === 'control' }" :transform="`translate(${controlPixels.x.toFixed(1)} ${controlPixels.y.toFixed(1)})`">
        <line x1="-11" x2="11" y1="0" y2="0" pointer-events="none" /><line x1="0" x2="0" y1="-11" y2="11" pointer-events="none" />
        <circle r="6" class="handle nodrag nopan" :tabindex="compact && !editable ? -1 : 0" role="slider" aria-label="Field control point handle" :aria-valuenow="control!.x" :aria-valuetext="describePoint(control!)" :aria-disabled="controlLocked || !editable" @pointerdown="down($event, 'control', CONTROL, control, controlLocked)" @pointermove="move($event, 'control', CONTROL, control)" @pointerup="up('control', CONTROL, control)" @pointercancel="up('control', CONTROL, control)" @lostpointercapture="up('control', CONTROL, control)" @keydown="keys($event, CONTROL, control, controlLocked)"><title>Control point · {{ describePoint(control!) }}</title></circle>
      </g>
      <g v-else class="control-point waiting" :transform="`translate(${SIZE / 2} ${SIZE / 2})`" aria-hidden="true"><circle r="6" /><text y="20" text-anchor="middle" class="tick">waiting for cabled x/y</text></g>
      <g v-if="preview" class="preview" aria-hidden="true" :transform="`translate(${preview.x.toFixed(1)} ${preview.y.toFixed(1)})`">
        <rect :x="-preview.width / 2" :y="-preview.height" :width="preview.width" :height="preview.height" rx="4" /><text x="0" :y="-preview.height / 2" text-anchor="middle" dominant-baseline="central">{{ preview.text }}</text>
      </g>
    </svg>
  </div>
</template>
<style scoped>
.field-pad-wrap{display:block;width:100%}
.field-pad{display:block;width:100%;height:auto;aspect-ratio:1;border-radius:12px;box-shadow:0 0 18px #ff9a3a26,0 0 0 1px color-mix(in srgb,var(--amber) 30%,transparent);touch-action:none;user-select:none;overflow:visible}
.panel{fill:color-mix(in srgb,var(--amber) 8%,var(--shade-deep))}.editable .panel{cursor:crosshair}
.axis{stroke:var(--well-ink-muted);stroke-opacity:.45;stroke-width:1}
.tick{fill:var(--well-ink-muted);font-size:6.5px;font-family:monospace;pointer-events:none}
.focus-ring{fill:none;stroke:var(--well-control-ink);stroke-opacity:.35;stroke-dasharray:3 4;stroke-width:1}
.source .halo{fill:var(--amber)}.source.live .halo{fill:var(--cyan)}
.source .handle{fill:color-mix(in srgb,var(--amber) 35%,var(--shade-deep));stroke:var(--amber);stroke-width:1.6;cursor:grab}
.source.live .handle{fill:color-mix(in srgb,var(--cyan) 35%,var(--shade-deep));stroke:var(--cyan)}
.source.missing .handle{stroke:var(--red);stroke-dasharray:2 2}
.source.locked .handle{stroke:#7a6248;cursor:not-allowed}.source .handle:hover,.source.active .handle{cursor:grabbing}
.source .label{fill:var(--well-ink-muted);font-size:6.5px;pointer-events:none}
.control-point line{stroke:var(--well-control-ink);stroke-width:1.2}
.control-point .handle{fill:var(--well-control-ink);stroke:var(--shade-deep);stroke-width:1.5;cursor:grab}.control-point.active .handle{cursor:grabbing}
.control-point.locked .handle{fill:#7a6248;cursor:not-allowed}.control-point.waiting circle{fill:none;stroke:var(--well-ink-muted);stroke-dasharray:2 2}
.handle:focus-visible{outline:none;stroke:#e7f5f1;stroke-width:3}
.live .source .halo{transition:r 120ms linear,opacity 120ms linear}
.preview{pointer-events:none}.preview rect{fill:var(--shade-deep);stroke:#ffb35c;stroke-width:.8}.preview text{fill:var(--well-control-ink);font-size:7.5px;font-family:monospace;font-variant-numeric:tabular-nums}
</style>
