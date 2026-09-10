<script setup lang="ts">
import { ref } from 'vue'
import ScoreDialog from './ScoreDialog.vue'
import type { Part, Dynamics, AutomationEvent } from '../types'
import { newId } from '../id'
import { scoreX, scoreBeat, type ScoreAnchor } from '../score'
const props = defineProps<{
  part: Part
  anchors: ScoreAnchor[]
  editable: boolean
  error?:string
  performance?: boolean
  origin: number
  scale: number
  width: number
}>()
const xAt = (beat: number) =>
  scoreX(beat, props.anchors, props.scale, props.origin)
const emit = defineEmits<{ update: [part: Part] }>()
const selected = ref('')
let drag: { x: number; event: AutomationEvent } | null = null
function select(e: AutomationEvent, event: PointerEvent) {
  if (!props.editable) return
  selected.value = e.id
  ;(event.currentTarget as SVGElement).focus()
  drag = { x: event.clientX, event: e }
  ;(event.currentTarget as SVGElement).setPointerCapture(event.pointerId)
  event.preventDefault()
}
function drop(event: PointerEvent) {
  if (!drag) return
  const d = drag
  drag = null
  if (Math.abs(event.clientX - d.x) < 4) return
  const at = Math.max(
    0,
    Math.round(
      scoreBeat(
        xAt(d.event.beat) + event.clientX - d.x,
        props.anchors,
        props.scale,
        props.origin,
      ) * 4,
    ) / 4,
  )
  change({
    events: props.part
      .dynamics!.events.map((e) =>
        e.id === d.event.id ? { ...e, beat: at } : e,
      )
      .sort((a, b) => a.beat - b.beat),
  })
}
function removeSelected() {
  if (props.editable)
    change({
      events: props.part.dynamics!.events.filter(
        (e) => e.id !== selected.value,
      ),
    })
}
const beat = ref(0),
  duration = ref(4),
  from = ref(48),
  to = ref(96),
  open = ref(false)
