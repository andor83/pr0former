<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { Project } from '../types'
import { staves } from '../score'
import {
  editBars,
  measures,
  setMeter,
  setClef,
  sharedTimeline,
} from '../scoreBars'
const props = defineProps<{
  project: Project
  editable: boolean
  partId: string
  staffId?: string
  beat: number
}>()
const emit = defineEmits<{ update: [project: Project] }>()
const error = ref(''),
  at = ref(props.beat),
  count = ref(1),
  bar = ref(1),
  beats = ref(props.project.beats_per_bar),
  unit = ref(props.project.beat_unit || 4),
  staffId = ref(props.staffId || ''),
  clef = ref('treble')
const bars = computed(() => measures(props.project)),
  part = computed(() => props.project.parts.find((p) => p.id === props.partId)),
  staffs = computed(() => (part.value ? staves(part.value) : []))
watch(
  at,
  (beat) => {
    const m = bars.value.find((m) => m.start <= beat && m.end > beat)
    if (m) {
      bar.value = m.number
      beats.value = m.beats
      unit.value = m.unit
    }
    const s =
      staffs.value.find((s) => s.id === staffId.value) || staffs.value[0]
    if (s)
      clef.value =
        s.clef_changes?.filter((c) => c.beat <= beat).at(-1)?.clef || s.clef
  },
  { immediate: true },
)
watch(
  staffs,
  (list) => {
    if (!list.some((s) => s.id === staffId.value))
      staffId.value = list[0]?.id || ''
  },
  { immediate: true },
)
watch(staffId, () => {
  const s = staffs.value.find((s) => s.id === staffId.value)
  if (s)
    clef.value =
      s.clef_changes?.filter((c) => c.beat <= at.value).at(-1)?.clef || s.clef
})
function chooseBar() {
  const m = bars.value[bar.value - 1]
  if (m) at.value = m.start
}
function change(action: () => Project) {
  if (!props.editable) return
  error.value = ''
  try {
    emit('update', action())
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
</script>
<template>
  <div class="structure-editor">
    <p v-if="!project.score">
      Applying a meter or bar edit creates a shared timeline for all parts.
    </p>
    <p v-if="error" role="alert" class="field-error">{{ error }}</p>
    <fieldset>
      <legend>Position</legend>
      <label
        >Bar<input
          v-model.number="bar"
          type="number"
          min="1"
          :max="bars.length"
          @change="chooseBar" /></label
      ><label
        >At quarter beat<input
          v-model.number="at"
          type="number"
          min="0"
          step="0.25"
          :max="sharedTimeline(project).length" /></label
      ><small
        >{{ bars.length }} bars · {{ sharedTimeline(project).length }} quarter
        beats</small
      >
    </fieldset>
    <fieldset>
      <legend>Time signature</legend>
      <label
        >Beats per bar<input
          v-model.number="beats"
          type="number"
          min="1"
          max="16" /></label
      ><label
        >Beat unit<select v-model.number="unit" aria-label="Beat unit">
          <option v-for="u in [1, 2, 4, 8, 16, 32]" :key="u" :value="u">
            {{ u }}
          </option>
        </select></label
      ><button
        :disabled="!editable"
        @click="change(() => setMeter(project, at, beats, unit))"
      >
        Set time signature
      </button>
      <p>
        Applies to every part from this beat. Existing notes keep their timing.
      </p>
    </fieldset>
    <fieldset>
      <legend>Blank bars</legend>
      <label
        >Number of bars<input
          v-model.number="count"
          type="number"
          min="1"
          max="1024" /></label
      ><button
        :disabled="!editable"
        @click="change(() => editBars(project, 'append', bar, count))"
      >
        Add bars at end</button
      ><button
        :disabled="!editable"
        @click="
          change(() =>
            editBars(
              project,
              bar === bars.length ? 'append' : 'insert',
              bar + 1,
              count,
            ),
          )
        "
      >
        Insert after bar</button
      ><button
        :disabled="!editable"
        @click="change(() => editBars(project, 'insert', bar, count))"
      >
        Insert before bar {{ bar }}</button
      ><button
        :disabled="!editable"
        @click="change(() => editBars(project, 'delete', bar, count))"
      >
        Delete from bar {{ bar }}
      </button>
      <p>
        Insertion and deletion shift all parts, signatures, repeats and MIDI
        events together. Delete removes the selected bars’ contents. Undo
        restores the complete edit.
      </p>
    </fieldset>
    <fieldset>
      <legend>Clef change · {{ part?.name }}</legend>
      <label
        >Staff<select v-model="staffId" aria-label="Staff">
          <option v-for="s in staffs" :key="s.id" :value="s.id">
            {{ s.name }}
          </option>
        </select></label
      ><label
        >Clef<select v-model="clef" aria-label="Clef">
          <option v-for="c in ['treble', 'bass', 'alto', 'tenor']" :key="c">
            {{ c }}
          </option>
        </select></label
      ><button
        :disabled="!editable || !staffId"
        @click="change(() => setClef(project, partId, staffId, at, clef))"
      >
        Set clef at beat
      </button>
      <p>Uses the position above; clef changes can occur between bar lines.</p>
    </fieldset>
  </div>
</template>
<style scoped>
.structure-editor {
  font-size: 12px;
}
.structure-editor fieldset {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 12px;
  border-top: 1px solid #d9e0e1;
  margin-top: 14px;
  padding: 14px 0;
}
.structure-editor legend {
  font-weight: 600;
  padding-right: 10px;
}
.structure-editor label {
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.structure-editor input {
  width: 100px;
}
.structure-editor select {
  min-width: 85px;
}
.structure-editor p {
  flex-basis: 100%;
  font-size: 11px;
  color: #526267;
  line-height: 1.5;
}
.structure-editor small {
  padding: 8px 0;
  color: #526267;
}
</style>
