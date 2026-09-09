<script setup lang="ts">
import { computed, ref } from 'vue'
import type { AutomationEvent, AutomationLane, MidiCurve, Part } from '../types'
import { newId } from '../id'
import { scoreX, scoreBeat, type ScoreAnchor } from '../score'
const props = defineProps<{
  part: Part
  scale: number
  anchors: ScoreAnchor[]
  origin: number
  width: number
  editable: boolean
  performance?: boolean
}>()
const xAt = (beat: number) =>
  scoreX(beat, props.anchors, props.scale, props.origin)
const beatAt = (x: number) =>
  scoreBeat(x, props.anchors, props.scale, props.origin)
const emit = defineEmits<{ update: [part: Part] }>()
const expanded = ref(!props.performance),
  mode = ref<'point' | 'ramp'>('point'),
  selected = ref(''),
  laneId = ref(''),
  curve = ref<MidiCurve>('linear')
const lanes = computed(() => props.part.automation || [])
const selectedLane = computed(() =>
  lanes.value.find((l) => l.id === laneId.value),
)
const event = computed(() =>
  selectedLane.value?.events.find((e) => e.id === selected.value),
)
function maximum(l: AutomationLane) {
  return l.message === 'bend' ? 16383 : 127
}
function shape(c: MidiCurve, t: number) {
  return c === 'step'
    ? t < 1
      ? 0
      : 1
    : c === 'ease_in'
      ? t * t
      : c === 'ease_out'
        ? 1 - (1 - t) ** 2
        : c === 's_curve'
          ? t * t * (3 - 2 * t)
          : t
}
function path(e: AutomationEvent, l: AutomationLane) {
  const x = xAt(e.beat),
    w = Math.max(8, xAt(e.beat + e.duration) - xAt(e.beat))
  let p = `M ${x} 96`
  for (let i = 0; i <= 32; i++) {
    const t = i / 32,
      value = e.start + (e.end - e.start) * shape(e.curve, t)
    p += ` L ${xAt(e.beat + t * e.duration)} ${96 - (value / maximum(l)) * 80}`
  }
  return `${p} L ${x + w} 96 Z`
}
function update(lane: AutomationLane) {
  emit('update', {
    ...props.part,
    automation: lanes.value.map((l) => (l.id === lane.id ? lane : l)),
  })
}
function addLane() {
  let number = 11
  while (
    lanes.value.some(
      (l) =>
        l.message === 'cc' &&
        l.number === number &&
        l.channel === (props.part.midi_channel || 1),
    )
  )
    number = (number + 1) % 128
  const lane: AutomationLane = {
    id: newId(),
    name: `CC ${number}`,
    channel: props.part.midi_channel || 1,
    message: 'cc',
    number,
    events: [],
  }
  emit('update', { ...props.part, automation: [...lanes.value, lane] })
  laneId.value = lane.id
  expanded.value = true
}
function add(event: MouseEvent, lane: AutomationLane) {
  if (
    !props.editable ||
    (event.target instanceof Element &&
      event.target.closest('[data-midi-event]'))
  )
    return
  const box = (event.currentTarget as SVGElement).getBoundingClientRect(),
    beat = Math.max(0, Math.round(beatAt(event.clientX - box.left) * 4) / 4),
    value = Math.round(
      Math.max(0, Math.min(1, (96 - (event.clientY - box.top)) / 80)) *
        maximum(lane),
    )
  const duration =
    lane.message === 'note'
      ? 1
      : mode.value === 'ramp' && lane.message !== 'program'
        ? 1
        : 0
  const e: AutomationEvent = {
    id: newId(),
    beat,
    duration,
    start: mode.value === 'ramp' && lane.message !== 'note' ? 0 : value,
    end: value,
    curve: curve.value,
  }
  selected.value = e.id
  laneId.value = lane.id
  update({
    ...lane,
    events: [...lane.events, e].sort((a, b) => a.beat - b.beat),
  })
}
function edit(patch: Partial<AutomationEvent>) {
  if (!event.value || !selectedLane.value || !props.editable) return
  update({
    ...selectedLane.value,
    events: selectedLane.value.events
      .map((e) => (e.id === selected.value ? { ...e, ...patch } : e))
      .sort((a, b) => a.beat - b.beat),
  })
}
function keydown(e: KeyboardEvent) {
  if (
    e.target instanceof Element &&
    e.target.closest('input,textarea,select,[contenteditable="true"]')
  )
    return
  if (e.key === 'Delete' || e.key === 'Backspace') {
    e.preventDefault()
    e.stopPropagation()
    remove()
  }
}
function remove() {
  if (!selectedLane.value || !props.editable) return
  update({
    ...selectedLane.value,
    events: selectedLane.value.events.filter((e) => e.id !== selected.value),
  })
  selected.value = ''
}
const drag = ref<{
  lane: AutomationLane
  event: AutomationEvent
  edge: 'start' | 'end'
  x: number
  y: number
  pointer: number
} | null>(null)
const preview = ref<AutomationEvent | null>(null)
function down(
  e: PointerEvent,
  lane: AutomationLane,
  item: AutomationEvent,
  edge: 'start' | 'end',
) {
  if (!props.editable) return
  e.preventDefault()
  e.stopPropagation()
  selected.value = item.id
  laneId.value = lane.id
  drag.value = {
    lane,
    event: item,
    edge,
    x: e.clientX,
    y: e.clientY,
    pointer: e.pointerId,
  }
  preview.value = { ...item }
  ;(e.currentTarget as Element).setPointerCapture(e.pointerId)
}
function move(e: PointerEvent) {
  const d = drag.value
  if (!d) return
  const value = Math.round(
      (d.edge === 'start' ? d.event.start : d.event.end) -
        ((e.clientY - d.y) / 80) * maximum(d.lane),
    ),
    delta =
      Math.round(
        (beatAt(xAt(d.event.beat) + (e.clientX - d.x)) - d.event.beat) * 4,
      ) / 4
  preview.value =
    d.edge === 'end'
      ? {
          ...d.event,
          end: Math.max(0, Math.min(maximum(d.lane), value)),
          duration: Math.max(0, d.event.duration + delta),
        }
      : {
          ...d.event,
          start: Math.max(0, Math.min(maximum(d.lane), value)),
          beat: Math.max(0, d.event.beat + delta),
        }
}
function up() {
  if (drag.value && preview.value) {
    const d = drag.value
    update({
      ...d.lane,
      events: d.lane.events
        .map((e) => (e.id === d.event.id ? preview.value! : e))
        .sort((a, b) => a.beat - b.beat),
    })
  }
  drag.value = null
  preview.value = null
}
function shown(e: AutomationEvent) {
  return preview.value?.id === e.id ? preview.value : e
}
</script>
<template>
  <section class="midi-lanes" @keydown="keydown">
    <header class="midi-header">
      <button :aria-expanded="expanded" @click="expanded = !expanded">
        {{ expanded ? '▾' : '▸' }} MIDI lanes · {{ lanes.length }}</button
      ><template v-if="!performance"
        ><button :disabled="!editable || lanes.length >= 32" @click="addLane">
          ＋ MIDI lane</button
        ><select v-model="mode" aria-label="MIDI drawing tool">
          <option value="point">Point / note</option>
          <option value="ramp">Ramp</option></select
        ><select v-model="curve" aria-label="New MIDI curve">
          <option
            v-for="c in ['step', 'linear', 'ease_in', 'ease_out', 's_curve']"
            :key="c"
          >
            {{ c }}
          </option></select
        ><small>Connect Part MIDI “events” to the graph</small></template
      >
    </header>
    <template v-if="expanded"
      ><div v-for="lane in lanes" :key="lane.id" class="midi-lane">
        <div class="midi-label">
          <button
            @click="
              laneId = lane.id;
              selected = ''
            "
          >
            {{ lane.name }}</button
          ><small
            >CH {{ lane.channel }} · {{ lane.message }}
            {{
              ['cc', 'note', 'poly_pressure'].includes(lane.message)
                ? lane.number
                : ''
            }}</small
          >
        </div>
        <svg
          :width="width"
          height="105"
          tabindex="0"
          :aria-label="`${part.name} ${lane.name} MIDI lane`"
          @click="add($event, lane)"
          @pointermove="move"
          @pointerup="up"
          @pointercancel="
            drag = null; preview = null
          "
        >
          <path
            :d="`M ${origin} 16 H ${width} M ${origin} 96 H ${width}`"
            stroke="#465254"
          />
          <g
            v-for="item in lane.events"
            :key="item.id"
            :data-midi-event="item.id"
            :class="{ selected: selected === item.id }"
            @click.stop="
              selected = item.id; laneId = lane.id
            "
          >
            <rect
              v-if="lane.message === 'note'"
              :x="xAt(shown(item).beat)"
              y="40"
              :width="
                Math.max(
                  8,
                  xAt(shown(item).beat + shown(item).duration) -
                    xAt(shown(item).beat),
                )
              "
              height="24"
              rx="4"
              class="event-note"
            />
            <path
              v-else-if="item.duration > 0"
              :d="path(shown(item), lane)"
              class="event-ramp"
            />
            <path
              v-else
              :d="`M ${xAt(item.beat)} 96 V ${96 - (item.end / maximum(lane)) * 80}`"
              class="event-point"
            />
            <circle
              :cx="xAt(shown(item).beat)"
              :cy="
                lane.message === 'note'
                  ? 52
                  : 96 - (shown(item).start / maximum(lane)) * 80
              "
              r="6"
              @pointerdown="down($event, lane, item, 'start')"
            />
            <circle
              v-if="item.duration > 0"
              :cx="xAt(shown(item).beat + shown(item).duration)"
              :cy="
                lane.message === 'note'
                  ? 52
                  : 96 - (shown(item).end / maximum(lane)) * 80
              "
              r="6"
              @pointerdown="down($event, lane, item, 'end')"
            />
          </g>
        </svg>
      </div>
      <div
        v-if="selectedLane && !performance"
        class="midi-inspector"
        @pointerdown.stop
      >
        <label
          >Lane name<input
            :value="selectedLane.name"
            :disabled="!editable"
            @change="
              update({
                ...selectedLane!,
                name: ($event.target as HTMLInputElement).value,
              })
            " /></label
        ><label
          >Message<select
            :value="selectedLane.message"
            :disabled="!editable || selectedLane.events.length > 0"
            @change="
              update({
                ...selectedLane!,
                message: ($event.target as HTMLSelectElement)
                  .value as AutomationLane['message'],
              })
            "
          >
            <option
              v-for="m in [
                'note',
                'cc',
                'bend',
                'program',
                'pressure',
                'poly_pressure',
              ]"
              :key="m"
            >
              {{ m }}
            </option>
          </select></label
        ><label
          >Channel<input
            type="number"
            min="1"
            max="16"
            :value="selectedLane.channel"
            :disabled="!editable"
            @change="
              update({
                ...selectedLane!,
                channel: Number(($event.target as HTMLInputElement).value),
              })
            " /></label
        ><label
          >Controller / pitch<input
            type="number"
            min="0"
            max="127"
            :value="selectedLane.number"
            :disabled="!editable"
            @change="
              update({
                ...selectedLane!,
                number: Number(($event.target as HTMLInputElement).value),
              })
            "
        /></label>
        <label
          >Initial value<input
            type="number"
            min="0"
            :max="maximum(selectedLane)"
            :value="
              selectedLane.initial ??
              (selectedLane.message === 'bend' ? 8192 : 0)
            "
            :disabled="!editable"
            @change="
              update({
                ...selectedLane!,
                initial: Number(($event.target as HTMLInputElement).value),
              })
            " /></label
        ><template v-if="event"
          ><label
            v-for="field in ['beat', 'duration', 'start', 'end'] as const"
            :key="field"
            >{{ field
            }}<input
              type="number"
              min="0"
              step="any"
              :value="event[field]"
              :disabled="!editable"
              @change="
                edit({
                  [field]: Number(($event.target as HTMLInputElement).value),
                })
              " /></label
          ><label
            >Curve<select
              :value="event.curve"
              :disabled="!editable"
              @change="
                edit({
                  curve: ($event.target as HTMLSelectElement)
                    .value as MidiCurve,
                })
              "
            >
              <option
                v-for="c in [
                  'step',
                  'linear',
                  'ease_in',
                  'ease_out',
                  's_curve',
                ]"
                :key="c"
              >
                {{ c }}
              </option>
            </select></label
          ><button :disabled="!editable" @click="remove">
            Delete event
          </button></template
        >
        <button
          :disabled="!editable"
          @click="
            emit('update', {
              ...part,
              automation: lanes.filter((l) => l.id !== selectedLane!.id),
            })
          "
        >
          Delete lane
        </button>
      </div>
    </template>
  </section>
