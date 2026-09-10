<script setup lang="ts">
import { computed, ref } from 'vue'
import type { AutomationLane, Part } from '../types'
import { scoreX, scoreBeat, staves, type ScoreAnchor } from '../score'
import {
  addLane,
  dynamicName,
  laneLabel,
  laneMax,
  nodesFromEvents,
  rampMessages,
  removeLane,
  removeNode,
  setNode,
  staffDynamicsEvents,
  updateLane,
  velocityLine,
  writeRamp,
  type LaneSettings,
  type RampNode,
} from '../scoreRamps'
const props = defineProps<{
  part: Part
  anchors: ScoreAnchor[]
  editable: boolean
  performance?: boolean
  origin: number
  scale: number
  width: number
  beat?: number
}>()
const emit = defineEmits<{ update: [part: Part] }>()
const height = 120,
  pad = 10
const xAt = (beat: number) => scoreX(beat, props.anchors, props.scale, props.origin)
const beatAt = (x: number) =>
  Math.max(0, Math.round(scoreBeat(x, props.anchors, props.scale, props.origin) * 4) / 4)
const palette = ['#7054a5', '#c2410c', '#0f766e', '#b45309', '#1d4ed8', '#9d174d']
interface Line {
  id: string
  name: string
  color: string
  max: number
  nodes: RampNode[]
}
const velocityColors = ['#087f8c', '#0f766e', '#1d4ed8', '#4338ca', '#0e7490', '#15803d', '#047857', '#1e40af']
const lines = computed<Line[]>(() => {
  const list = staves(props.part)
  return [
    ...list.map((s, i) => ({
      id: velocityLine(s.id),
      name: list.length > 1 ? `Velocity · ${s.name}` : 'Velocity',
      color: velocityColors[i % velocityColors.length]!,
      max: 127,
      nodes: nodesFromEvents(staffDynamicsEvents(props.part, s)),
    })),
    ...(props.part.automation || [])
      .filter((l) => (rampMessages as readonly string[]).includes(l.message))
      .map((l, i) => ({
        id: l.id,
        name: laneLabel(l),
        color: palette[i % palette.length]!,
        max: laneMax(l),
        nodes: nodesFromEvents(l.events),
      })),
  ]
})
const firstVelocity = computed(() => velocityLine(staves(props.part)[0]?.id ?? ''))
const open = ref(false),
  visible = ref(new Set<string>([velocityLine(staves(props.part)[0]?.id ?? '')])),
  active = ref(velocityLine(staves(props.part)[0]?.id ?? '')),
  selected = ref<{ line: string; beat: number } | null>(null)
/** Inline lane settings: `null` closed, `'new'` for a lane being added, else the lane id. */
const laneForm = ref<'new' | string | null>(null)
const laneDraft = ref<LaneSettings>({ name: 'Expression', channel: 1, message: 'cc', number: 11 })
const activeLane = computed<AutomationLane | undefined>(() =>
  (props.part.automation || []).find((l) => l.id === active.value),
)
function openLaneForm(id: 'new' | string) {
  const lane = (props.part.automation || []).find((l) => l.id === id)
  laneDraft.value = lane
    ? { name: lane.name, channel: lane.channel, message: lane.message, number: lane.number }
    : { name: 'Expression', channel: props.part.midi_channel || 1, message: 'cc', number: 11 }
  laneForm.value = id
}
function saveLane() {
  const id = laneForm.value
  if (!id || !props.editable) return
  const settings = { ...laneDraft.value, number: Math.max(0, Math.min(127, Number(laneDraft.value.number) || 0)), channel: Math.max(1, Math.min(16, Number(laneDraft.value.channel) || 1)) }
  if (id === 'new') {
    const next = addLane(props.part, settings)
    const created = next.automation!.at(-1)!.id
    emit('update', next)
    visible.value = new Set([...visible.value, created])
    active.value = created
  } else emit('update', updateLane(props.part, id, settings))
  laneForm.value = null
}
function deleteLane() {
  const id = laneForm.value
  if (!id || id === 'new' || !props.editable) return
  emit('update', removeLane(props.part, id))
  const set = new Set(visible.value)
  set.delete(id)
  visible.value = set
  if (active.value === id) active.value = firstVelocity.value
  laneForm.value = null
}
const svg = ref<SVGSVGElement>()
let drag: { line: Line; from: RampNode; node: RampNode; moved: boolean; pointer: number } | null =
  null
