<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { Renderer, Stave, StaveNote, Voice, Formatter, Accidental, Dot, StaveTie } from 'vexflow'
import { Plus, Trash2 } from 'lucide-vue-next'
import type { Part, Note } from '../types'
import { durationGlyphs } from '../notation'
const props = defineProps<{ part: Part; beat: number; editable: boolean; performance?: boolean; beatsPerBar: number; beatUnit: number }>()
const emit = defineEmits<{ update: [part: Part]; meter: [beats: number, unit: number] }>()
const unsupported = computed(() => props.part.notes.filter(n => !durationGlyphs(n.duration)).length)
const staff = ref<HTMLDivElement>()
const scroll = ref<HTMLDivElement>()
const follow = ref(true)
const origin = ref(140)
const pixelsPerBeat = 100
const playheadX = computed(() => origin.value + localBeat.value * pixelsPerBeat)
const selected = ref<string | null>(null)
const view = ref(props.part.view)
const partError = ref('')
const outsideLoop = computed(() => props.part.notes.filter(n => n.beat + n.duration > props.part.loop_beats).length)
const note = computed(() => props.part.notes.find(n => n.id === selected.value))
const pitches = Array.from({ length: 25 }, (_, i) => 84 - i)
const keys = ['Cb', 'Gb', 'Db', 'Ab', 'Eb', 'Bb', 'F', 'C', 'G', 'D', 'A', 'E', 'B', 'F#', 'C#']
const names = ['C', 'C♯', 'D', 'D♯', 'E', 'F', 'F♯', 'G', 'G♯', 'A', 'A♯', 'B']
const localBeat = computed(() => props.beat % props.part.loop_beats)
function update(part: Part) { if (props.editable) emit('update', part) }
function setupPart(patch: Partial<Part>) {
  const next = { ...props.part, ...patch }
  next.name = next.name.trim()
  partError.value = ''
  if (!next.name || new TextEncoder().encode(next.name).length > 120) { partError.value = 'Part name must contain 1–120 bytes.'; return }
  if (!Number.isFinite(next.loop_beats) || next.loop_beats < 0.25 || next.loop_beats > 4096) { partError.value = 'Loop length must be between 0.25 and 4096 quarter beats.'; return }
  update(next)
}
function add(pitch: number, beat: number) { if (!props.editable) return; const n: Note = { id: crypto.randomUUID(), pitch, beat, duration: 1, velocity: 90, rest: false, tied: false }; selected.value = n.id; update({ ...props.part, notes: [...props.part.notes, n].sort((a, b) => a.beat - b.beat) }) }
function gridClick(event: MouseEvent, pitch: number) { const rect = (event.currentTarget as HTMLElement).getBoundingClientRect(); add(pitch, Math.floor(((event.clientX - rect.left) / rect.width * props.part.loop_beats) * 4) / 4) }
function edit(key: keyof Note, value: number | boolean) { if (!note.value) return; update({ ...props.part, notes: props.part.notes.map(n => n.id === selected.value ? { ...n, [key]: value } : n) }) }
function remove() { update({ ...props.part, notes: props.part.notes.filter(n => n.id !== selected.value) }); selected.value = null }
function render() {
  if (!staff.value || view.value !== 'notation') return
  staff.value.innerHTML = ''
  try {
    const width = Math.max(900, props.part.loop_beats * pixelsPerBeat + 360)
    const renderer = new Renderer(staff.value, Renderer.Backends.SVG); renderer.resize(width, 200)
    const context = renderer.getContext(); context.setFillStyle('#dedbd0'); context.setStrokeStyle('#a5aaa8')
    const stave = new Stave(20, 30, width - 40); stave.addClef(props.part.clef); if (props.part.key_signature) stave.addKeySignature(props.part.key_signature); if (props.part.show_time_signature !== false) stave.addTimeSignature(`${props.beatsPerBar}/${props.beatUnit}`); stave.setContext(context).draw()
    const list = props.part.notes.flatMap(n => {
      const glyphs = durationGlyphs(n.duration)
      let offset = 0
      return (glyphs || [{ duration: 'q', beats: n.duration, dots: 0 }]).map((glyph, fragment) => {
        const item = { ...n, beat: n.beat + offset, glyph, fragment, unsupported: !glyphs }
        offset += glyph.beats
        return item
      })
    }).sort((a, b) => a.beat - b.beat)
    origin.value = stave.getNoteStartX() + 40
    if (!list.length) return
    const notes = list.map(n => {
      const pitchNames = ['c', 'c', 'd', 'd', 'e', 'f', 'f', 'g', 'g', 'a', 'a', 'b']; const octave = Math.floor(n.pitch / 12) - 1
      const duration = n.glyph.duration
      const vn = new StaveNote({ clef: props.part.clef, keys: [`${pitchNames[n.pitch % 12]}${[1, 3, 6, 8, 10].includes(n.pitch % 12) ? '#' : ''}/${octave}`], duration: duration + (n.rest ? 'r' : ''), dots: n.glyph.dots })
      for (let dot = 0; dot < n.glyph.dots; dot++) Dot.buildAndAttach([vn], { all: true })
      return vn
    })
    const voice = new Voice({ numBeats: props.part.loop_beats, beatValue: 4 }).setStrict(false); voice.addTickables(notes); Accidental.applyAccidentals([voice], props.part.key_signature || 'C')
    new Formatter().joinVoices([voice]).format([voice], width - 180)
    notes.forEach((vn, i) => {
      vn.setStave(stave)
      vn.getTickContext().setX(0)
      const offset = vn.getNoteHeadBeginX()
      vn.getTickContext().setX(origin.value + list[i]!.beat * pixelsPerBeat - offset)
    })
    voice.draw(context, stave)
    const previous = new Map<string, StaveNote>()
    notes.forEach((vn, i) => {
      const item = list[i]!
      const first = previous.get(item.id)
      if (first && !item.rest) new StaveTie({ firstNote: first, lastNote: vn, firstIndexes: [0], lastIndexes: [0] }).setContext(context).draw()
      previous.set(item.id, vn)
      if (item.unsupported) context.fillText(`${item.duration} beats*`, origin.value + item.beat * pixelsPerBeat, 185)
      const element = vn.getSVGElement()
      if (element) { element.dataset.scoreBeat = String(list[i]!.beat); element.dataset.noteId = list[i]!.id; element.dataset.duration = list[i]!.glyph.duration; element.dataset.dots = String(list[i]!.glyph.dots) }
    })
  } catch (e) { staff.value.textContent = `Notation rendering: ${e instanceof Error ? e.message : e}` }
}
watch(() => [props.part.id, props.part.view], () => { view.value = props.part.view; selected.value = null; partError.value = '' })
watch(() => props.beat, () => {
  if (!follow.value || props.editable || !scroll.value) return
  const viewport = scroll.value
  const x = view.value === 'notation' ? playheadX.value : 56 + localBeat.value / props.part.loop_beats * (viewport.scrollWidth - 56)
  if (x < viewport.scrollLeft || x > viewport.scrollLeft + viewport.clientWidth * 0.75) viewport.scrollLeft = Math.max(0, x - viewport.clientWidth * 0.25)
})
watch(() => [props.part, props.beatsPerBar, props.beatUnit, view.value], () => nextTick(render), { deep: true }); onMounted(render)
</script>
<template>
  <section class="score-editor">
    <header class="section-heading"><div><div class="eyebrow">PLAYER PART</div><h2>{{ part.name }}</h2></div><div class="segmented"><button :class="{ active: view === 'notation' }" @click="view = 'notation'">Notation</button><button :class="{ active: view === 'grid' }" @click="view = 'grid'">Piano roll</button></div><button v-if="!performance" class="button small" :disabled="!editable" @click="add(60, Math.min(part.loop_beats - 1, part.notes.length))"><Plus :size="14" /> Note</button></header>
    <div v-if="!performance" class="part-routing">
      <label>Part name<input :value="part.name" maxlength="120" required :disabled="!editable" @change="setupPart({ name: ($event.target as HTMLInputElement).value })"></label>
      <label>Loop length (quarter beats)<input :value="part.loop_beats" type="number" min="0.25" max="4096" step="any" :disabled="!editable" @change="setupPart({ loop_beats: Number(($event.target as HTMLInputElement).value) })"></label>
      <label>Default display<select aria-label="Default display" :value="part.view" :disabled="!editable" @change="setupPart({ view: ($event.target as HTMLSelectElement).value })"><option value="notation">Notation</option><option value="grid">Piano roll</option></select></label>
      <label>Clef<select aria-label="Clef" :value="part.clef" :disabled="!editable" @change="update({ ...part, clef: ($event.target as HTMLSelectElement).value })"><option value="treble">Treble</option><option value="bass">Bass</option><option value="alto">Alto</option><option value="tenor">Tenor</option></select></label>
      <label>Key signature<select aria-label="Key signature" :value="part.key_signature || ''" :disabled="!editable" @change="update({ ...part, key_signature: ($event.target as HTMLSelectElement).value || null })"><option value="">None</option><option v-for="key in keys" :key="key" :value="key">{{ key }} major</option></select></label>
      <label>Beats per bar<input type="number" min="1" max="16" step="1" :value="beatsPerBar" :disabled="!editable" @change="emit('meter', Number(($event.target as HTMLInputElement).value), beatUnit)"></label>
      <label>Beat unit<select aria-label="Beat unit" :value="beatUnit" :disabled="!editable" @change="emit('meter', beatsPerBar, Number(($event.target as HTMLSelectElement).value))"><option v-for="unit in [1, 2, 4, 8, 16, 32]" :key="unit" :value="unit">1/{{ unit }} note</option></select></label>
      <label>Time signature<select aria-label="Time signature" :value="part.show_time_signature !== false ? 'show' : 'hide'" :disabled="!editable" @change="update({ ...part, show_time_signature: ($event.target as HTMLSelectElement).value === 'show' })"><option value="show">Show {{ beatsPerBar }}/{{ beatUnit }}</option><option value="hide">Hidden</option></select></label>
    </div>
    <p v-if="partError" class="field-error" role="alert">{{ partError }}</p>
    <p v-if="outsideLoop" class="feature-note" role="status">{{ outsideLoop }} note(s) extend beyond the loop. Playback skips notes starting outside it and releases held notes at the loop end; stored notes are retained.</p>
    <p v-if="unsupported && view === 'notation'" class="field-error" role="status">{{ unsupported }} note duration(s) need custom rhythm notation. Notes marked * show their exact duration in quarter beats; their placeholder glyph does not represent the duration. Use Piano roll to view timing.</p>
    <label class="score-follow"><input v-model="follow" type="checkbox"> Follow playback</label>
    <div ref="scroll" class="score-scroll"><div v-if="view === 'notation'" class="notation-surface"><div ref="staff"></div><div class="score-playhead" :style="{ left: `${playheadX}px` }"></div></div>
      <div v-else class="piano-roll"><div class="beat-ruler"><span v-for="beatNo in Math.ceil(part.loop_beats)" :key="beatNo">{{ beatNo }}</span></div><div v-for="pitch in pitches" :key="pitch" class="piano-row" :class="{ black: [1, 3, 6, 8, 10].includes(pitch % 12) }"><span class="piano-key">{{ names[pitch % 12] }}{{ Math.floor(pitch / 12) - 1 }}</span><div class="piano-lane" @click="gridClick($event, pitch)"><button v-for="n in part.notes.filter(n => n.pitch === pitch)" :key="n.id" class="midi-note" :class="{ selected: selected === n.id }" :style="{ left: `${n.beat / part.loop_beats * 100}%`, width: `${n.duration / part.loop_beats * 100}%` }" :aria-label="`Note ${n.pitch} at beat ${n.beat}`" @click.stop="selected = n.id"></button></div></div><div class="grid-playhead" :style="{ left: `calc(56px + (100% - 56px) * ${localBeat / part.loop_beats})` }"></div></div>
    </div>
    <div v-if="!performance" class="note-strip"><button v-for="n in part.notes" :key="n.id" :class="{ selected: selected === n.id }" @click="selected = n.id">{{ n.rest ? 'Rest' : names[n.pitch % 12] }} <small>{{ n.beat + 1 }}</small></button></div>
    <div v-if="note && !performance" class="note-inspector"><label>Pitch<input type="number" min="0" max="127" :value="note.pitch" :disabled="!editable" @change="edit('pitch', +($event.target as HTMLInputElement).value)"></label><label>Beat<input type="number" min="0" step="0.25" :value="note.beat" :disabled="!editable" @change="edit('beat', +($event.target as HTMLInputElement).value)"></label><label>Duration<input type="number" min="0.0625" step="0.25" :value="note.duration" :disabled="!editable" @change="edit('duration', +($event.target as HTMLInputElement).value)"></label><label>Velocity<input type="number" min="0" max="127" :value="note.velocity" :disabled="!editable" @change="edit('velocity', +($event.target as HTMLInputElement).value)"></label><button class="icon-button danger" :disabled="!editable" aria-label="Delete note" @click="remove"><Trash2 :size="18" /></button></div>
  </section>
</template>
