<script setup lang="ts">
import { nextTick, onMounted, onBeforeUnmount, ref, watch } from 'vue'
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
  type ScoreAnchor,
} from '../score'
import { durationGlyphs } from '../notation'
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
  beat: number
  origin: number
}>()
const xAt = (beat: number) =>
  scoreX(beat, props.anchors, props.scale, props.origin)
const host = ref<HTMLDivElement>(),
  error = ref('')
let observer: IntersectionObserver | undefined,
  visible = false
function render() {
  if (!host.value || !visible) return
  host.value.innerHTML = ''
  error.value = ''
  try {
    const width = xAt(props.length) + 100
    const renderer = new Renderer(host.value, Renderer.Backends.SVG)
    renderer.resize(width, 190)
    const ctx = renderer.getContext()
    ctx.setFillStyle('#dedbd0')
    ctx.setStrokeStyle('#a5aaa8')
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
      ctx.beginPath()
      ctx.moveTo(x - 12, 78)
      ctx.lineTo(x - 12, 118)
      ctx.stroke()
      ctx.fillText(String(measure.number), x - 8, 28)
    }
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
    }
    for (const r of props.timeline?.repeats || []) {
      const bar = new Barline(Barline.type.REPEAT_BEGIN).setContext(ctx)
      bar.drawRepeatBar(stave, xAt(r.start) - 12, true)
      bar.drawRepeatBar(stave, xAt(r.end) - 12, false)
      ctx.fillText(`×${r.times}`, xAt(r.end) - 28, 62)
      if (r.first_ending != null) {
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
    }
    const nav = props.timeline?.navigation
    if (nav) {
      ctx.fillText(nav.target === 0 ? 'D.C.' : 'D.S.', xAt(nav.at), 55)
      if (nav.target) ctx.fillText('Segno', xAt(nav.target), 55)
      if (nav.fine != null) ctx.fillText('Fine', xAt(nav.fine), 55)
      if (nav.coda) {
        ctx.fillText('To coda', xAt(nav.coda[0]), 55)
        ctx.fillText('Coda', xAt(nav.coda[1]), 55)
      }
    }

    const automatic = new Set<Note>()
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
          displayNotes.push(n)
        }
        for (const n of covered) {
          if (n.beat > cursor) gap(cursor, n.beat)
          cursor = Math.max(cursor, n.beat + n.duration)
        }
        if (cursor < m.end) gap(cursor, m.end)
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
          }[] = []
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
    const beams: StaveNote[][] = []
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
        note
          .getTickContext()
          .setX(xAt(items[i]!.beat) - offset - (items[i]!.v.grace_to ? 24 : 0))
        items[i]!.heads.forEach((h, index) => {
          if (props.selected.has(noteKey(props.part.id, h.n.id)))
            note.setKeyStyle(index, {
              fillStyle: '#5de1df',
              strokeStyle: '#5de1df',
            })
        })
      })
      voice.draw(ctx, stave)
      notes.forEach((note, i) => {
        const item = items[i]!
        item.heads.forEach((h, index) => {
          const el =
            item.heads.length === 1
              ? note.getSVGElement()
              : note.noteHeads[index]?.getSVGElement()
          if (el && !automatic.has(h.n)) {
            el.dataset.noteId = h.n.id
            el.dataset.partId = props.part.id
            el.dataset.scoreBeat = String(h.beat)
            el.dataset.duration = h.glyph
            el.dataset.dots = String(h.dots)
            el.setAttribute('role', 'button')
            el.setAttribute(
              'aria-label',
              `${h.n.rest ? 'Rest' : 'Note ' + h.n.pitch} at beat ${h.n.beat + 1}`,
            )
          }
          const first = previous.get(h.n.id)
          if (first && !h.n.rest)
            new StaveTie({
              firstNote: first.note,
              lastNote: note,
              firstIndexes: [first.index],
              lastIndexes: [index],
            })
              .setContext(ctx)
              .draw()
          previous.set(h.n.id, { note, index })
          if (
            !automatic.has(h.n) &&
            !durationGlyphs(h.n.duration) &&
            h.v.tuplet_actual === h.v.tuplet_normal
          )
            ctx.fillText(`${h.n.duration} beats*`, xAt(h.n.beat), 180)
        })
      })
      beams.push(notes)
      for (let i = 0; i < notes.length; ) {
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
          new Tuplet(notes.slice(i, end), {
            numNotes: v.tuplet_actual,
            notesOccupied: v.tuplet_normal,
          })
            .setContext(ctx)
            .draw()
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
        new StaveTie({
          firstNote: first.note,
          lastNote: tied.note,
          firstIndexes: [first.index],
          lastIndexes: [tied.index],
        })
          .setContext(ctx)
          .draw()
      if (slurred)
        new Curve(first.note, slurred.note, {}).setContext(ctx).draw()
      if (v.octave) {
        const x = xAt(n.beat)
        ctx.fillText(
          v.octave > 0
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
      }
    }
    for (const notes of beams)
      if (notes.length > 1)
        for (const beam of Beam.generateBeams(notes))
          beam.setContext(ctx).draw()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
watch(
  () => [
    props.part,
    props.staff,
    props.anchors,
    props.timeline,
    props.viewStart,
    props.viewEnd,
    props.scale,
    props.length,
    props.barLength,
    props.beatsPerBar,
    props.beatUnit,
    props.selected,
  ],
  () => nextTick(render),
  { deep: true },
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
onBeforeUnmount(() => observer?.disconnect())
</script>
<template>
  <div
    class="score-staff notation-surface"
    :data-staff-id="staff.id"
    :data-part-id="part.id"
    :data-bottom-step="bottomStep(staff.clef)"
  >
    <div ref="host" class="staff-engraving" />
    <div class="score-playhead" :style="{ left: `${xAt(beat)}px` }" />
    <p v-if="error" role="alert">Notation: {{ error }}</p>
  </div>
</template>
<style scoped>
.score-staff {
  position: relative;
  height: 190px;
  min-height: 190px;
}
.score-playhead {
  pointer-events: none;
  top: 25px;
  bottom: 12px;
}
.staff-engraving :deep([data-note-id]) {
  cursor: pointer;
}
.score-staff p {
  position: absolute;
  top: 0;
  color: var(--amber);
}
</style>
