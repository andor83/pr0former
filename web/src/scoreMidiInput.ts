import { onBeforeUnmount, ref, watch } from 'vue'
import { HeldNotes, parseMidiMessage, readMidiEntryMode, midiEntryStorageKey, type MidiEntryMode } from './midiEntry'

/** Browser input lifetime is independent of score editing and gesture state. */
export function useScoreMidiInput(noteOn: (pitch: number, chord: boolean) => void) {
  const mode = ref<MidiEntryMode>(readMidiEntryMode(typeof localStorage === 'undefined' ? undefined : localStorage))
  const inputs = ref<string[]>([]), error = ref('')
  const held = new HeldNotes()
  let access: MIDIAccess | null = null
  let generation = 0
  function message(event: MIDIMessageEvent) {
    const data = parseMidiMessage(event.data)
    if (!data) return
    if (data.kind === 'off') held.release(data.pitch)
    else noteOn(data.pitch, held.press(data.pitch, event.timeStamp))
  }
  function bind() {
    if (!access) return
    inputs.value = [...access.inputs.values()].map(input => { input.onmidimessage = message; return input.name || input.id })
  }
  function detach() {
    generation++
    if (access) {
      access.onstatechange = null
      for (const input of access.inputs.values()) input.onmidimessage = null
    }
    held.clear()
    inputs.value = []
  }
  async function attach() {
    const request = ++generation
    error.value = ''
    if (!('requestMIDIAccess' in navigator)) { error.value = 'Web MIDI is not available in this browser.'; mode.value = 'off'; return }
    try {
      const granted = access ?? await navigator.requestMIDIAccess()
      if (request !== generation) return
      access = granted
      access.onstatechange = bind
      bind()
    } catch (e) {
      if (request !== generation) return
      error.value = `MIDI access failed: ${e instanceof Error ? e.message : e}`
      mode.value = 'off'
    }
  }
  watch(mode, value => {
    try { localStorage.setItem(midiEntryStorageKey, value) } catch { /* private storage */ }
    if (value === 'off') detach()
    else void attach()
  }, { immediate: true })
  onBeforeUnmount(detach)
  return { mode, inputs, error, held }
}
