import { describe, expect, it } from 'vitest'
import { HeldNotes, midiHead, parseMidiMessage, readMidiEntryMode } from './midiEntry'
describe('midi entry', () => {
  it('parses note on/off including running velocity-zero note-offs', () => {
    expect(parseMidiMessage([0x90, 60, 100])).toEqual({
      kind: 'on',
      pitch: 60,
      velocity: 100,
      channel: 1,
    })
    expect(parseMidiMessage([0x91, 60, 0])).toEqual({
      kind: 'off',
      pitch: 60,
      velocity: 0,
      channel: 2,
    })
    expect(parseMidiMessage([0x80, 61, 64])!.kind).toBe('off')
    expect(parseMidiMessage([0xb0, 64, 127])).toBeNull()
    expect(parseMidiMessage([0xf8])).toBeNull()
    expect(parseMidiMessage(null)).toBeNull()
  })
  it('spells sounding pitches for transposing staves in the current key', () => {
    const staff = { id: 's', name: 'S', clef: 'treble', transpose: 0 }
    expect(midiHead(61, staff, 'C')).toEqual({ step: 28, alter: 1 })
    expect(midiHead(61, staff, 'F')).toEqual({ step: 29, alter: -1 })
    // Horn in F: sounding F4 (65) is written C5 (step 35).
    expect(midiHead(65, { ...staff, transpose: -7 }, 'C')).toEqual({
      step: 35,
      alter: 0,
    })
    expect(midiHead(128, staff, 'C')).toBeNull()
  })
  it('tracks held notes so simultaneous presses become chords', () => {
    const held = new HeldNotes()
    expect(held.press(60)).toBe(false)
    expect(held.press(64)).toBe(true)
    expect(held.pitches).toEqual([60, 64])
    held.release(60)
    expect(held.press(67)).toBe(true)
    held.release(64)
    held.release(67)
    expect(held.press(60)).toBe(false)
    held.clear()
    expect(held.size).toBe(0)
  })
  it('reads a stored entry mode defensively', () => {
    expect(readMidiEntryMode({ getItem: () => 'hold' })).toBe('hold')
    expect(readMidiEntryMode({ getItem: () => 'nonsense' })).toBe('off')
    expect(
      readMidiEntryMode({
        getItem: () => {
          throw new Error('blocked')
        },
      }),
    ).toBe('off')
    expect(readMidiEntryMode(undefined)).toBe('off')
  })
})
