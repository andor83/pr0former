<script setup lang="ts">
import { computed, nextTick, ref, watch, onMounted, onBeforeUnmount } from 'vue'
import type { Note, NoteNotation, Part, Project, Staff } from '../types'
import { newId } from '../id'
import { ApiError } from '../api'
import {
  atBeat,
  scoreAnchors,
  scoreMeasures,
  scoreX,
  scoreBeat,
  bottomStep,
  changeNote,
  durationKeys,
  durationLabels,
  durationSymbols,
  keyNames,
  keyLabel,
  keyAlter,
  metadata,
  noteKey,
  performanceParts,
  pitchAt,
  staves,
  withNotation,
  type NoteCommand,
} from '../score'
import ScoreStaff from './ScoreStaff.vue'
import { durationGlyphs } from '../notation'
import MidiLanes from './MidiLanes.vue'
import ScoreDynamics from './ScoreDynamics.vue'
import ScoreTimelineEditor from './ScoreTimelineEditor.vue'
const props = defineProps<{
  project: Project
  userId: string
  editable: boolean
  performance?: boolean
  saving?: boolean
  beats: Record<string, number>
  members?: { id: string; username: string }[]
  save?: (project: Project) => Promise<void>
}>()
const emit = defineEmits<{ focus: [id: string] }>()
const selected = ref(new Set<string>()),
  visible = ref(new Set(props.project.parts.map((p) => p.id))),
  focused = ref(
    props.project.parts.find((p) => p.performer === props.userId)?.id ||
      props.project.parts[0]?.id ||
      '',
  )
const collapsed = ref(false),
  showAll = ref(false),
  tool = ref<'select' | 'write'>('select'),
  scale = ref(90),
  follow = ref(true),
  view = ref('notation')
const clefBeat = ref(4),
  newClef = ref('bass')
const base = ref(1),
  dots = ref(0),
  alter = ref<number | null>(null),
  rest = ref(false),
  voice = ref(1),
  actual = ref(1),
  normal = ref(1)
const commandQueue: (() => void)[] = []
let arrowHeld = false
const error = ref(''),
  pending = ref(false),
  conflict = ref<Project | null>(null),
  viewport = ref<HTMLDivElement>(),
  root = ref<HTMLElement>()
const viewportStart = ref(0),
  viewportEnd = ref(20)
let resize: ResizeObserver | undefined
function updateViewport() {
  const v = viewport.value
  if (v) {
    viewportStart.value = Math.max(0, beatAt(v.scrollLeft) - 8)
    viewportEnd.value = beatAt(v.scrollLeft + v.clientWidth) + 8
  }
}
onMounted(() => {
  resize = new ResizeObserver(updateViewport)
  if (viewport.value) resize.observe(viewport.value)
  updateViewport()
})
onBeforeUnmount(() => resize?.disconnect())
watch(scale, () => nextTick(updateViewport))
const origin = 210,
  history = ref<Project[]>([]),
  future = ref<Project[]>([])
