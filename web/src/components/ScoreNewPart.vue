<script setup lang="ts">
import { ref } from 'vue'
import type { Part } from '../types'
import { newId } from '../id'
const props = defineProps<{ length: number; editable: boolean }>()
const emit = defineEmits<{ create: [part: Part]; cancel: [] }>()
const groups = [
  { name: 'Orchestral', presets: ['Violin', 'Viola', 'Cello', 'Double bass', 'Flute', 'Piccolo', 'Oboe', 'Bassoon', 'Clarinet in B♭', 'Horn in F', 'Trumpet in B♭', 'Trombone', 'Tuba', 'Harp'] },
  { name: 'Keyboard', presets: ['Piano', 'Organ', 'Electric piano', 'Harpsichord'] },
  { name: 'Percussion', presets: ['Drum kit', 'Snare drum', 'Bass drum', 'Timpani', 'Marimba', 'Vibraphone', 'Glockenspiel'] },
  { name: 'Guitar', presets: ['Classical guitar', 'Acoustic guitar', 'Electric guitar', 'Bass guitar'] },
]
const name = ref('Violin'), clef = ref('treble'), transpose = ref(0), layout = ref('single'), channel = ref(1), selected = ref('Violin')
function choose(preset: string) {
  selected.value = name.value = preset
  clef.value = preset === 'Viola' ? 'alto' : ['Cello', 'Double bass', 'Bassoon', 'Trombone', 'Tuba', 'Bass guitar', 'Timpani'].includes(preset) ? 'bass' : ['Drum kit', 'Snare drum', 'Bass drum'].includes(preset) ? 'percussion' : 'treble'
  layout.value = ['Piano', 'Organ', 'Electric piano', 'Harpsichord', 'Harp', 'Marimba'].includes(preset) ? 'grand' : 'single'
  transpose.value = preset.includes('B♭') ? -2 : preset === 'Horn in F' ? -7 : preset === 'Piccolo' ? 12 : preset === 'Glockenspiel' ? 24 : preset.includes('guitar') || preset === 'Double bass' ? -12 : 0
  channel.value = clef.value === 'percussion' ? 10 : 1
}
function create() {
  if (!props.editable || !name.value.trim() || !Number.isInteger(transpose.value) || Math.abs(transpose.value) > 48 || !Number.isInteger(channel.value) || channel.value < 1 || channel.value > 16) return
  emit('create', { id: newId(), name: name.value.trim(), performer: null, view: 'notation', clef: clef.value, notes: [], loop_beats: props.length, instrument_node: null, midi_channel: channel.value, osc_address: '/pr0former/note', staves: (layout.value === 'grand' ? [clef.value, 'bass'] : [clef.value]).map((c, i) => ({ id: newId(), name: layout.value === 'grand' ? (i ? 'Lower staff' : 'Upper staff') : name.value.trim(), clef: c, transpose: transpose.value })) })
}
</script>
<template>
  <form class="new-part" @submit.prevent="create">
    <aside aria-label="Instrument presets"><section v-for="group in groups" :key="group.name"><h3>{{ group.name }}</h3><button v-for="preset in group.presets" :key="preset" type="button" :aria-pressed="selected === preset" @click="choose(preset)">{{ preset }}</button></section></aside>
    <div class="definition">
      <label>Part name<input v-model="name" maxlength="120" required /></label>
      <label>Staff layout<select v-model="layout"><option value="single">Single staff</option><option value="grand">Grand staff (two staves)</option></select></label>
      <label>Clef<select v-model="clef"><option v-for="c in ['treble', 'bass', 'alto', 'tenor', 'percussion']" :key="c">{{ c }}</option></select></label>
      <label>Sounding transposition (semitones)<input v-model.number="transpose" type="number" min="-48" max="48" step="1" required /></label>
      <label>MIDI channel<input v-model.number="channel" type="number" min="1" max="16" step="1" required /></label>
      <p>Presets configure notation, not an audio instrument. Assign playback in Part settings. Percussion noteheads and roll marks are available in Entry settings and Edit selected notes; pitches are not automatically mapped to a drum kit.</p>
      <footer><button type="submit" :disabled="!editable">Create part</button><button type="button" @click="emit('cancel')">Cancel</button></footer>
    </div>
  </form>
</template>
<style scoped>
.new-part { display: grid; grid-template-columns: minmax(150px, 220px) minmax(220px, 1fr); gap: 24px; }
aside { max-height: 60vh; overflow-y: auto; border-right: 1px solid #444; padding-right: 12px; }
h3 { color: #c4adf5; font-size: 13px; }
aside button { display: block; width: 100%; text-align: left; min-height: 36px; margin-bottom: 3px; }
aside button[aria-pressed=true] { border-color: #53d9ef; color: #53d9ef; }
.definition { display: flex; flex-direction: column; gap: 16px; }
label { display: flex; flex-direction: column; gap: 5px; }
p { font-size: 12px; color: #bbb; max-width: 45ch; }
footer { display: flex; gap: 10px; }
@media (max-width: 520px) { .new-part { grid-template-columns: 135px minmax(0, 1fr); gap: 10px; } }
</style>