const levels = [
  ['ppp', 16],
  ['pp', 32],
  ['p', 48],
  ['mp', 64],
  ['mf', 80],
  ['f', 96],
  ['ff', 112],
  ['fff', 127],
] as const
function change(patch: Partial<Dynamics>) {
  emit('update', {
    ...props.part,
    dynamics: {
      mode: 'velocity',
      controller: 11,
      events: [],
      ...props.part.dynamics,
      ...patch,
    },
  })
}
function add(start: number, end: number, length = 0) {
  const events = [
    ...(props.part.dynamics?.events || []),
    {
      id: newId(),
      beat: beat.value,
      duration: length,
      start,
      end,
      curve: 'linear' as const,
    },
  ].sort((a, b) => a.beat - b.beat)
  change({ events })
}
function label(value: number) {
  return levels.find((l) => l[1] === value)?.[0] || String(value)
}
</script>
<template>
  <div class="score-dynamics">
    <svg
      v-if="part.dynamics?.events.length"
      :width="width"
      height="42"
      aria-label="Written dynamics"
    >
      <g
        v-for="e in part.dynamics.events"
        :key="e.id"
        :class="{ selected: selected === e.id }"
        tabindex="0"
        role="button"
        :aria-label="`Dynamic ${label(e.start)} at beat ${e.beat + 1}`"
        @pointerdown.stop="select(e, $event)"
        @pointerup.stop="drop"
        @pointercancel="drag = null"
        @dblclick.stop="open = true"
        @keydown.delete.stop.prevent="removeSelected"
        @keydown.backspace.stop.prevent="removeSelected"
        @keydown.enter.stop.prevent="open = true"
        style="cursor: pointer"
      >
        <text :x="xAt(e.beat)" y="28" font-style="italic" font-weight="bold">
          {{ label(e.start) }}
        </text>
        <template v-if="e.duration > 0">
          <path
            :d="
              e.end >= e.start
                ? `M ${xAt(e.beat) + 26} 24 L ${xAt(e.beat + e.duration) - 8} 16 M ${xAt(e.beat) + 26} 24 L ${xAt(e.beat + e.duration) - 8} 32`
                : `M ${xAt(e.beat) + 26} 16 L ${xAt(e.beat + e.duration) - 8} 24 L ${xAt(e.beat) + 26} 32`
            "
            fill="none"
            stroke="currentColor"
          />
          <text :x="xAt(e.beat + e.duration)" y="28" font-style="italic">
            {{ label(e.end) }}
          </text>
        </template>
      </g>
    </svg>
    <template v-if="!performance"
      ><button :aria-expanded="open" @click="open = !open">
        {{ open ? '▾' : '▸' }} Dynamics
      </button>
      <ScoreDialog :error="error" v-if="open" title="Dynamics" @close="open = false"
        ><div class="dynamics-controls">
          <label
            >Output<select
              :value="part.dynamics?.mode || 'velocity'"
              :disabled="!editable"
              @change="
                change({
                  mode: ($event.target as HTMLSelectElement)
                    .value as Dynamics['mode'],
                })
              "
            >
              <option value="velocity">Note velocity</option>
              <option value="cc">MIDI CC</option>
              <option value="both">Velocity + CC</option>
            </select></label
          ><label
            >CC<input
              type="number"
              min="0"
              max="127"
              :value="part.dynamics?.controller ?? 11"
              :disabled="!editable"
              @change="
                change({
                  controller: Number(($event.target as HTMLInputElement).value),
                })
              " /></label
          ><label
            >At beat<input
              v-model.number="beat"
              type="number"
              min="0"
              step="0.25" /></label
          ><button
            v-for="[name, value] in levels"
            :key="name"
            :disabled="!editable"
            @click="add(value, value)"
          >
            {{ name }}</button
          ><label
            >Start<input
              v-model.number="from"
              type="number"
              min="0"
              max="127" /></label
          ><label
            >End<input
              v-model.number="to"
              type="number"
              min="0"
              max="127" /></label
          ><label
            >Length<input
              v-model.number="duration"
              type="number"
              min="0.0625"
              step="0.25" /></label
          ><button :disabled="!editable" @click="add(from, to, duration)">
            Add hairpin
          </button>
          <div v-for="e in part.dynamics?.events" :key="e.id">
            <label
              >Beat<input
                type="number"
                min="0"
                step="0.25"
                :value="e.beat"
                :disabled="!editable"
                @change="
                  change({
                    events: part
                      .dynamics!.events.map((x) =>
                        x.id === e.id
                          ? {
                              ...x,
                              beat: Number(
                                ($event.target as HTMLInputElement).value,
                              ),
                            }
                          : x,
                      )
                      .sort((a, b) => a.beat - b.beat),
                  })
                " /></label
            ><label
              >Duration<input
                type="number"
                min="0"
                step="0.25"
                :value="e.duration"
                :disabled="!editable"
                @change="
                  change({
                    events: part.dynamics!.events.map((x) =>
                      x.id === e.id
                        ? {
                            ...x,
                            duration: Number(
                              ($event.target as HTMLInputElement).value,
                            ),
                          }
                        : x,
                    ),
                  })
                " /></label
            ><label v-for="field in ['start', 'end'] as const" :key="field"
              >{{ field
              }}<input
                type="number"
                min="0"
                max="127"
                :value="e[field]"
                :disabled="!editable"
                @change="
                  change({
                    events: part.dynamics!.events.map((x) =>
                      x.id === e.id
                        ? {
                            ...x,
                            [field]: Number(
                              ($event.target as HTMLInputElement).value,
                            ),
                          }
                        : x,
                    ),
                  })
                "
            /></label>
            · {{ label(e.start)
            }}<span v-if="e.duration">
              → {{ label(e.end) }} over {{ e.duration }} beats</span
            ><button
              :disabled="!editable"
              @click="
                change({
                  events: part.dynamics!.events.filter((x) => x.id !== e.id),
                })
              "
            >
              Remove
            </button>
          </div>
        </div></ScoreDialog
      ></template
    >
  </div>
</template>
<style scoped>
.score-dynamics g:hover {color:#e87816;fill:#e87816}
.score-dynamics g.selected {color:#16803c;fill:#16803c}
.score-dynamics {
  color: var(--amber);
  padding: 4px 0;
}
.score-dynamics > svg {
  display: block;
  background: #ffffff;
  color: #000000;
}
.score-dynamics > button,
.dynamics-controls {
  position: sticky;
  left: 12px;
}
.score-dynamics button {
  border: 1px solid var(--line);
  color: var(--amber);
  padding: 6px 10px;
  border-radius: 4px;
  background: var(--panel);
  min-height: 32px;
}
.dynamics-controls {
  width: 950px;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  padding: 8px;
  font-size: 11px;
}
.dynamics-controls label {
  display: flex;
  align-items: center;
  gap: 4px;
}
.dynamics-controls input {
  width: 60px;
}
.dynamics-controls select {
  width: 130px;
}
.score-dynamics {
  padding: 0;
}
.score-dynamics > button {
  padding: 2px 8px;
  min-height: 22px;
  font-size: 10px;
}
.dynamics-controls {
  max-width: 100%;
  position: static;
}
</style>
