<script setup lang="ts">
import { ref } from 'vue'
import type { Project, ScoreTimeline } from '../types'
import { keyNames, keyLabel } from '../score'
const props = defineProps<{ project: Project; editable: boolean }>()
const emit = defineEmits<{ update: [score: ScoreTimeline | null] }>()
const keyMode = ref<'major' | 'minor'>('major')
const beat = ref(4),
  beats = ref(3),
  unit = ref(4),
  key = ref('C'),
  start = ref(0),
  end = ref(8),
  times = ref(2),
  ending = ref<number | ''>('')
const jumpAt = ref(16),
  jumpTarget = ref(0),
  fine = ref<number | ''>(''),
  codaFrom = ref<number | ''>(''),
  codaTo = ref<number | ''>('')
const clone = () =>
  JSON.parse(JSON.stringify(props.project.score)) as ScoreTimeline
function enable() {
  emit('update', {
    version: 1,
    length: Math.max(
      4,
      ...props.project.parts.map((p) => p.loop_beats),
      ...props.project.parts.flatMap((p) =>
        p.notes.map((n) => n.beat + n.duration),
      ),
    ),
    loop_score: false,
    meters: [],
    keys: [],
    repeats: [],
  })
}
function add(kind: 'meters' | 'keys' | 'repeats') {
  const s = clone()
  if (kind === 'meters')
    s.meters = [
      ...s.meters.filter((m) => m.beat !== beat.value),
      { beat: beat.value, beats: beats.value, unit: unit.value },
    ].sort((a, b) => a.beat - b.beat)
  if (kind === 'keys')
    s.keys = [
      ...s.keys.filter((k) => k.beat !== beat.value),
      { beat: beat.value, key: key.value, mode: keyMode.value },
    ].sort((a, b) => a.beat - b.beat)
  if (kind === 'repeats')
    s.repeats = [
      ...s.repeats,
      {
        start: start.value,
        end: end.value,
        times: times.value,
        first_ending: ending.value === '' ? null : ending.value,
      },
    ].sort((a, b) => a.start - b.start)
  emit('update', s)
}
function remove(kind: 'meters' | 'keys' | 'repeats', index: number) {
  const s = clone()
  s[kind].splice(index, 1)
  emit('update', s)
}
</script>
<template>
  <details class="timeline-editor">
    <summary>Shared score · meter, keys, repeats and navigation</summary>
    <template v-if="!project.score"
      ><p>
        Legacy playback loops each part independently. Converting aligns all
        parts on one score, preserves note timing, and stops at its end unless
        whole-score looping is enabled.
      </p>
      <button :disabled="!editable" @click="enable">
        Convert to shared score
      </button></template
    >
    <template v-else>
      <div class="timeline-row">
        <label
          >Score length<input
            type="number"
            min="0.25"
            max="4096"
            :value="project.score.length"
            :disabled="!editable"
            @change="
              emit('update', {
                ...clone(),
                length: Number(($event.target as HTMLInputElement).value),
              })
            " /></label
        ><label
          ><input
            type="checkbox"
            :checked="project.score.loop_score"
            :disabled="!editable"
            @change="
              emit('update', {
                ...clone(),
                loop_score: ($event.target as HTMLInputElement).checked,
              })
            "
          />
          Loop whole score</label
        >
      </div>
      <div class="timeline-row">
        <label
          >At quarter beat (from 0)<input
            v-model.number="beat"
            type="number"
            min="0"
            step="0.25" /></label
        ><label
          >Meter<input v-model.number="beats" type="number" min="1" max="16" />
          /
          <select v-model.number="unit">
            <option v-for="u in [1, 2, 4, 8, 16, 32]" :key="u">{{ u }}</option>
          </select></label
        ><button :disabled="!editable" @click="add('meters')">Set meter</button
        ><label
          >Key<select v-model="key">
            <option v-for="k in keyNames" :key="k" :value="k">
              {{ keyLabel(k, keyMode) }}
            </option></select
          ><select v-model="keyMode" aria-label="Key mode">
            <option value="major">Major</option>
            <option value="minor">Minor</option>
          </select></label
        ><button :disabled="!editable" @click="add('keys')">Set key</button>
      </div>
      <div class="timeline-row">
        <label
          >Repeat start<input
            v-model.number="start"
            type="number"
            min="0"
            step="0.25" /></label
        ><label
          >Repeat end<input
            v-model.number="end"
            type="number"
            min="0"
            step="0.25" /></label
        ><label
          >Passes<input
            v-model.number="times"
            type="number"
            min="2"
            max="32" /></label
        ><label
          >First ending starts<input
            v-model="ending"
            type="number"
            min="0"
            step="0.25" /></label
        ><button :disabled="!editable" @click="add('repeats')">
          Add repeat
        </button>
      </div>
      <div class="timeline-row">
        <label
          >Jump at<input v-model.number="jumpAt" type="number" min="0" /></label
        ><label
          >Jump to (0 = D.C.)<input
            v-model.number="jumpTarget"
            type="number"
            min="0" /></label
        ><label>Fine<input v-model="fine" type="number" min="0" /></label
        ><label
          >To coda at<input v-model="codaFrom" type="number" min="0" /></label
        ><label
          >Coda target<input v-model="codaTo" type="number" min="0" /></label
        ><button
          :disabled="!editable"
          @click="
            emit('update', {
              ...clone(),
              navigation: {
                at: jumpAt,
                target: jumpTarget,
                fine: fine === '' ? null : Number(fine),
                coda:
                  codaFrom === '' || codaTo === ''
                    ? null
                    : [Number(codaFrom), Number(codaTo)],
              },
            })
          "
        >
          Set D.C./D.S.</button
        ><button
          :disabled="!editable"
          @click="emit('update', { ...clone(), navigation: null })"
        >
          Clear jump
        </button>
      </div>
      <ul>
        <li v-for="(m, i) in project.score.meters" :key="`m${i}`">
          Beat {{ m.beat }}: {{ m.beats }}/{{ m.unit }}
          <button :disabled="!editable" @click="remove('meters', i)">
            Remove meter
          </button>
        </li>
        <li v-for="(k, i) in project.score.keys" :key="`k${i}`">
          Beat {{ k.beat }}: {{ keyLabel(k.key, k.mode) }}
          <button :disabled="!editable" @click="remove('keys', i)">
            Remove key
          </button>
        </li>
        <li v-for="(r, i) in project.score.repeats" :key="`r${i}`">
          Repeat {{ r.start }}–{{ r.end }} × {{ r.times
          }}<span v-if="r.first_ending != null">
            · first ending {{ r.first_ending }}–{{ r.end }}</span
          >
          <button :disabled="!editable" @click="remove('repeats', i)">
            Remove repeat
          </button>
        </li>
      </ul>
      <p>
        All positions use quarter beats. Meter edits preserve note timing.
        Written repeats run before a D.C./D.S.; the jump is taken once.
      </p>
    </template>
  </details>
</template>
<style scoped>
.timeline-editor {
  padding: 10px;
  border-bottom: 1px solid var(--line);
  max-height: 250px;
  overflow: auto;
  flex-shrink: 0;
}
.timeline-editor summary {
  color: var(--cyan);
  cursor: pointer;
}
.timeline-editor p {
  font-size: 12px;
  color: var(--muted);
  margin: 8px 0;
}
.timeline-row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  margin: 8px 0;
}
.timeline-row label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
}
.timeline-row input[type='number'] {
  width: 70px;
}
.timeline-editor button {
  padding: 6px 10px;
  background: var(--panel);
  color: var(--text);
  border: 1px solid var(--line);
  border-radius: 4px;
  min-height: 36px;
}
.timeline-editor li {
  font-size: 12px;
  padding: 4px;
}
.timeline-editor ul {
  margin-left: 20px;
}
</style>
