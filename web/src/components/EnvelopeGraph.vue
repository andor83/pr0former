<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { MAX_SCALE, autoScale, dragHandle, envelopePoints, fitScale, minScale, timeLabel, type Envelope, type Handle } from '../envelopeGeometry'
// The zoom chosen in one editor is remembered for the next one opened this session.
const rememberedZoom = ref<number | null>(null)
const props = defineProps<{ envelope: Envelope; level?: number; gate?: boolean; editable?: boolean; compact?: boolean; locked?: string[] }>()
const emit = defineEmits<{ change: [key: string, value: number] }>()
const W = 240, H = 120, PAD = 10
const uid = useId()
const svg = ref<SVGSVGElement>()
// Parameter edits reach the project after a server round trip, so a drag
// keeps its own draft for immediate feedback until the props catch up.
const draft = ref<Partial<Envelope>>({})
const envelope = computed<Envelope>(() => ({ ...props.envelope, ...draft.value }))
const dragging = ref<Handle | null>(null)
// The time axis is logarithmic per stage and the whole envelope is always
// visible; the plateau absorbs the leftover width. The zoom (log width across
// the graph) is sticky: a drag never changes it (drags are clamped so the
// plateau keeps a minimum width) and it freezes while a handle is held. It
// refits automatically only when the parameters change from outside this
// graph, unless the user set a zoom with the slider.
const zoom = ref<number | null>(props.editable ? rememberedZoom.value : null)
const scale = ref(zoom.value ?? autoScale(props.envelope))
const frozen = ref<number | null>(null)
const total = computed(() => frozen.value ?? fitScale(envelope.value, scale.value))
watch(() => props.envelope, (incoming, previous) => {
  let external = false
  for (const key of Object.keys(incoming) as (keyof Envelope)[]) {
    if (incoming[key] === previous?.[key]) continue
    if (draft.value[key] === incoming[key]) delete draft.value[key]
    else external = true
  }
  if (external && !dragging.value && zoom.value === null) scale.value = autoScale({ ...incoming, ...draft.value })
}, { deep: true })
const zoomMin = computed(() => minScale(envelope.value))
function setZoom(value: number) { const v = Math.max(zoomMin.value, Math.min(MAX_SCALE, value)); zoom.value = v; rememberedZoom.value = v; scale.value = v }
function autoZoom() { zoom.value = null; rememberedZoom.value = null; scale.value = autoScale(envelope.value) }
const toPixels = (p: { x: number; y: number }) => ({ x: PAD + p.x * (W - 2 * PAD), y: PAD + (1 - p.y) * (H - 2 * PAD) })
const points = computed(() => envelopePoints(envelope.value, total.value).map(toPixels))
const path = computed(() => points.value.map((p, i) => `${i ? 'L' : 'M'}${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join(' '))
const area = computed(() => `${path.value} L${(W - PAD).toFixed(1)} ${(H - PAD).toFixed(1)} L${PAD} ${(H - PAD).toFixed(1)} Z`)
// The release always ends at the right edge; its handle is the start of the
// release ramp, so dragging it left lengthens the release.
const handles = computed(() => ([
  { key: 'attack' as Handle, index: 1, params: ['attack'], title: 'Attack' },
  { key: 'decay' as Handle, index: 2, params: ['decay', 'sustain'], title: 'Decay and sustain' },
  { key: 'release' as Handle, index: 3, params: ['release', 'sustain'], title: 'Release and sustain (drag left to lengthen the release)' },
]).map(h => ({ ...h, point: points.value[h.index]!, locked: h.params.some(p => props.locked?.includes(p)) })))
// Stage durations along the bottom, since the axis is not linear.
const stageLabels = computed(() => {
  const e = envelope.value, p = points.value
  return [
    { key: 'attack', x: (p[0]!.x + p[1]!.x) / 2, text: timeLabel(e.attack) },
    { key: 'decay', x: (p[1]!.x + p[2]!.x) / 2, text: timeLabel(e.decay) },
    { key: 'release', x: (p[3]!.x + p[4]!.x) / 2, text: timeLabel(e.release) },
  ]
})
// Pointer position in SVG units while dragging, for the value preview.
const pointer = ref<{ x: number; y: number } | null>(null)
const preview = computed(() => {
  if (!dragging.value || !pointer.value) return null
  const e = envelope.value
  const text = dragging.value === 'attack' ? `Attack ${e.attack} ms`
    : dragging.value === 'decay' ? `Decay ${e.decay} ms · Sustain ${e.sustain.toFixed(2)}`
    : `Release ${e.release} ms · Sustain ${e.sustain.toFixed(2)}`
  const width = text.length * 4.4 + 10, height = 14
  const x = Math.min(W - width / 2 - 2, Math.max(width / 2 + 2, pointer.value.x))
  const above = pointer.value.y - 16
  const y = above - height < 1 ? pointer.value.y + 18 : above
  return { text, width, height, x, y }
})
const levelY = computed(() => props.level === undefined ? null : PAD + (1 - Math.min(1, Math.max(0, props.level))) * (H - 2 * PAD))
function unit(event: PointerEvent) {
  const rect = svg.value!.getBoundingClientRect()
  const x = (event.clientX - rect.left) / rect.width * W, y = (event.clientY - rect.top) / rect.height * H
  pointer.value = { x, y }
  return { x: (x - PAD) / (W - 2 * PAD), y: (y - PAD) / (H - 2 * PAD) }
}
function apply(handle: Handle, position: { x: number; y: number }) {
  const next = dragHandle(envelope.value, handle, position, total.value)
  for (const [key, value] of Object.entries(next) as [keyof Envelope, number][]) {
    if (props.locked?.includes(key) || value === envelope.value[key]) continue
    draft.value[key] = value
    emit('change', key, value)
  }
}
function down(event: PointerEvent, handle: Handle, locked: boolean) {
  if (!props.editable || locked || event.button !== 0) return
  event.preventDefault(); event.stopPropagation()
  ;(event.currentTarget as Element).setPointerCapture(event.pointerId)
  frozen.value = total.value; dragging.value = handle
  apply(handle, unit(event))
}
function move(event: PointerEvent, handle: Handle) { if (dragging.value === handle) apply(handle, unit(event)) }
function up(handle: Handle) { if (dragging.value === handle) { dragging.value = null; frozen.value = null; pointer.value = null } }
function keys(event: KeyboardEvent, handle: Handle, locked: boolean) {
  if (!props.editable || locked) return
  const factor = event.shiftKey ? 10 : 1
  const step = (key: keyof Envelope, delta: number) => {
    if (props.locked?.includes(key)) return
    const limit = key === 'sustain' ? 1 : 10000
    const raw = Math.min(limit, Math.max(0, envelope.value[key] + delta))
    const value = key === 'sustain' ? Math.round(raw * 100) / 100 : Math.round(raw)
    draft.value[key] = value
    emit('change', key, value)
  }
  // Arrows follow the handle on screen: right moves the point right, which
  // for the release handle means a shorter release. Time steps scale with the
  // value so they stay useful on the log axis.
  const sign = handle === 'release' ? -1 : 1
  const timeStep = Math.max(1, Math.round(envelope.value[handle] * 0.05)) * factor
  switch (event.key) {
    case 'ArrowRight': step(handle, sign * timeStep); break
    case 'ArrowLeft': step(handle, -sign * timeStep); break
    case 'ArrowUp': if (handle === 'attack') return; step('sustain', 0.01 * factor); break
    case 'ArrowDown': if (handle === 'attack') return; step('sustain', -0.01 * factor); break
    default: return
  }
  event.preventDefault(); event.stopPropagation()
}
</script>
<template>
  <div class="envelope-graph-wrap" :class="{ compact, editable }">
  <svg ref="svg" class="envelope-graph" :class="{ compact, editable }" :viewBox="`0 0 ${W} ${H}`" role="img" :aria-label="`ADSR envelope: attack ${envelope.attack} ms, decay ${envelope.decay} ms, sustain ${envelope.sustain}, release ${envelope.release} ms`" @dblclick.stop>
    <defs>
      <pattern :id="`grid-${uid}`" width="20" height="20" patternUnits="userSpaceOnUse"><path d="M20 0H0V20" fill="none" stroke="#f2953a" stroke-opacity=".2" stroke-width="1" /></pattern>
      <filter :id="`glow-${uid}`" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="2.4" result="blur" /><feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge></filter>
    </defs>
    <rect x="0" y="0" :width="W" :height="H" rx="12" class="panel" />
    <rect :x="PAD" :y="PAD" :width="W - 2 * PAD" :height="H - 2 * PAD" rx="6" :fill="`url(#grid-${uid})`" class="grid" />
    <path :d="area" class="area" />
    <path :d="path" class="line" :filter="`url(#glow-${uid})`" />
    <line v-if="levelY !== null" :x1="PAD" :x2="W - PAD" :y1="levelY" :y2="levelY" class="level" :class="{ gated: gate }" />
    <template v-if="editable">
      <text v-for="l in stageLabels" :key="l.key" :x="l.x.toFixed(1)" :y="H - PAD - 3" class="stage" text-anchor="middle">{{ l.text }}</text>
      <g v-if="preview" class="preview" aria-hidden="true" :transform="`translate(${preview.x.toFixed(1)} ${preview.y.toFixed(1)})`">
        <rect :x="-preview.width / 2" :y="-preview.height" :width="preview.width" :height="preview.height" rx="4" />
        <text x="0" :y="-preview.height / 2" text-anchor="middle" dominant-baseline="central">{{ preview.text }}</text>
      </g>
      <circle v-for="h in handles" :key="h.key" :cx="h.point.x" :cy="h.point.y" r="6" class="handle nodrag nopan" :class="{ locked: h.locked, active: dragging === h.key }" tabindex="0" role="slider" :aria-label="`${h.title} handle`" :aria-valuenow="envelope[h.key]" :aria-disabled="h.locked" :filter="`url(#glow-${uid})`" @pointerdown="down($event, h.key, h.locked)" @pointermove="move($event, h.key)" @pointerup="up(h.key)" @pointercancel="up(h.key)" @lostpointercapture="up(h.key)" @keydown="keys($event, h.key, h.locked)"><title>{{ h.title }}</title></circle>
    </template>
  </svg>
  <label v-if="editable" class="envelope-zoom nodrag nopan">
    <span>Zoom</span>
    <input type="range" :min="zoomMin" :max="MAX_SCALE" step="0.1" :value="total" aria-label="Time axis zoom" @input="setZoom(Number(($event.target as HTMLInputElement).value))" />
    <output>{{ zoom === null ? 'auto' : `${Math.round((MAX_SCALE - total) / (MAX_SCALE - zoomMin) * 100)}%` }}</output>
    <button type="button" class="text-button" :disabled="zoom === null" @click="autoZoom">Auto</button>
  </label>
  </div>
</template>
<style scoped>
.envelope-graph-wrap{display:block;width:100%}.envelope-graph-wrap.compact{height:100%}
.envelope-graph{display:block;width:100%;height:auto;aspect-ratio:2/1;border-radius:12px;box-shadow:0 0 18px #ff9a3a26,0 0 0 1px #3f2c16;touch-action:none;user-select:none;overflow:visible}
.panel{fill:#17120c}.grid{stroke:#5a3d1c;stroke-width:1}
.area{fill:#f2953a;fill-opacity:.12}.line{fill:none;stroke:#ffb35c;stroke-width:2.2;stroke-linejoin:round;stroke-linecap:round}
.compact .line{stroke-width:2.8}
.stage{fill:#f2953a;fill-opacity:.6;font-size:6.5px;font-family:monospace;pointer-events:none}
.envelope-zoom{display:flex;align-items:center;gap:10px;margin-top:8px;font-size:10px;color:var(--muted);letter-spacing:.6px}.envelope-zoom input{flex:1;min-width:0;accent-color:#f2953a;margin:0}.envelope-zoom output{font-family:monospace;color:#ffb35c;min-width:40px;text-align:right}.envelope-zoom .text-button{padding:4px 0}
.level{stroke:#ffd9a8;stroke-opacity:.5;stroke-width:1;stroke-dasharray:3 3}.level.gated{stroke:#fff1dc;stroke-opacity:.95}
.preview{pointer-events:none}.preview rect{fill:#17120c;fill-opacity:.94;stroke:#ffb35c;stroke-width:.8}.preview text{fill:#ffd9a8;font-size:7.5px;font-family:monospace;font-variant-numeric:tabular-nums}
.handle{fill:#1a130b;stroke:#ffb35c;stroke-width:2;cursor:grab}.handle:hover,.handle.active{fill:#ffb35c;cursor:grabbing}.handle.locked{stroke:#7a6248;cursor:not-allowed}.handle:focus-visible{outline:none;stroke:#e7f5f1;stroke-width:3}
</style>
