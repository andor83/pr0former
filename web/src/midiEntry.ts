import type { Staff } from './types'
import { spelling } from './score'
export type MidiEntryMode = 'off' | 'play' | 'hold'
export interface MidiNoteMessage {
  kind: 'on' | 'off'
  pitch: number
  velocity: number
  channel: number
}
/** Note on/off from a raw MIDI packet; running status and other messages are ignored. */
export function parseMidiMessage(
  data: ArrayLike<number> | null | undefined,
): MidiNoteMessage | null {
  if (!data || data.length < 3) return null
  const status = data[0]! & 0xf0,
    channel = (data[0]! & 0x0f) + 1,
    pitch = data[1]! & 0x7f,
    velocity = data[2]! & 0x7f
  if (status === 0x90 && velocity > 0)
    return { kind: 'on', pitch, velocity, channel }
  if (status === 0x80 || status === 0x90)
    return { kind: 'off', pitch, velocity, channel }
  return null
}
/** Written step/alter for a sounding MIDI pitch on a (possibly transposing) staff in a key. */
export function midiHead(pitch: number, staff: Staff, key: string | null | undefined) {
  if (pitch < 0 || pitch > 127) return null
  return spelling(pitch, { ...staff, key_signature: key ?? null })
}
/** Held-note tracker shared by “play to enter” (chords while held) and “hold + number”. */
export class HeldNotes {
  private readonly held = new Map<number, number>()
  get pitches() {
    return [...this.held.keys()].sort((a, b) => a - b)
  }
  get size() {
    return this.held.size
  }
  /** Returns true when other notes were already held, i.e. this note joins a chord. */
  press(pitch: number, time = 0) {
    const chord = this.held.size > 0
    this.held.set(pitch, time)
    return chord
  }
  release(pitch: number) {
    this.held.delete(pitch)
  }
  clear() {
    this.held.clear()
  }
}
export const midiEntryStorageKey = 'pr0former.score.midiEntry'
export function readMidiEntryMode(storage?: Pick<Storage, 'getItem'>): MidiEntryMode {
  try {
    const value = storage?.getItem(midiEntryStorageKey)
    return value === 'play' || value === 'hold' ? value : 'off'
  } catch {
    return 'off'
  }
}
