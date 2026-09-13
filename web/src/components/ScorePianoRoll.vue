<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import type { Note, Part } from '../types'
import { newId } from '../id'
import {
  atBeat,
  metadata,
  noteKey,
  staves,
  withNotation,
  spelling,
  type ScoreAnchor,
} from '../score'
const props = defineProps<{
  part: Part
  selected: Set<string>
  editable: boolean
  fitNotes?: boolean
  length?: number
  tool: 'write' | 'select'
  anchors: ScoreAnchor[]
  scale: number
  origin: number
  width: number
  beat: number
}>()
const emit = defineEmits<{
  update: [part: Part]
  select: [keys: Set<string>]
  focus: [id: string]
  inspect: []
}>()
const root = ref<HTMLElement>(),
  grid = ref<HTMLElement>()
onMounted(() => {
  if (root.value)
    root.value.scrollTop = props.fitNotes ? 0 : Math.max(0, (pitches.value[0]! - 72) * 20)
})
const pitches = computed(() => {
  const high = Math.min(
      127,
      Math.max(props.fitNotes && props.part.notes.length ? 0 : 84, ...props.part.notes.map((n) => n.pitch + 3)),
    ),
    low = Math.max(0, Math.min(props.fitNotes && props.part.notes.length ? 127 : 36, ...props.part.notes.map((n) => n.pitch - 3)))
  return Array.from({ length: high - low + 1 }, (_, i) => high - i)
})
const xAt = (beat: number) => props.origin + beat * props.scale
const beatAt = (x: number) =>
  Math.max(0, Math.round(((x - props.origin) / props.scale) * 4) / 4)