const part = computed(() =>
  props.project.parts.find((p) => p.id === focused.value),
)
const shown = computed(() =>
  props.performance
    ? performanceParts(props.project.parts, props.userId, showAll.value)
    : props.project.parts.filter((p) => visible.value.has(p.id)),
)
const length = computed(
  () =>
    props.project.score?.length ??
    Math.max(
      4,
      ...props.project.parts.map((p) => p.loop_beats),
      ...props.project.parts.flatMap((p) =>
        p.notes.map((n) => n.beat + n.duration),
      ),
    ),
)
const barLength = computed(
  () => (props.project.beats_per_bar * 4) / (props.project.beat_unit || 4),
)
const anchors = computed(() =>
  scoreAnchors(
    props.project.parts,
    length.value,
    scale.value,
    origin,
    scoreMeasures(
      length.value,
      props.project.beats_per_bar,
      props.project.beat_unit || 4,
      props.project.score?.meters,
    ),
    [
      ...(props.project.score?.meters || []).map((m) => m.beat),
      ...(props.project.score?.keys || []).map((k) => k.beat),
      ...props.project.parts.flatMap((p) =>
        staves(p).flatMap((s) => (s.clef_changes || []).map((c) => c.beat)),
      ),
    ],
  ),
)
const xAt = (beat: number) => scoreX(beat, anchors.value, scale.value, origin)
const beatAt = (x: number) => scoreBeat(x, anchors.value, scale.value, origin)
const width = computed(() => xAt(length.value) + 100)
const unsupported = computed(
  () =>
    shown.value
      .flatMap((p) => p.notes)
      .filter(
        (n) =>
          !durationGlyphs(n.duration) &&
          (!n.notation ||
            n.notation.tuplet_actual === n.notation.tuplet_normal),
      ).length,
)
const outsideLoop = computed(
  () =>
    part.value?.notes.filter(
      (n) => n.beat + n.duration > part.value!.loop_beats,
    ).length || 0,
)
const canEdit = computed(
  () =>
    props.editable &&
    !props.performance &&
    !props.saving &&
    !pending.value &&
    !conflict.value,
)
const picked = computed(() =>
  props.project.parts.flatMap((p) =>
    p.notes
      .filter((n) => selected.value.has(noteKey(p.id, n.id)))
      .map((n) => ({ p, n })),
  ),
)
const activeBase = computed(() =>
  picked.value.length
    ? picked.value.every(
        ({ p, n }) =>
          metadata(n, p).base ===
          metadata(picked.value[0]!.n, picked.value[0]!.p).base,
      )
      ? metadata(picked.value[0]!.n, picked.value[0]!.p).base
      : null
    : base.value,
)
watch(
  () =>
    JSON.stringify([
      props.project.parts,
      props.project.score,
      props.project.beats_per_bar,
      props.project.beat_unit,
    ]),
  () => {
    if (!pending.value) {
      history.value = []
      future.value = []
    }
  },
)
function pitches(p: Part) {
  const high = Math.min(127, Math.max(96, ...p.notes.map((n) => n.pitch + 2))),
    low = Math.max(0, Math.min(48, ...p.notes.map((n) => n.pitch - 2)))
  return Array.from({ length: high - low + 1 }, (_, i) => high - i)
}
const clone = <T,>(x: T): T => JSON.parse(JSON.stringify(x))
function focus(id: string) {
  focused.value = id
  emit('focus', id)
}
function toggleVisible(id: string) {
  const set = new Set(visible.value)
  set.has(id) ? set.delete(id) : set.add(id)
  visible.value = set
}
async function commit(next: Project, record = true) {
  if (!canEdit.value || !props.save) return false
  const previous = clone(props.project)
  pending.value = true
  error.value = ''
  try {
    await props.save(next)
    if (record) {
      history.value = [...history.value.slice(-49), previous]
      future.value = []
    }
    return true
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    commandQueue.length = 0
    if (!(e instanceof ApiError && e.status === 400)) conflict.value = next
  } finally {
    pending.value = false
    await nextTick()
    if (!conflict.value && commandQueue.length) {
      commandQueue.shift()!()
    }
  }
}
watch(()=>props.saving,saving=>{if(!saving&&!pending.value&&!conflict.value&&commandQueue.length)commandQueue.shift()!()})
function discardDraft() { conflict.value=null; error.value='' }
function updatePart(p: Part) {
  const next = clone(props.project)
  next.parts = next.parts.map((x) => (x.id === p.id ? p : x))
  void commit(next)
}
function apply(command: NoteCommand, record = true) {
  if ((pending.value || props.saving) && props.editable && !conflict.value) {
    const targets = new Set(selected.value)
    commandQueue.push(() => {
      selected.value = targets
      apply(command, record)
    })
    return
  }
  if (!canEdit.value) return
  if (!picked.value.length) {
    if (command.kind === 'duration') base.value = command.value
    if (command.kind === 'dots') dots.value = (dots.value + 1) % 3
    if (command.kind === 'alter')
      alter.value = Math.max(
        -2,
        Math.min(2, (alter.value || 0) + command.value),
      )
    if (command.kind === 'natural') alter.value = 0
    if (command.kind === 'rest') rest.value = !rest.value
    if (command.kind === 'tuplet') {
      actual.value = command.actual
      normal.value = command.normal
    }
    return
  }
  try {
    const next = clone(props.project)
    for (const p of next.parts) {
      if (!p.notes.some((n) => selected.value.has(noteKey(p.id, n.id))))
        continue
      p.staves = staves(p)
      p.notes = p.notes.map((n) =>
        selected.value.has(noteKey(p.id, n.id))
          ? changeNote(
              n,
              {
                ...p,
                staves: staves(p).map((s) => ({
                  ...s,
                  key_signature:
                    s.key_signature ??
                    next.score?.keys.filter((k) => k.beat <= n.beat).at(-1)
                      ?.key ??
                    p.key_signature,
                })),
              },
              command,
            )
          : n,
      )
    }
    void commit(next, record)
  } catch (e) {
    error.value = String(e)
  }
}
function patchNotation(patch: Partial<NoteNotation>) {
  if (!canEdit.value || !picked.value.length) return
  try {
    const next = clone(props.project)
    for (const p of next.parts) {
      if (!p.notes.some((n) => selected.value.has(noteKey(p.id, n.id))))
        continue
      p.staves = staves(p)
      p.notes = p.notes.map((n) =>
        selected.value.has(noteKey(p.id, n.id))
          ? withNotation(n, { ...metadata(n, p), ...patch }, p)
          : n,
      )
    }
    void commit(next)
  } catch (e) {
    error.value = String(e)
  }
}
function relation(kind: 'tie_to' | 'slur_to' | 'grace_to') {
  if (!canEdit.value) return
  const items = picked.value.slice().sort((a, b) => a.n.beat - b.n.beat)
  if (items.length === 1) {
    patchNotation({ [kind]: null })
    return
  }
  if (items.length !== 2 || items[0]!.p.id !== items[1]!.p.id) {
    error.value =
      'Select two notes in the same part to connect them, or one note to remove its connection.'
    return
  }
  const [first, last] = items as [
    (typeof items)[number],
    (typeof items)[number],
  ]
  const p = clone(first.p)
  p.staves = staves(p)
  p.notes = p.notes.map((n) => {
    if (n.id !== first.n.id) return n
    const v = { ...metadata(n, p), [kind]: last.n.id }
    if (kind === 'grace_to') {
      v.base = 0.125
      v.dots = 0
      v.tuplet_actual = 1
      v.tuplet_normal = 1
      n = { ...n, beat: last.n.beat }
    }
    return withNotation(n, v, p)
  })
  updatePart(p)
}
function noteValue(field: 'beat' | 'velocity', value: number) {
  if (!canEdit.value) return
  const next = clone(props.project)
  for (const p of next.parts)
    p.notes = p.notes.map((n) =>
      selected.value.has(noteKey(p.id, n.id))
        ? field === 'beat'
          ? atBeat(n, value)
          : { ...n, [field]: value }
        : n,
    )
  void commit(next)
}
function remove() {
  if ((pending.value || props.saving) && props.editable && !conflict.value) {
    const targets = new Set(selected.value)
    commandQueue.push(() => {
      selected.value = targets
      remove()
    })
    return
  }
  if (!picked.value.length || !canEdit.value) return
  const next = clone(props.project)
  for (const p of next.parts) {
    p.notes = p.notes.filter((n) => !selected.value.has(noteKey(p.id, n.id)))
    for (const n of p.notes)
      if (n.notation)
        for (const kind of ['tie_to', 'slur_to', 'grace_to'] as const)
          if (
            n.notation[kind] &&
            !p.notes.some((other) => other.id === n.notation![kind])
          )
            n.notation[kind] = null
  }
  void commit(next).then((ok) => {
    if (ok) selected.value = new Set()
  })
}
async function undo(redo = false) {
  if ((pending.value || props.saving) && props.editable && !conflict.value) {
    commandQueue.push(() => void undo(redo))
    return
  }
  const stack = redo ? future : history,
    other = redo ? history : future,
    previous = stack.value.at(-1)
  if (!previous || !canEdit.value) return
  const current = clone(props.project),
    next = {
      ...clone(props.project),
      parts: clone(previous.parts),
      score: clone(previous.score ?? null),
      beats_per_bar: previous.beats_per_bar,
      beat_unit: previous.beat_unit,
    }
  const ok=await commit(next, false)
  if (ok) {
    stack.value = stack.value.slice(0, -1)
    other.value.push(current)
    selected.value = new Set()
  }
}
function reorder(id: string, delta: number) {
  const next = clone(props.project),
    index = next.parts.findIndex((p) => p.id === id),
    target = index + delta
  if (target < 0 || target >= next.parts.length) return
  const [p] = next.parts.splice(index, 1)
  next.parts.splice(target, 0, p!)
  void commit(next)
}
function addPart() {
  if (props.project.parts.length >= 32) return
  const next = clone(props.project),
    id = newId()
  next.parts.push({
    id,
    name: `Part ${next.parts.length + 1}`,
    performer: null,
    view: 'notation',
    clef: 'treble',
    notes: [],
    loop_beats: length.value,
    instrument_node: null,
    midi_channel: 1,
    osc_address: '/pr0former/note',
  })
  visible.value = new Set([...visible.value, id])
  focus(id)
  void commit(next)
}
function addStaff() {
  if (!part.value) return
  const p = clone(part.value)
  p.staves = staves(p)
  if (p.staves.length >= 8) return
  p.staves.push({
    id: newId(),
    name: `Staff ${p.staves.length + 1}`,
    clef: 'bass',
    transpose: 0,
  })
  updatePart(p)
}
function editStaff(staff: Staff, patch: Partial<Staff>) {
  if (!part.value) return
  try {
    const p = clone(part.value)
    p.staves = staves(p)
    p.notes = p.notes.map((n) => ({ ...n, notation: metadata(n, p) }))
    p.staves = p.staves.map((s) => (s.id === staff.id ? { ...s, ...patch } : s))
    if (p.staves[0]?.id === staff.id) {
      if (patch.clef) p.clef = patch.clef
      if ('key_signature' in patch) p.key_signature = patch.key_signature
    }
    p.notes = p.notes.map((n) => withNotation(n, n.notation!, p))
    updatePart(p)
  } catch (e) {
    error.value = String(e)
  }
}
function meter(key: 'beats_per_bar' | 'beat_unit', value: number) {
  const next = clone(props.project)
  next[key] = value
  void commit(next)
}
function enterNote(
  partId: string,
  staffId: string,
  step: number,
  beat: number,
  exactAlter?: number,
) {
  if (!canEdit.value) return
  const p = clone(props.project.parts.find((p) => p.id === partId)!)
  p.staves = staves(p)
  const staff = p.staves.find((s) => s.id === staffId)!,
    acc =
      exactAlter ??
      alter.value ??
      keyAlter(
        step,
        staff.key_signature ??
          props.project.score?.keys.filter((k) => k.beat <= beat).at(-1)?.key ??
          p.key_signature,
      )
  const v = {
    staff: staffId,
    step,
    alter: acc,
    voice: voice.value,
    base: base.value,
    dots: dots.value,
    tuplet_actual: actual.value,
    tuplet_normal: normal.value,
  }
  const pitch = pitchAt(step, acc, staff.transpose)
  if (pitch < 0 || pitch > 127) return
  const n: Note = withNotation(
    {
      id: newId(),
      pitch,
      beat,
      duration: base.value,
      velocity: 90,
      rest: rest.value,
      tied: false,
    },
    v,
    p,
  )
  p.notes.push(n)
  p.notes.sort((a, b) => a.beat - b.beat)
  selected.value = new Set()
  updatePart(p)
}
const gesture = ref<{
  x: number
  y: number
  cx: number
  cy: number
  add: boolean
  part: string
  staff: string
  step: number
  beat: number
  pointer: number
  move?: boolean
} | null>(null)
const marquee = ref<{
  left: number
  top: number
  width: number
  height: number
} | null>(null)
const ghost = ref<{ left: number; top: number } | null>(null)
function point(event: PointerEvent) {
  const v = viewport.value!,
    rect = v.getBoundingClientRect()
  return {
    x: event.clientX - rect.left + v.scrollLeft,
    y: event.clientY - rect.top + v.scrollTop,
  }
}
function pointerDown(event: PointerEvent) {
  if (event.button !== 0 || !(event.target instanceof Element)) return
  const row = event.target.closest<HTMLElement>('[data-staff-id]'),
    node = event.target.closest<HTMLElement>('[data-note-id]')
  if (!row) return
  root.value?.focus({ preventScroll: true })
  focus(row.dataset.partId!)
  if (node) {
    const key = noteKey(row.dataset.partId!, node.dataset.noteId!),
      multi = event.ctrlKey || event.metaKey,
      set = multi
        ? new Set(selected.value)
        : selected.value.has(key)
          ? new Set(selected.value)
          : new Set<string>()
    if (multi && set.has(key)) set.delete(key)
    else set.add(key)
    selected.value = set
    if (canEdit.value && !multi) {
      const pos = point(event)
      gesture.value = {
        ...pos,
        cx: event.clientX,
        cy: event.clientY,
        add: false,
        part: row.dataset.partId!,
        staff: row.dataset.staffId!,
        step: 0,
        beat: 0,
        pointer: event.pointerId,
        move: true,
      }
      viewport.value?.setPointerCapture(event.pointerId)
    }
    event.preventDefault()
    return
  }
  if (props.performance) return
  const pos = point(event),
    rect = row.getBoundingClientRect()
  const snap = (base.value * normal.value) / actual.value
  const at = Math.max(0, Math.round(beatAt(pos.x) / snap) * snap),
    staff = staves(
      props.project.parts.find((p) => p.id === row.dataset.partId)!,
    ).find((s) => s.id === row.dataset.staffId)!,
    clef =
      staff.clef_changes?.filter((c) => c.beat <= at).at(-1)?.clef || staff.clef
  gesture.value = {
    ...pos,
    cx: event.clientX,
    cy: event.clientY,
    add: event.ctrlKey || event.metaKey,
    part: row.dataset.partId!,
    staff: row.dataset.staffId!,
    step: bottomStep(clef) + Math.round((118 - (event.clientY - rect.top)) / 5),
    beat: Math.max(0, Math.round(beatAt(pos.x) / snap) * snap),
    pointer: event.pointerId,
  }
  viewport.value?.setPointerCapture(event.pointerId)
  event.preventDefault()
}
function pointerMove(event: PointerEvent) {
  const pos = point(event)
  if (!gesture.value) {
    const row =
      event.target instanceof Element
        ? event.target.closest<HTMLElement>('[data-staff-id]')
        : null
    ghost.value =
      tool.value === 'write' && row && canEdit.value
        ? {
            left: xAt(
              Math.max(
                0,
                Math.round(
                  beatAt(pos.x) / ((base.value * normal.value) / actual.value),
                ),
              ) *
                ((base.value * normal.value) / actual.value),
            ),
            top: Math.round(pos.y / 5) * 5,
          }
        : null
    return
  }
  const g = gesture.value
  if (Math.hypot(event.clientX - g.cx, event.clientY - g.cy) < 5) return
  marquee.value = {
    left: Math.min(g.x, pos.x),
    top: Math.min(g.y, pos.y),
    width: Math.abs(g.x - pos.x),
    height: Math.abs(g.y - pos.y),
  }
}
function pointerUp(event: PointerEvent) {
  const g = gesture.value
  if (!g) return
  if (g.move && marquee.value && canEdit.value) {
    const delta =
        Math.round((beatAt(g.x + event.clientX - g.cx) - beatAt(g.x)) * 4) / 4,
      steps = -Math.round((event.clientY - g.cy) / 5)
    try {
      const next = clone(props.project)
      for (const p of next.parts) {
        p.staves = staves(p)
        p.notes = p.notes.map((n) => {
          if (!selected.value.has(noteKey(p.id, n.id))) return n
          let moved = n
          for (let i = 0; i < Math.abs(steps); i++)
            moved = changeNote(moved, p, {
              kind: 'pitch',
              value: Math.sign(steps),
            })
          return atBeat(moved, Math.max(0, n.beat + delta))
        })
      }
      void commit(next)
    } catch (e) {
      error.value = String(e)
    }
  } else if (marquee.value) {
    const rect = viewport.value!.getBoundingClientRect(),
      m = marquee.value,
      set = g.add ? new Set(selected.value) : new Set<string>()
    viewport
      .value!.querySelectorAll<HTMLElement>('[data-note-id]')
      .forEach((el) => {
        const box = el.getBoundingClientRect(),
          x = box.left - rect.left + viewport.value!.scrollLeft,
          y = box.top - rect.top + viewport.value!.scrollTop
        if (
          x + box.width >= m.left &&
          x <= m.left + m.width &&
          y + box.height >= m.top &&
          y <= m.top + m.height
        )
          set.add(noteKey(el.dataset.partId!, el.dataset.noteId!))
      })
    selected.value = set
  } else if (g.move) {
  } else if (tool.value === 'write') enterNote(g.part, g.staff, g.step, g.beat)
  else if (!g.add) selected.value = new Set()
  gesture.value = null
  marquee.value = null
  if (viewport.value?.hasPointerCapture(event.pointerId))
    viewport.value.releasePointerCapture(event.pointerId)
}
const clipboard = ref<{ part: string; note: Note }[]>([])
function keydown(event: KeyboardEvent) {
  if (
    event.isComposing ||
    event.defaultPrevented ||
    (event.target instanceof Element &&
      event.target.closest(
        'input,textarea,select,[contenteditable="true"],dialog',
      ))
  )
    return
  if (props.performance) return
  const mod = event.ctrlKey || event.metaKey,
    key = event.key
  if (mod && key.toLowerCase() === 'z') {
    event.preventDefault()
    void undo(event.shiftKey)
    return
  }
  if (mod && key.toLowerCase() === 'c') {
    if (picked.value.length) {
      clipboard.value = clone(
        picked.value.map(({ p, n }) => ({ part: p.id, note: n })),
      )
      event.preventDefault()
    }
    return
  }
  if (
    mod &&
    key.toLowerCase() === 'v' &&
    clipboard.value.length &&
    canEdit.value
  ) {
    event.preventDefault()
    const next = clone(props.project),
      offset =
        Math.max(...clipboard.value.map((c) => c.note.beat + c.note.duration)) -
        Math.min(...clipboard.value.map((c) => c.note.beat))
    const set = new Set<string>(),
      ids = new Map(
        clipboard.value.map((c) => [noteKey(c.part, c.note.id), newId()]),
      )
    for (const c of clipboard.value) {
      const p = next.parts.find((p) => p.id === c.part)
      if (!p) continue
      const n = atBeat(
        { ...clone(c.note), id: ids.get(noteKey(c.part, c.note.id))! },
        c.note.beat + offset,
      )
      if (n.notation)
        for (const kind of ['tie_to', 'slur_to', 'grace_to'] as const)
          n.notation[kind] = n.notation[kind]
            ? ids.get(noteKey(c.part, n.notation[kind]!)) || null
            : null
      p.notes.push(n)
      set.add(noteKey(p.id, n.id))
    }
    selected.value = set
    void commit(next)
    return
  }
  if (mod || event.altKey) return
  if (key === 'Escape') {
    gesture.value = null
    marquee.value = null
    selected.value = new Set()
    return
  }
  let command: NoteCommand | undefined
  if (/^[1-8]$/.test(key))
    command = { kind: 'duration', value: durationKeys[Number(key) - 1]! }
  if (key === '.' || event.code === 'NumpadDecimal') command = { kind: 'dots' }
  if (key === '+' || event.code === 'NumpadAdd')
    command = { kind: 'alter', value: 1 }
  if (key === '-' || event.code === 'NumpadSubtract')
    command = { kind: 'alter', value: -1 }
  if (key === 'ArrowUp' || key === 'ArrowDown')
    command = {
      kind: 'pitch',
      value: key === 'ArrowUp' ? 1 : -1,
      octave: event.shiftKey,
    }
  if (key.toLowerCase() === 'r') command = { kind: 'rest' }
  if (command) {
    event.preventDefault()
    const arrow = key === 'ArrowUp' || key === 'ArrowDown'
    if (arrow) {
      apply(command, !arrowHeld)
      arrowHeld = true
    } else if (!event.repeat) apply(command)
    return
  }
  if (key === 'Delete' || key === 'Backspace') {
    event.preventDefault()
    remove()
  }
  if (key === 'ArrowLeft' || key === 'ArrowRight') {
    const notes =
        part.value?.notes.slice().sort((a, b) => a.beat - b.beat) || [],
      index = notes.findIndex((n) =>
        selected.value.has(noteKey(part.value!.id, n.id)),
      ),
      n =
        notes[
          Math.max(
            0,
            Math.min(notes.length - 1, index + (key === 'ArrowLeft' ? -1 : 1)),
          )
        ]
    if (n) {
      event.preventDefault()
      selected.value = new Set([noteKey(part.value!.id, n.id)])
    }
  }
}
watch(
  () => props.project.parts.map((p) => p.id),
  (ids, old) => {
    visible.value = new Set(
      [...visible.value, ...ids.filter((id) => !old?.includes(id))].filter(
        (id) => ids.includes(id),
      ),
    )
    if (!ids.includes(focused.value)) focus(shown.value[0]?.id || '')
  },
)
watch(
  [() => focused.value, () => part.value?.view],
  () => {
    if (!props.performance) view.value = part.value?.view || 'notation'
  },
  { immediate: true },
)
watch(
  () => shown.value.map((p) => p.id).join(','),
  () => {
    if (props.performance) {
      focus(shown.value[0]?.id || '')
      nextTick(() => {
        if (viewport.value) viewport.value.scrollTop = 0
      })
    }
  },
  { immediate: true },
)
watch(
  () => props.beats[focused.value],
  (beat) => {
    if (!follow.value || !viewport.value || beat == null) return
    const x = xAt(beat),
      v = viewport.value
    if (x < v.scrollLeft || x > v.scrollLeft + v.clientWidth * 0.8)
      v.scrollLeft = Math.max(0, x - v.clientWidth * 0.25)
  },
)
</script>
<template>
  <section
    ref="root"
    class="score-workspace"
    tabindex="0"
    aria-label="Score workspace"
    @keydown="keydown"
    @keyup="
      (event) => {
        if (event.key === 'ArrowUp' || event.key === 'ArrowDown')
          arrowHeld = false
      }
    "
    @focusout="arrowHeld = false"
    @contextmenu="($event.ctrlKey || $event.metaKey) && $event.preventDefault()"
  >
    <header v-if="performance" class="performance-score-options">
      <label><input v-model="showAll" type="checkbox" /> Show all parts</label
      ><span
        >{{ shown.filter((p) => p.performer === userId).length }} assigned
        part(s)</span
      >
    </header>
    <header
      v-else
      class="notation-toolbar"
      role="toolbar"
      aria-label="Notation tools"
    >
      <button :aria-pressed="tool === 'select'" @click="tool = 'select'">
        Select</button
      ><button :aria-pressed="tool === 'write'" @click="tool = 'write'">
        Write
      </button>
      <button
        v-for="(value, i) in durationKeys"
        :key="value"
        :aria-label="`${durationLabels[i]} note (${i + 1})`"
        :title="`${durationLabels[i]} · ${i + 1}`"
        :aria-pressed="activeBase === value"
        :disabled="!canEdit"
        @click="apply({ kind: 'duration', value })"
      >
        <svg
          class="note-symbol"
          viewBox="0 0 30 32"
          width="26"
          height="28"
          aria-hidden="true"
        >
          <ellipse
            cx="11"
            cy="23"
            rx="6"
            ry="4"
            transform="rotate(-20 11 23)"
            :fill="value >= 2 ? 'none' : 'currentColor'"
            stroke="currentColor"
            stroke-width="2"
          />
          <path
            v-if="value < 4"
            d="M16 23V3"
            stroke="currentColor"
            stroke-width="2"
          />
          <path
            v-for="f in value < 1 ? Math.round(Math.log2(1 / value)) : 0"
            :key="f"
            :d="`M16 ${3 + (f - 1) * 5} Q29 ${8 + (f - 1) * 5} 22 ${15 + (f - 1) * 5}`"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          />
          <path
            v-if="value === 8"
            d="M3 17V29 M21 17V29"
            stroke="currentColor"
            stroke-width="2"
          /></svg
        ><kbd>{{ i + 1 }}</kbd>
      </button>
      <button
        :disabled="!canEdit"
        title="Rest (R)"
        :aria-pressed="rest"
        @click="apply({ kind: 'rest' })"
      >
        Rest
      </button>
      <button
        :disabled="!canEdit"
        title="Dots (.)"
        :aria-label="`Dots: ${dots}`"
        @click="apply({ kind: 'dots' })"
      >
        • <small>{{ picked.length ? '' : dots }}</small>
      </button>
      <button
        :disabled="!canEdit"
        aria-label="Add sharp (+)"
        @click="apply({ kind: 'alter', value: 1 })"
      >
        ♯</button
      ><button
        :disabled="!canEdit"
        aria-label="Add flat (-)"
        @click="apply({ kind: 'alter', value: -1 })"
      >
        ♭</button
      ><button
        :disabled="!canEdit"
        aria-label="Natural"
        @click="apply({ kind: 'natural' })"
      >
        ♮</button
      ><button :aria-pressed="alter === null" @click="alter = null">
        Auto ♮
      </button>
      <label
        >Voice<select v-model.number="voice" aria-label="Entry voice">
          <option v-for="v in 4" :key="v">{{ v }}</option>
        </select></label
      >
      <label
        >Tuplet<input
          v-model.number="actual"
          aria-label="Tuplet notes"
          type="number"
          min="1"
          max="32" />:<input
          v-model.number="normal"
          aria-label="Tuplet occupied"
          type="number"
          min="1"
          max="32" /></label
      ><button
        :disabled="!canEdit"
        @click="apply({ kind: 'tuplet', actual, normal })"
      >
        Apply ratio
      </button>
      <button
        :disabled="!canEdit || !picked.length"
        @click="relation('tie_to')"
      >
        Tie</button
      ><button
        :disabled="!canEdit || !picked.length"
        @click="relation('slur_to')"
      >
        Slur</button
      ><button
        :disabled="!canEdit || !picked.length"
        @click="relation('grace_to')"
      >
        Grace
      </button>
      <button :disabled="!history.length || !canEdit" @click="undo()">
        Undo</button
      ><button :disabled="!future.length || !canEdit" @click="undo(true)">
        Redo
      </button>
    </header>
    <div v-if="!performance" class="score-context">
      <span
        >{{
          picked.length
            ? `Editing ${picked.length} selected note(s)`
            : 'Entry settings'
        }}
        ·
        {{
          tool === 'write'
            ? 'Click the staff to write'
            : 'Click or drag to select'
        }}</span
      ><button :disabled="!canEdit || !picked.length" @click="remove">
        Delete selected</button
      ><button @click="view = view === 'notation' ? 'grid' : 'notation'">
        {{ view === 'notation' ? 'Piano roll' : 'Notation' }}
      </button>
    </div>
    <div v-if="picked.length && !performance" class="selection-inspector">
      <label v-if="picked.length === 1"
        >Onset<input
          type="number"
          min="0"
          step="0.25"
          :value="picked[0]!.n.beat"
          :disabled="!canEdit"
          @change="
            noteValue('beat', Number(($event.target as HTMLInputElement).value))
          " /></label
      ><label
        >Velocity<input
          type="number"
          min="0"
          max="127"
          :value="picked[0]!.n.velocity"
          :disabled="!canEdit"
          @change="
            noteValue(
              'velocity',
              Number(($event.target as HTMLInputElement).value),
            )
          " /></label
      ><label
        >Voice<select aria-label="Voice"
          :value="metadata(picked[0]!.n, picked[0]!.p).voice"
          :disabled="!canEdit"
          @change="
            patchNotation({
              voice: Number(($event.target as HTMLSelectElement).value),
            })
          "
        >
          <option v-for="v in 4" :key="v">{{ v }}</option>
        </select></label
      ><label
        >Articulation<select aria-label="Articulation"
          :value="picked[0]!.n.notation?.articulation || ''"
          :disabled="!canEdit"
          @change="
            patchNotation({
              articulation: ($event.target as HTMLSelectElement).value || null,
            })
          "
        >
          <option value="">None</option>
          <option
            v-for="a in ['staccato', 'tenuto', 'accent', 'marcato']"
            :key="a"
          >
            {{ a }}
          </option>
        </select></label
      ><label
        >Octave line<select aria-label="Octave line"
          :value="picked[0]!.n.notation?.octave || 0"
          :disabled="!canEdit"
          @change="
            patchNotation({
              octave: Number(($event.target as HTMLSelectElement).value),
            })
          "
        >
          <option :value="0">None</option>
          <option :value="1">8va</option>
          <option :value="-1">8vb</option>
          <option :value="2">15ma</option>
          <option :value="-2">15mb</option>
        </select></label
      >
    </div>
    <p v-if="unsupported" class="field-error" role="status">
      {{ unsupported }} note duration(s) need custom rhythm notation. Notes
      marked * show their exact duration in quarter beats.
    </p>
    <p
      v-if="outsideLoop && (!project.score || project.mode !== 'structured')"
      class="feature-note"
      role="status"
    >
      {{ outsideLoop }} note(s) extend beyond the loop. Stored notes are
      retained; playback clips to the loop.
    </p>
    <p v-if="error" role="alert" class="field-error">{{ error }}</p>
    <div v-if="conflict" class="score-conflict">
      <span
        >Your draft is retained. Review the current score before
        reapplying.</span
      ><button
        @click="discardDraft"
      >
        Discard draft</button
      ><button
        :disabled="saving || pending || !editable"
        @click="
          () => {
            const draft = conflict!
            conflict = null
            void commit({
              ...project,
              parts: draft.parts,
              score: draft.score,
              beats_per_bar: draft.beats_per_bar,
              beat_unit: draft.beat_unit,
            })
          }
        "
      >
        Reapply draft
      </button>
    </div>
    <ScoreTimelineEditor
      v-if="!performance"
      :project="project"
      :editable="canEdit"
      @update="(score) => commit({ ...clone(project), score })"
    />
    <div class="score-layout">
      <aside v-if="!performance" class="score-parts" :class="{ collapsed }">
        <button
          class="collapse-parts"
          :aria-expanded="!collapsed"
          @click="collapsed = !collapsed"
        >
          {{ collapsed ? 'Parts ›' : '‹ Parts' }}
        </button>
        <template v-if="!collapsed"
          ><div class="parts-actions">
            <button @click="visible = new Set(project.parts.map((p) => p.id))">
              Show all</button
            ><button @click="visible = new Set()">Hide all</button>
          </div>
          <div
            v-for="(p, index) in project.parts"
            :key="p.id"
            class="part-entry"
            :class="{ focused: focused === p.id }"
          >
            <input
              type="checkbox"
              :aria-label="`Show ${p.name}`"
              :checked="visible.has(p.id)"
              @change="toggleVisible(p.id)"
            /><button @click="focus(p.id)">
              <strong>{{ p.name }}</strong
              ><small
                >{{
                  members?.find((m) => m.id === p.performer)?.username ||
                  'Unassigned'
                }}
                · {{ staves(p).length }} staff/staves</small
              ><small
                >{{
                  project.graph.nodes.find((n) => n.id === p.instrument_node)
                    ?.label || 'External / acoustic'
                }}
                · MIDI {{ p.midi_channel || 1 }}</small
              ></button
            ><span class="part-order"
              ><button
                :disabled="!canEdit || index === 0"
                :aria-label="`Move ${p.name} up`"
                @click="reorder(p.id, -1)"
              >
                ↑</button
              ><button
                :disabled="!canEdit || index === project.parts.length - 1"
                :aria-label="`Move ${p.name} down`"
                @click="reorder(p.id, 1)"
              >
                ↓
              </button></span
            >
          </div>
          <button
            :disabled="!canEdit || project.parts.length >= 32"
            aria-label="Add part"
            @click="addPart"
          >
            ＋ Add part
          </button>
        </template>
      </aside>
      <div class="score-main">
        <div class="score-view-tools">
          <label
            ><input v-model="follow" type="checkbox" /> Follow playback</label
          ><label
            >Zoom<input
              v-model.number="scale"
              type="range"
              min="45"
              max="180"
              step="5"
          /></label>
        </div>
        <div
          ref="viewport"
          class="ensemble-scroll"
          @scroll.passive="updateViewport"
          @pointerdown="pointerDown"
          @pointermove="pointerMove"
          @pointerup="pointerUp"
          @pointercancel="gesture = null; marquee = null"
          @pointerleave="ghost = null"
        >
          <div class="ensemble-surface" :style="{ width: `${width}px` }">
            <p v-if="!shown.length" class="empty-score">
              {{
                performance
                  ? 'No parts assigned to you. Select “Show all parts” to view the score.'
                  : 'Select parts in the sidebar to view the score.'
              }}
            </p>
            <section
              v-for="p in shown"
              :key="p.id"
              class="ensemble-part"
              :data-score-part="p.id"
              :class="{ assigned: performance && p.performer === userId }"
            >
              <header class="ensemble-part-name">
                <button @click="focus(p.id)">{{ p.name }}</button
                ><small v-if="performance && p.performer === userId"
                  >YOUR PART</small
                >
              </header>
              <template
                v-if="performance ? p.view !== 'grid' : view === 'notation'"
                ><div v-for="s in staves(p)" :key="s.id" class="ensemble-staff">
                  <span class="staff-name">{{ s.name }}</span
                  ><ScoreStaff
                    :part="p"
                    :staff="s"
                    :length="length"
                    :bar-length="barLength"
                    :beats-per-bar="project.beats_per_bar"
                    :beat-unit="project.beat_unit || 4"
                    :scale="scale"
                    :origin="origin"
                    :anchors="anchors"
                    :timeline="project.score"
                    :view-start="viewportStart"
                    :view-end="viewportEnd"
                    :selected="selected"
                    :beat="beats[p.id] || 0"
                  /></div
              ></template>
              <div v-else class="ensemble-grid piano-roll">
                <div v-for="pitch in pitches(p)" :key="pitch" class="roll-row">
                  <span>{{ pitch }}</span>
                  <div
                    @click="
                      (event) => {
                        if (tool === 'write' && canEdit) {
                          const rect = (
                            event.currentTarget as HTMLElement
                          ).getBoundingClientRect()
                          const s = staves(p)[0]!
                          const temp = metadata(
                            {
                              id: '',
                              pitch,
                              beat: 0,
                              duration: base,
                              velocity: 90,
                              rest: false,
                              tied: false,
                            },
                            p,
                          )
                          enterNote(
                            p.id,
                            s.id,
                            temp.step,
                            Math.max(
                              0,
                              Math.round(
                                beatAt(event.clientX - rect.left + origin) /
                                  base,
                              ) * base,
                            ),
                            temp.alter,
                          )
                        }
                      }
                    "
                  >
                    <button
                      class="midi-note"
                      v-for="n in p.notes.filter((n) => n.pitch === pitch)"
                      :key="n.id"
                      :class="{ selected: selected.has(noteKey(p.id, n.id)) }"
                      :style="{
                        left: `${xAt(n.beat) - origin}px`,
                        width: `${xAt(n.beat + n.duration) - xAt(n.beat)}px`,
                      }"
                      :aria-label="`Note ${pitch} at beat ${n.beat + 1}`"
                      @click.stop="
                        selected = new Set([noteKey(p.id, n.id)]); focus(p.id)
                      "
                    ></button>
                  </div>
                </div>
              </div>
              <ScoreDynamics
                :part="p"
                :scale="scale"
                :anchors="anchors"
                :origin="origin"
                :width="width"
                :editable="canEdit"
                :performance="performance"
                @update="updatePart"
              />
              <MidiLanes
                :part="p"
                :scale="scale"
                :anchors="anchors"
                :origin="origin"
                :width="width"
                :editable="canEdit"
                :performance="performance"
                @update="updatePart"
              />
            </section>
            <div
              v-if="marquee"
              class="score-marquee"
              :style="{
                left: `${marquee.left}px`,
                top: `${marquee.top}px`,
                width: `${marquee.width}px`,
                height: `${marquee.height}px`,
              }"
            />
            <span
              v-if="ghost && !gesture"
              class="entry-ghost"
              :style="{ left: `${ghost.left}px`, top: `${ghost.top}px` }"
              >●</span
            >
          </div>
        </div>
      </div>
    </div>
    <details v-if="part && !performance" class="score-settings">
      <summary>Part settings · {{ part.name }}</summary>
      <div class="part-routing">
        <label
          >Part name<input
            :value="part.name"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                name: ($event.target as HTMLInputElement).value,
              })
            "
        /></label>
        <label
          >Performer<select aria-label="Performer"
            :value="part.performer || ''"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                performer: ($event.target as HTMLSelectElement).value || null,
              })
            "
          >
            <option value="">Unassigned</option>
            <option v-for="m in members" :key="m.id" :value="m.id">
              {{ m.username }}
            </option>
          </select></label
        >
        <label
          >Instrument / input<select aria-label="Instrument / input"
            :value="part.instrument_node || ''"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                instrument_node:
                  ($event.target as HTMLSelectElement).value || null,
              })
            "
          >
            <option value="">Acoustic / external only</option>
            <option
              v-for="n in project.graph.nodes.filter((n) =>
                ['synth', 'fm_synth', 'browser_input', 'input'].includes(
                  n.kind,
                ),
              )"
              :key="n.id"
              :value="n.id"
            >
              {{ n.label }}
            </option>
          </select></label
        >
        <label
          >Loop length (quarter beats)<input
            type="number"
            min="0.25"
            max="4096"
            :value="part.loop_beats"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                loop_beats: Number(($event.target as HTMLInputElement).value),
              })
            "
        /></label>
        <label
          >Default display<select aria-label="Default display"
            :value="part.view"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                view: ($event.target as HTMLSelectElement).value,
              })
            "
          >
            <option value="notation">Notation</option>
            <option value="grid">Piano roll</option>
          </select></label
        ><label
          >Time signature<select aria-label="Time signature"
            :value="part.show_time_signature === false ? 'hide' : 'show'"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                show_time_signature:
                  ($event.target as HTMLSelectElement).value === 'show',
              })
            "
          >
            <option value="show">Show</option>
            <option value="hide">Hidden</option>
          </select></label
        >
        <label
          >MIDI port<input
            :value="part.midi_port || ''"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                midi_port: ($event.target as HTMLInputElement).value || null,
              })
            " /></label
        ><label
          >OSC destination<input
            :value="part.osc_destination || ''"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                osc_destination:
                  ($event.target as HTMLInputElement).value || null,
              })
            " /></label
        ><label
          >OSC address<input
            :value="part.osc_address"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                osc_address: ($event.target as HTMLInputElement).value,
              })
            "
        /></label>
        <label
          >MIDI channel<input
            type="number"
            min="1"
            max="16"
            :value="part.midi_channel || 1"
            :disabled="!canEdit"
            @change="
              updatePart({
                ...part!,
                midi_channel: Number(($event.target as HTMLInputElement).value),
              })
            "
        /></label>
        <label
          >Beats per bar<input
            type="number"
            min="1"
            max="16"
            :value="project.beats_per_bar"
            :disabled="!canEdit"
            @change="
              meter(
                'beats_per_bar',
                Number(($event.target as HTMLInputElement).value),
              )
            "
        /></label>
        <label
          >Beat unit<select aria-label="Beat unit"
            :value="project.beat_unit || 4"
            :disabled="!canEdit"
            @change="
              meter(
                'beat_unit',
                Number(($event.target as HTMLSelectElement).value),
              )
            "
          >
            <option v-for="u in [1, 2, 4, 8, 16, 32]" :key="u" :value="u">
              {{ u }}
            </option>
          </select></label
        >
      </div>
      <div v-for="s in staves(part)" :key="s.id" class="part-routing">
        <label
          >Staff name<input
            :value="s.name"
            :disabled="!canEdit"
            @change="
              editStaff(s, { name: ($event.target as HTMLInputElement).value })
            " /></label
        ><label
          >Clef<select aria-label="Clef"
            :value="s.clef"
            :disabled="!canEdit"
            @change="
              editStaff(s, { clef: ($event.target as HTMLSelectElement).value })
            "
          >
            <option v-for="c in ['treble', 'bass', 'alto', 'tenor']" :key="c">
              {{ c }}
            </option>
          </select></label
        ><label
          >Key signature<select aria-label="Key signature"
            :value="s.key_signature || ''"
            :disabled="!canEdit"
            @change="
              editStaff(s, {
                key_signature:
                  ($event.target as HTMLSelectElement).value || null,
              })
            "
          >
            <option value="">Inherit score / part key</option>
            <option v-for="k in keyNames" :key="k" :value="k">
              {{ keyLabel(k, s.key_mode) }}
            </option>
          </select></label
        ><label
          >Key mode<select aria-label="Key mode"
            :value="s.key_mode || 'major'"
            :disabled="!canEdit"
            @change="
              editStaff(s, {
                key_mode: ($event.target as HTMLSelectElement).value as
                  | 'major'
                  | 'minor',
              })
            "
          >
            <option value="major">Major</option>
            <option value="minor">Minor</option>
          </select></label
        ><label
          >Sounding transposition<input
            type="number"
            min="-48"
            max="48"
            :value="s.transpose"
            :disabled="!canEdit"
            @change="
              editStaff(s, {
                transpose: Number(($event.target as HTMLInputElement).value),
              })
            " /></label
        ><label
          >Staff instrument<select aria-label="Staff instrument"
            :value="s.instrument_node || ''"
            :disabled="!canEdit"
            @change="
              editStaff(s, {
                instrument_node:
                  ($event.target as HTMLSelectElement).value || null,
              })
            "
          >
            <option value="">Inherit part</option>
            <option
              v-for="n in project.graph.nodes.filter((n) =>
                ['synth', 'fm_synth', 'input', 'browser_input'].includes(
                  n.kind,
                ),
              )"
              :key="n.id"
              :value="n.id"
            >
              {{ n.label }}
            </option>
          </select></label
        ><label>Staff MIDI port<input :value="s.midi_port || ''" :disabled="!canEdit" placeholder="Inherit part" @change="editStaff(s,{midi_port:($event.target as HTMLInputElement).value||null})" /></label
        ><label
          >Staff MIDI channel<input
            type="number"
            min="1"
            max="16"
            placeholder="Inherit"
            :value="s.midi_channel ?? ''"
            :disabled="!canEdit"
            @change="
              editStaff(s, {
                midi_channel: ($event.target as HTMLInputElement).value
                  ? Number(($event.target as HTMLInputElement).value)
                  : null,
              })
            " /></label
        ><label
          >Change at beat<input
            v-model.number="clefBeat"
            type="number"
            min="0"
            step="0.25" /></label
        ><select v-model="newClef" aria-label="New clef">
          <option v-for="c in ['treble', 'bass', 'alto', 'tenor']" :key="c">
            {{ c }}
          </option></select
        ><button
          :disabled="!canEdit"
          @click="
            editStaff(s, {
              clef_changes: [
                ...(s.clef_changes || []).filter((c) => c.beat !== clefBeat),
                { beat: clefBeat, clef: newClef },
              ].sort((a, b) => a.beat - b.beat),
            })
          "
        >
          Add clef change</button
        ><span v-for="c in s.clef_changes" :key="c.beat"
          >{{ c.beat }}: {{ c.clef }}
          <button
            :disabled="!canEdit"
            @click="
              editStaff(s, {
                clef_changes: s.clef_changes!.filter((x) => x.beat !== c.beat),
              })
            "
          >
            Remove
          </button></span
        >
      </div>
      <button
        :disabled="!canEdit || staves(part).length >= 8"
        @click="addStaff"
      >
        ＋ Add staff
      </button>
    </details>
  </section>