</template>
<style scoped>
.midi-lanes {
  border-top: 1px solid var(--line);
  background: #172022;
}
.midi-header {
  position: sticky;
  left: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  width: 900px;
  padding: 6px 12px;
}
.midi-header small {
  color: var(--muted);
  font-size: 10px;
}
.midi-lanes button {
  color: var(--violet);
  background: transparent;
  border: 1px solid var(--line);
  border-radius: 4px;
  padding: 6px;
  min-height: 32px;
}
.midi-label {
  position: sticky;
  left: 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  width: 300px;
  font-size: 10px;
  color: var(--muted);
  height: 24px;
}
.midi-label button {
  border: 0;
}
.midi-lane {
  border-bottom: 1px solid var(--line);
}
.event-ramp {
  fill: #aa8fe740;
  stroke: var(--violet);
}
.event-note {
  fill: #aa8fe780;
  stroke: var(--violet);
}
.event-point {
  stroke: var(--violet);
  stroke-width: 3;
}
.midi-lane circle {
  fill: var(--violet);
  cursor: grab;
}
.midi-lane .selected path,
.midi-lane .selected rect {
  stroke: var(--cyan);
}
.midi-lane .selected circle {
  fill: var(--cyan);
}
.midi-inspector {
  position: sticky;
  left: 0;
  width: 900px;
  display: flex;
  gap: 10px;
  align-items: end;
  flex-wrap: wrap;
  padding: 10px;
}
.midi-inspector label {
  font-size: 10px;
  color: var(--muted);
}
.midi-inspector input,
.midi-inspector select {
  display: block;
  width: 90px;
}
.midi-header select {
  width: auto;
  font-size: 11px;
}
</style>
