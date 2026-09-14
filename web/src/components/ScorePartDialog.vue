<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { Part, Project, Staff } from '../types'
import { keyLabel, keyNames, staves } from '../score'
const props = defineProps<{
  project: Project
  part: Part
  editable: boolean
  members?: { id: string; username: string }[]
  initialStaff?: string
}>()
const emit = defineEmits<{
  update: [part: Part]
  staff: [staff: Staff, patch: Partial<Staff>]
  meter: [key: 'beats_per_bar' | 'beat_unit', value: number]
  addStaff: []
}>()
const list = computed(() => staves(props.part))
const tab = ref<string>(props.initialStaff ? `staff:${props.initialStaff}` : 'part')
watch(list, (l) => {
  if (tab.value.startsWith('staff:') && !l.some((s) => `staff:${s.id}` === tab.value))
    tab.value = l.length ? `staff:${l.at(-1)!.id}` : 'part'
})
const staff = computed(() =>
  list.value.find((s) => `staff:${s.id}` === tab.value),
)
const instruments = computed(() =>
  props.project.graph.nodes.filter((n) =>
    ['synth', 'fm_synth', 'browser_input', 'input'].includes(n.kind),
  ),
)
const clefBeat = ref(4),
  newClef = ref('bass')
const units = [1, 2, 4, 8, 16, 32]
const accidentals = (k: string) => {
  const f = keyNames.indexOf(k) - 7
  return f === 0 ? '♮' : `${Math.abs(f)}${f > 0 ? '♯' : '♭'}`
}
const clefGlyph = (c: string) =>
  ({ treble: '𝄞', bass: '𝄢', alto: '𝄡', tenor: '𝄡', percussion: 'Ⅱ' })[c] ?? '𝄞'