const gesture = ref<{
  x: number
  y: number
  pitch: number
  beat: number
  lastX: number
  lastY: number
  mode: 'draw' | 'move' | 'resize' | 'select'
  keys: Set<string>
  pointer: number
} | null>(null)
const preview = ref<Note[]>([])
const marquee = computed(() => {
  const g = gesture.value
  return g?.mode === 'select'
    ? {
        left: Math.min(g.x, g.lastX),
        top: Math.min(g.y, g.lastY),
        width: Math.abs(g.lastX - g.x),
        height: Math.abs(g.lastY - g.y),
      }
    : null
})
function point(e: PointerEvent) {
  const r = grid.value!.getBoundingClientRect()
  return { x: e.clientX - r.left, y: e.clientY - r.top }
}
function down(e: PointerEvent) {
  if (e.button !== 0 || !(e.target instanceof Element) || !props.editable)
    return
  if (e.target.closest('.roll-key')) return
  const row = e.target.closest<HTMLElement>('[data-roll-pitch]')
  if (!row) return
  root.value?.focus({ preventScroll: true })
  emit('focus', props.part.id)
  const pos = point(e),
    el = e.target.closest<HTMLElement>('[data-roll-note]'),
    keys = new Set(props.selected),
    multi = e.ctrlKey || e.metaKey
  let mode: 'draw' | 'move' | 'resize' | 'select'
  if (el) {
    const key = noteKey(props.part.id, el.dataset.rollNote!)
    if (multi) {
      keys.has(key) ? keys.delete(key) : keys.add(key)
      emit('select', keys)
      e.preventDefault()
      return
    }
    if (!keys.has(key)) {
      keys.clear()
      keys.add(key)
    }
    emit('select', keys)
    mode = e.target.closest('[data-resize]') ? 'resize' : 'move'
  } else {
    mode = props.tool === 'write' && !multi ? 'draw' : 'select'
    if (!multi) keys.clear()
    emit('select', keys)
  }
  gesture.value = {
    ...pos,
    lastX: pos.x,
    lastY: pos.y,
    pitch: Number(row.dataset.rollPitch),
    beat: beatAt(pos.x),
    mode,
    keys,
    pointer: e.pointerId,
  }
  grid.value?.setPointerCapture(e.pointerId)
  e.preventDefault()
}
function move(e: PointerEvent) {
  const g = gesture.value
  if (!g) return
  const pos = point(e)
  g.lastX = pos.x
  g.lastY = pos.y
  if (g.mode === 'select') return
  if (g.mode === 'draw') {
    const start = Math.min(g.beat, beatAt(pos.x)),
      end = Math.max(g.beat, beatAt(pos.x))
    preview.value = [
      {
        id: 'preview',
        pitch: g.pitch,
        beat: start,
        duration: Math.max(0.25, end - start),
        velocity: 90,
        rest: false,
        tied: false,
      },
    ]
    return
  }
  const notes = props.part.notes.filter((n) =>
      g.keys.has(noteKey(props.part.id, n.id)),
    ),
    delta = Math.max(
      -Math.min(...notes.map((n) => n.beat)),
      beatAt(pos.x) - g.beat,
    ),
    pitchDelta = Math.max(
      -Math.min(...notes.map((n) => n.pitch)),
      Math.min(
        127 - Math.max(...notes.map((n) => n.pitch)),
        -Math.round((pos.y - g.y) / 20),
      ),
    )
  preview.value = notes.map((n) => ({
    ...n,
    beat: g.mode === 'move' ? n.beat + delta : n.beat,
    pitch: g.mode === 'move' ? n.pitch + pitchDelta : n.pitch,
    duration:
      g.mode === 'resize'
        ? Math.max(0.25, n.duration + beatAt(pos.x) - g.beat)
        : n.duration,
  }))
}
function up(e: PointerEvent) {
  const g = gesture.value
  if (!g) return
  if (g.mode === 'select') {
    const m = marquee.value!,
      keys = new Set(g.keys)
    for (const n of props.part.notes) {
      const x = xAt(n.beat),
        y = pitches.value.indexOf(n.pitch) * 20
      if (
        xAt(n.beat + n.duration) >= m.left &&
        x <= m.left + m.width &&
        y + 20 >= m.top &&
        y <= m.top + m.height
      )
        keys.add(noteKey(props.part.id, n.id))
    }
    emit('select', keys)
  } else if (preview.value.length) {
    const part = { ...props.part, staves: staves(props.part) },
      byId = new Map(part.notes.map((n) => [n.id, n]))
    const changed = preview.value.map((n) => {
      const original = byId.get(n.id),
        s =
          part.staves.find((s) => s.id === original?.notation?.staff) ||
          part.staves[0]!,
        v = {
          ...metadata(original || n, part),
          ...spelling(n.pitch, s),
          staff: s.id,
          octave: 0,
          base: n.duration,
          dots: 0,
          tuplet_actual: 1,
          tuplet_normal: 1,
        }
      return withNotation(
        atBeat({ ...n, id: original?.id || newId() }, n.beat),
        v,
        part,
      )
    })
    const placed = part.notes.filter((n) => !changed.some((c) => c.id === n.id))
    for (const n of changed) {
      const v = n.notation!
      const fits = (voice: number) =>
        !placed.some(
          (other) =>
            metadata(other, part).staff === v.staff &&
            metadata(other, part).voice === voice &&
            Math.abs(other.beat - n.beat) > 1e-8 &&
            other.beat < n.beat + n.duration - 1e-8 &&
            n.beat < other.beat + other.duration - 1e-8,
        )
      v.voice = [v.voice, 1, 2, 3, 4].find(fits) || v.voice
      placed.push(n)
    }
    part.notes = placed.sort((a, b) => a.beat - b.beat)
    emit('select', new Set(changed.map((n) => noteKey(part.id, n.id))))
    emit('update', part)
  }
  cancel()
  if (grid.value?.hasPointerCapture(e.pointerId))
    grid.value.releasePointerCapture(e.pointerId)
}
function cancel() {
  gesture.value = null
  preview.value = []
}
const shown = computed(() => [
  ...props.part.notes.filter(
    (n) => !n.rest && !preview.value.some((p) => p.id === n.id),
  ),
  ...preview.value,
])
</script>
<template>
  <div
    ref="root"
    class="piano-roll"
    tabindex="0"
    aria-label="Piano roll"
    @dblclick="
      ($event.target as Element).closest('[data-roll-note]') && emit('inspect')
    "
  >
    <div class="roll-ruler" :style="{ width: `${width}px` }">
      <span
        v-for="b in Math.ceil(length ?? ((width - origin) / scale))"
        :key="b"
        :style="{ left: `${xAt(b - 1)}px`, width: `${scale}px` }"
        >{{ b }}</span
      >
    </div>
    <div
      ref="grid"
      class="roll-grid"
      :style="{
        width: `${width}px`,
        '--beat-width': `${scale}px`,
        '--key-width': `${origin}px`,
      }"
      @pointerdown.stop="down"
      @pointermove.stop="move"
      @pointerup.stop="up"
      @pointercancel="cancel"
      @contextmenu.prevent
    >
      <div
        v-for="pitch in pitches"
        :key="pitch"
        class="roll-row"
        :class="{ black: [1, 3, 6, 8, 10].includes(pitch % 12) }"
        :data-roll-pitch="pitch"
      >
        <span class="roll-key" :style="{ width: `${origin}px` }"
          >{{
            ['C', 'C♯', 'D', 'E♭', 'E', 'F', 'F♯', 'G', 'A♭', 'A', 'B♭', 'B'][
              pitch % 12
            ]
          }}{{ Math.floor(pitch / 12) - 1 }}</span
        ><button
          v-for="n in shown.filter((n) => n.pitch === pitch)"
          :key="n.id"
          class="midi-note"
          :tabindex="editable ? 0 : -1"
          :aria-disabled="!editable"
          :data-roll-note="n.id"
          :class="{
            selected: selected.has(noteKey(part.id, n.id)),
            preview: n.id === 'preview',
          }"
          :style="{
            left: `${xAt(n.beat)}px`,
            width: `${Math.max(6, xAt(n.beat + n.duration) - xAt(n.beat))}px`,
          }"
          :aria-label="`Note ${n.pitch} at beat ${n.beat + 1}`"
        >
          <span v-if="editable" data-resize title="Drag to resize" />
        </button>
      </div>
      <div class="roll-playhead" :style="{ left: `${xAt(beat)}px` }" />
      <div
        v-if="marquee"
        class="roll-marquee"
        :style="{
          left: `${marquee.left}px`,
          top: `${marquee.top}px`,
          width: `${marquee.width}px`,
          height: `${marquee.height}px`,
        }"
      />
    </div>
  </div>
