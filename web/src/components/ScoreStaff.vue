<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue'
import {
  Renderer,
  Stave,
  StaveNote,
  Voice,
  Formatter,
  Accidental,
  Dot,
  StaveTie,
  Beam,
  Tuplet,
  Articulation,
  GraceNote,
  Curve,
  Barline,
} from 'vexflow'
import type { Part, Staff, ScoreTimeline, Note } from '../types'
import {
  metadata,
  bottomStep,
  noteKey,
  scoreMeasures,
  scoreX,
  staves,
  type ScoreAnchor,
} from '../score'
import { durationGlyphs } from '../notation'
import { elementKey, type ScoreElement } from '../scoreElements'
import { bracketPath, hairpinPath, isHairpin, slurPath } from '../scoreCurves'
import { dynamicName, nodesFromEvents, staffDynamicsEvents } from '../scoreRamps'
const props = defineProps<{
  part: Part
  staff: Staff
  anchors: ScoreAnchor[]
  viewStart: number
  viewEnd: number
  timeline?: ScoreTimeline | null
  length: number
  barLength: number
  beatsPerBar: number
  beatUnit: number
  scale: number
  selected: Set<string>
  selectedElement?: string | null
  /** Performance score is a read-only display; its notation must not capture input. */
  interactive?: boolean
  beat: number
  origin: number
  /** First staff of its part: bar numbers, repeat counts and navigation text are drawn once per system. */
  first?: boolean
}>()
const xAt = (beat: number) =>
  scoreX(beat, props.anchors, props.scale, props.origin)
const host = ref<HTMLDivElement>(),
  error = ref('')
let observer: IntersectionObserver | undefined,
  visible = false,
  frame = 0
/**
 * Everything the engraving depends on, restricted to this staff and the visible
 * window. Re-engraving only when this changes means edits in other parts or
 * staves, selection changes and small scrolls inside the overscan cost nothing.
 */
