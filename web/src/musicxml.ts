import { newId } from './id'
import type { Part } from './types'

const escape = (s: string) => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;')
const keys = ['Cb', 'Gb', 'Db', 'Ab', 'Eb', 'Bb', 'F', 'C', 'G', 'D', 'A', 'E', 'B', 'F#', 'C#']
const clefs: Record<string, [string, number]> = { treble: ['G', 2], bass: ['F', 4], alto: ['C', 3], tenor: ['C', 4] }
const pitchNames = ['C', 'C', 'D', 'D', 'E', 'F', 'F', 'G', 'G', 'A', 'A', 'B']
/** Deliberately limited, loss-reporting interchange. No external entity resolution. */
export function exportMusicXML(part: Part, beatsPerBar: number, beatUnit = 4): string {
  const barLength = beatsPerBar * 4 / beatUnit
  const divisions = 960
  const [clefSign, clefLine] = clefs[part.clef] || clefs.treble!
  const measures: string[] = []
  for (let bar = 0; bar < Math.ceil(part.loop_beats / barLength); bar++) {
    let cursor = bar * barLength
    const elements: string[] = []
    for (const note of part.notes.filter(n => n.beat >= bar * barLength && n.beat < (bar + 1) * barLength).sort((a, b) => a.beat - b.beat)) {
      const gap = Math.round((note.beat - cursor) * divisions)
      if (gap > 0) elements.push(`<forward><duration>${gap}</duration></forward>`)
      if (gap < 0) elements.push(`<backup><duration>${-gap}</duration></backup>`)
      const pitch = note.rest ? '<rest/>' : `<pitch><step>${pitchNames[note.pitch % 12]}</step>${[1, 3, 6, 8, 10].includes(note.pitch % 12) ? '<alter>1</alter>' : ''}<octave>${Math.floor(note.pitch / 12) - 1}</octave></pitch>`
      elements.push(`<note dynamics="${Math.round(note.velocity / 127 * 100)}">${pitch}<duration>${Math.round(note.duration * divisions)}</duration><voice>1</voice>${note.tied ? '<tie type="start"/>' : ''}</note>`)
      cursor = note.beat + note.duration
    }
    measures.push(`<measure number="${bar + 1}">${bar === 0 ? `<attributes><divisions>${divisions}</divisions>${part.key_signature ? `<key><fifths>${keys.indexOf(part.key_signature) - 7}</fifths><mode>major</mode></key>` : ''}<time${part.show_time_signature === false ? ' print-object="no"' : ''}><beats>${beatsPerBar}</beats><beat-type>${beatUnit}</beat-type></time><clef><sign>${clefSign}</sign><line>${clefLine}</line></clef></attributes>` : ''}${elements.join('')}</measure>`)
  }
  return `<?xml version="1.0" encoding="UTF-8"?><score-partwise version="4.0"><part-list><score-part id="P1"><part-name>${escape(part.name)}</part-name></score-part></part-list><part id="P1">${measures.join('')}</part></score-partwise>`
}

