<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { ScoreElement } from '../scoreElements'
import type { Project, ScoreTimeline } from '../types'
import { keyNames, keyLabel } from '../score'
import { measures } from '../scoreBars'
type TimelineTab = 'score' | 'meters' | 'keys' | 'repeats' | 'navigation'
const props = defineProps<{
  project: Project
  editable: boolean
  selection?: ScoreElement | null
}>()
const emit = defineEmits<{ update: [score: ScoreTimeline | null] }>()
const tabs: { id: TimelineTab; label: string; symbol: string }[] = [
  { id: 'score', label: 'Score', symbol: '♫' },
  { id: 'meters', label: 'Meter changes', symbol: '⁴₄' },
  { id: 'keys', label: 'Key changes', symbol: '♯' },
  { id: 'repeats', label: 'Repeats', symbol: '𝄆' },
  { id: 'navigation', label: 'D.C. / D.S. / Coda', symbol: '𝄋' },
]
const tab = ref<TimelineTab>('score')
const repeatIndex = ref<number | null>(null)
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
const units = [1, 2, 4, 8, 16, 32]
const bars = computed(() => measures(props.project))
const accidentals = (k: string) => {
  const f = keyNames.indexOf(k) - 7
  return f === 0 ? '♮' : `${Math.abs(f)}${f > 0 ? '♯' : '♭'}`
}
const clone = () =>
  JSON.parse(JSON.stringify(props.project.score)) as ScoreTimeline
watch(
  () => props.selection,
  (e) => {
    if (!e) return
    if (e.beat !== undefined) beat.value = e.beat
    const m = props.project.score?.meters.find((m) => m.beat === e.beat),
      k = props.project.score?.keys.find((k) => k.beat === e.beat)
    if (m) {
      beats.value = m.beats
      unit.value = m.unit
      tab.value = 'meters'
    }
    if (k) {
      key.value = k.key
      keyMode.value = k.mode || 'major'
      tab.value = 'keys'
    }
    if (e.kind === 'meter') tab.value = 'meters'
    if (e.kind === 'key') tab.value = 'keys'
    if (e.kind === 'navigation') tab.value = 'navigation'
    if (e.kind === 'repeat') {
      tab.value = 'repeats'
      const r = props.project.score?.repeats[e.index!]
      if (r) {
        repeatIndex.value = e.index!
        start.value = r.start
        end.value = r.end
        times.value = r.times
        ending.value = r.first_ending ?? ''
      }
    }
  },
  { immediate: true },
)
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
      ...s.repeats.filter((_, i) => i !== repeatIndex.value),
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
const barOf = (b: number) =>
  bars.value.find((m) => m.start <= b + 1e-9 && m.end > b + 1e-9)?.number