const preview = ref<RampNode | null>(null)
const yOf = (value: number, max: number) => pad + (1 - value / max) * (height - 2 * pad)
const valueAt = (y: number, max: number) =>
  Math.max(0, Math.min(max, Math.round((1 - (y - pad) / (height - 2 * pad)) * max)))
function toggleVisible(id: string) {
  const set = new Set(visible.value)
  if (set.has(id)) {
    set.delete(id)
    if (active.value === id) active.value = [...set][0] ?? firstVelocity.value
  } else {
    set.add(id)
    active.value = id
  }
  visible.value = set
}
function shownNodes(line: Line) {
  const d = drag
  if (d && d.line.id === line.id && preview.value)
    return setNode(removeNode(line.nodes, d.from.beat), preview.value.beat, preview.value.value)
  return line.nodes
}
function path(line: Line) {
  const nodes = shownNodes(line)
  if (!nodes.length) return ''
  const points = nodes.map((n) => `${xAt(n.beat)},${yOf(n.value, line.max)}`)
  // Hold the last value to the end of the visible width.
  return `M${xAt(0)},${yOf(nodes[0]!.value, line.max)} L${points.join(' L')} L${props.width},${yOf(nodes.at(-1)!.value, line.max)}`
}
function localPoint(event: PointerEvent) {
  const rect = svg.value!.getBoundingClientRect()
  return { x: event.clientX - rect.left, y: event.clientY - rect.top }
}
function commit(line: Line, nodes: RampNode[]) {
  emit('update', writeRamp(props.part, line.id, nodes))
}
function pointerDown(event: PointerEvent) {
  if (!props.editable || event.button !== 0 || !(event.target instanceof Element)) return
  svg.value?.focus({ preventScroll: true })
  const hit = event.target.closest<SVGElement>('[data-node-beat]')
  if (hit) {
    const line = lines.value.find((l) => l.id === hit.dataset.nodeLine)
    const beat = Number(hit.dataset.nodeBeat)
    const node = line?.nodes.find((n) => Math.abs(n.beat - beat) < 1e-9)
    if (!line || !node) return
    selected.value = { line: line.id, beat }
    active.value = line.id
    drag = { line, from: node, node, moved: false, pointer: event.pointerId }
    svg.value?.setPointerCapture(event.pointerId)
    event.preventDefault()
    return
  }
  const line = lines.value.find((l) => l.id === active.value)
  if (!line || !visible.value.has(line.id)) return
  const { x, y } = localPoint(event)
  const node = { beat: beatAt(x), value: valueAt(y, line.max) }
  commit(line, setNode(line.nodes, node.beat, node.value))
  selected.value = { line: line.id, beat: node.beat }
  event.preventDefault()
}
function pointerMove(event: PointerEvent) {
  if (!drag) return
  const { x, y } = localPoint(event)
  preview.value = { beat: beatAt(x), value: valueAt(y, drag.line.max) }
  drag.moved = true
}
function pointerUp(event: PointerEvent) {
  const d = drag
  drag = null
  if (svg.value?.hasPointerCapture(event.pointerId)) svg.value.releasePointerCapture(event.pointerId)
  if (!d) return
  const target = preview.value
  preview.value = null
  if (!d.moved || !target) return
  const others = removeNode(d.line.nodes, d.from.beat)
  commit(d.line, setNode(others, target.beat, target.value))
  selected.value = { line: d.line.id, beat: target.beat }
}
function nudge(delta: number) {
  const s = selected.value
  const line = lines.value.find((l) => l.id === s?.line)
  const node = line?.nodes.find((n) => Math.abs(n.beat - s!.beat) < 1e-9)
  if (!line || !node) return
  commit(line, setNode(line.nodes, node.beat, Math.max(0, Math.min(line.max, node.value + delta))))
}
function keydown(event: KeyboardEvent) {
  if (!props.editable) return
  const s = selected.value
  if (event.key === 'Escape' && s) {
    selected.value = null
    event.stopPropagation()
    return
  }
  if (!s) return
  if (event.key === 'Backspace' || event.key === 'Delete') {
    const line = lines.value.find((l) => l.id === s.line)
    if (line) commit(line, removeNode(line.nodes, s.beat))
    selected.value = null
    event.preventDefault()
    event.stopPropagation()
  } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
    nudge((event.key === 'ArrowUp' ? 1 : -1) * (event.shiftKey ? 10 : 1))
    event.preventDefault()
    event.stopPropagation()
  }
}
const activeLine = computed(() => lines.value.find((l) => l.id === active.value))
const selectedValue = computed(() => {
  const s = selected.value
  const line = lines.value.find((l) => l.id === s?.line)
  return line?.nodes.find((n) => Math.abs(n.beat - s!.beat) < 1e-9)
})
</script>
<template>
  <div class="score-ramps" :class="{ open }">
    <div class="ramp-header">
      <button
        type="button"
        class="ramp-toggle"
        :aria-expanded="open"
        @click="open = !open"
      >
        {{ open ? '▾' : '▸' }} Dynamics & ramps
        <small v-if="!open && lines.some((l) => l.nodes.length)"
          >· {{ lines.reduce((n, l) => n + l.nodes.length, 0) }} point(s)</small
        >
      </button>
      <template v-if="open">
        <div class="ramp-lines" role="group" aria-label="Ramp lines">
          <button
            v-for="line in lines"
            :key="line.id"
            type="button"
            class="ramp-chip"
            :class="{ active: active === line.id && visible.has(line.id) }"
            :style="{ '--line': line.color }"
            role="checkbox"
            :aria-checked="visible.has(line.id)"
            :aria-label="`Show ${line.name}`"
            :title="visible.has(line.id) ? `Hide ${line.name} (click again to edit)` : `Show ${line.name}`"
            @click="
              visible.has(line.id) && active !== line.id
                ? (active = line.id)
                : toggleVisible(line.id)
            "
          >
            <i aria-hidden="true"></i>{{ line.name }}
          </button>
        </div>
        <button
          v-if="editable"
          type="button"
          class="ramp-chip"
          aria-label="Add MIDI lane"
          title="Add a continuous MIDI lane (CC, bend, pressure, program)"
          @click="openLaneForm('new')"
        >
          ＋ Lane
        </button>
        <button
          v-if="editable && activeLane"
          type="button"
          class="ramp-chip"
          :aria-label="`Lane settings: ${activeLane.name}`"
          title="Edit or delete this lane"
          @click="openLaneForm(activeLane.id)"
        >
          ⚙ {{ activeLane.name }}
        </button>
        <form v-if="laneForm" class="lane-form" @submit.prevent="saveLane">
          <label
            >Name<input v-model="laneDraft.name" maxlength="120" aria-label="Lane name"
          /></label>
          <label
            >Message<select v-model="laneDraft.message" aria-label="Message">
              <option value="cc">Control change</option>
              <option value="bend">Pitch bend</option>
              <option value="pressure">Channel pressure</option>
              <option value="poly_pressure">Poly pressure</option>
              <option value="program">Program change</option>
            </select></label
          >
          <label v-if="['cc', 'poly_pressure'].includes(laneDraft.message)"
            >Number<input
              v-model.number="laneDraft.number"
              type="number"
              min="0"
              max="127"
              aria-label="Controller or note number"
          /></label>
          <label
            >Channel<input
              v-model.number="laneDraft.channel"
              type="number"
              min="1"
              max="16"
              aria-label="Lane channel"
          /></label>
          <button type="submit">{{ laneForm === 'new' ? 'Add lane' : 'Save lane' }}</button>
          <button
            v-if="laneForm !== 'new'"
            type="button"
            class="danger"
            @click="deleteLane"
          >
            Delete lane
          </button>
          <button type="button" @click="laneForm = null">Cancel</button>
        </form>
        <small v-if="!laneForm" class="ramp-hint">{{
          selectedValue
            ? `Selected ${activeLine?.name}: beat ${selected!.beat + 1} · ${selectedValue.value}${dynamicName(selectedValue.value) ? ' (' + dynamicName(selectedValue.value) + ')' : ''} · ↑↓ nudge · Backspace deletes`
            : editable
              ? `Editing ${activeLine?.name || 'velocity'} · click the graph to add a point · drag points to move · click a chip to edit that line`
              : 'Velocity and continuous MIDI ramps'
        }}</small>
      </template>
    </div>
    <svg
      v-if="open"
      ref="svg"
      class="ramp-graph"
      :width="width"
      :height="height"
      tabindex="0"
      role="application"
      aria-label="Dynamics and ramp graph"
      @pointerdown="pointerDown"
      @pointermove="pointerMove"
      @pointerup="pointerUp"
      @pointercancel="pointerUp"
      @keydown="keydown"
    >
      <line
        v-for="f in [0.25, 0.5, 0.75]"
        :key="f"
        :x1="0"
        :x2="width"
        :y1="pad + f * (height - 2 * pad)"
        :y2="pad + f * (height - 2 * pad)"
        class="ramp-grid"
      />
      <line :x1="0" :x2="width" :y1="pad" :y2="pad" class="ramp-grid strong" />
      <line
        :x1="0"
        :x2="width"
        :y1="height - pad"
        :y2="height - pad"
        class="ramp-grid strong"
      />
      <template v-for="line in lines" :key="line.id">
        <g v-if="visible.has(line.id)" :class="{ inactive: active !== line.id }">
          <path :d="path(line)" fill="none" :stroke="line.color" stroke-width="2" />
          <g
            v-for="n in shownNodes(line)"
            :key="`${line.id}:${n.beat}`"
            :data-node-line="line.id"
            :data-node-beat="n.beat"
            class="ramp-node"
            :class="{
              selected: selected?.line === line.id && Math.abs(selected.beat - n.beat) < 1e-9,
            }"
            role="button"
            :aria-label="`${line.name} ${n.value}${dynamicName(n.value) && line.id.startsWith('velocity:') ? ' ' + dynamicName(n.value) : ''} at beat ${n.beat + 1}`"
            :transform="`translate(${xAt(n.beat)} ${yOf(n.value, line.max)})`"
          >
            <circle r="9" class="hit" />
            <circle r="4.5" :fill="line.color" />
            <text
              v-if="line.id.startsWith('velocity:') && dynamicName(n.value)"
              y="-9"
              text-anchor="middle"
              class="dynamic-label"
              >{{ dynamicName(n.value) }}</text
            >
          </g>
        </g>
      </template>
      <line
        v-if="beat != null"
        :x1="xAt(beat)"
        :x2="xAt(beat)"
        :y1="0"
        :y2="height"
        class="ramp-playhead"
      />
    </svg>
  </div>
