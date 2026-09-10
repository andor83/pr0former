<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Project } from '../types'
import { keyNames, keyLabel, staves } from '../score'
import {
  moveRegionToStaff,
  moveRegionToVoice,
  scaleRegionDurations,
  transposeRegion,
} from '../scoreRegion'
import {
  barsInRange,
  clearRange,
  editBars,
  keyAt,
  measures,
  meterAt,
  removeRepeats,
  setBarline,
  setClef,
  setKeyRange,
  setMeterRange,
  setNavigation,
  setRepeat,
  setTempo,
  removeTempo,
  tempoAt,
} from '../scoreBars'
export type MeasureMode =
  | 'meter'
  | 'key'
  | 'repeat'
  | 'navigation'
  | 'barline'
  | 'clef'
  | 'bars'
  | 'edit'
  | 'tempo'
const props = defineProps<{
  project: Project
  editable: boolean
  mode: MeasureMode
  region: { part: string; staff: string; start: number; end: number }
}>()
const emit = defineEmits<{
  update: [project: Project, region?: { start: number; end: number }]
  close: []
}>()
const tabs: { id: MeasureMode; label: string; symbol: string }[] = [
  { id: 'meter', label: 'Time signature', symbol: '⁴₄' },
  { id: 'key', label: 'Key signature', symbol: '♯' },
  { id: 'repeat', label: 'Repeat', symbol: '𝄆' },
  { id: 'navigation', label: 'D.C. / D.S. / Coda', symbol: '𝄋' },
  { id: 'barline', label: 'Barline', symbol: '𝄁' },
  { id: 'clef', label: 'Clef', symbol: '𝄞' },
  { id: 'tempo', label: 'Tempo', symbol: '♩=' },
  { id: 'bars', label: 'Add / delete bars', symbol: '+𝄀' },
  { id: 'edit', label: 'Mass edit', symbol: '♫' },
]
const tab = ref<MeasureMode>(props.mode)
const bars = computed(() => measures(props.project))
const barIndex = (beat: number) =>
  Math.max(
    0,
    bars.value.findIndex((m) => m.start <= beat + 1e-9 && m.end > beat + 1e-9),
  )
// Beats are the source of truth so bar numbers follow meter edits made in this dialog.
const startBeat = ref(bars.value[barIndex(props.region.start)]?.start ?? 0),
  endBeat = ref(
    bars.value[barIndex(Math.max(props.region.start, props.region.end - 1e-6))]
      ?.end ?? props.project.score?.length ?? 4,
  ),
  toEnd = ref(false)
const fromBar = computed({
  get: () => barIndex(startBeat.value) + 1,
  set: (v: number) => {
    const m = bars.value[v - 1]
    if (!m) return
    startBeat.value = m.start
    if (endBeat.value <= m.start) endBeat.value = m.end
  },
})
const toBar = computed({
  get: () => barIndex(Math.max(startBeat.value, endBeat.value - 1e-6)) + 1,
  set: (v: number) => {
    const m = bars.value[v - 1]
    if (!m) return
    endBeat.value = m.end
    if (startBeat.value >= m.end) startBeat.value = m.start
  },
})
const start = computed(() => startBeat.value),
  end = computed(() => endBeat.value),
  regionEnd = computed(() => (toEnd.value ? null : end.value))
const part = computed(() =>
    props.project.parts.find((p) => p.id === props.region.part),
  ),
  staffList = computed(() => (part.value ? staves(part.value) : [])),
  staffId = ref(props.region.staff)
const current = computed(() => meterAt(props.project, start.value))
const beats = ref(current.value.beats),
  unit = ref(current.value.unit)
const units = [1, 2, 4, 8, 16, 32]
const keyNow = computed(() => keyAt(props.project, start.value))
const key = ref(keyNow.value.key),
  keyMode = ref<'major' | 'minor'>(keyNow.value.mode || 'major'),
  transposeNotes = ref<'hold' | 1 | -1>('hold')
const accidentals = (k: string) => {
  const f = keyNames.indexOf(k) - 7
  return f === 0 ? '' : `${Math.abs(f)}${f > 0 ? '♯' : '♭'}`
}
const existingRepeat = computed(() =>
  (props.project.score?.repeats || []).find(
    (r) => r.start < end.value && r.end > start.value,
  ),
)
const times = ref(existingRepeat.value?.times ?? 2),
  ending = ref<number | ''>(
    existingRepeat.value?.first_ending != null
      ? barIndex(existingRepeat.value.first_ending) + 1
      : '',
  )
