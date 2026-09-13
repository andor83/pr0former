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
export type StructureTab = 'meter' | 'bars' | 'clef'
const props = defineProps<{
  project: Project
  editable: boolean
  partId: string
  staffId?: string
  beat: number
  initialTab?: StructureTab
}>()
const emit = defineEmits<{ update: [project: Project] }>()
const tabs: { id: StructureTab; label: string; symbol: string }[] = [
  { id: 'meter', label: 'Time signature', symbol: '⁴₄' },
  { id: 'bars', label: 'Add / delete bars', symbol: '+𝄀' },
  { id: 'clef', label: 'Clef change', symbol: '𝄞' },
]
const tab = ref<StructureTab>(props.initialTab ?? 'meter')
const error = ref(''),
  at = ref(props.beat),
  count = ref(1),
  bar = ref(1),
  beats = ref(props.project.beats_per_bar),
  unit = ref(props.project.beat_unit || 4),
  staffId = ref(props.staffId || ''),
  clef = ref('treble')
const units = [1, 2, 4, 8, 16, 32]
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
  <div class="sd-dialog">
    <p v-if="!project.score" class="sd-note" style="padding: 10px 14px 0">
      Applying a meter or bar edit creates a shared timeline for all parts.
    </p>
    <p v-if="error" role="alert" class="field-error" style="padding: 0 14px">
      {{ error }}
    </p>
    <div class="sd-strip">
      <strong>Bar {{ bar }}</strong>
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
    </div>
    <div class="sd-layout">
      <nav class="sd-nav" aria-label="Bar tools">
        <button
          v-for="t in tabs"
          :key="t.id"
          type="button"
          :class="{ active: tab === t.id }"
          :aria-pressed="tab === t.id"
          :aria-label="t.label"
          @click="tab = t.id"
        >
          <span class="sd-symbol">{{ t.symbol }}</span>{{ t.label }}
        </button>
      </nav>
      <section class="sd-section">
        <template v-if="tab === 'meter'">
          <div class="sd-meter-editor">
            <div class="sd-meter-preview" aria-live="polite">
              <span>{{ beats }}</span><span>{{ unit }}</span>
            </div>
            <div class="sd-steppers">
              <div class="sd-stepper">
                <span>Beats per bar</span>
                <button
                  type="button"
                  aria-label="Fewer beats"
                  @click="beats = Math.max(1, beats - 1)"
                >
                  −
                </button>
                <input
                  v-model.number="beats"
                  type="number"
                  min="1"
                  max="16"
                  aria-label="Beats per bar"
                />
                <button
                  type="button"
                  aria-label="More beats"
                  @click="beats = Math.min(16, beats + 1)"
                >
                  +
                </button>
              </div>
              <div class="sd-stepper">
                <span>Beat unit</span>
                <button
                  type="button"
                  aria-label="Longer beat unit"
                  @click="unit = units[Math.max(0, units.indexOf(unit) - 1)]!"
                >
                  −
                </button>
                <select v-model.number="unit" aria-label="Beat unit">
                  <option v-for="u in units" :key="u" :value="u">{{ u }}</option>
                </select>
                <button
                  type="button"
                  aria-label="Shorter beat unit"
                  @click="
                    unit = units[Math.min(units.length - 1, units.indexOf(unit) + 1)]!
                  "
                >
                  +
                </button>
              </div>
            </div>
          </div>
          <div class="sd-quick">
            <button
              v-for="m in ['2/4', '3/4', '4/4', '5/4', '6/8', '7/8', '9/8', '12/8']"
              :key="m"
              type="button"
              :aria-pressed="`${beats}/${unit}` === m"
              @click="
                () => {
                  const [b, u] = m.split('/').map(Number)
                  beats = b!
                  unit = u!
                }
              "
            >
              {{ m }}
            </button>
          </div>
          <div class="sd-actions">
            <button
              type="button"
              class="sd-primary"
              :disabled="!editable"
              @click="change(() => setMeter(project, at, beats, unit))"
            >
              Set time signature
            </button>
          </div>
          <div class="help-section-title">Project meter<HelpNote label="Project meter">
            Applies to every part from the position above; at beat zero it also
            sets the initial and count-in meter. Existing notes keep their timing.
          </HelpNote></div>
        </template>
        <template v-else-if="tab === 'bars'">
          <div class="sd-stepper">
            <span>Number of bars</span>
            <button
              type="button"
              aria-label="Fewer bars"
              @click="count = Math.max(1, count - 1)"
            >
              −
            </button>
            <input
              v-model.number="count"
              type="number"
              min="1"
              max="1024"
              aria-label="Number of bars"
            />
            <button
              type="button"
              aria-label="More bars"
              @click="count = Math.min(1024, count + 1)"
            >
              +
            </button>
          </div>
          <div class="sd-actions">
            <button
              type="button"
              :disabled="!editable"
              @click="change(() => editBars(project, 'append', bar, count))"
            >
              Add bars at end</button
            ><button
              type="button"
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
              type="button"
              :disabled="!editable"
              @click="change(() => editBars(project, 'insert', bar, count))"
            >
              Insert before bar {{ bar }}</button
            ><button
              type="button"
              class="sd-danger"
              :disabled="!editable"
              @click="change(() => editBars(project, 'delete', bar, count))"
            >
              Delete from bar {{ bar }}
            </button>
          </div>
          <div class="help-section-title">Insert and delete time<HelpNote label="Insert and delete time">
            Insertion and deletion shift all parts, signatures, repeats and MIDI
            events together. Delete removes the selected bars’ contents. Undo
            restores the complete edit.
          </HelpNote></div>
        </template>
        <template v-else>
          <h3>Clef change · {{ part?.name }}</h3>
          <div class="sd-fields">
            <label
              ><span class="field-title">Staff<HelpNote label="Staff">
            Uses the position above; clef changes can occur between bar lines.
          </HelpNote></span><select v-model="staffId" aria-label="Staff">
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
            >
          </div>
          <div class="sd-quick" role="radiogroup" aria-label="Clef glyphs">
            <button
              v-for="[c, symbol] in [
                ['treble', '𝄞'],
                ['bass', '𝄢'],
                ['alto', '𝄡'],
                ['tenor', '𝄡'],
              ]"
              :key="c"
              type="button"
              role="radio"
              :aria-checked="clef === c"
              :aria-label="`${c} clef glyph`"
              @click="clef = c!"
            >
              <span class="sd-symbol">{{ symbol }}</span>{{ c }}
            </button>
          </div>
          <div class="sd-actions">
            <button
              type="button"
              class="sd-primary"
              :disabled="!editable || !staffId"
              @click="change(() => setClef(project, partId, staffId, at, clef))"
            >
              Set clef at beat
            </button>
          </div>

        </template>
      </section>
    </div>
  </div>
</template>
<style src="./scoreDialogLayout.css"></style>
