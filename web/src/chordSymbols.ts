/** Chord labels remain notation: parsing supports the editor and MusicXML, not voicing. */
export function splitChord(text: string) {
  const match = /^([A-G](?:[#b♭♯])?)(.*?)(?:\/([A-G](?:[#b♭♯])?))?$/.exec(text.trim())
  return match ? { root: match[1]!, quality: match[2]!, bass: match[3] ?? '' } : null
}
export const chordQualities = ['maj7', 'm7', '7', 'm7♭5', 'dim7', 'mMaj7', '6', '6/9', '9', '11', '13', '7sus4', '7♭9', '7♯9', 'maj7♯11', '7alt']
export const chordRoots = ['C', 'C♯', 'D♭', 'D', 'D♯', 'E♭', 'E', 'F', 'F♯', 'G♭', 'G', 'G♯', 'A♭', 'A', 'A♯', 'B♭', 'B']
export const chordKinds: Record<string, string> = {
  '': 'major', m: 'minor', maj7: 'major-seventh', 'Δ7': 'major-seventh', m7: 'minor-seventh', '7': 'dominant',
  m7b5: 'half-diminished', 'm7♭5': 'half-diminished', 'ø7': 'half-diminished', dim: 'diminished', dim7: 'diminished-seventh', '°7': 'diminished-seventh',
  mMaj7: 'major-minor', '6': 'major-sixth', m6: 'minor-sixth', '9': 'dominant-ninth', maj9: 'major-ninth', m9: 'minor-ninth',
  '11': 'dominant-11th', '13': 'dominant-13th', sus2: 'suspended-second', sus4: 'suspended-fourth', aug: 'augmented', '+': 'augmented',
}
const xmlEscape = (s: string) => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;')
/** Root, displayed quality and slash bass use MusicXML harmony. Unknown qualities stay literal. */
export function chordHarmony(text: string, offset: number, staff: number): string | null {
  const noChord = /^N\.?C\.?$/i.test(text.trim()), chord = splitChord(text)
  if (!chord && !noChord) return null
  const pitch = (name: string, kind: 'root' | 'bass') => {
    const alter = /[#♯]/.test(name) ? 1 : /[b♭]/.test(name) ? -1 : 0
    return `<${kind}><${kind}-step>${name[0]}</${kind}-step>${alter ? `<${kind}-alter>${alter}</${kind}-alter>` : ''}</${kind}>`
  }
  const body = noChord
    ? '<root><root-step text="">C</root-step></root><kind text="N.C.">none</kind>'
    : `${pitch(chord!.root, 'root')}<kind text="${xmlEscape(chord!.quality)}">${chordKinds[chord!.quality] ?? 'other'}</kind>${chord!.bass ? pitch(chord!.bass, 'bass') : ''}`
  return `<harmony placement="above">${body}<offset>${offset}</offset><staff>${staff}</staff></harmony>`
}
export function readChordHarmony(harmony: Element): string | null {
  const kind = harmony.querySelector('kind')
  if (kind?.textContent === 'none') return 'N.C.'
  const root = harmony.querySelector('root-step')?.textContent?.trim()
  if (!root || !/^[A-G]$/.test(root)) return null
  const accidental = (kind: 'root' | 'bass') => {
    const alter = Number(harmony.querySelector(`${kind}-alter`)?.textContent ?? 0)
    return alter === -1 ? '♭' : alter === 1 ? '♯' : alter === -2 ? '♭♭' : alter === 2 ? '♯♯' : ''
  }
  const quality = kind?.getAttribute('text') ?? Object.entries(chordKinds).find(([, value]) => value === kind?.textContent)?.[0] ?? ''
  const bass = harmony.querySelector('bass-step')?.textContent?.trim()
  const degrees = kind?.hasAttribute('text') ? '' : [...harmony.querySelectorAll('degree')].map(d => {
    const value = d.querySelector('degree-value')?.textContent ?? ''
    const alter = Number(d.querySelector('degree-alter')?.textContent ?? 0)
    const type = d.querySelector('degree-type')?.textContent
    return `${type === 'subtract' ? 'no' : alter < 0 ? '♭' : alter > 0 ? '♯' : 'add'}${value}`
  }).join('')
  return `${root}${accidental('root')}${quality}${degrees}${bass ? `/${bass}${accidental('bass')}` : ''}`
}
