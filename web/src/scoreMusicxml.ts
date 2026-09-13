import type {
  Project,
  Part,
  Note,
  Staff,
  ScoreTimeline,
  NoteNotation,
} from './types'
import { newId } from './id'
import { chordHarmony, readChordHarmony } from './chordSymbols'
import {
  keyNames,
  metadata,
  scoreMeasures,
  staves,
  withNotation,
  rational,
} from './score'
import { durationGlyphs } from './notation'
const escape = (s: string) =>
  s
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
const clefs: Record<string, [string, number]> = {
  treble: ['G', 2],
  bass: ['F', 4],
  alto: ['C', 3],
  tenor: ['C', 4],
}
const typeNames: Record<number, string> = {
  8: 'breve',
  4: 'whole',
  2: 'half',
  1: 'quarter',
  0.5: 'eighth',
  0.25: '16th',
  0.125: '32nd',
  0.0625: '64th',
}
const valueTypes = Object.fromEntries(
  Object.entries(typeNames).map(([k, v]) => [v, Number(k)]),
)
const divisions = 20160 // divisible by common tuplet denominators through nine
function attributes(p: Part, project: Project, start: number) {
  const ss = staves(p),
    meter = project.score?.meters.find((m) => m.beat === start),
    key = project.score?.keys.find((k) => k.beat === start)
  const settings = ss
    .map((s, i) => {
      const change = s.clef_changes?.find((c) => c.beat === start),
        clef = change?.clef || s.clef,
        [sign, line] = clefs[clef]!
      const k = s.key_signature ?? key?.key ?? p.key_signature
      return `${start === 0 && k ? `<key number="${i + 1}"><fifths>${keyNames.indexOf(k) - 7}</fifths><mode>${s.key_mode || key?.mode || 'major'}</mode></key>` : ''}${start === 0 || change ? `<clef number="${i + 1}"><sign>${sign}</sign><line>${line}</line></clef>` : ''}${start === 0 && s.transpose ? `<transpose number="${i + 1}"><chromatic>${s.transpose}</chromatic></transpose>` : ''}`
    })
    .join('')
  return `<attributes>${start === 0 ? `<divisions>${divisions}</divisions><staves>${ss.length}</staves>` : ''}${start === 0 || meter ? `<time${p.show_time_signature === false ? ' print-object="no"' : ''}><beats>${meter?.beats ?? project.beats_per_bar}</beats><beat-type>${meter?.unit ?? project.beat_unit ?? 4}</beat-type></time>` : ''}${key && start > 0 ? `<key><fifths>${keyNames.indexOf(key.key) - 7}</fifths><mode>${key.mode || 'major'}</mode></key>` : ''}${settings}</attributes>`
}
export function exportScoreMusicXML(
  project: Project,
  parts = project.parts,
): string {
  const length =
    project.score?.length ?? Math.max(4, ...parts.map((p) => p.loop_beats))
  const measures = scoreMeasures(
    length,
    project.beats_per_bar,
    project.beat_unit || 4,
    project.score?.meters,
  )
    .flatMap((m) => {
      const cuts = [
        ...new Set([
          m.start,
          ...(project.score?.repeats || [])
            .flatMap((r) => [
              r.start,
              r.end,
              ...(r.first_ending == null ? [] : [r.first_ending]),
            ])
            .filter((b) => b > m.start && b < m.end),
          m.end,
        ]),
      ].sort((a, b) => a - b)
      return cuts
        .slice(0, -1)
        .map((start, i) => ({ ...m, start, end: cuts[i + 1]! }))
    })
    .map((m, i) => ({ ...m, number: i + 1 }))
  const list = parts
    .map(
      (p, i) =>
        `<score-part id="P${i + 1}"><part-name>${escape(p.name)}</part-name></score-part>`,
    )
    .join('')
  const body = parts
    .map((p, pi) => {
      const ss = staves(p),
        tieTargets = new Set(
          p.notes.flatMap((n) =>
            n.notation?.tie_to ? [n.notation.tie_to] : [],
          ),
        ),
        curveSlurs = ss.flatMap((s) =>
          (s.curves || []).filter((c) => c.kind === 'slur' && c.start_note && c.end_note),
        ),
        slurStarts = new Set([
          ...p.notes.filter((n) => n.notation?.slur_to).map((n) => n.id),
          ...curveSlurs.map((c) => c.start_note!),
        ]),
        slurTargets = new Map([
          ...p.notes.flatMap((n) =>
            n.notation?.slur_to ? [[n.notation.slur_to, n.id] as const] : [],
          ),
          ...curveSlurs.map((c) => [c.end_note!, c.start_note!] as const),
        ])
      const bars = measures
        .map((m, i) => {
          let cursor = m.start,
            content = attributes(p, project, m.start)
          for (const r of project.score?.repeats || []) {
            if (r.start === m.start)
              content +=
                '<barline location="left"><repeat direction="forward"/></barline>'
            if (r.first_ending === m.start)
              content +=
                '<barline location="left"><ending number="1" type="start"/></barline>'
            if (r.first_ending != null && r.end === m.start)
              content +=
                '<barline location="left"><ending number="2" type="start"/></barline>'
          }
          const direction = (beat: number, body: string, sound = '') =>
            `<direction>${body}<offset>${Math.round((beat - m.start) * divisions)}</offset>${sound}</direction>`
          for (const [si, s] of ss.entries()) for (const e of (s.dynamics?.events ?? (si === 0 ? p.dynamics?.events : undefined)) || []) {
            const staffTag = `<staff>${si + 1}</staff>`
            if (e.beat >= m.start && e.beat < m.end) {
              const label = (
                {
                  16: 'ppp',
                  32: 'pp',
                  48: 'p',
                  64: 'mp',
                  80: 'mf',
                  96: 'f',
                  112: 'ff',
                  127: 'fff',
                } as Record<number, string>
              )[e.start]
              content += direction(
                e.beat,
                `<direction-type><dynamics>${label ? `<${label}/>` : `<other-dynamics>${e.start}</other-dynamics>`}</dynamics></direction-type>`,
                `${staffTag}<sound dynamics="${(e.start / 127) * 100}"/>`,
              )
              if (e.duration > 0)
                content += direction(
                  e.beat,
                  `<direction-type><wedge type="${e.end >= e.start ? 'crescendo' : 'diminuendo'}"/></direction-type>`,
                  staffTag,
                )
            }
            if (
              e.duration > 0 &&
              e.beat + e.duration >= m.start &&
              (e.beat + e.duration < m.end ||
                (e.beat + e.duration === length && m.end === length))
            )
              content += direction(
                e.beat + e.duration,
                '<direction-type><wedge type="stop"/></direction-type>',
                `${staffTag}<sound dynamics="${(e.end / 127) * 100}"/>`,
              )
          }
          const nav = project.score?.navigation
          if (nav) {
            for (const [beat, label, sound] of [
              [
                nav.at,
                nav.target === 0 ? 'D.C.' : 'D.S.',
                nav.target === 0 ? 'dacapo="yes"' : 'dalsegno="S1"',
              ],
              [nav.target, 'Segno', 'segno="S1"'],
              [nav.fine, 'Fine', 'fine="yes"'],
              [nav.coda?.[0], 'To coda', 'tocoda="C1"'],
              [nav.coda?.[1], 'Coda', 'coda="C1"'],
            ] as const) {
              if (
                beat != null &&
                beat >= m.start &&
                (beat < m.end || (beat === length && m.end === length))
              )
                content += direction(
                  beat,
                  `<direction-type><words>${label}</words></direction-type>`,
                  `<sound ${sound}/>`,
                )
            }
          }
          if (pi === 0) {
            const tempos = project.score?.tempos || []
            if (i === 0 && !tempos.some((t) => t.beat === 0))
              content += direction(
                m.start,
                `<direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>${project.bpm}</per-minute></metronome></direction-type>`,
                `<sound tempo="${project.bpm}"/>`,
              )
            for (const t of tempos)
              if (t.beat >= m.start && t.beat < m.end)
                content += direction(
                  t.beat,
                  `<direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>${t.bpm}</per-minute></metronome></direction-type>`,
                  `<sound tempo="${t.bpm}"/>`,
                )
          }
          ss.forEach((s, si) => {
            for (const mark of s.marks || [])
              if (mark.beat >= m.start && mark.beat < m.end)
                content += (mark.kind === 'chord' ? chordHarmony(mark.text, Math.round((mark.beat - m.start) * divisions), si + 1) : null) ?? direction(
                  mark.beat,
                  `<direction-type>${
                    mark.kind === 'rehearsal'
                      ? `<rehearsal>${escape(mark.text)}</rehearsal>`
                      : `<words${mark.kind === 'expression' ? ' font-style="italic"' : ''}${mark.kind === 'cue' ? ' font-weight="bold"' : ''}${mark.kind === 'lyric' ? ' placement="below"' : ''}>${escape(mark.text)}</words>`
                  }</direction-type>`,
                  `<staff>${si + 1}</staff>`,
                )
          })
          // Attribute changes have a musical cursor even when no note occurs there.
          const changes = [
            ...new Set([
              ...(project.score?.keys || []).map((k) => k.beat),
              ...ss.flatMap((s) => (s.clef_changes || []).map((c) => c.beat)),
            ]),
          ]
            .filter((b) => b > m.start && b < m.end)
            .sort((a, b) => a - b)
          for (const beat of changes) {
            const delta = Math.round((beat - m.start) * divisions)
            content += `<forward><duration>${delta}</duration></forward>${attributes(p, project, beat)}<backup><duration>${delta}</duration></backup>`
          }
          for (const n of p.notes
            .filter((n) => n.beat < m.end && n.beat + n.duration > m.start)
            .sort((a, b) => a.beat - b.beat)) {
            const v = metadata(n, p),
              staff = ss.findIndex((s) => s.id === v.staff) + 1,
              start = Math.max(n.beat, m.start),
              duration = Math.min(n.beat + n.duration, m.end) - start,
              grace = !!v.grace_to
            const gap = Math.round((start - cursor) * divisions)
            if (gap)
              content += `<${gap > 0 ? 'forward' : 'backup'}><duration>${Math.abs(gap)}</duration></${gap > 0 ? 'forward' : 'backup'}>`
            const octaveDirection = (type: string) =>
              `<direction><direction-type><octave-shift type="${type}" size="${Math.abs(v.octave || 0) === 2 ? 15 : 8}" number="${v.voice}"/></direction-type><voice>${v.voice}</voice><staff>${staff}</staff></direction>`
            if (v.octave)
              content += octaveDirection(v.octave > 0 ? 'down' : 'up')
            const step = ((v.step % 7) + 7) % 7,
              pitch = n.rest
                ? '<rest/>'
                : `<pitch><step>${['C', 'D', 'E', 'F', 'G', 'A', 'B'][step]}</step>${v.alter ? `<alter>${v.alter}</alter>` : ''}<octave>${Math.floor(v.step / 7)}</octave></pitch>`
            const tieStart = !!v.tie_to || n.beat + n.duration > m.end,
              tieStop = tieTargets.has(n.id) || n.beat < m.start
            const exact = durationGlyphs(duration),
              base =
                exact?.length === 1
                  ? exact[0]!.beats / (2 - 2 ** -exact[0]!.dots)
                  : v.base,
              dots = exact?.length === 1 ? exact[0]!.dots : v.dots
            const ties = `${tieStop ? '<tie type="stop"/>' : ''}${tieStart ? '<tie type="start"/>' : ''}`
            const notation = `${tieStop ? '<tied type="stop"/>' : ''}${tieStart ? '<tied type="start"/>' : ''}${slurStarts.has(n.id) ? '<slur type="start" number="1"/>' : ''}${slurTargets.has(n.id) ? '<slur type="stop" number="1"/>' : ''}${v.articulation ? `<articulations><${v.articulation === 'marcato' ? 'strong-accent' : v.articulation}/></articulations>` : ''}`
            content += `<note dynamics="${(n.velocity / 127) * 100}">${grace ? '<grace slash="yes"/>' : ''}${pitch}${grace ? '' : `<duration>${Math.round(duration * divisions)}</duration>`}${ties}<voice>${v.voice}</voice>${typeNames[base] ? `<type>${typeNames[base]}</type>` : ''}${'<dot/>'.repeat(dots)}${v.tuplet_actual !== v.tuplet_normal ? `<time-modification><actual-notes>${v.tuplet_actual}</actual-notes><normal-notes>${v.tuplet_normal}</normal-notes></time-modification>` : ''}<staff>${staff}</staff>${notation ? `<notations>${notation}</notations>` : ''}</note>`
            if (v.octave) content += octaveDirection('stop')
            cursor = start + (grace ? 0 : duration)
          }
          if (cursor < m.end)
            content += `<forward><duration>${Math.round((m.end - cursor) * divisions)}</duration></forward>`
          for (const r of project.score?.repeats || [])
            if (r.end === m.end)
              content += `<barline location="right">${r.first_ending != null ? '<ending number="1" type="stop"/>' : ''}<repeat direction="backward" times="${r.times}"/></barline>`
          return `<measure number="${m.number}"${m.end - m.start < (m.beats * 4) / m.unit ? ' implicit="yes"' : ''}>${content}</measure>`
        })
        .join('')
      return `<part id="P${pi + 1}">${bars}</part>`
    })
    .join('')
  return `<?xml version="1.0" encoding="UTF-8"?><score-partwise version="4.0"><part-list>${list}</part-list>${body}</score-partwise>`
}

