<script setup lang="ts">
import { computed } from 'vue'
import { chordQualities, chordRoots, splitChord } from '../chordSymbols'
const props = defineProps<{ modelValue: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: string]; submit: [] }>()
const chord = computed(() => {
  const c = splitChord(props.modelValue) ?? { root: 'C', quality: '', bass: '' }
  return { ...c, root: c.root.replace('b', '♭').replace('#', '♯'), bass: c.bass.replace('b', '♭').replace('#', '♯') }
})
function change(patch: Partial<ReturnType<typeof splitChord>>) {
  const next = { ...chord.value, ...patch }
  emit('update:modelValue', `${next.root}${next.quality}${next.bass ? `/${next.bass}` : ''}`)
}
</script>
<template>
  <section class="chord-editor" aria-label="Jazz chord symbol editor">
    <output aria-label="Chord preview">{{modelValue || 'Cmaj7'}}</output>
    <label>Chord symbol<input :value="modelValue" aria-label="Chord symbol" maxlength="64" placeholder="Dm7, G7♭9, Cmaj7/E, N.C." autofocus @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)" @keydown.enter.prevent="emit('submit')" /></label>
    <div class="chord-pitches"><label>Root<select :value="chord.root" aria-label="Chord root" @change="change({root:($event.target as HTMLSelectElement).value})"><option v-for="root in chordRoots" :key="root">{{root}}</option></select></label>
      <label>Slash bass<select :value="chord.bass" aria-label="Chord bass" @change="change({bass:($event.target as HTMLSelectElement).value})"><option value="">None</option><option v-for="root in chordRoots" :key="root">{{root}}</option></select></label></div>
    <div class="chord-qualities" role="group" aria-label="Jazz chord qualities"><button v-for="quality in chordQualities" :key="quality" type="button" :aria-label="`Chord quality ${quality}`" :aria-pressed="chord.quality===quality" @click="change({quality})">{{quality}}</button></div>
    <p>Choose a root and quality, or type any chord symbol, including altered extensions, slash chords and N.C. Chord symbols guide performers; enter notes separately for playback.</p>
  </section>
</template>
<style scoped>
.chord-editor{display:grid;gap:14px}.chord-editor output{font:600 30px 'Space Grotesk',sans-serif;min-height:48px;overflow-wrap:anywhere;color:var(--amber)}.chord-editor label{display:grid;gap:6px;font-size:12px}.chord-pitches{display:grid;grid-template-columns:1fr 1fr;gap:12px}.chord-editor input,.chord-editor select{min-height:40px;padding:8px;min-width:0}.chord-qualities{display:flex;flex-wrap:wrap;gap:6px}.chord-qualities button{min-height:40px;min-width:54px}.chord-qualities [aria-pressed='true']{border-color:var(--amber);color:var(--amber);background:#fff4df}.chord-editor p{font-size:12px;color:var(--muted);line-height:1.6}
</style>
