<script lang="ts">
/** Horizontal room kept left of the notehead for an accidental. */
export const GHOST_PAD = 40
</script>
<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import {
  Accidental,
  Dot,
  Formatter,
  Renderer,
  Stave,
  StaveNote,
  Voice,
} from 'vexflow'
/**
 * Pointer preview for the Write tool: the same VexFlow glyph the staff will
 * engrave, on an invisible stave at the staff's own geometry, so the ghost sits
 * exactly on the line or space where the click will land.
 */
const props = defineProps<{
  clef: string
  step: number
  alter: number | null
  base: number
  dots: number
  rest: boolean
  voice: number
}>()
const WIDTH = 100,
  HEIGHT = 190
const host = ref<HTMLDivElement>()
const codes: Record<number, string> = {
  0.0625: '64',
  0.125: '32',
  0.25: '16',
  0.5: '8',
  1: 'q',
  2: 'h',
  4: 'w',
  8: '1/2',
}
const accidentals: Record<string, string> = {
  '-2': 'bb',
  '-1': 'b',
  '1': '#',
  '2': '##',
}
function render() {
  const el = host.value
  if (!el) return
  el.innerHTML = ''
  try {
    const renderer = new Renderer(el, Renderer.Backends.SVG)
    renderer.resize(WIDTH, HEIGHT)
    const ctx = renderer.getContext()
    ctx.setFillStyle('#087f8c')
    ctx.setStrokeStyle('#087f8c')
    // Same vertical placement as ScoreStaff's Stave(12, 38, …); lines never drawn.
    const stave = new Stave(0, 38, WIDTH)
    const letter = ['c', 'd', 'e', 'f', 'g', 'a', 'b'][((props.step % 7) + 7) % 7],
      octave = Math.floor(props.step / 7),
      acc = props.alter == null ? '' : accidentals[String(props.alter)] || ''
    const note = new StaveNote({
      clef: props.clef,
      keys: [`${letter}${acc}/${octave}`],
      duration: (codes[props.base] || 'q') + (props.rest ? 'r' : ''),
      dots: props.dots,
      stemDirection: props.voice % 2 ? 1 : -1,
    })
    if (acc && !props.rest) note.addModifier(new Accidental(acc))
    for (let d = 0; d < props.dots; d++) Dot.buildAndAttach([note], { all: true })
    const voice = new Voice({ numBeats: 4, beatValue: 4 }).setStrict(false)
    voice.addTickables([note])
    new Formatter().joinVoices([voice]).format([voice], 60)
    note.setStave(stave)
    note.getTickContext().setX(GHOST_PAD - note.getNoteHeadBeginX())
    voice.draw(ctx, stave)
  } catch {
    el.innerHTML = ''
  }
}
onMounted(render)
watch(() => ({ ...props }), render, { flush: 'post' })
</script>
<template>
  <div class="ghost-note" aria-hidden="true"><div ref="host" /></div>
</template>
<style scoped>
.ghost-note {
  position: absolute;
  width: 100px;
  height: 190px;
  pointer-events: none;
  opacity: 0.6;
  z-index: 4;
}
.ghost-note :deep(svg) {
  display: block;
  overflow: visible;
}
/* VexFlow stamps stroke/fill and a pointer-events="auto" hit box as attributes;
   the preview must be teal throughout and never swallow the click it previews. */
.ghost-note :deep(*) {
  pointer-events: none !important;
}
.ghost-note :deep(path) {
  stroke: #087f8c;
}
.ghost-note :deep(text) {
  fill: #087f8c;
}
</style>