</script>
<template>
  <div class="sd-dialog">
    <template v-if="!project.score">
      <div class="sd-section">
        <div class="help-section-title">Convert legacy score<HelpNote label="Convert legacy score">
          Legacy playback loops each part independently. Converting aligns all
          parts on one score, preserves note timing, and stops at its end unless
          whole-score looping is enabled.
        </HelpNote></div>
        <div class="sd-actions">
          <button
            type="button"
            class="sd-primary"
            :disabled="!editable"
            @click="enable"
          >
            Convert to shared score
          </button>
        </div>
      </div>
    </template>
    <template v-else>
      <div class="sd-strip">
        <strong>{{ bars.length }} bars</strong>
        <span>{{ project.score.length }} quarter beats</span>
        <span
          >{{ project.score.meters.length }} meter ·
          {{ project.score.keys.length }} key ·
          {{ project.score.repeats.length }} repeat change(s)</span
        >
        <small>{{
          project.score.loop_score ? 'Loops whole score' : 'Stops at the end'
        }}</small>
      </div>
      <div class="sd-layout">
        <nav class="sd-nav" aria-label="Shared score sections">
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
          <template v-if="tab === 'score'">
            <div class="sd-fields">
              <label
                ><span class="field-title">Score length<HelpNote label="Score length">
              Positions in this dialog are zero-based quarter beats: a 4/4 bar
              lasts four, a 6/8 bar lasts three. Structured scores play every part
              through the shared traversal and stop at the end unless looping is
              enabled. For bar-based editing, select bars in the score and use
              the Measure dialog.
            </HelpNote></span><input aria-label="Score length"
                  type="number"
                  min="0.25"
                  max="4096"
                  step="0.25"
                  :value="project.score.length"
                  :disabled="!editable"
                  @change="
                    emit('update', {
                      ...clone(),
                      length: Number(($event.target as HTMLInputElement).value),
                    })
                  " /></label
              ><label class="sd-check" style="flex-direction: row"
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

          </template>
          <template v-else-if="tab === 'meters'">
            <div class="sd-row">
              <label
                ><span class="field-title">At quarter beat<HelpNote label="At quarter beat">Meter changes never stretch notes.</HelpNote></span><input aria-label="At quarter beat"
                  v-model.number="beat"
                  type="number"
                  min="0"
                  step="0.25" /></label
              ><label
                >Meter<input
                  v-model.number="beats"
                  type="number"
                  min="1"
                  max="16"
                  aria-label="Meter beats"
                />
                /
                <select v-model.number="unit" aria-label="Meter unit">
                  <option v-for="u in units" :key="u">{{ u }}</option>
                </select></label
              ><button
                type="button"
                class="sd-primary"
                :disabled="!editable"
                @click="add('meters')"
              >
                Set meter
              </button>
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
            <ul class="sd-list" aria-label="Meter changes">
              <li v-for="(m, i) in project.score.meters" :key="`m${i}`">
                <span
                  >Bar {{ barOf(m.beat) ?? '—' }} · beat {{ m.beat }}:
                  {{ m.beats }}/{{ m.unit }}</span
                >
                <button
                  type="button"
                  :disabled="!editable"
                  @click="remove('meters', i)"
                >
                  Remove meter
                </button>
              </li>
            </ul>

          </template>
          <template v-else-if="tab === 'keys'">
            <div class="sd-row">
              <label
                >At quarter beat<input
                  v-model.number="beat"
                  type="number"
                  min="0"
                  step="0.25" /></label
              ><label
                >Key<select v-model="key" aria-label="Key">
                  <option v-for="k in keyNames" :key="k" :value="k">
                    {{ keyLabel(k, keyMode) }}
                  </option></select
                ><select v-model="keyMode" aria-label="Key mode">
                  <option value="major">Major</option>
                  <option value="minor">Minor</option>
                </select></label
              ><button
                type="button"
                class="sd-primary"
                :disabled="!editable"
                @click="add('keys')"
              >
                Set key
              </button>
            </div>
            <div class="sd-keys" role="radiogroup" aria-label="Key glyphs">
              <button
                v-for="k in keyNames"
                :key="k"
                type="button"
                role="radio"
                :aria-checked="key === k"
                :aria-label="`${keyLabel(k, keyMode)} glyph`"
                @click="key = k"
              >
                <strong>{{
                  keyMode === 'minor'
                    ? keyLabel(k, 'minor').replace(' minor', 'm')
                    : k
                }}</strong
                ><small>{{ accidentals(k) }}</small>
              </button>
            </div>
            <ul class="sd-list" aria-label="Key changes">
              <li v-for="(k, i) in project.score.keys" :key="`k${i}`">
                <span
                  >Bar {{ barOf(k.beat) ?? '—' }} · beat {{ k.beat }}:
                  {{ keyLabel(k.key, k.mode) }}</span
                >
                <button
                  type="button"
                  :disabled="!editable"
                  @click="remove('keys', i)"
                >
                  Remove key
                </button>
              </li>
            </ul>
          </template>
          <template v-else-if="tab === 'repeats'">
            <div class="sd-row">
              <label
                ><span class="field-title">Repeat start<HelpNote label="Repeat start">
              Repeats use ordered, nonoverlapping ranges and 2–32 passes. An
              optional first-ending start skips that ending on the final pass.
            </HelpNote></span><input aria-label="Repeat start"
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
              ><button
                type="button"
                class="sd-primary"
                :disabled="!editable"
                @click="add('repeats')"
              >
                {{ repeatIndex === null ? 'Add repeat' : 'Update repeat' }}
              </button>
            </div>
            <ul class="sd-list" aria-label="Repeats">
              <li v-for="(r, i) in project.score.repeats" :key="`r${i}`">
                <span
                  >𝄆 {{ r.start }}–{{ r.end }} 𝄇 × {{ r.times
                  }}<template v-if="r.first_ending != null">
                    · first ending {{ r.first_ending }}–{{ r.end }}</template
                  ></span
                >
                <button
                  type="button"
                  :disabled="!editable"
                  @click="remove('repeats', i)"
                >
                  Remove repeat
                </button>
              </li>
            </ul>

          </template>
          <template v-else>
            <div class="sd-row">
              <label
                ><span class="field-title">Jump at<HelpNote label="Jump at">
              Written repeats run before a D.C./D.S.; the jump is taken once and
              may end at Fine or use one coda pair.
            </HelpNote></span><input aria-label="Jump at"
                  v-model.number="jumpAt"
                  type="number"
                  min="0" /></label
              ><label
                >Jump to (0 = D.C.)<input
                  v-model.number="jumpTarget"
                  type="number"
                  min="0" /></label
              ><label>Fine<input v-model="fine" type="number" min="0" /></label
              ><label
                >To coda at<input
                  v-model="codaFrom"
                  type="number"
                  min="0" /></label
              ><label
                >Coda target<input
                  v-model="codaTo"
                  type="number"
                  min="0" /></label
              >
            </div>
            <div class="sd-actions">
              <button
                type="button"
                class="sd-primary"
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
                type="button"
                class="sd-danger"
                :disabled="!editable"
                @click="emit('update', { ...clone(), navigation: null })"
              >
                Clear jump
              </button>
            </div>
            <div class="help-section-title" v-if="project.score.navigation">Navigation<HelpNote label="Navigation">
              Current: {{ project.score.navigation.target === 0 ? 'D.C.' : 'D.S.' }}
              at beat {{ project.score.navigation.at }}<template
                v-if="project.score.navigation.fine != null"
              >
                · Fine at {{ project.score.navigation.fine }}</template
              ><template v-if="project.score.navigation.coda">
                · To coda {{ project.score.navigation.coda[0] }} → coda
                {{ project.score.navigation.coda[1] }}</template
              >.
            </HelpNote></div>

          </template>
        </section>
      </div>
    </template>
  </div>
</template>
<style src="./scoreDialogLayout.css"></style>