const value = (event: Event) => (event.target as HTMLInputElement).value
const update = (patch: Partial<Part>) => emit('update', { ...props.part, ...patch })
const performanceMeters = computed(() => {
  const meters = [...(props.part.performance_meters || [])]
  if (!meters.some(meter => meter.beat === 0)) meters.unshift({ beat:0, beats:props.project.beats_per_bar, unit:props.project.beat_unit || 4 })
  return meters.sort((a,b) => a.beat-b.beat)
})
const performanceMeter = computed(() => performanceMeters.value[0]!)
function setPerformanceMeter(index:number, patch:{beat?:number;beats?:number;unit?:number}) {
  const meters = performanceMeters.value.map(meter => ({...meter}))
  meters[index] = {...meters[index]!,...patch}
  update({performance_meters:meters.sort((a,b)=>a.beat-b.beat)})
}
function updatePerformanceMeter(patch:{beats?:number;unit?:number}) { setPerformanceMeter(0,patch) }
function addPerformanceMeter() {
  const used = new Set(performanceMeters.value.map(meter=>meter.beat))
  let beat = Math.min(props.part.loop_beats-0.25, Math.max(0.25, Math.floor(props.part.loop_beats/2)))
  while (used.has(beat) && beat < props.part.loop_beats) beat += 0.25
  if (beat >= props.part.loop_beats) return
  update({performance_meters:[...performanceMeters.value,{beat,beats:performanceMeter.value.beats,unit:performanceMeter.value.unit}].sort((a,b)=>a.beat-b.beat)})
}
function removePerformanceMeter(index:number) { update({performance_meters:performanceMeters.value.filter((_,item)=>item!==index)}) }
</script>
<template>
  <div class="sd-dialog">
    <div class="sd-strip">
      <strong>{{ part.name }}</strong>
      <span>{{
        members?.find((m) => m.id === part.performer)?.username || 'Unassigned'
      }}</span>
      <span
        >{{
          project.graph.nodes.find((n) => n.id === part.instrument_node)
            ?.label || 'External / acoustic'
        }}
        · MIDI {{ part.midi_channel || 1 }}</span
      >
      <small>{{ list.length }} staff/staves · loop {{ part.loop_beats }} beats</small>
    </div>
    <div class="sd-layout">
      <nav class="sd-nav" aria-label="Part settings sections">
        <button
          type="button"
          :class="{ active: tab === 'part' }"
          :aria-pressed="tab === 'part'"
          aria-label="Part"
          @click="tab = 'part'"
        >
          <span class="sd-symbol">♩</span>Part
        </button>
        <button
          type="button"
          :class="{ active: tab === 'routing' }"
          :aria-pressed="tab === 'routing'"
          aria-label="Routing"
          @click="tab = 'routing'"
        >
          <span class="sd-symbol">⇉</span>Routing
        </button>
        <button
          type="button"
          :class="{ active: tab === 'meter' }"
          :aria-pressed="tab === 'meter'"
          aria-label="Meter"
          @click="tab = 'meter'"
        >
          <span class="sd-symbol">⁴₄</span>Meter
        </button>
        <button
          v-for="s in list"
          :key="s.id"
          type="button"
          :class="{ active: tab === `staff:${s.id}` }"
          :aria-pressed="tab === `staff:${s.id}`"
          :aria-label="`Staff settings: ${s.name}`"
          @click="tab = `staff:${s.id}`"
        >
          <span class="sd-symbol">{{ clefGlyph(s.clef) }}</span
          ><span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">{{
            s.name
          }}</span>
        </button>
        <button
          type="button"
          class="sd-nav-action"
          :disabled="!editable || list.length >= 8"
          aria-label="＋ Add staff"
          @click="emit('addStaff')"
        >
          <span class="sd-symbol">＋</span>Add staff
        </button>
      </nav>
      <section class="sd-section">
        <template v-if="tab === 'part'">
          <div class="sd-fields">
            <label
              ><span class="field-title">Part name</span><input aria-label="Part name"
                :value="part.name"
                :disabled="!editable"
                @change="update({ name: value($event) })"
            /></label>
            <label
              >Performer<select
                aria-label="Performer"
                :value="part.performer || ''"
                :disabled="!editable"
                @change="update({ performer: value($event) || null })"
              >
                <option value="">Unassigned</option>
                <option v-for="m in members?.filter(member=>member.id!==project.conductor)" :key="m.id" :value="m.id">
                  {{ m.username }}
                </option>
              </select></label
            >
            <label
              >Default display<select
                aria-label="Default display"
                :value="part.view"
                :disabled="!editable"
                @change="update({ view: value($event) })"
              >
                <option value="notation">Notation</option>
                <option value="grid">Piano roll</option>
              </select></label
            ><label
              >Time signature<select
                aria-label="Time signature"
                :value="part.show_time_signature === false ? 'hide' : 'show'"
                :disabled="!editable"
                @change="update({ show_time_signature: value($event) === 'show' })"
              >
                <option value="show">Show</option>
                <option value="hide">Hidden</option>
              </select></label
            >
            <label
              ><span class="field-title">Loop length (quarter beats)<HelpNote label="Loop length">
            Loop length applies to independent conducted/freeform parts; structured
            scores follow the shared timeline. Each staff has its own clef, key and
            routing tab.
          </HelpNote></span><input aria-label="Loop length (quarter beats)"
                type="number"
                min="0.25"
                max="4096"
                :value="part.loop_beats"
                :disabled="!editable"
                @change="update({ loop_beats: Number(value($event)) })"
            /></label>
          </div>

        </template>
        <template v-else-if="tab === 'routing'">
          <div class="sd-fields">
            <label
              ><span class="field-title">Instrument / input<HelpNote label="Instrument / input">
            The graph instrument receives score notes and MIDI automation; MIDI and
            OSC routes deliver the same events to external devices. Staves can
            override the instrument and MIDI route individually.
          </HelpNote></span><select
                aria-label="Instrument / input"
                :value="part.instrument_node || ''"
                :disabled="!editable"
                @change="update({ instrument_node: value($event) || null })"
              >
                <option value="">Acoustic / external only</option>
                <option v-for="n in instruments" :key="n.id" :value="n.id">
                  {{ n.label }}
                </option>
              </select></label
            >
            <label
              >MIDI channel<input
                type="number"
                min="1"
                max="16"
                :value="part.midi_channel || 1"
                :disabled="!editable"
                @change="update({ midi_channel: Number(value($event)) })"
            /></label>
            <label
              >MIDI port<input
                :value="part.midi_port || ''"
                :disabled="!editable"
                placeholder="Default output"
                @change="update({ midi_port: value($event) || null })"
            /></label>
            <label
              >OSC destination<input
                :value="part.osc_destination || ''"
                :disabled="!editable"
                placeholder="host:port"
                @change="update({ osc_destination: value($event) || null })"
            /></label>
            <label
              >OSC address<input
                :value="part.osc_address"
                :disabled="!editable"
                @change="update({ osc_address: value($event) })"
            /></label>
          </div>

        </template>
        <template v-else-if="tab === 'meter'">
          <template v-if="project.mode==='conducted'">
            <div class="sd-meter-editor"><div class="sd-meter-preview"><span>{{performanceMeter.beats}}</span><span>{{performanceMeter.unit}}</span></div><div class="sd-steppers"><label><span class="field-title">Part beats per bar<HelpNote label="Part beats per bar">This part’s denominator beat follows one global conducting pulse. Other parts may use different meters without changing pulse duration.</HelpNote></span><input aria-label="Part beats per bar" type="number" min="1" max="16" :value="performanceMeter.beats" :disabled="!editable" @change="updatePerformanceMeter({beats:Number(value($event))})"></label><label>Part beat unit<select :value="performanceMeter.unit" :disabled="!editable" @change="updatePerformanceMeter({unit:Number(value($event))})"><option v-for="u in units" :key="u" :value="u">{{u}}</option></select></label></div></div>
            <div class="performance-meter-changes"><header><strong>Meter changes</strong><button type="button" class="button small" :disabled="!editable||part.loop_beats<=.25" @click="addPerformanceMeter">Add change</button></header><div v-for="(meter,index) in performanceMeters.slice(1)" :key="`${meter.beat}:${index}`" class="performance-meter-row"><label>Beat<input type="number" min="0.25" :max="part.loop_beats-0.001" step="0.25" :value="meter.beat" :disabled="!editable" @change="setPerformanceMeter(index+1,{beat:Number(value($event))})"></label><label>Beats<input type="number" min="1" max="16" :value="meter.beats" :disabled="!editable" @change="setPerformanceMeter(index+1,{beats:Number(value($event))})"></label><label>Unit<select :value="meter.unit" :disabled="!editable" @change="setPerformanceMeter(index+1,{unit:Number(value($event))})"><option v-for="u in units" :key="u" :value="u">{{u}}</option></select></label><button type="button" class="icon-button" aria-label="Remove meter change" :disabled="!editable" @click="removePerformanceMeter(index+1)">×</button></div><p v-if="performanceMeters.length===1" class="feature-note">No later meter changes. Each denominator beat always maps to one global pulse.</p></div>

          </template>
          <template v-else>
          <div class="sd-meter-editor">
            <div class="sd-meter-preview" aria-live="polite">
              <span>{{ project.beats_per_bar }}</span
              ><span>{{ project.beat_unit || 4 }}</span>
            </div>
            <div class="sd-steppers">
              <div class="sd-stepper">
                <span>Beats per bar</span>
                <button
                  type="button"
                  aria-label="Fewer beats"
                  :disabled="!editable"
                  @click="emit('meter', 'beats_per_bar', Math.max(1, project.beats_per_bar - 1))"
                >
                  −
                </button>
                <input
                  type="number"
                  min="1"
                  max="16"
                  aria-label="Beats per bar"
                  :value="project.beats_per_bar"
                  :disabled="!editable"
                  @change="emit('meter', 'beats_per_bar', Number(value($event)))"
                />
                <button
                  type="button"
                  aria-label="More beats"
                  :disabled="!editable"
                  @click="emit('meter', 'beats_per_bar', Math.min(16, project.beats_per_bar + 1))"
                >
                  +
                </button>
              </div>
              <div class="sd-stepper">
                <span>Beat unit</span>
                <button
                  type="button"
                  aria-label="Longer beat unit"
                  :disabled="!editable"
                  @click="
                    emit(
                      'meter',
                      'beat_unit',
                      units[Math.max(0, units.indexOf(project.beat_unit || 4) - 1)]!,
                    )
                  "
                >
                  −
                </button>
                <select
                  aria-label="Beat unit"
                  :value="project.beat_unit || 4"
                  :disabled="!editable"
                  @change="emit('meter', 'beat_unit', Number(value($event)))"
                >
                  <option v-for="u in units" :key="u" :value="u">{{ u }}</option>
                </select>
                <button
                  type="button"
                  aria-label="Shorter beat unit"
                  :disabled="!editable"
                  @click="
                    emit(
                      'meter',
                      'beat_unit',
                      units[
                        Math.min(units.length - 1, units.indexOf(project.beat_unit || 4) + 1)
                      ]!,
                    )
                  "
                >
                  +
                </button>
              </div>
            </div>
          </div>
          <div class="help-section-title">Project meter<HelpNote label="Project meter">
            The initial time signature for the whole project, also used for the
            count-in. Mid-score meter changes are made from the Measure dialog or
            the Shared score dialog.
          </HelpNote></div>
          </template>
        </template>
        <template v-else-if="staff">
          <h3>{{ staff.name }}</h3>
          <div class="sd-fields">
            <label
              ><span class="field-title">Staff name</span><input aria-label="Staff name"
                :value="staff.name"
                :disabled="!editable"
                @change="emit('staff', staff, { name: value($event) })"
            /></label>
            <label
              >Clef<select
                aria-label="Clef"
                :value="staff.clef"
                :disabled="!editable"
                @change="emit('staff', staff, { clef: value($event) })"
              >
                <option v-for="c in ['treble', 'bass', 'alto', 'tenor', 'percussion']" :key="c">
                  {{ c }}
                </option>
              </select></label
            >
            <label
              >Key signature<select
                aria-label="Key signature"
                :value="staff.key_signature || ''"
                :disabled="!editable"
                @change="emit('staff', staff, { key_signature: value($event) || null })"
              >
                <option value="">Inherit score / part key</option>
                <option v-for="k in keyNames" :key="k" :value="k">
                  {{ keyLabel(k, staff.key_mode) }}
                </option>
              </select></label
            >
            <label
              >Key mode<select
                aria-label="Key mode"
                :value="staff.key_mode || 'major'"
                :disabled="!editable"
                @change="
                  emit('staff', staff, { key_mode: value($event) as 'major' | 'minor' })
                "
              >
                <option value="major">Major</option>
                <option value="minor">Minor</option>
              </select></label
            >
          </div>
          <div class="sd-quick" role="radiogroup" aria-label="Clef glyphs">
            <button
              v-for="c in ['treble', 'bass', 'alto', 'tenor', 'percussion']"
              :key="c"
              type="button"
              role="radio"
              :aria-checked="staff.clef === c"
              :aria-label="`${c} clef glyph`"
              :disabled="!editable"
              @click="emit('staff', staff, { clef: c })"
            >
              <span class="sd-symbol">{{ clefGlyph(c) }}</span>{{ c }}
            </button>
          </div>
          <div class="sd-keys" role="radiogroup" aria-label="Key signature glyphs">
            <button
              type="button"
              role="radio"
              :aria-checked="!staff.key_signature"
              aria-label="Inherit key glyph"
              :disabled="!editable"
              @click="emit('staff', staff, { key_signature: null })"
            >
              <strong>—</strong><small>inherit</small>
            </button>
            <button
              v-for="k in keyNames"
              :key="k"
              type="button"
              role="radio"
              :aria-checked="staff.key_signature === k"
              :aria-label="`${keyLabel(k, staff.key_mode)} glyph`"
              :disabled="!editable"
              @click="emit('staff', staff, { key_signature: k })"
            >
              <strong>{{
                staff.key_mode === 'minor'
                  ? keyLabel(k, 'minor').replace(' minor', 'm')
                  : k
              }}</strong
              ><small>{{ accidentals(k) }}</small>
            </button>
          </div>
          <div class="sd-stepper">
            <span>Sounding transposition<HelpNote label="Sounding transposition">
            Semitones between written and sounding pitch (Horn in F: −7, B♭
            clarinet: −2). Written spelling is kept; playback follows the sounding
            pitch.
          </HelpNote></span>
            <button
              type="button"
              aria-label="Transpose down a semitone"
              :disabled="!editable"
              @click="emit('staff', staff, { transpose: Math.max(-48, staff.transpose - 1) })"
            >
              −
            </button>
            <input
              type="number"
              min="-48"
              max="48"
              aria-label="Sounding transposition"
              :value="staff.transpose"
              :disabled="!editable"
              @change="emit('staff', staff, { transpose: Number(value($event)) })"
            />
            <button
              type="button"
              aria-label="Transpose up a semitone"
              :disabled="!editable"
              @click="emit('staff', staff, { transpose: Math.min(48, staff.transpose + 1) })"
            >
              +
            </button>
          </div>

          <h3>Staff routing</h3>
          <div class="sd-fields">
            <label
              >Staff instrument<select
                aria-label="Staff instrument"
                :value="staff.instrument_node || ''"
                :disabled="!editable"
                @change="emit('staff', staff, { instrument_node: value($event) || null })"
              >
                <option value="">Inherit part</option>
                <option v-for="n in instruments" :key="n.id" :value="n.id">
                  {{ n.label }}
                </option>
              </select></label
            >
            <label
              >Staff MIDI port<input
                :value="staff.midi_port || ''"
                :disabled="!editable"
                placeholder="Inherit part"
                @change="emit('staff', staff, { midi_port: value($event) || null })"
            /></label>
            <label
              >Staff MIDI channel<input
                type="number"
                min="1"
                max="16"
                placeholder="Inherit"
                :value="staff.midi_channel ?? ''"
                :disabled="!editable"
                @change="
                  emit('staff', staff, {
                    midi_channel: value($event) ? Number(value($event)) : null,
                  })
                "
            /></label>
          </div>
          <h3>Clef changes</h3>
          <div class="sd-row">
            <label
              >Change at beat<input
                v-model.number="clefBeat"
                type="number"
                min="0"
                step="0.25" /></label
            ><label
              >New clef<select v-model="newClef" aria-label="New clef">
                <option v-for="c in ['treble', 'bass', 'alto', 'tenor', 'percussion']" :key="c">
                  {{ c }}
                </option>
              </select></label
            ><button
              type="button"
              class="sd-primary"
              style="min-height: 40px; border-radius: 4px"
              :disabled="!editable"
              @click="
                emit('staff', staff, {
                  clef_changes: [
                    ...(staff.clef_changes || []).filter((c) => c.beat !== clefBeat),
                    { beat: clefBeat, clef: newClef },
                  ].sort((a, b) => a.beat - b.beat),
                })
              "
            >
              Add clef change
            </button>
          </div>
          <ul v-if="staff.clef_changes?.length" class="sd-list" aria-label="Clef changes">
            <li v-for="c in staff.clef_changes" :key="c.beat">
              <span>Beat {{ c.beat }}: {{ clefGlyph(c.clef) }} {{ c.clef }}</span>
              <button
                type="button"
                :disabled="!editable"
                @click="
                  emit('staff', staff, {
                    clef_changes: staff.clef_changes!.filter((x) => x.beat !== c.beat),
                  })
                "
              >
                Remove
              </button>
            </li>
          </ul>
        </template>
      </section>
    </div>
  </div>
</template>
<style src="./scoreDialogLayout.css"></style>
<style scoped>
.performance-meter-changes{display:grid;gap:10px;margin-top:18px;padding-top:14px;border-top:1px solid var(--line)}
.performance-meter-changes>header,.performance-meter-row{display:flex;align-items:end;gap:10px}.performance-meter-changes>header{justify-content:space-between;align-items:center}.performance-meter-row label{min-width:0;flex:1}.performance-meter-row input,.performance-meter-row select{width:100%}.performance-meter-row .icon-button{flex:0 0 40px}
</style>