const nav = computed(() => props.project.score?.navigation)
const jump = ref<'dc_fine' | 'ds_fine' | 'dc_coda' | 'ds_coda'>(
    nav.value
      ? `${nav.value.target === 0 ? 'dc' : 'ds'}_${nav.value.coda ? 'coda' : 'fine'}`
      : 'dc_fine',
  ),
  fineBar = ref(
    nav.value?.fine != null
      ? barIndex(nav.value.fine - 1e-6) + 1
      : Math.max(1, fromBar.value - 1),
  ),
  codaBar = ref(
    nav.value?.coda ? barIndex(nav.value.coda[1]) + 1 : toBar.value + 1,
  ),
  toCodaBar = ref(
    nav.value?.coda
      ? barIndex(nav.value.coda[0] - 1e-6) + 1
      : Math.max(1, fromBar.value - 1),
  )
const barlineStyle = ref(
  props.project.score?.barlines?.find((b) => b.beat === end.value)?.style ||
    'normal',
)
const clef = ref(
  (() => {
    const s = staffList.value.find((s) => s.id === staffId.value)
    return (
      s?.clef_changes?.filter((c) => c.beat <= start.value).at(-1)?.clef ||
      s?.clef ||
      'treble'
    )
  })(),
)
const count = ref(1)
const bpm = ref(Math.round(tempoAt(props.project, start.value) * 10) / 10)
const tempoHere = computed(() =>
  (props.project.score?.tempos || []).some((t) => t.beat === start.value),
)
const steps = ref(0),
  semitones = ref(0),
  targetVoice = ref(1),
  targetStaff = ref(props.region.staff)
