<script setup lang="ts">
import { ref } from 'vue'
import type { Part, Dynamics } from '../types'
import { newId } from '../id'
import { scoreX, type ScoreAnchor } from '../score'
const props = defineProps<{
  part: Part
  anchors: ScoreAnchor[]
  editable: boolean
  performance?: boolean
  origin: number
  scale: number
  width: number
}>()
const xAt = (beat: number) =>
  scoreX(beat, props.anchors, props.scale, props.origin)
const emit = defineEmits<{ update: [part: Part] }>()
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
      <g v-for="e in part.dynamics.events" :key="e.id" fill="currentColor">
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
      <div v-if="open" class="dynamics-controls">
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
          Beat {{ e.beat }} · {{ label(e.start)
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
      </div></template
    >
  </div>
</template>
<style scoped>
.score-dynamics {
  color: var(--amber);
  padding: 4px 0;
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
</style>