</template>
<style scoped>
.selection-inspector {
  display: flex;
  gap: 12px;
  padding: 8px;
  flex-wrap: wrap;
  flex-shrink: 0;
}
.selection-inspector label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
}
.selection-inspector input {
  width: 70px;
}
.selection-inspector select {
  width: 110px;
}
.score-workspace {
  display: flex;
  flex-direction: column;
  min-height: 400px;
  height: 100%;
  outline: none;
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 8px;
  overflow: hidden;
}
.notation-toolbar,
.score-context,
.score-view-tools,
.performance-score-options {
  display: flex;
  gap: 6px;
  align-items: center;
  flex-wrap: wrap;
  padding: 8px;
  border-bottom: 1px solid var(--line);
  flex-shrink: 0;
}
.notation-toolbar button,
.score-context button,
.score-parts button,
.score-conflict button {
  min-height: 40px;
  border: 1px solid var(--line);
  border-radius: 4px;
  padding: 4px 9px;
  background: var(--panel);
  color: var(--text);
  cursor: pointer;
}
.notation-toolbar button[aria-pressed='true'] {
  background: #194144;
  border-color: var(--cyan);
  color: var(--cyan);
}
.notation-toolbar button:disabled {
  opacity: 0.4;
}
.notation-toolbar kbd {
  font-size: 10px;
  display: block;
  color: var(--muted);
}
.note-symbol {
  font-family: serif;
  font-size: 24px;
  line-height: 26px;
}
.notation-toolbar label {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
}
.notation-toolbar input {
  width: 42px;
}
.notation-toolbar select {
  width: 58px;
  padding: 6px;
}
.score-context {
  font-size: 12px;
  justify-content: space-between;
  color: var(--muted);
}
.score-layout {
  display: flex;
  min-height: 200px;
  flex: 1;
  overflow: hidden;
}
.score-parts {
  width: 230px;
  flex-shrink: 0;
  border-right: 1px solid var(--line);
  padding: 8px;
  overflow: auto;
}
.score-parts.collapsed {
  width: 64px;
}
.parts-actions {
  display: flex;
  gap: 6px;
  margin: 8px 0;
}
.part-entry {
  display: flex;
  gap: 6px;
  align-items: center;
  margin-bottom: 6px;
}
.part-entry button {
  flex: 1;
  text-align: left;
  min-width: 0;
}
.part-entry small,
.part-entry strong {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.part-entry small {
  font-size: 10px;
  color: var(--muted);
  margin-top: 5px;
}
.part-order {
  display: flex;
  flex-direction: column;
}
.part-order button {
  min-height: 24px;
  padding: 2px 5px;
}
.part-entry.focused > button {
  border-color: var(--cyan);
}
.score-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.score-view-tools {
  font-size: 12px;
}
.score-view-tools label {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ensemble-scroll {
  flex: 1;
  overflow: auto;
  position: relative;
  touch-action: pan-x pan-y;
  min-height: 200px;
}
.ensemble-surface {
  position: relative;
  min-height: 100%;
  user-select: none;
}
.ensemble-part {
  border-bottom: 1px solid var(--line);
}
.ensemble-part.assigned {
  border-left: 3px solid var(--cyan);
}
.ensemble-part-name {
  position: sticky;
  left: 0;
  width: 260px;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 16px;
  color: var(--cyan);
  background: var(--panel);
  z-index: 2;
}
.ensemble-part-name button {
  background: none;
  border: 0;
  color: inherit;
  font-weight: 600;
  cursor: pointer;
}
.ensemble-part-name small {
  font-size: 9px;
  color: var(--amber);
}
.ensemble-staff {
  position: relative;
}
.staff-name {
  position: sticky;
  left: 8px;
  top: 0;
  font-size: 10px;
  color: var(--muted);
  z-index: 1;
  display: block;
  height: 0;
  transform: translateY(8px);
  width: 170px;
}
.score-marquee {
  position: absolute;
  pointer-events: none;
  border: 1px solid var(--cyan);
  background: #5de1df20;
  z-index: 5;
}
.entry-ghost {
  position: absolute;
  pointer-events: none;
  color: var(--cyan);
  opacity: 0.55;
  transform: translate(-3px, -8px);
}
.score-settings {
  max-height: 260px;
  overflow: auto;
  border-top: 1px solid var(--line);
  padding: 10px;
}
.score-settings summary {
  cursor: pointer;
  color: var(--cyan);
}
.score-settings .part-routing {
  margin: 8px 0;
  padding: 8px;
}
.score-conflict {
  padding: 12px;
  color: var(--amber);
}
.empty-score {
  position: sticky;
  left: 20px;
  width: 500px;
  padding: 24px;
  color: var(--muted);
}
.ensemble-grid {
  max-height: 360px;
  overflow-y: auto;
}
.roll-row {
  height: 18px;
  display: flex;
  border-bottom: 1px solid #ffffff10;
}
.roll-row > span {
  width: 210px;
  flex-shrink: 0;
  padding-left: 12px;
  font-size: 10px;
}
.roll-row > div {
  position: relative;
  flex: 1;
}
.roll-row button {
  position: absolute;
  height: 16px;
  background: var(--violet);
  border: 1px solid #ffffff40;
  min-width: 4px;
}
.roll-row button.selected {
  background: var(--cyan);
}
@media (max-width: 800px) {
  .score-parts {
    width: 175px;
  }
  .notation-toolbar {
    max-height: 150px;
    overflow: auto;
  }
  .score-workspace {
    min-height: 500px;
  }
  .score-settings {
    max-height: 180px;
  }
}
@media (pointer: coarse) {
  button,
  .notation-toolbar button,
  .score-parts button {
    min-height: 44px;
  }
  .notation-toolbar {
    max-height: 170px;
  }
}
</style>