const massRegion = computed(() => ({
  part: props.region.part,
  staff: props.region.staff,
  start: start.value,
  end: end.value,
}))
const error = ref('')
function change(action: () => Project, region?: { start: number; end: number }) {
  if (!props.editable) return
  error.value = ''
  try {
    emit('update', action(), region)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
function apply() {
  const s = start.value,
    e = regionEnd.value
  if (tab.value === 'meter')
    change(() => setMeterRange(props.project, s, e, beats.value, unit.value))
  if (tab.value === 'key')
    change(() =>
      setKeyRange(props.project, s, e, key.value, keyMode.value, transposeNotes.value),
    )
  if (tab.value === 'repeat')
    change(() =>
      setRepeat(
        props.project,
        s,
        end.value,
        times.value,
        ending.value === '' ? null : bars.value[Number(ending.value) - 1]?.start ?? null,
      ),
    )
  if (tab.value === 'navigation') {
    const target = jump.value.startsWith('ds') ? s : 0,
      at = end.value
    change(() =>
      setNavigation(props.project, {
        at,
        target,
        fine: jump.value.endsWith('fine')
          ? bars.value[fineBar.value - 1]?.end ?? null
          : null,
        coda: jump.value.endsWith('coda')
          ? [
              bars.value[toCodaBar.value - 1]?.end ?? 0,
              bars.value[codaBar.value - 1]?.start ?? 0,
            ]
          : null,
      }),
    )
  }
  if (tab.value === 'barline')
    change(() =>
      setBarline(
        props.project,
        end.value,
        barlineStyle.value === 'normal' ? null : barlineStyle.value,
      ),
    )
  if (tab.value === 'clef')
    change(() =>
      setClef(props.project, props.region.part, staffId.value, s, clef.value),
    )
  if (tab.value === 'tempo') change(() => setTempo(props.project, s, bpm.value))
}
const barCount = computed(() => toBar.value - fromBar.value + 1)
</script>
<template>
  <div class="sd-dialog">
    <p v-if="error" role="alert" class="field-error">{{ error }}</p>
    <div class="sd-strip">
      <strong>{{
        toEnd
          ? `Bar ${fromBar} to end`
          : barCount === 1
            ? `Bar ${fromBar}`
            : `Bars ${fromBar}–${toBar}`
      }}</strong>
      <label
        >From bar<input
          v-model.number="fromBar"
          type="number"
          min="1"
          :max="bars.length" /></label
      ><label
        >Through bar<input
          v-model.number="toBar"
          type="number"
          :min="fromBar"
          :max="bars.length"
          :disabled="toEnd" /></label
      ><label class="sd-check"
        ><input v-model="toEnd" type="checkbox" /> To end of score</label
      ><small>{{ part?.name }} · {{ bars.length }} bars</small>
    </div>
    <div class="sd-layout">
      <nav class="sd-nav" aria-label="Measure tools">
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
                <output aria-label="Beats per bar">{{ beats }}</output>
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
                <output aria-label="Beat unit">{{ unit }}</output>
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
          <p class="sd-note">
            The time signature changes at bar {{ fromBar }}.
            {{
              toEnd
                ? 'It stays in force to the end of the score.'
                : `The previous meter resumes after bar ${toBar}.`
            }}
            Existing notes keep their timing; bar lines are redrawn.
          </p>
        </template>
        <template v-else-if="tab === 'key'">
          <div class="sd-keys" role="radiogroup" aria-label="Key signature">
            <button
              v-for="k in keyNames"
              :key="k"
              type="button"
              role="radio"
              :aria-checked="key === k"
              :aria-label="keyLabel(k, keyMode)"
              @click="key = k"
            >
              <strong>{{
                keyMode === 'minor' ? keyLabel(k, 'minor').replace(' minor', 'm') : k
              }}</strong
              ><small>{{ accidentals(k) || '♮' }}</small>
            </button>
          </div>
          <div class="sd-row">
            <label
              ><input v-model="keyMode" type="radio" value="major" /> Major</label
            ><label
              ><input v-model="keyMode" type="radio" value="minor" /> Minor</label
            >
          </div>
          <fieldset class="sd-row">
            <legend>Notes in these bars</legend>
            <label
              ><input v-model="transposeNotes" type="radio" value="hold" /> Hold
              to original pitches</label
            ><label
              ><input v-model="transposeNotes" type="radio" :value="1" />
              Transpose up</label
            ><label
              ><input v-model="transposeNotes" type="radio" :value="-1" />
              Transpose down</label
            >
          </fieldset>
          <p class="sd-note">
            {{ keyLabel(key, keyMode) }} from bar {{ fromBar }}{{
              toEnd ? ' to the end' : `; the previous key resumes after bar ${toBar}`
            }}.
          </p>
        </template>
        <template v-else-if="tab === 'repeat'">
          <div class="sd-stepper">
            <span>Passes</span>
            <button
              type="button"
              aria-label="Fewer passes"
              @click="times = Math.max(2, times - 1)"
            >
              −
            </button>
            <output aria-label="Passes">{{ times }}</output>
            <button
              type="button"
              aria-label="More passes"
              @click="times = Math.min(32, times + 1)"
            >
              +
            </button>
          </div>
          <label
            >First ending starts at bar<select
              v-model="ending"
              aria-label="First ending starts at bar"
            >
              <option value="">No ending brackets</option>
              <option
                v-for="m in barsInRange(project, start, end).slice(1)"
                :key="m.number"
                :value="m.number"
              >
                {{ m.number }}
              </option>
            </select></label
          >
          <p class="sd-note">
            Repeat bars {{ fromBar }}–{{ toBar }} {{ times }} times
            (𝄆 at bar {{ fromBar }}, 𝄇 after bar {{ toBar }}).
            {{
              ending !== ''
                ? `Bars ${ending}–${toBar} are played on the first pass only.`
                : ''
            }}
          </p>
          <button
            v-if="existingRepeat"
            type="button"
            class="sd-danger"
            :disabled="!editable"
            @click="change(() => removeRepeats(project, start, end))"
          >
            Remove repeat
          </button>
        </template>
        <template v-else-if="tab === 'navigation'">
          <fieldset class="sd-column">
            <legend>Text repeat at the end of bar {{ toBar }}</legend>
            <label
              ><input v-model="jump" type="radio" value="dc_fine" /> D.C. al
              Fine — back to the start, stop at Fine</label
            ><label
              ><input v-model="jump" type="radio" value="ds_fine" /> D.S. al
              Fine — segno at bar {{ fromBar }}, stop at Fine</label
            ><label
              ><input v-model="jump" type="radio" value="dc_coda" /> D.C. al
              Coda</label
            ><label
              ><input v-model="jump" type="radio" value="ds_coda" /> D.S. al
              Coda — segno at bar {{ fromBar }}</label
            >
          </fieldset>
          <div class="sd-row">
            <label v-if="jump.endsWith('fine')"
              >Fine after bar<input
                v-model.number="fineBar"
                type="number"
                min="1"
                :max="bars.length" /></label
            ><template v-else
              ><label
                >To Coda after bar<input
                  v-model.number="toCodaBar"
                  type="number"
                  min="1"
                  :max="bars.length" /></label
              ><label
                >Coda starts at bar<input
                  v-model.number="codaBar"
                  type="number"
                  min="1"
                  :max="bars.length" /></label
            ></template>
          </div>
          <button
            v-if="nav"
            type="button"
            class="sd-danger"
            :disabled="!editable"
            @click="change(() => setNavigation(project, null))"
          >
            Clear D.C./D.S.
          </button>
        </template>
        <template v-else-if="tab === 'barline'">
          <div class="sd-quick" role="radiogroup" aria-label="Barline style">
            <button
              v-for="[style, symbol, name] in [
                ['normal', '𝄀', 'Normal'],
                ['double', '𝄁', 'Double'],
                ['final', '𝄂', 'Final'],
                ['dashed', '┊', 'Dashed'],
              ]"
              :key="style"
              type="button"
              role="radio"
              :aria-checked="barlineStyle === style"
              :aria-label="`${name} barline`"
              @click="barlineStyle = style!"
            >
              <span class="sd-symbol">{{ symbol }}</span>{{ name }}
            </button>
          </div>
          <p class="sd-note">Applies to the barline after bar {{ toBar }}.</p>
        </template>
        <template v-else-if="tab === 'clef'">
          <label
            >Staff<select v-model="staffId" aria-label="Clef staff">
              <option v-for="s in staffList" :key="s.id" :value="s.id">
                {{ s.name }}
              </option>
            </select></label
          >
          <div class="sd-quick" role="radiogroup" aria-label="Clef">
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
              :aria-label="`${c} clef`"
              @click="clef = c!"
            >
              <span class="sd-symbol">{{ symbol }}</span>{{ c }}
            </button>
          </div>
          <p class="sd-note">The clef changes at the start of bar {{ fromBar }}.</p>
        </template>
        <template v-else-if="tab === 'tempo'">
          <div class="sd-stepper">
            <span>♩ per minute</span>
            <button type="button" aria-label="Slower" @click="bpm = Math.max(1, bpm - 1)">
              −
            </button>
            <input
              v-model.number="bpm"
              type="number"
              min="1"
              max="400"
              step="0.5"
              aria-label="Quarter notes per minute"
              class="sd-tempo-input"
            />
            <button type="button" aria-label="Faster" @click="bpm = Math.min(400, bpm + 1)">
              +
            </button>
          </div>
          <div class="sd-quick">
            <button
              v-for="t in [60, 72, 84, 96, 108, 120, 132, 144, 160]"
              :key="t"
              type="button"
              :aria-pressed="bpm === t"
              @click="bpm = t"
            >
              {{ t }}
            </button>
          </div>
          <p class="sd-note">
            The tempo changes at the start of bar {{ fromBar }} and stays in force
            until the next tempo mark. Manual tempo edits during playback last until
            the next mark. At bar 1 this also sets the project tempo and count-in.
          </p>
          <button
            v-if="tempoHere"
            type="button"
            class="sd-danger"
            :disabled="!editable"
            @click="change(() => removeTempo(project, start))"
          >
            Remove tempo mark
          </button>
        </template>
        <template v-else-if="tab === 'edit'">
          <fieldset class="sd-column">
            <legend>Transpose notes in bars {{ fromBar }}–{{ toBar }}</legend>
            <div class="sd-stepper">
              <span>Diatonic steps</span>
              <button type="button" aria-label="Step down" @click="steps--">−</button>
              <output aria-label="Diatonic steps">{{ steps }}</output>
              <button type="button" aria-label="Step up" @click="steps++">+</button>
            </div>
            <div class="sd-stepper">
              <span>Semitones</span>
              <button type="button" aria-label="Semitone down" @click="semitones--">
                −
              </button>
              <output aria-label="Semitones">{{ semitones }}</output>
              <button type="button" aria-label="Semitone up" @click="semitones++">
                +
              </button>
            </div>
            <div class="sd-quick">
              <button
                type="button"
                :disabled="!editable || (!steps && !semitones)"
                @click="
                  change(() =>
                    transposeRegion(
                      project,
                      massRegion,
                      semitones ? { semitones } : { steps },
                    ),
                  )
                "
              >
                Transpose</button
              ><button
                type="button"
                :disabled="!editable"
                @click="change(() => transposeRegion(project, massRegion, { steps: 7 }))"
              >
                Up an octave</button
              ><button
                type="button"
                :disabled="!editable"
                @click="change(() => transposeRegion(project, massRegion, { steps: -7 }))"
              >
                Down an octave
              </button>
            </div>
          </fieldset>
          <fieldset class="sd-column">
            <legend>Change note durations</legend>
            <div class="sd-quick">
              <button
                v-for="[factor, label] in [
                  [2, 'Double (×2)'],
                  [0.5, 'Halve (÷2)'],
                  [4, '×4'],
                  [0.25, '÷4'],
                ]"
                :key="label"
                type="button"
                :disabled="!editable"
                @click="
                  change(() =>
                    scaleRegionDurations(project, massRegion, Number(factor)),
                  )
                "
              >
                {{ label }}
              </button>
            </div>
            <p class="sd-note">
              Onsets are scaled from the start of bar {{ fromBar }}; later music is
              not shifted, so doubling can overlap following bars.
            </p>
          </fieldset>
          <fieldset class="sd-row">
            <legend>Move notes</legend>
            <label
              >To voice<select v-model.number="targetVoice" aria-label="Move to voice">
                <option v-for="v in 4" :key="v" :value="v">{{ v }}</option>
              </select></label
            ><button
              type="button"
              :disabled="!editable"
              @click="
                change(() => moveRegionToVoice(project, massRegion, targetVoice))
              "
            >
              Move to voice</button
            ><label
              >To staff<select v-model="targetStaff" aria-label="Move to staff">
                <option v-for="s in staffList" :key="s.id" :value="s.id">
                  {{ s.name }}
                </option>
              </select></label
            ><button
              type="button"
              :disabled="!editable || targetStaff === region.staff"
              @click="
                change(() => moveRegionToStaff(project, massRegion, targetStaff))
              "
            >
              Move to staff
            </button>
          </fieldset>
          <p class="sd-note">
            Copy, cut and paste selected bars with Ctrl/Cmd-C, X and V; paste
            lands at the caret in Write mode or at the selected bars in Select mode.
          </p>
        </template>
        <template v-else>
          <label
            >Number of bars<input
              v-model.number="count"
              type="number"
              min="1"
              max="1024"
          /></label>
          <div class="sd-quick">
            <button
              type="button"
              :disabled="!editable"
              @click="change(() => editBars(project, 'insert', fromBar, count))"
            >
              Insert {{ count }} before bar {{ fromBar }}</button
            ><button
              type="button"
              :disabled="!editable"
              @click="
                change(() =>
                  editBars(
                    project,
                    toBar === bars.length ? 'append' : 'insert',
                    toBar + 1,
                    count,
                  ),
                )
              "
            >
              Add {{ count }} after bar {{ toBar }}</button
            ><button
              type="button"
              :disabled="!editable"
              @click="change(() => editBars(project, 'append', bars.length, count))"
            >
              Add {{ count }} at end</button
            ><button
              type="button"
              class="sd-danger"
              :disabled="!editable"
              @click="change(() => editBars(project, 'delete', fromBar, barCount))"
            >
              Delete bars {{ fromBar }}–{{ toBar }}</button
            ><button
              type="button"
              :disabled="!editable"
              @click="
                change(() => clearRange(project, region.part, null, start, end))
              "
            >
              Clear contents of bars {{ fromBar }}–{{ toBar }}
            </button>
          </div>
          <p class="sd-note">
            Insertion and deletion shift every part, signature, repeat and MIDI
            event together. Undo restores the whole edit.
          </p>
        </template>
      </section>
    </div>
    <footer v-if="tab !== 'bars' && tab !== 'edit'" class="sd-footer">
      <button
        type="button"
        class="sd-primary"
        :disabled="!editable"
        @click="apply"
      >
        Apply
      </button>
      <button type="button" @click="emit('close')">Close</button>
    </footer>
  </div>
</template>
<style src="./scoreDialogLayout.css"></style>