export function importScoreMusicXML(xml: string): {
  parts: Part[]
  warnings: string[]
  beatsPerBar: number
  beatUnit: number
  score: ScoreTimeline
} {
  if (xml.length > 4 * 1024 * 1024) throw new Error('MusicXML exceeds 4 MB')
  if (/<!DOCTYPE|<!ENTITY/i.test(xml))
    throw new Error('DOCTYPE and entity declarations are not supported.')
  const doc = new DOMParser().parseFromString(xml, 'application/xml')
  if (
    doc.querySelector('parsererror') ||
    doc.documentElement.tagName !== 'score-partwise'
  )
    throw new Error('Expected uncompressed score-partwise MusicXML')
  const warnings = new Set<string>(),
    parts: Part[] = [],
    tempos = new Map<number, number>(),
    meters = new Map<number, { beat: number; beats: number; unit: number }>(),
    keys = new Map<
      number,
      { beat: number; key: string; mode: 'major' | 'minor' }
    >()
  const text = (el: Element, sel: string, fallback = '') =>
    el.querySelector(sel)?.textContent || fallback
  const num = (el: Element, sel: string, fallback = 0) =>
    Number(text(el, sel, String(fallback)))
  let initialBeats = 4,
    initialUnit = 4,
    scoreLength = 0
  const score: ScoreTimeline = {
    version: 1,
    length: 4,
    loop_score: false,
    meters: [],
    keys: [],
    repeats: [],
    navigation: null,
  }
  let jump: { at: number; target: string } | undefined,
    segno = 0,
    fine: number | undefined,
    toCoda: number | undefined,
    coda: number | undefined
  for (const pe of Array.from(doc.documentElement.children).filter(
    (e) => e.tagName === 'part',
  )) {
    if (parts.length >= 32) throw new Error('At most 32 parts are supported')
    const name = Array.from(doc.querySelectorAll('score-part')).find(
      (e) => e.id === pe.id,
    )
    const part: Part = {
      id: newId(),
      name: name ? text(name, 'part-name', 'Imported part') : 'Imported part',
      performer: null,
      view: 'notation',
      clef: 'treble',
      notes: [],
      staves: [],
      loop_beats: 4,
      instrument_node: null,
      midi_channel: 1,
      osc_address: '/pr0former/note',
    }
    const staff = (index: number) => {
      if (!Number.isInteger(index) || index < 1 || index > 8)
        throw new Error('A part supports 1–8 staves')
      while (part.staves!.length < index)
        part.staves!.push({
          id: newId(),
          name: `Staff ${part.staves!.length + 1}`,
          clef: 'treble',
          transpose: 0,
        })
      return part.staves![index - 1]!
    }
    let div = 1,
      measureStart = 0,
      beats = 4,
      unit = 4,
      repeatStart = 0,
      ending: number | undefined,
      dynamic = 80
    const octaveChanges = new Map<string, { beat: number; shift: number }[]>()
    const ties = new Map<string, Note>(),
      slurs = new Map<string, Note>(),
      graces: Note[] = [],
      wedges = new Map<string, { beat: number; start: number; up: boolean }>()
    for (const measure of Array.from(pe.children).filter(
      (e) => e.tagName === 'measure',
    )) {
      let position = measureStart,
        previousStart = measureStart,
        furthest = measureStart
      const pendingRepeats: { times: number }[] = []
      for (const item of Array.from(measure.children)) {
        if (item.tagName === 'attributes') {
          div = num(item, 'divisions', div)
          if (!Number.isFinite(div) || div <= 0)
            throw new Error('Invalid MusicXML divisions')
          if (item.querySelector('staves')) staff(num(item, 'staves', 1))
          if (item.querySelector('time')) {
            beats = num(item, 'time > beats', beats)
            unit = num(item, 'time > beat-type', unit)
            if (
              !Number.isInteger(beats) ||
              beats < 1 ||
              beats > 16 ||
              ![1, 2, 4, 8, 16, 32].includes(unit)
            )
              throw new Error('Unsupported MusicXML meter')
            const previous = meters.get(measureStart)
            if (
              previous &&
              (previous.beats !== beats || previous.unit !== unit)
            )
              throw new Error('Parts have conflicting time signatures')
            meters.set(measureStart, { beat: measureStart, beats, unit })
            if (measureStart === 0) {
              initialBeats = beats
              initialUnit = unit
              part.show_time_signature =
                item.querySelector('time')?.getAttribute('print-object') !==
                'no'
            }
          }
          for (const c of Array.from(item.children).filter(
            (e) => e.tagName === 'clef',
          )) {
            const s = staff(Number(c.getAttribute('number') || 1)),
              sign = text(c, 'sign', 'G'),
              line = num(c, 'line', 2),
              clef = Object.entries(clefs).find(
                ([, v]) => v[0] === sign && v[1] === line,
              )?.[0]
            if (!clef) throw new Error('Unsupported MusicXML clef')
            if (position === 0) s.clef = clef
            else
              s.clef_changes = [
                ...(s.clef_changes || []),
                { beat: position, clef },
              ]
          }
          for (const t of Array.from(item.children).filter(
            (e) => e.tagName === 'transpose',
          ))
            staff(Number(t.getAttribute('number') || 1)).transpose =
              num(t, 'chromatic', 0) + num(t, 'octave-change', 0) * 12
          for (const k of Array.from(item.children).filter(
            (e) => e.tagName === 'key',
          )) {
            const fifths = num(k, 'fifths')
            if (!Number.isInteger(fifths) || fifths < -7 || fifths > 7)
              throw new Error('Unsupported key signature')
            const key = keyNames[fifths + 7]!
            const mode =
              text(k, 'mode', 'major') === 'minor' ? 'minor' : 'major'
            if (!['major', 'minor'].includes(text(k, 'mode', 'major')))
              warnings.add('Non-major/minor mode labels require review.')
            staff(Number(k.getAttribute('number') || 1)).key_mode = mode
            if (position === 0) {
              staff(Number(k.getAttribute('number') || 1)).key_signature = key
              part.key_signature ??= key
              if (parts.length === 0 && !keys.has(0))
                keys.set(0, { beat: 0, key, mode })
            } else if (parts.length === 0)
              keys.set(position, { beat: position, key, mode })
            else if (keys.get(position)?.key !== key)
              warnings.add(
                'Independent mid-score staff keys require review; shared key changes are taken from the first part.',
              )
          }
        } else if (item.tagName === 'backup' || item.tagName === 'forward') {
          position +=
            (num(item, 'duration') / div) * (item.tagName === 'backup' ? -1 : 1)
          furthest = Math.max(furthest, position)
        } else if (item.tagName === 'note') {
          const s = staff(num(item, 'staff', 1)),
            grace = !!item.querySelector('grace'),
            chord = !!item.querySelector('chord'),
            beat = chord ? previousStart : position
          const base = valueTypes[text(item, 'type')] || 0.125,
            dots = item.querySelectorAll(':scope > dot').length,
            actual = num(item, 'time-modification > actual-notes', 1),
            normal = num(item, 'time-modification > normal-notes', 1)
          const duration = grace
            ? (base * (2 - 2 ** -dots) * normal) / actual
            : num(item, 'duration', div) / div
          const step = 'CDEFGAB'.indexOf(text(item, 'pitch > step', 'C')),
            octave = num(item, 'pitch > octave', 4),
            alter = num(item, 'pitch > alter', 0),
            voice = num(item, 'voice', 1)
          if (step < 0 || !Number.isFinite(duration) || duration <= 0)
            throw new Error('Invalid MusicXML note')
          const v: NoteNotation = {
            octave:
              octaveChanges
                .get(`${s.id}:${voice}`)
                ?.filter((c) => c.beat <= beat)
                .sort((a, b) => a.beat - b.beat)
                .at(-1)?.shift || 0,
            staff: s.id,
            step: octave * 7 + step,
            alter,
            voice,
            base: ((duration / (2 - 2 ** -dots)) * actual) / normal,
            dots,
            tuplet_actual: actual,
            tuplet_normal: normal,
          }
          const n = withNotation(
            {
              id: newId(),
              pitch: 60,
              beat,
              duration,
              velocity: Math.max(
                0,
                Math.min(
                  127,
                  Math.round(
                    (Number(
                      item.getAttribute('dynamics') || String((90 / 127) * 100),
                    ) /
                      100) *
                      127,
                  ),
                ),
              ),
              rest: !!item.querySelector('rest'),
              tied: false,
            },
            v,
            part,
          )
          const tieKey = `${s.id}:${voice}:${n.pitch}`
          if (item.querySelector('tie[type="stop"],tied[type="stop"]')) {
            const from = ties.get(tieKey)
            if (from) from.notation!.tie_to = n.id
            else warnings.add('An unmatched tie stop was omitted.')
            ties.delete(tieKey)
          }
          if (item.querySelector('tie[type="start"],tied[type="start"]'))
            ties.set(tieKey, n)
          for (const slur of Array.from(item.querySelectorAll('slur'))) {
            const key = `${s.id}:${slur.getAttribute('number') || 1}`
            if (slur.getAttribute('type') === 'start') slurs.set(key, n)
            else {
              const from = slurs.get(key)
              if (from) from.notation!.slur_to = n.id
              slurs.delete(key)
            }
          }
          const art =
            item.querySelector('articulations')?.firstElementChild?.tagName
          if (
            art &&
            ['staccato', 'tenuto', 'accent', 'strong-accent'].includes(art)
          )
            n.notation!.articulation = art === 'strong-accent' ? 'marcato' : art
          if (grace) graces.push(n)
          else {
            for (const g of graces.filter(
              (g) => g.notation!.staff === s.id && g.notation!.voice === voice,
            )) {
              g.notation!.grace_to = n.id
              g.beat = n.beat
              g.notation!.onset = rational(n.beat)
              graces.splice(graces.indexOf(g), 1)
            }
          }
          part.notes.push(n)
          previousStart = beat
          if (!grace) {
            if (!chord) position += duration
            furthest = Math.max(furthest, beat + duration)
          }
        } else if (item.tagName === 'harmony') {
          const text = readChordHarmony(item), at = position + num(item, 'offset', 0) / div
          if (text && text.length <= 64 && at >= 0 && at <= 4096) {
            const s = staff(num(item, 'staff', 1))
            s.marks = [...(s.marks || []), { id: newId(), beat: at, kind: 'chord', text }]
          } else warnings.add('An unsupported harmony symbol was omitted; review the original chord chart.')
        } else if (item.tagName === 'barline') {
          const r = item.querySelector('repeat')
          if (r?.getAttribute('direction') === 'forward')
            repeatStart = measureStart
          if (item.querySelector('ending[type="start"][number="1"]'))
            ending = measureStart
          if (r?.getAttribute('direction') === 'backward')
            pendingRepeats.push({ times: Number(r.getAttribute('times') || 2) })
        } else if (item.tagName === 'direction') {
          const at = position + num(item, 'offset', 0) / div,
            sound = item.querySelector('sound'),
            mark = item.querySelector('dynamics')
          if (sound?.hasAttribute('dynamics') || mark) {
            const label = mark?.firstElementChild?.tagName,
              values: Record<string, number> = {
                ppp: 16,
                pp: 32,
                p: 48,
                mp: 64,
                mf: 80,
                f: 96,
                ff: 112,
                fff: 127,
              }
            dynamic = sound?.hasAttribute('dynamics')
              ? Math.round((Number(sound.getAttribute('dynamics')) / 100) * 127)
              : values[label || ''] || 80
            const target = staff(num(item, 'staff', 1))
            target.dynamics ??= { mode: 'velocity', controller: 11, events: [] }
            target.dynamics.events.push({
              id: newId(),
              beat: at,
              duration: 0,
              start: dynamic,
              end: dynamic,
              curve: 'linear',
            })
          }
          const octave = item.querySelector('octave-shift')
          if (octave) {
            const key = `${staff(num(item, 'staff', 1)).id}:${num(item, 'voice', 1)}`,
              shift =
                octave.getAttribute('type') === 'stop'
                  ? 0
                  : (Number(octave.getAttribute('size') || 8) === 15 ? 2 : 1) *
                    (octave.getAttribute('type') === 'down' ? 1 : -1)
            octaveChanges.set(key, [
              ...(octaveChanges.get(key) || []),
              { beat: at, shift },
            ])
          }
          const wedge = item.querySelector('wedge')
          if (wedge) {
            const id = wedge.getAttribute('number') || '1',
              type = wedge.getAttribute('type')
            if (type === 'stop') {
              const from = wedges.get(id)
              if (from) {
                const target = staff(num(item, 'staff', 1))
                target.dynamics ??= {
                  mode: 'velocity',
                  controller: 11,
                  events: [],
                }
                target.dynamics.events = target.dynamics.events.filter(
                  (e) => e.beat !== from.beat && e.beat !== at,
                )
                target.dynamics.events.push({
                  id: newId(),
                  beat: from.beat,
                  duration: at - from.beat,
                  start: from.start,
                  end:
                    dynamic === from.start
                      ? Math.max(
                          0,
                          Math.min(127, dynamic + (from.up ? 32 : -32)),
                        )
                      : dynamic,
                  curve: 'linear',
                })
                wedges.delete(id)
              }
            } else
              wedges.set(id, {
                beat: at,
                start: dynamic,
                up: type === 'crescendo',
              })
          }
          const perMinute =
            sound?.getAttribute('tempo') ??
            item.querySelector('metronome > per-minute')?.textContent
          if (parts.length === 0 && perMinute) {
            const bpm = Number(perMinute)
            if (Number.isFinite(bpm) && bpm >= 1 && bpm <= 400)
              tempos.set(at, bpm)
          }
          const words = item.querySelector(
            'direction-type > words, direction-type > rehearsal',
          )
          const navigationWord =
            sound &&
            ['dacapo', 'dalsegno', 'segno', 'fine', 'tocoda', 'coda'].some((a) =>
              sound.hasAttribute(a),
            )
          if (
            words?.textContent?.trim() &&
            !navigationWord &&
            !['D.C.', 'D.S.', 'Segno', 'Fine', 'To coda', 'Coda'].includes(
              words.textContent.trim(),
            )
          ) {
            const s = staff(num(item, 'staff', 1)),
              text = words.textContent.trim().slice(0, 256)
            s.marks = [
              ...(s.marks || []),
              {
                id: newId(),
                beat: at,
                kind:
                  words.tagName === 'rehearsal'
                    ? 'rehearsal'
                    : words.getAttribute('placement') === 'below'
                      ? 'lyric'
                      : words.getAttribute('font-style') === 'italic'
                        ? 'expression'
                        : words.getAttribute('font-weight') === 'bold'
                          ? 'cue'
                          : /^(rit|rall|accel|a tempo|tempo|allegro|adagio|andante|presto|lento|largo|vivace|moderato)/i.test(text)
                            ? 'tempo'
                            : 'text',
                text,
              },
            ]
          }
          if (parts.length === 0 && sound) {
            if (sound.hasAttribute('dacapo')) jump = { at, target: 'dc' }
            if (sound.hasAttribute('dalsegno')) jump = { at, target: 'ds' }
            if (sound.hasAttribute('segno')) segno = at
            if (sound.hasAttribute('fine')) fine = at
            if (sound.hasAttribute('tocoda')) toCoda = at
            if (sound.hasAttribute('coda')) coda = at
          }
        }
      }
      const end =
        measure.getAttribute('implicit') === 'yes' && furthest > measureStart
          ? furthest
          : Math.max(measureStart + (beats * 4) / unit, furthest)
      if (parts.length === 0)
        for (const r of pendingRepeats) {
          score.repeats.push({
            start: repeatStart,
            end,
            times: r.times,
            first_ending: ending ?? null,
          })
          repeatStart = end
          ending = undefined
        }
      measureStart = end
    }
    if (graces.length)
      warnings.add(
        'Grace notes without a following principal note were omitted.',
      )
    for (const s of part.staves || [])
      if (s.key_signature === keys.get(0)?.key) s.key_signature = null
    part.notes = part.notes.filter((n) => !graces.includes(n))
    part.loop_beats = measureStart || 4
    part.clef = staff(1).clef
    part.notes.sort((a, b) => a.beat - b.beat)
    for (const s of part.staves || []) s.dynamics?.events.sort((a, b) => a.beat - b.beat)
    if (part.notes.length > 10000)
      throw new Error('At most 10,000 notes per part')
    scoreLength = Math.max(scoreLength, part.loop_beats)
    parts.push(part)
  }
  if (!parts.length) throw new Error('The file contains no score parts')
  score.length = scoreLength
  score.meters = [...meters.values()].sort((a, b) => a.beat - b.beat)
  score.keys = [...keys.values()].sort((a, b) => a.beat - b.beat)
  score.tempos = [...tempos.entries()]
    .map(([beat, bpm]) => ({ beat, bpm }))
    .sort((a, b) => a.beat - b.beat)
  if (jump)
    score.navigation = {
      at: jump.at,
      target: jump.target === 'dc' ? 0 : segno,
      fine: fine ?? null,
      coda: toCoda != null && coda != null ? [toCoda, coda] : null,
    }
  if (doc.querySelector('lyric,ornaments,technical,unpitched'))
    warnings.add(
      'Lyrics, ornaments, technical marks and unpitched percussion are outside the supported interchange subset.',
    )
  return {
    parts,
    warnings: [...warnings],
    beatsPerBar: initialBeats,
    beatUnit: initialUnit,
    score,
  }
}

