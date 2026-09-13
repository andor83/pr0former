import { expect, it } from 'vitest'
import { chordHarmony, splitChord } from './chordSymbols'
it('separates jazz qualities from slash bass without treating 6/9 as a bass note', () => {
  expect(splitChord('Cmaj7/E')).toEqual({ root: 'C', quality: 'maj7', bass: 'E' })
  expect(splitChord('B♭7♯9/F')).toEqual({ root: 'B♭', quality: '7♯9', bass: 'F' })
  expect(splitChord('C6/9')).toEqual({ root: 'C', quality: '6/9', bass: '' })
  expect(splitChord('N.C.')).toBeNull()
})
it('exports chord qualities, slash bass, no-chord and exact display labels as harmony', () => {
  expect(chordHarmony('Dm7', 10080, 2)).toContain('<kind text="m7">minor-seventh</kind>')
  expect(chordHarmony('Cmaj7/E', 0, 1)).toContain('<bass><bass-step>E</bass-step></bass>')
  expect(chordHarmony('B♭7♯9', 0, 1)).toContain('<root-alter>-1</root-alter>')
  expect(chordHarmony('G7♭9', 0, 1)).toContain('<kind text="7♭9">other</kind>')
  expect(chordHarmony('N.C.', 0, 1)).toContain('<kind text="N.C.">none</kind>')
  expect(chordHarmony('C<"&', 0, 1)).toContain('text="&lt;&quot;&amp;"')
  expect(chordHarmony('free harmony', 0, 1)).toBeNull()
})