const signature = computed(() => {
  const lo = props.viewStart - 2 * props.barLength,
    hi = props.viewEnd + 2 * props.barLength
  const notes = props.part.notes
    .filter((n) => {
      if (metadata(n, props.part).staff !== props.staff.id) return false
      const v = n.notation
      // Ties/slurs/grace links may reach notes outside the window; include linked ids.
      return (
        (n.beat + n.duration >= lo && n.beat <= hi) ||
        !!(v && (v.tie_to || v.slur_to || v.grace_to))
      )
    })
    .map((n) => [
      n.id,
      n.pitch,
      n.beat,
      n.duration,
      n.rest,
      n.tied,
      n.notation ?? null,
    ])
  const anchors = props.anchors.filter((a) => a.beat >= lo && a.beat <= hi)
  return JSON.stringify([
    props.staff,
    props.part.id,
    props.part.key_signature,
    props.part.show_time_signature,
    props.timeline,
    props.scale,
    props.origin,
    props.length,
    props.barLength,
    props.beatsPerBar,
    props.beatUnit,
    props.viewStart,
    props.viewEnd,
    props.first,
    anchors,
    props.anchors.at(-1),
    notes,
  ])
})
function scheduleRender() {
  if (frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    render()
  })
}
function applySelection() {
  if (!host.value) return
  for (const el of host.value.querySelectorAll<SVGElement>('[data-note-id]'))
    el.dataset.selected = String(
      props.selected.has(noteKey(props.part.id, el.dataset.noteId!)),
    )
}
function render() {
  if (!host.value || !visible) return
  host.value.innerHTML = ''
  error.value = ''
  try {
    const width = xAt(props.length) + 100
    const renderer = new Renderer(host.value, Renderer.Backends.SVG)
    renderer.resize(width, 190)
    const ctx = renderer.getContext()
    ctx.setFillStyle('#000000')
    ctx.setStrokeStyle('#000000')
    const tag = (
      el: SVGElement | undefined,
      data: Omit<ScoreElement, 'part' | 'staff'>,
    ) => {
      if (!el) return
      const e = { ...data, part: props.part.id, staff: props.staff.id },
        key = elementKey(e)
      el.dataset.scoreElement = key
      el.dataset.selected = String(props.selectedElement === key)
      if (props.interactive !== false) {
        el.setAttribute('role', 'button')
        el.setAttribute('aria-label', `${e.kind} at beat ${(e.beat ?? 0) + 1}`)
        el.style.cursor = 'pointer'
        el.style.pointerEvents = 'all'
      } else {
        el.style.pointerEvents = 'none'
      }
      if (props.selectedElement === key) {
        el.style.filter = 'drop-shadow(0 0 2px #16803c)'
        el.setAttribute('color', '#16803c')
      }
    }
    const grouped = (
      data: Omit<ScoreElement, 'part' | 'staff'>,
      draw: () => void,
    ) => {
      const el = ctx.openGroup('editable-score-mark')
      draw()
      ctx.closeGroup()
      tag(el, data)
    }
    const signatureTags = (s: Stave, beat?: number) => {
      for (const m of s.getModifiers()) {
        const kind = (
          {
            Clef: 'clef',
            KeySignature: 'key',
            TimeSignature: 'meter',
          } as Record<string, string>
        )[m.getCategory()]
        if (kind) tag(m.getSVGElement(), { kind, beat })
      }
    }
    const stave = new Stave(12, 38, width - 24)
    stave.addClef(props.staff.clef)
    const key =
      props.staff.key_signature ??
      props.timeline?.keys.find((k) => k.beat === 0)?.key ??
      props.part.key_signature
    if (key) stave.addKeySignature(key)
    if (props.part.show_time_signature !== false)
      stave.addTimeSignature(
        `${props.timeline?.meters.find((m) => m.beat === 0)?.beats ?? props.beatsPerBar}/${props.timeline?.meters.find((m) => m.beat === 0)?.unit ?? props.beatUnit}`,
      )
    stave.setContext(ctx).draw()
    signatureTags(stave)
    const measures = scoreMeasures(
      props.length,
      props.beatsPerBar,
      props.beatUnit,
      props.timeline?.meters,
    )
    for (const measure of measures) {
      const beat = measure.start
      if (beat < props.viewStart || beat > props.viewEnd) continue
      const x = xAt(beat)
      if (
        props.part.notes.some(
          (n) =>
            metadata(n, props.part).staff === props.staff.id &&
            n.beat >= measure.start &&
            n.beat < measure.end &&
            n.beat + n.duration > measure.end + 1e-8 &&
            !n.tied,
        )
      ) {
        const outline = ctx.openGroup('overfull-bar')
        outline?.setAttribute('aria-label', `Overfull bar ${measure.number}`)
        outline?.setAttribute('role', 'img')
        outline?.setAttribute('pointer-events', 'none')
        ctx.save()
        ctx.setStrokeStyle('#dc2626')
        ctx.setLineWidth(2)
        ctx.beginPath()
        ctx.moveTo(x - 12, 49)
        ctx.lineTo(xAt(measure.end) - 12, 49)
        ctx.lineTo(xAt(measure.end) - 12, 140)
        ctx.lineTo(x - 12, 140)
        ctx.lineTo(x - 12, 49)
        ctx.stroke()
        ctx.restore()
        ctx.closeGroup()
      }

      grouped({ kind: 'barline', beat }, () => {
        if (!props.timeline?.barlines?.some((b) => b.beat === beat)) {
          ctx.beginPath()
          ctx.moveTo(x - 12, 78)
          ctx.lineTo(x - 12, 118)
          ctx.stroke()
        }
        if (props.first !== false) ctx.fillText(String(measure.number), x - 8, 28)
      })
    }
    props.timeline?.barlines?.forEach((b, index) => {
      if (b.beat < props.viewStart || b.beat > props.viewEnd) return
      grouped({ kind: 'special-barline', beat: b.beat, index }, () => {
        const x = xAt(b.beat) - 12
        ctx.beginPath()
        if (b.style === 'dashed') {
          for (let y = 78; y < 118; y += 8) {
            ctx.moveTo(x, y)
            ctx.lineTo(x, y + 4)
          }
        } else {
          ctx.moveTo(x - 4, 78)
          ctx.lineTo(x - 4, 118)
          ctx.moveTo(x, 78)
          ctx.lineTo(x, 118)
        }
        ctx.stroke()
        if (b.style === 'final') {
          ctx.beginPath()
          ctx.moveTo(x + 2, 78)
          ctx.lineTo(x + 2, 118)
          ctx.stroke()
        }
      })
    })
    const signatures = new Set([
      ...(props.timeline?.meters || []).map((m) => m.beat),
      ...(props.timeline?.keys || []).map((k) => k.beat),
      ...(props.staff.clef_changes || []).map((c) => c.beat),
    ])
    for (const beat of signatures) {
      if (
        beat <= 0 ||
        beat < props.viewStart - props.barLength ||
        beat > props.viewEnd + props.barLength
      )
        continue
      const signature = new Stave(xAt(beat) - 168, 38, 168)
        .setBegBarType(Barline.type.NONE)
        .setEndBarType(Barline.type.NONE)
        .setConfigForLines(
          Array.from({ length: 5 }, () => ({ visible: false })),
        )
      const clef = props.staff.clef_changes?.find((c) => c.beat === beat),
        key = props.timeline?.keys.find((k) => k.beat === beat),
        meter = props.timeline?.meters.find((m) => m.beat === beat)
      if (clef) signature.addClef(clef.clef, 'small')
      if (key && !props.staff.key_signature) signature.addKeySignature(key.key)
      if (meter && props.part.show_time_signature !== false)
        signature.addTimeSignature(`${meter.beats}/${meter.unit}`)
      signature.setContext(ctx).draw()
      signatureTags(signature, beat)
    }
    for (const [index, r] of (props.timeline?.repeats || []).entries()) {
      grouped({ kind: 'repeat', index, beat: r.start }, () => {
        const bar = new Barline(Barline.type.REPEAT_BEGIN).setContext(ctx)
        bar.drawRepeatBar(stave, xAt(r.start) - 12, true)
        bar.drawRepeatBar(stave, xAt(r.end) - 12, false)
        if (props.first !== false) ctx.fillText(`×${r.times}`, xAt(r.end) - 28, 62)
        if (r.first_ending != null && props.first !== false) {
          const a = xAt(r.first_ending),
            b = xAt(r.end) - 12
          ctx.fillText('1.', a + 4, 54)
          ctx.beginPath()
          ctx.moveTo(a, 58)
          ctx.lineTo(a, 40)
          ctx.lineTo(b, 40)
          ctx.lineTo(b, 58)
          ctx.stroke()
        }
      })
    }
    // Tempo marks sit above the first staff of each part; text marks belong to a staff.
    const lo = props.viewStart - props.barLength,
      hi = props.viewEnd + props.barLength
    if (staves(props.part)[0]?.id === props.staff.id)
      for (const t of props.timeline?.tempos || []) {
        if (t.beat < lo || t.beat > hi) continue
        grouped({ kind: 'tempo', beat: t.beat }, () => {
          ctx.save()
          ctx.setFont('Arial', 12, 'bold')
          // Right of the bar number so the two never overlap.
          ctx.fillText(
            `♩ = ${Math.round(t.bpm * 10) / 10}`,
            xAt(t.beat) + 6,
            18,
          )
          ctx.restore()
        })
      }
    for (const mark of props.staff.marks || []) {
      if (mark.beat < lo || mark.beat > hi) continue
      grouped({ kind: 'mark', mark: mark.id, beat: mark.beat }, () => {
        const x = xAt(mark.beat) - 12
        ctx.save()
        if (mark.kind === 'chord') {
          ctx.setFont('Space Grotesk', 16, 'bold')
          ctx.fillText(mark.text, x, 48)
        } else if (mark.kind === 'rehearsal') {
          ctx.setFont('Arial', 13, 'bold')
          const width = Math.max(18, mark.text.length * 9 + 8)
          ctx.beginPath()
          ctx.rect(x + 8, 4, width, 18)
          ctx.stroke()
          ctx.fillText(mark.text, x + 12, 18)
        } else if (mark.kind === 'lyric') {
          ctx.setFont('Arial', 12, '', 'italic')
          ctx.fillText(mark.text, x, 152)
        } else if (mark.kind === 'expression') {
          ctx.setFont('Arial', 12, '', 'italic')
          ctx.fillText(mark.text, x, 70)
        } else if (mark.kind === 'tempo') {
          ctx.setFont('Arial', 12, 'bold')
          ctx.fillText(
            mark.text,
            x +
              18 +
              (staves(props.part)[0]?.id === props.staff.id &&
              props.timeline?.tempos?.some((t) => t.beat === mark.beat)
                ? 60
                : 0),
            18,
          )
        } else {
          ctx.setFont('Arial', 12, mark.kind === 'cue' ? 'bold' : '')
          ctx.fillText(mark.kind === 'cue' ? `▶ ${mark.text}` : mark.text, x, 70)
        }
        ctx.restore()
      })
    }
    const nav = props.timeline?.navigation
    if (nav && props.first !== false) {
      grouped({ kind: 'navigation', beat: nav.at }, () => {
        ctx.fillText(nav.target === 0 ? 'D.C.' : 'D.S.', xAt(nav.at), 55)
        if (nav.target) ctx.fillText('Segno', xAt(nav.target), 55)
        if (nav.fine != null) ctx.fillText('Fine', xAt(nav.fine), 55)
        if (nav.coda) {
          ctx.fillText('To coda', xAt(nav.coda[0]), 55)
          ctx.fillText('Coda', xAt(nav.coda[1]), 55)
        }
      })
    }

    const automatic = new Set<Note>(),
      wholeBar = new Map<Note, { start: number; end: number }>()
    const displayNotes = props.part.notes.filter(
      (n) =>
        n.beat + n.duration >= props.viewStart &&
        n.beat <= props.viewEnd &&
        metadata(n, props.part).staff === props.staff.id,
    )
    const voiceNumbers = [
      ...new Set(displayNotes.map((n) => metadata(n, props.part).voice)),
    ]
    if (!voiceNumbers.length) voiceNumbers.push(1)
    for (const m of measures.filter(
      (m) => m.end >= props.viewStart && m.start <= props.viewEnd,
    ))
      for (const voice of voiceNumbers) {
        let cursor = m.start
        const covered = displayNotes
          .filter(
            (n) =>
              !n.notation?.grace_to &&
              metadata(n, props.part).voice === voice &&
              n.beat < m.end &&
              n.beat + n.duration > m.start,
          )
          .sort((a, b) => a.beat - b.beat)
        const gap = (start: number, end: number) => {
          if (end - start < 1e-8) return
          const n: Note = {
            id: `rest-${m.number}-${voice}-${start}`,
            pitch: 60,
            beat: start,
            duration: end - start,
            velocity: 0,
            rest: true,
            tied: false,
            notation: {
              staff: props.staff.id,
              step: bottomStep(props.staff.clef) + 4,
              alter: 0,
              voice,
              base: end - start,
              dots: 0,
              tuplet_actual: 1,
              tuplet_normal: 1,
            },
          }
          automatic.add(n)
          if (start <= m.start + 1e-8 && end >= m.end - 1e-8)
            wholeBar.set(n, { start: m.start, end: m.end })
          displayNotes.push(n)
        }
        const coverage = [
          ...covered,
          ...(props.staff.hidden_rests || []).filter(
            (r) =>
              r.voice === voice &&
              r.beat < m.end &&
              r.beat + r.duration > m.start,
          ),
        ].sort((a, b) => a.beat - b.beat)
        for (const n of coverage) {
          if (n.beat > cursor) gap(cursor, n.beat)
          cursor = Math.max(cursor, n.beat + n.duration)
        }
        // While editing, a bar's unfilled tail stays blank; the performance view
        // completes every bar with rests.
        if (cursor < m.end && props.interactive === false) gap(cursor, m.end)
      }
    const fragments = displayNotes
      .flatMap((n) => {
        const v = metadata(n, props.part),
          result: {
            n: typeof n
            v: typeof v
            beat: number
            glyph: string
            dots: number
            fragment: number
            center?: { start: number; end: number }
          }[] = []
        // A rest filling its bar is a centred whole rest whatever the meter.
        const bar = wholeBar.get(n)
        if (bar) {
          result.push({ n, v, beat: n.beat, glyph: 'w', dots: 0, fragment: 0, center: bar })
          return result
        }
        let beat = n.beat,
          remaining = n.duration,
          fragment = 0
        while (remaining > 1e-8 && fragment < 256) {
          const boundary =
            measures.find((m) => m.end > beat + 1e-8)?.end ??
            beat + props.barLength
          const length = Math.min(remaining, boundary - beat)
          const glyphs = durationGlyphs(length) || [
            {
              duration:
                (
                  {
                    0.0625: '64',
                    0.125: '32',
                    0.25: '16',
                    0.5: '8',
                    1: 'q',
                    2: 'h',
                    4: 'w',
                    8: '1/2',
                  } as Record<number, string>
                )[v.base] || 'q',
              dots: v.dots,
              beats: length,
            },
          ]
          for (const glyph of glyphs) {
            result.push({
              n,
              v,
              beat,
              glyph: glyph.duration,
              dots: glyph.dots,
              fragment: fragment++,
            })
            beat += glyph.beats
            remaining -= glyph.beats
          }
        }
        return result
      })
      .filter(
        (item) =>
          item.beat >= props.viewStart - props.barLength &&
          item.beat <= props.viewEnd + props.barLength,
      )
      .sort((a, b) => a.beat - b.beat || a.v.voice - b.v.voice)
    const chordMap = new Map<string, typeof fragments>()
    for (const item of fragments) {
      const key = `${item.beat}:${item.v.voice}:${item.glyph}:${item.dots}:${item.n.rest ? item.n.id : ''}:${item.v.grace_to ? item.n.id : ''}:${item.v.articulation || ''}`
      if (!chordMap.has(key)) chordMap.set(key, [])
      chordMap.get(key)!.push(item)
    }
    const chords = [...chordMap.values()].map((heads) => {
      heads.sort((a, b) => a.v.step - b.v.step || a.v.alter - b.v.alter)
      return { ...heads[0]!, heads }
    })
    const previous = new Map<string, { note: StaveNote; index: number }>()
    const beams: Beam[] = []
    const voices = new Map<
      string,
      { notes: StaveNote[]; items: typeof chords }
    >()
    for (const item of chords) {
      const step = ((item.v.step % 7) + 7) % 7,
        octave = Math.floor(item.v.step / 7)
      const accidental =
        (
          { '-2': 'bb', '-1': 'b', '0': '', '1': '#', '2': '##' } as Record<
            string,
            string
          >
        )[String(item.v.alter)] || ''
      const note = new (item.v.grace_to ? GraceNote : StaveNote)({
        clef:
          props.staff.clef_changes?.filter((c) => c.beat <= item.beat).at(-1)
            ?.clef || props.staff.clef,
        keys: item.heads.map(
          (h) =>
            `${['c', 'd', 'e', 'f', 'g', 'a', 'b'][((h.v.step % 7) + 7) % 7]}${({ '-2': 'bb', '-1': 'b', '0': '', '1': '#', '2': '##' } as Record<string, string>)[String(h.v.alter)] || ''}/${Math.floor(h.v.step / 7)}`,
        ),
        duration: item.glyph + (item.n.rest ? 'r' : ''),
        dots: item.dots,
        stemDirection: item.v.voice % 2 ? 1 : -1,
      })
      if (item.v.articulation)
        note.addModifier(
          new Articulation(
            (
              {
                staccato: 'a.',
                tenuto: 'a-',
                accent: 'a>',
                marcato: 'a^',
              } as Record<string, string>
            )[item.v.articulation]!,
          ),
        )
      for (let d = 0; d < item.dots; d++)
        Dot.buildAndAttach([note], { all: true })
      const group = `${measures.findIndex((m) => item.beat >= m.start && item.beat < m.end)}:${item.v.voice}`
      if (!voices.has(group)) voices.set(group, { notes: [], items: [] })
      voices.get(group)!.notes.push(note)
      voices.get(group)!.items.push(item)
    }
    for (const { notes, items } of voices.values()) {
      const voice = new Voice({
        numBeats: props.beatsPerBar,
        beatValue: props.beatUnit,
      }).setStrict(false)
      voice.addTickables(notes)
      Accidental.applyAccidentals(
        [voice],
        props.staff.key_signature ||
          props.timeline?.keys.filter((k) => k.beat <= items[0]!.beat).at(-1)
            ?.key ||
          key ||
          'C',
      )
      new Formatter()
        .joinVoices([voice])
        .format([voice], Math.max(200, props.barLength * props.scale))
      notes.forEach((note, i) => {
        note.setStave(stave)
        note.getTickContext().setX(0)
        const offset = note.getNoteHeadBeginX()
        const center = items[i]!.center
        note
          .getTickContext()
          .setX(
            center
              ? (xAt(center.start) + xAt(center.end)) / 2 - offset - 7
              : xAt(items[i]!.beat) - offset - (items[i]!.v.grace_to ? 24 : 0),
          )
      })
      notes.forEach((note, i) => {
        for (const modifier of note.getModifiers()) {
          const kind = (
            {
              Articulation: 'articulation',
              Accidental: 'accidental',
              Dot: 'dot',
            } as Record<string, string>
          )[modifier.getCategory()]
          if (!kind) continue
          const draw = modifier.draw.bind(modifier)
          modifier.draw = () =>
            grouped(
              {
                kind,
                note:
                  items[i]!.heads[modifier.getIndex() ?? 0]?.n.id ||
                  items[i]!.n.id,
                beat: items[i]!.beat,
              },
              draw,
            )
        }
      })
      // Beams only join notes that follow each other directly; a hidden rest or
      // other unfilled gap ends the group so the written values read correctly.
      const glyphBeats: Record<string, number> = {
        '1/2': 8, w: 4, h: 2, q: 1, '8': 0.5, '16': 0.25, '32': 0.125, '64': 0.0625,
      }
      let run: StaveNote[] = [],
        runEnd = -1
      items.forEach((item, i) => {
        const beats = item.center
          ? item.center.end - item.center.start
          : ((glyphBeats[item.glyph] ?? 1) * (2 - 2 ** -item.dots) * item.v.tuplet_normal) /
            item.v.tuplet_actual
        if (run.length && Math.abs(item.beat - runEnd) > 1e-6) {
          beams.push(...Beam.generateBeams(run))
          run = []
        }
        run.push(notes[i]!)
        runEnd = item.beat + beats
      })
      if (run.length) beams.push(...Beam.generateBeams(run))
      voice.draw(ctx, stave)
      notes.forEach((note, i) => {
        const item = items[i]!
        item.heads.forEach((h, index) => {
          const el =
            item.heads.length === 1
              ? note.getSVGElement()
              : note.noteHeads[index]?.getSVGElement()
          if (el && automatic.has(h.n))
            tag(el, { kind: 'rest', beat: h.n.beat, rest: h.n })
          if (el && !automatic.has(h.n)) {
            el.dataset.selected = String(
              props.selected.has(noteKey(props.part.id, h.n.id)),
            )
            el.dataset.noteId = h.n.id
            el.dataset.partId = props.part.id
            el.dataset.scoreBeat = String(h.beat)
            el.dataset.duration = h.glyph
            el.dataset.dots = String(h.dots)
            el.setAttribute('role', props.interactive === false ? 'img' : 'button')
            if (props.interactive === false) el.style.pointerEvents = 'none'
            el.setAttribute(
              'aria-label',
              `${h.n.rest ? 'Rest' : 'Note ' + h.n.pitch} at beat ${h.n.beat + 1}`,
            )
          }
          const first = previous.get(h.n.id)
          if (first && !h.n.rest)
            grouped({ kind: 'rhythm', note: h.n.id, beat: h.beat }, () =>
              new StaveTie({
                firstNote: first.note,
                lastNote: note,
                firstIndexes: [first.index],
                lastIndexes: [index],
              })
                .setContext(ctx)
                .draw(),
            )
          previous.set(h.n.id, { note, index })
          if (
            !automatic.has(h.n) &&
            !durationGlyphs(h.n.duration) &&
            h.v.tuplet_actual === h.v.tuplet_normal
          )
            ctx.fillText(`${h.n.duration} beats*`, xAt(h.n.beat), 180)
        })
      })
      for (let i = 0; i < notes.length;) {
        const v = items[i]!.v
        if (v.tuplet_actual === v.tuplet_normal) {
          i++
          continue
        }
        let end = i + 1
        while (
          end < notes.length &&
          end - i < v.tuplet_actual &&
          items[end]!.v.tuplet_actual === v.tuplet_actual &&
          items[end]!.v.tuplet_normal === v.tuplet_normal
        )
          end++
        if (end - i > 1)
          grouped(
            {
              kind: 'tuplet',
              note: items[i]!.n.id,
              notes: items.slice(i, end).map((i) => i.n.id),
              beat: items[i]!.beat,
            },
            () =>
              new Tuplet(notes.slice(i, end), {
                numNotes: v.tuplet_actual,
                notesOccupied: v.tuplet_normal,
              })
                .setContext(ctx)
                .draw(),
          )
        i = end
      }
    }
    for (const n of props.part.notes) {
      const v = n.notation,
        first = previous.get(n.id)
      if (!v || !first) continue
      const tied = v.tie_to ? previous.get(v.tie_to) : undefined,
        slurred = v.slur_to ? previous.get(v.slur_to) : undefined
      if (tied)
        grouped({ kind: 'tie', note: n.id, beat: n.beat }, () =>
          new StaveTie({
            firstNote: first.note,
            lastNote: tied.note,
            firstIndexes: [first.index],
            lastIndexes: [tied.index],
          })
            .setContext(ctx)
            .draw(),
        )
      if (slurred)
        grouped({ kind: 'slur', note: n.id, beat: n.beat }, () =>
          new Curve(first.note, slurred.note, {}).setContext(ctx).draw(),
        )
      if (v.octave) {
        grouped({ kind: 'octave', note: n.id, beat: n.beat }, () => {
          const x = xAt(n.beat)
          ctx.fillText(
            v.octave! > 0
              ? v.octave === 1
                ? '8va'
                : '15ma'
              : v.octave === -1
                ? '8vb'
                : '15mb',
            x,
            12,
          )
          ctx.beginPath()
          ctx.moveTo(x + 26, 10)
          ctx.lineTo(xAt(n.beat + n.duration), 10)
          ctx.lineTo(xAt(n.beat + n.duration), 16)
          ctx.stroke()
        })
      }
    }
    // Written dynamics: velocity ramp points at a standard level show their name.
    for (const node of nodesFromEvents(staffDynamicsEvents(props.part, props.staff))) {
      const name = dynamicName(node.value)
      if (!name || node.beat < lo || node.beat > hi) continue
      grouped({ kind: 'dynamic', beat: node.beat }, () => {
        ctx.save()
        ctx.setFont('serif', 14, 'bold', 'italic')
        ctx.fillText(name, xAt(node.beat) - 4, 172)
        ctx.restore()
      })
    }
    // Phrasing curves and brackets: note anchors follow the engraved heads,
    // free ends sit at their beat above the staff. Selected ones get handles.
    for (const curve of props.staff.curves || []) {
      const startNote = curve.start_note ? previous.get(curve.start_note) : undefined,
        endNote = curve.end_note ? previous.get(curve.end_note) : undefined
      const startBeat = curve.start_note
          ? props.part.notes.find((n) => n.id === curve.start_note)?.beat ?? curve.start_beat
          : curve.start_beat,
        endBeat = curve.end_note
          ? props.part.notes.find((n) => n.id === curve.end_note)?.beat ?? curve.end_beat
          : curve.end_beat
      if (endBeat < lo || startBeat > hi) continue
      const hairpin = isHairpin(curve.kind)
      const above = hairpin ? false : curve.kind === 'bracket' ? curve.height >= 0 : curve.height <= 0
      const anchor = (
        note: { note: StaveNote; index: number } | undefined,
        beat: number,
        end: boolean,
      ) => {
        if (note) {
          const heads = note.note.getYs()
          const x = end ? note.note.getNoteHeadEndX() : note.note.getNoteHeadBeginX()
          const y = heads[note.index] ?? heads[0] ?? 98
          return { x: (x + note.note.getNoteHeadBeginX()) / 2 + (end ? 4 : 4), y: above ? y - 9 : y + 9 }
        }
        return { x: xAt(beat), y: hairpin ? 152 : above ? 66 : 132 }
      }
      const a = anchor(startNote, startBeat, false),
        b = anchor(endNote, endBeat, true)
      a.y += curve.lift
      b.y += curve.end_lift ?? 0
      const key = elementKey({ kind: 'curve', curve: curve.id, beat: startBeat, part: props.part.id, staff: props.staff.id })
      const selectedCurve = props.selectedElement === key
      grouped({ kind: 'curve', curve: curve.id, beat: startBeat }, () => {
        ctx.save()
        ctx.setLineWidth(curve.kind === 'slur' ? 1.8 : 1.4)
        ctx.beginPath()
        const d = hairpin
          ? hairpinPath(a.x, b.x, (a.y + b.y) / 2, curve.height, curve.kind === 'crescendo')
          : curve.kind === 'slur'
            ? slurPath(a.x, a.y, b.x, b.y, curve.height)
            : bracketPath(a.x, b.x, Math.min(a.y, b.y), curve.height)
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path')
        path.setAttribute('d', d)
        path.setAttribute('fill', 'none')
        path.setAttribute('stroke', '#000')
        path.setAttribute('stroke-width', curve.kind === 'slur' ? '1.8' : '1.4')
        ;(ctx as unknown as { parent: SVGElement }).parent.appendChild(path)
        ctx.restore()
      })
      if (selectedCurve) {
        const midX = (a.x + b.x) / 2,
          midY = hairpin
            ? (a.y + b.y) / 2 - Math.max(2, curve.height) / 2 - 4
            : (a.y + b.y) / 2 + curve.height * (curve.kind === 'slur' ? 0.75 : 1)
        const box = ctx.openGroup('curve-box')
        ctx.save()
        ctx.setStrokeStyle('#16803c')
        ctx.setLineWidth(1)
        ctx.beginPath()
        const top = Math.min(a.y, b.y, midY) - 6,
          bottom = Math.max(a.y, b.y, midY) + 6
        ctx.rect(Math.min(a.x, b.x) - 6, top, Math.abs(b.x - a.x) + 12, bottom - top)
        ctx.stroke()
        ctx.restore()
        ctx.closeGroup()
        box?.setAttribute('pointer-events', 'none')
        box?.setAttribute('stroke-dasharray', '3 3')
        for (const [handle, x, y] of [
          ['start', a.x, a.y],
          ['end', b.x, b.y],
          ['shape', midX, midY],
        ] as const) {
          const g = ctx.openGroup('curve-handle')
          ctx.save()
          ctx.setFillStyle('#fff')
          ctx.setStrokeStyle('#16803c')
          ctx.setLineWidth(1.5)
          ctx.beginPath()
          if (handle === 'shape') ctx.arc(x, y, 5, 0, Math.PI * 2, false)
          else ctx.rect(x - 4.5, y - 4.5, 9, 9)
          ctx.fill()
          ctx.stroke()
          ctx.restore()
          ctx.closeGroup()
          if (g) {
            g.dataset.curveHandle = handle
            g.dataset.curveId = curve.id
            g.dataset.curvePart = props.part.id
            g.dataset.curveStaff = props.staff.id
            g.setAttribute('role', 'button')
            g.setAttribute(
              'aria-label',
              `${handle === 'shape' ? 'Shape' : handle === 'start' ? 'Start' : 'End'} handle of ${curve.kind}`,
            )
            g.style.cursor = handle === 'shape' ? 'ns-resize' : 'ew-resize'
            g.style.pointerEvents = 'all'
          }
        }
      }
    }
    for (const beam of beams)
      grouped(
        {
          kind: 'rhythm',
          note: beam.getNotes()[0]?.getSVGElement()?.dataset.noteId,
          notes: beam
            .getNotes()
            .map((n) => n.getSVGElement()?.dataset.noteId)
            .filter((id): id is string => !!id),
        },
        () => beam.setContext(ctx).draw(),
      )
    // VexFlow places a note-wide pointer rectangle after its modifiers. Put
    // modifier targets above that rectangle so selecting an accent selects it.
    for (const el of host.value.querySelectorAll<SVGGElement>(
      '[data-score-element]',
    )) {
      const owner = el.parentElement?.closest('[data-note-id]')
      if (owner) owner.appendChild(el)
      if (
        ['repeat', 'navigation', 'rhythm', 'tie', 'slur', 'octave', 'curve'].includes(
          JSON.parse(el.dataset.scoreElement!).kind,
        )
      ) {
        for (const path of el.querySelectorAll('path,line')) {
          const hit = path.cloneNode(true) as SVGElement
          hit.dataset.scoreHit = 'true'
          hit.setAttribute('opacity', '0')
          hit.setAttribute('stroke', '#000')
          hit.setAttribute('stroke-width', '8')
          hit.setAttribute('fill', 'none')
          hit.style.pointerEvents = 'stroke'
          el.insertBefore(hit, el.firstChild)
        }
        continue
      }
      const b = el.getBBox(),
        target = document.createElementNS('http://www.w3.org/2000/svg', 'rect')
      target.setAttribute('x', String(b.x - 2))
      target.setAttribute('y', String(b.y - 2))
      target.setAttribute('width', String(Math.max(6, b.width + 4)))
      target.setAttribute('height', String(Math.max(6, b.height + 4)))
      target.dataset.scoreHit = 'true'
      target.setAttribute('fill', '#000')
      target.setAttribute('stroke', 'none')
      target.setAttribute('opacity', '0')
      target.setAttribute('pointer-events', 'all')
      el.insertBefore(target, el.firstChild)
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
watch(signature, scheduleRender)
watch(() => props.selected, applySelection)
watch(
  () => props.selectedElement,
  (next, previous) => {
    // Curves draw handles while selected, which needs a re-engrave.
    if (next?.includes('"kind":"curve"') || previous?.includes('"kind":"curve"'))
      scheduleRender()
    for (const el of host.value?.querySelectorAll<SVGElement>(
      '[data-score-element]',
    ) || []) {
      el.dataset.selected = String(
        el.dataset.scoreElement === props.selectedElement,
      )
      el.style.filter =
        el.dataset.scoreElement === props.selectedElement
          ? 'drop-shadow(0 0 2px #16803c)'
          : ''
    }
  },
)
onMounted(() => {
  observer = new IntersectionObserver(
    (entries) => {
      visible = entries[0]?.isIntersecting ?? false
      if (visible) render()
      else if (host.value) host.value.innerHTML = ''
    },
    { rootMargin: '300px' },
  )
  if (host.value) observer.observe(host.value)
})
onBeforeUnmount(() => {
  observer?.disconnect()
  if (frame) cancelAnimationFrame(frame)
})
</script>
<template>
  <div
    class="score-staff notation-surface"
    :data-staff-id="staff.id"
    :data-part-id="part.id"
    :data-bottom-step="bottomStep(staff.clef)"
  >
    <div ref="host" class="staff-engraving" />
    <div
      class="score-playhead"
      :style="{ transform: `translateX(${xAt(beat)}px)` }"
    />
    <p v-if="error" role="alert">Notation: {{ error }}</p>
  </div>
</template>
<style scoped>
.score-staff {
  position: relative;
  height: 190px;
  min-height: 190px;
  background: #ffffff;
  color: #000000;
}
.score-playhead {
  pointer-events: none;
  position: absolute;
  left: 0;
  top: 25px;
  bottom: 12px;
  width: 2px;
  background: #16803c;
  box-shadow: none;
  will-change: transform;
}
.staff-engraving :deep([data-note-id]) {
  cursor: pointer;
}
.staff-engraving :deep([data-note-id]:not([data-selected='true']):hover path),
.staff-engraving
  :deep([data-score-element]:not([data-selected='true']):hover path),
.staff-engraving
  :deep([data-score-element]:not([data-selected='true']):hover text),
.staff-engraving
  :deep([data-score-element]:not([data-selected='true']):hover line) {
  fill: #e87816 !important;
  stroke: #e87816 !important;
}
.staff-engraving :deep([data-selected='true'] path),
.staff-engraving :deep([data-selected='true'] text) {
  fill: #16803c !important;
  stroke: #16803c !important;
}
.score-staff p {
  position: absolute;
  top: 0;
  color: #9c2d16;
}
</style>