/** Native project JSON remains the lossless format for graph and performance settings. */
export function musicXMLExportWarnings(project: Project): string[] {
  const warnings: string[] = []
  if (project.parts.some(p => staves(p).some(s => s.marks?.some(m => m.kind === 'chord' && !chordHarmony(m.text, 0, 1))))) warnings.push('Custom chord labels without a pitch root export as text; native JSON retains their chord-symbol type.')
  if(project.score?.barlines?.length || project.parts.some(p => p.muted || p.solo)) warnings.push('Special barline styles and mute/solo settings remain in the native project.')
  if (project.parts.some((p) => p.staves?.some((s) => s.hidden_rests?.length)))
    warnings.push(
      'Hidden rest layout is omitted from MusicXML. Keep the native project to retain it.',
    )
  if (project.parts.some((p) => p.automation?.length))
    warnings.push(
      'Raw MIDI lanes are omitted from MusicXML. Keep the native project to retain them.',
    )
  if (
    project.parts.some((p) =>
      (p.staves || []).some((s) =>
        (s.curves || []).some((c) => c.kind === 'bracket' || !c.start_note || !c.end_note),
      ),
    )
  )
    warnings.push(
      'Brackets and slurs with free ends are display marks without a MusicXML equivalent and are omitted.',
    )
  if (
    project.parts.some(
      (p) =>
        (p.dynamics?.mode && p.dynamics.mode !== 'velocity') ||
        (p.staves || []).some((s) => s.dynamics?.mode && s.dynamics.mode !== 'velocity'),
    )
  )
    warnings.push(
      'Dynamics notation is exported; MIDI controller mappings remain in the native project.',
    )
  if (
    project.parts.some((p) =>
      p.notes.some(
        (n) =>
          Math.abs(n.beat * divisions - Math.round(n.beat * divisions)) >
            1e-7 ||
          Math.abs(
            n.duration * divisions - Math.round(n.duration * divisions),
          ) > 1e-7,
      ),
    )
  )
    warnings.push(
      'Some musical fractions are rounded to 1/20160 quarter beats in MusicXML.',
    )
  return warnings
}