</template>
<style scoped>
.piano-roll {
  height: 440px;
  overflow-y: auto;
  outline: none;
  background: #182225;
}
.roll-grid {
  position: relative;
  touch-action: none;
}
.roll-row {
  position: relative;
  background-image: linear-gradient(90deg, #ffffff10 1px, transparent 1px);
  background-size: var(--beat-width) 100%;
  background-position: var(--key-width) 0;
  height: 20px;
  border-bottom: 1px solid #ffffff12;
}
.roll-row.black {
  background: #111a1d;
}
.roll-key {
  position: sticky;
  left: 0;
  z-index: 2;
  display: block;
  height: 20px;
  padding-left: 12px;
  background: #dfe5e5;
  color: #172123;
  font-size: 10px;
  border-bottom: 1px solid #aebaba;
}
.black .roll-key {
  background: #26373c;
  color: #dfe5e5;
}
.midi-note {
  position: absolute;
  top: 2px;
  height: 16px;
  min-height: 0;
  padding: 0;
  background: #78b9ad;
  border: 1px solid #9ae0cf;
  border-radius: 3px;
  cursor: grab;
}
.midi-note.selected,
.midi-note.preview {
  background: #16803c;
  border-color: #14532d;
}
.midi-note:not(.selected):hover { background:#e87816;border-color:#c55d08 }
.midi-note > span {
  position: absolute;
  right: 0;
  top: 0;
  bottom: 0;
  width: 7px;
  cursor: ew-resize;
}
.roll-marquee {
  position: absolute;
  border: 1px solid #79d5ce;
  background: #79d5ce20;
  pointer-events: none;
  z-index: 3;
}
.roll-playhead {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 2px;
  background: #16803c;
  pointer-events: none;
  z-index: 1;
}
.roll-ruler {
  position: sticky;
  top: 0;
  height: 22px;
  z-index: 4;
  background: #202b30;
  color: #aebdbf;
  font-size: 10px;
}
.roll-ruler span {
  position: absolute;
  padding: 3px 5px;
  border-left: 1px solid #ffffff25;
}
.piano-roll {
  min-width: 0;
}
.piano-roll {
  background: #fff;
}
.roll-row {
  background-color: #fff;
  background-image: linear-gradient(90deg, #dae2e3 1px, transparent 1px);
}
.roll-row.black {
  background-color: #f0f3f4;
  background-image: linear-gradient(90deg, #dae2e3 1px, transparent 1px);
}
.roll-ruler {
  background: #f0f3f4;
  color: #26373c;
}
.roll-ruler span {
  border-color: #cdd7d9;
}
</style>