</template>
<style scoped>
.score-ramps {
  background: #fff;
  border-top: 1px solid #edf0f0;
}
.ramp-header {
  position: sticky;
  left: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 2px 8px;
  width: max-content;
  max-width: 100%;
}
.ramp-toggle {
  min-height: 24px;
  padding: 2px 6px;
  border: 1px solid #d9e0e1;
  border-radius: 4px;
  background: #fff;
  color: #087f8c;
  font-size: 11px;
  cursor: pointer;
}
.ramp-toggle small {
  color: #526267;
}
.ramp-lines {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.ramp-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-height: 22px;
  padding: 1px 8px;
  border: 1px solid #d9e0e1;
  border-radius: 11px;
  background: #fff;
  color: #526267;
  font-size: 10px;
  cursor: pointer;
}
.ramp-chip i {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  border: 2px solid var(--line);
  background: transparent;
}
.ramp-chip[aria-checked='true'] {
  color: #111;
}
.ramp-chip[aria-checked='true'] i {
  background: var(--line);
}
.ramp-chip.active {
  border-color: var(--line);
  box-shadow: inset 0 0 0 1px var(--line);
}
.ramp-hint {
  font-size: 10px;
  color: #526267;
}
.lane-form {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 8px;
  padding: 4px 0;
  font-size: 10px;
}
.lane-form label {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.lane-form input,
.lane-form select {
  min-height: 26px;
  padding: 2px 6px;
  font-size: 11px;
  width: 110px;
}
.lane-form input[type='number'] {
  width: 64px;
}
.lane-form button {
  min-height: 26px;
  padding: 0 10px;
  border: 1px solid #cbd5d7;
  border-radius: 4px;
  background: #fff;
  color: #111;
  font-size: 11px;
  cursor: pointer;
}
.lane-form button.danger {
  color: #9c2d16;
}
.ramp-graph {
  display: block;
  outline: none;
  touch-action: none;
}
.ramp-graph:focus-visible {
  box-shadow: inset 0 0 0 2px #087f8c66;
}
.ramp-grid {
  stroke: #e2e6e6;
}
.ramp-grid.strong {
  stroke: #cbd5d7;
}
.inactive {
  opacity: 0.45;
}
.ramp-node {
  cursor: pointer;
}
.ramp-node .hit {
  fill: transparent;
}
.ramp-node:hover circle:not(.hit) {
  fill: #e87816;
}
.ramp-node.selected circle:not(.hit) {
  fill: #16803c;
  stroke: #16803c;
  stroke-width: 3;
}
.dynamic-label {
  font: italic 700 11px serif;
  fill: #111;
}
.ramp-playhead {
  stroke: #16803c;
  stroke-width: 2;
  pointer-events: none;
}
</style>