export function importMusicXML(xml: string): { parts: Part[]; warnings: string[]; beatsPerBar: number; beatUnit: number } {
  if (xml.length > 4 * 1024 * 1024) throw new Error('MusicXML exceeds 4 MB')
  if (/<!DOCTYPE|<!ENTITY/i.test(xml)) throw new Error('Remove DOCTYPE/entity declarations before import; external entities are not supported.')
  const document = new DOMParser().parseFromString(xml, 'application/xml')
  if (document.querySelector('parsererror') || document.documentElement.tagName !== 'score-partwise') throw new Error('Expected valid uncompressed score-partwise MusicXML')
  const warnings = new Set<string>()
  for (const tag of ['direction', 'notations', 'time-modification', 'transpose', 'barline']) if (document.querySelector(tag)) warnings.add(`${tag} information is not fully retained by the basic editor.`)
  let beatsPerBar = 4, beatUnit = 4, meterSeen = false
  const parts: Part[] = []
  const text = (el: Element, selector: string, fallback: string) => el.querySelector(selector)?.textContent || fallback
  for (const element of Array.from(document.documentElement.children).filter(el => el.tagName === 'part')) {
    if (parts.length >= 32) throw new Error('At most 32 parts are supported')
    const nameElement = Array.from(document.querySelectorAll('score-part')).find(p => p.getAttribute('id') === element.getAttribute('id'))
    const part: Part = { id: newId(), name: nameElement ? text(nameElement, 'part-name', 'Imported part') : 'Imported part', performer: null, view: 'notation', clef: 'treble', key_signature: null, show_time_signature: true, notes: [], loop_beats: 4, instrument_node: null, midi_port: null, osc_destination: null, osc_address: '/pr0former/note' }
    let divisions = 1, measureStart = 0, barLength = beatsPerBar * 4 / beatUnit
    for (const measure of Array.from(element.children).filter(el => el.tagName === 'measure')) {
      let position = measureStart, furthest = measureStart, previousStart = measureStart
      for (const item of Array.from(measure.children)) {
        if (item.tagName === 'attributes') {
          divisions = Number(text(item, 'divisions', String(divisions)))
          const numerator = Number(text(item, 'time > beats', String(beatsPerBar))), denominator = Number(text(item, 'time > beat-type', String(beatUnit)))
          if (!(divisions > 0) || !(denominator > 0)) throw new Error('Invalid MusicXML divisions or meter')
          if (!Number.isInteger(numerator) || numerator < 1 || numerator > 16 || ![1, 2, 4, 8, 16, 32].includes(denominator)) throw new Error('Unsupported MusicXML meter')
          if (item.querySelector('time')) {
            if (meterSeen && (numerator !== beatsPerBar || denominator !== beatUnit)) throw new Error('Changing or conflicting meters are not supported yet')
            beatsPerBar = numerator; beatUnit = denominator; meterSeen = true
          }
          barLength = beatsPerBar * 4 / beatUnit
          if (item.querySelector('time')) part.show_time_signature = item.querySelector('time')?.getAttribute('print-object') !== 'no'
          if (item.querySelector('clef')) {
            const sign = text(item, 'clef > sign', 'G'), line = Number(text(item, 'clef > line', '2'))
            const clef = Object.entries(clefs).find(([, value]) => value[0] === sign && value[1] === line)?.[0]
            if (!clef) throw new Error('Unsupported MusicXML clef')
            if (measureStart > 0 && clef !== part.clef) warnings.add('Mid-score clef changes are not retained; the initial clef is used.')
            else part.clef = clef
          }
          if (item.querySelector('key')) {
            const fifths = Number(text(item, 'key > fifths', '0'))
            if (!Number.isInteger(fifths) || fifths < -7 || fifths > 7) throw new Error('Unsupported MusicXML key signature')
            if (text(item, 'key > mode', 'major') !== 'major') warnings.add('Key mode is displayed as its relative major; the signature and pitches are retained.')
            if (measureStart > 0) warnings.add('Mid-score key changes are not retained; the initial signature is used.')
            else part.key_signature = keys[fifths + 7]!
          }
        } else if (item.tagName === 'backup' || item.tagName === 'forward') {
          position += Number(text(item, 'duration', '0')) / divisions * (item.tagName === 'backup' ? -1 : 1)
        } else if (item.tagName === 'note') {
          if (item.querySelector('grace')) { warnings.add('Grace notes were skipped.'); continue }
          const duration = Number(text(item, 'duration', '1')) / divisions
          const chord = !!item.querySelector('chord'), beat = chord ? previousStart : position
          const step = text(item, 'pitch > step', 'C'), octave = Number(text(item, 'pitch > octave', '4')), alter = Number(text(item, 'pitch > alter', '0'))
          const base: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 }
          const pitch = (octave + 1) * 12 + (base[step] ?? 0) + alter
          if (!Number.isInteger(pitch) || pitch < 0 || pitch > 127 || duration <= 0 || !Number.isFinite(duration) || beat < 0) throw new Error('Unsupported pitch or note timing')
          part.notes.push({ id: newId(), pitch, beat, duration, velocity: Math.max(0, Math.min(127, Math.round(Number(item.getAttribute('dynamics') || '71') * 127 / 100))), rest: !!item.querySelector('rest'), tied: !!item.querySelector('tie[type="start"]') })
          if (!chord) { previousStart = position; position += duration }
          furthest = Math.max(furthest, beat + duration)
        }
      }
      measureStart = Math.max(measureStart + barLength, furthest)
    }
    part.loop_beats = measureStart || 4; parts.push(part)
  }
  if (!parts.length) throw new Error('No parts found')
  return { parts, warnings: [...warnings], beatsPerBar, beatUnit }
}
