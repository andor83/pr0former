<script setup lang="ts">
import { computed, nextTick, ref, watch, onMounted, onBeforeUnmount } from 'vue'
import type {
  MarkKind,
  Note,
  NoteNotation,
  Part,
  Project,
  Staff,
} from '../types'
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
  keyNames,
  keyLabel,
  keyAlter,
  metadata,
  noteKey,
  performanceParts,
  pitchAt,
  spelling,
  staves,
  withNotation,
  caretStops,
  nextCaretStop,
  nearestLetterStep,
  writtenDuration,
  type NoteCommand,
} from '../score'
import ScoreStaff from './ScoreStaff.vue'
import ScoreToolMenu from './ScoreToolMenu.vue'
import ScoreRestIcon from './ScoreRestIcon.vue'
import {
  setMeter,
  setClef,
  measures,
  sharedTimeline,
  barsInRange,
  clearRange,
  setRepeat,
  tempoAt,
  setTempo,
  removeTempo,
  addMark,
  removeMark,
} from '../scoreBars'
import ScoreContextMenu, { type MenuItem } from './ScoreContextMenu.vue'
import {
  HeldNotes,
  midiEntryStorageKey,
  midiHead,
  parseMidiMessage,
  readMidiEntryMode,
  type MidiEntryMode,
} from '../midiEntry'
import ScoreMeasureDialog from './ScoreMeasureDialog.vue'
import {
  copyRegion,
  pasteRegion,
  transposeRegion,
  type RegionClipboard,
} from '../scoreRegion'
const placement = ref<{ kind: string; value?: string } | null>(null)
const entryArticulation = ref<string | null>(null)
const onsetSnap = ref(0.125)
function placeTool(kind: string, value?: string) {
  placement.value = { kind, value }
  tool.value = 'select'
}
import {
  elementKey,
  deleteElement,
  materializeRest,
  moveElement,
  hideRest,
  type ScoreElement,
} from '../scoreElements'
const selectedElement = ref<ScoreElement | null>(null)
const palette = ref<HTMLElement>(),
  paletteHeight = ref(40)
let lastClick = { key: '', time: 0 },
  inspectAfterPointer = false
function trackClick(key: string) {
  const now = performance.now()
  inspectAfterPointer = key === lastClick.key && now - lastClick.time < 450
  lastClick = { key, time: inspectAfterPointer ? 0 : now }
}
let elementDrag: {
  e: ScoreElement
  x: number
  y: number
  beat: number
} | null = null
import ScoreDialog from './ScoreDialog.vue'
import ScoreStructureEditor from './ScoreStructureEditor.vue'
import ScorePianoRoll from './ScorePianoRoll.vue'
import {
  MousePointer2,
  Pencil,
  Undo2,
  Redo2,
  Settings2,
  SlidersHorizontal,
  Music2,
  Piano,
  Trash2,
} from '@lucide/vue'
const dialog = ref<
  | 'part'
  | 'shared'
  | 'entry'
  | 'note'
  | 'structure'
  | 'measure'
  | 'tempo'
  | 'mark'
  | null
>(null)
/** Drafts for the tempo-mark and text-mark dialogs. */
const tempoDraft = ref<{ beat: number; bpm: number } | null>(null)
const markDraft = ref<{
  part: string
  staff: string
  id?: string
  beat: number
  kind: MarkKind
  text: string
} | null>(null)
const markKinds: { id: MarkKind; label: string }[] = [
  { id: 'text', label: 'Text' },
  { id: 'cue', label: 'Cue' },
  { id: 'rehearsal', label: 'Rehearsal letter' },
  { id: 'expression', label: 'Expression' },
  { id: 'tempo', label: 'Tempo text' },
  { id: 'lyric', label: 'Lyric' },
]
function openTempoDialog(beat: number) {
  tempoDraft.value = { beat, bpm: tempoAt(doc.value, beat) }
  menu.value = null
  dialog.value = 'tempo'
}
function openMarkDialog(
  partId: string,
  staffId: string,
  beat: number,
  kind: MarkKind = 'cue',
  existing?: { id: string; text: string; kind: MarkKind },
) {
  markDraft.value = {
    part: partId,
    staff: staffId,
    beat,
    kind: existing?.kind ?? kind,
    text: existing?.text ?? '',
    id: existing?.id,
  }
  menu.value = null
  dialog.value = 'mark'
}
function saveTempo() {
  const t = tempoDraft.value
  if (!t) return
  try {
    void commit(setTempo(doc.value, t.beat, t.bpm)).then((ok) => {
      if (ok) dialog.value = null
    })
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
function saveMark() {
  const m = markDraft.value
  if (!m) return
  try {
    void commit(
      addMark(doc.value, m.part, m.staff, {
        id: m.id,
        beat: m.beat,
        kind: m.kind,
        text: m.text,
      }),
    ).then((ok) => {
      if (ok) {
        dialog.value = null
        selectedElement.value = null
      }
    })
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
type MeasureMode =
  | 'meter'
  | 'key'
  | 'repeat'
  | 'navigation'
  | 'barline'
  | 'clef'
  | 'bars'
  | 'edit'
const regionClipboard = ref<RegionClipboard | null>(null)
const measureMode = ref<MeasureMode>('meter')
/** Finale-style bar selection: a beat range on one staff, bar-aligned when clicked. */
const region = ref<{
  part: string
  staff: string
  start: number
  end: number
} | null>(null)
const menu = ref<{ x: number; y: number; items: MenuItem[] } | null>(null)
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
const emit = defineEmits<{
  focus: [id: string]
  audition: [part: string, note: string]
}>()
/**
 * Local working copy. Edits apply here immediately and are saved on a short
 * debounce, so typing never waits on the network; `doc` is what the editor shows.
 */
const draft = ref<Project | null>(null)
const doc = computed<Project>(() => draft.value ?? props.project)
const flushing = ref(false)
let saveTimer: ReturnType<typeof setTimeout> | null = null
let savedRevision = props.project.revision
let pendingAuditions: { part: string; notes: string[] }[] = []
const SAVE_DELAY = 250
const selected = ref(new Set<string>()),
  visible = ref(new Set(doc.value.parts.map((p) => p.id))),
  focused = ref(
    doc.value.parts.find((p) => p.performer === props.userId)?.id ||
      doc.value.parts[0]?.id ||
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
/** Speedy-Entry style insertion point: digits/letters insert here and advance. */
const caret = ref<{
  part: string
  staff: string
  voice: number
  beat: number
  step: number
} | null>(null)
const tieNext = ref<string | null>(null)
let lastEntry: {
  part: string
  staff: string
  voice: number
  beat: number
  base: number
  dots: number
  actual: number
  normal: number
} | null = null
/** Run now, or after the in-flight save so rapid keyboard entry is never dropped. */
function enqueue(fn: () => void) {
  if (props.saving && props.editable && !conflict.value) commandQueue.push(fn)
  else fn()
}
const error = ref(''),
  conflict = ref<Project | null>(null),
  viewport = ref<HTMLDivElement>(),
  root = ref<HTMLElement>()
const viewportStart = ref(0),
  viewportEnd = ref(20)
let resize: ResizeObserver | undefined
function updateViewport() {
  paletteHeight.value = palette.value?.offsetHeight || 0
  const v = viewport.value
  if (v) {
    // Quantize the window so small scrolls inside the overscan do not re-engrave.
    const step = Math.max(1, barLength.value)
    viewportStart.value = Math.max(
      0,
      Math.floor((beatAt(v.scrollLeft) - 8) / step) * step,
    )
    viewportEnd.value =
      Math.ceil((beatAt(v.scrollLeft + v.clientWidth) + 8) / step) * step
  }
}
onMounted(() => {
  resize = new ResizeObserver(updateViewport)
  if (viewport.value) resize.observe(viewport.value)
  if (palette.value) resize.observe(palette.value)
  updateViewport()
})
onBeforeUnmount(() => {
  resize?.disconnect()
  detachMidi()
  void flushNow()
})
/** Web MIDI step entry: play to enter (chords while held) or hold pitches and press a number. */
const midiMode = ref<MidiEntryMode>(
  readMidiEntryMode(typeof localStorage === 'undefined' ? undefined : localStorage),
)
const midiInputs = ref<string[]>([]),
  midiError = ref('')
const midiHeld = new HeldNotes()
let midiAccess: MIDIAccess | null = null
function midiEntryHead(pitch: number) {
  const c = caret.value,
    p = doc.value.parts.find((p) => p.id === c?.part),
    staff = p ? staves(p).find((s) => s.id === c!.staff) : undefined
  if (!c || !p || !staff) return null
  return midiHead(
    pitch,
    staff,
    staff.key_signature ??
      doc.value.score?.keys.filter((k) => k.beat <= c.beat).at(-1)?.key ??
      p.key_signature,
  )
}
function onMidiMessage(event: MIDIMessageEvent) {
  const message = parseMidiMessage(event.data)
  if (!message) return
  if (message.kind === 'off') {
    midiHeld.release(message.pitch)
    return
  }
  const chord = midiHeld.press(message.pitch, event.timeStamp)
  if (
    midiMode.value !== 'play' ||
    props.performance ||
    tool.value !== 'write' ||
    view.value !== 'notation' ||
    !caret.value
  )
    return
  const head = midiEntryHead(message.pitch)
  if (!head) return
  if (chord) addToChord(head)
  else insertAtCaret([head])
}
function bindMidiInputs() {
  if (!midiAccess) return
  const names: string[] = []
  for (const input of midiAccess.inputs.values()) {
    input.onmidimessage = onMidiMessage
    names.push(input.name || input.id)
  }
  midiInputs.value = names
}
async function attachMidi() {
  midiError.value = ''
  if (!('requestMIDIAccess' in navigator)) {
    midiError.value = 'Web MIDI is not available in this browser.'
    midiMode.value = 'off'
    return
  }
  try {
    midiAccess ??= await navigator.requestMIDIAccess()
    midiAccess.onstatechange = bindMidiInputs
    bindMidiInputs()
  } catch (e) {
    midiError.value = `MIDI access failed: ${e instanceof Error ? e.message : e}`
    midiMode.value = 'off'
  }
}
function detachMidi() {
  if (midiAccess) {
    midiAccess.onstatechange = null
    for (const input of midiAccess.inputs.values()) input.onmidimessage = null
  }
  midiHeld.clear()
  midiInputs.value = []
}
watch(
  midiMode,
  (mode) => {
    try {
      localStorage.setItem(midiEntryStorageKey, mode)
    } catch {
      /* private mode */
    }
    if (mode === 'off') detachMidi()
    else void attachMidi()
  },
  { immediate: true },
)
watch(scale, () => nextTick(updateViewport))
const origin = 210,
  history = ref<Project[]>([]),
  future = ref<Project[]>([])
const part = computed(() =>
  doc.value.parts.find((p) => p.id === focused.value),
)
const shown = computed(() =>
  props.performance
    ? performanceParts(doc.value.parts, props.userId, showAll.value)
    : doc.value.parts.filter((p) => visible.value.has(p.id)),
)
const length = computed(
  () =>
    doc.value.score?.length ??
    Math.max(
      4,
      ...doc.value.parts.map((p) => p.loop_beats),
      ...doc.value.parts.flatMap((p) =>
        p.notes.map((n) => n.beat + n.duration),
      ),
    ),
)
const barLength = computed(
  () => (doc.value.beats_per_bar * 4) / (doc.value.beat_unit || 4),
)
const anchors = computed(() =>
  scoreAnchors(
    doc.value.parts,
    length.value,
    scale.value,
    origin,
    scoreMeasures(
      length.value,
      doc.value.beats_per_bar,
      doc.value.beat_unit || 4,
      doc.value.score?.meters,
    ),
    [
      ...(doc.value.score?.meters || []).map((m) => m.beat),
      ...(doc.value.score?.keys || []).map((k) => k.beat),
      ...doc.value.parts.flatMap((p) =>
        staves(p).flatMap((s) => (s.clef_changes || []).map((c) => c.beat)),
      ),
    ],
  ),
)
const xAt = (beat: number) =>
  scoreX(
    beat,
    (props.performance ? part.value?.view === 'grid' : view.value === 'grid')
      ? []
      : anchors.value,
    scale.value,
    (props.performance ? part.value?.view === 'grid' : view.value === 'grid')
      ? 64
      : origin,
  )
const beatAt = (x: number) =>
  scoreBeat(
    x,
    (props.performance ? part.value?.view === 'grid' : view.value === 'grid')
      ? []
      : anchors.value,
    scale.value,
    (props.performance ? part.value?.view === 'grid' : view.value === 'grid')
      ? 64
      : origin,
  )
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
  () => props.editable && !props.performance && !conflict.value,
)
/** Saving in progress (local draft not yet acknowledged by the server). */
const pending = computed(() => flushing.value || !!draft.value)
const picked = computed(() =>
  doc.value.parts.flatMap((p) =>
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
// A revision we did not produce means a collaborator changed the project.
watch(
  () => props.project.revision,
  (revision) => {
    if (revision !== savedRevision && !flushing.value) {
      history.value = []
      future.value = []
    }
    savedRevision = revision
  },
)
const clone = <T,>(x: T): T => JSON.parse(JSON.stringify(x))
function focus(id: string) {
  focused.value = id
  emit('focus', id)
}
function mixPart(id: string, solo: boolean) {
  const next = clone(doc.value),
    p = next.parts.find((p) => p.id === id)!
  if (solo) {
    const enabled = !p.solo
    next.parts.forEach((p) => (p.solo = false))
    p.solo = enabled
    if (enabled) p.muted = false
  } else {
    p.muted = !p.muted
    if (p.muted) p.solo = false
  }
  void commit(next)
}
function toggleVisible(id: string) {
  const set = new Set(visible.value)
  set.has(id) ? set.delete(id) : set.add(id)
  visible.value = set
}
function scheduleSave(delay = SAVE_DELAY) {
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    saveTimer = null
    void flushSave()
  }, delay)
}
async function flushSave() {
  if (flushing.value || !draft.value || !props.save || conflict.value) return
  if (props.saving) {
    // Another save (graph, settings) is in flight; retry shortly.
    scheduleSave(300)
    return
  }
  const toSave = draft.value
  flushing.value = true
  error.value = ''
  try {
    await props.save(toSave)
    savedRevision = props.project.revision
    if (draft.value === toSave) draft.value = null
    else {
      draft.value = { ...draft.value, revision: props.project.revision }
      scheduleSave(0)
    }
    const auditions = pendingAuditions
    pendingAuditions = []
    for (const a of auditions) for (const n of a.notes) emit('audition', a.part, n)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    pendingAuditions = []
    commandQueue.length = 0
    if (e instanceof ApiError && e.status === 400) {
      // Validation failure: revert the unsaved edits to the last accepted state.
      draft.value = null
    } else {
      conflict.value = toSave
      draft.value = null
    }
  } finally {
    flushing.value = false
    await nextTick()
    if (!conflict.value && commandQueue.length) commandQueue.shift()!()
  }
}
/** Apply an edit locally and schedule its save. Resolves true when applied. */
async function commit(next: Project, record = true) {
  if (!canEdit.value || !props.save) return false
  const previous = doc.value
  draft.value = { ...next, revision: doc.value.revision }
  if (record) {
    history.value = [...history.value.slice(-49), previous]
    future.value = []
  }
  scheduleSave()
  return true
}
/** Save now (used before actions that need the server to know the note). */
async function flushNow() {
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = null
  await flushSave()
  // A save that was already in flight may have left newer edits behind.
  if (draft.value && !flushing.value && !conflict.value) await flushSave()
}
defineExpose({ flush: flushNow })
watch(
  () => props.saving,
  (saving) => {
    if (!saving && !conflict.value && commandQueue.length)
      commandQueue.shift()!()
  },
)
function discardDraft() {
  conflict.value = null
  error.value = ''
}
function updatePart(p: Part) {
  const next = clone(doc.value)
  next.parts = next.parts.map((x) => (x.id === p.id ? p : x))
  const added = p.notes.filter(
    (n) =>
      !doc.value.parts
        .find((x) => x.id === p.id)
        ?.notes.some((old) => old.id === n.id) && !n.rest,
  )
  void commit(next).then((saved) => {
    if (saved && added.length)
      pendingAuditions.push({ part: p.id, notes: added.map((n) => n.id) })
  })
}
function editNote(n: Note, p: Part, command: NoteCommand) {
  if (view.value === 'grid' && command.kind === 'pitch') {
    const s = staves(p).find((s) => s.id === metadata(n, p).staff)!,
      pitch = Math.max(
        0,
        Math.min(127, n.pitch + command.value * (command.octave ? 12 : 1)),
      )
    return withNotation(
      n,
      { ...metadata(n, p), ...spelling(pitch, s), octave: 0 },
      p,
    )
  }
  return changeNote(n, p, command)
}
function apply(command: NoteCommand, record = true) {
  if (props.saving && props.editable && !conflict.value) {
    const targets = new Set(selected.value)
    commandQueue.push(() => {
      selected.value = targets
      apply(command, record)
    })
    return
  }
  if (!canEdit.value) return
  if (selectedElement.value?.rest) {
    try {
      const next = clone(doc.value),
        e = selectedElement.value,
        n = materializeRest(next, e),
        p = next.parts.find((p) => p.id === e.part)!
      p.notes = p.notes.map((x) =>
        x.id === n.id ? changeNote(x, p, command) : x,
      )
      void commit(next).then((ok) => {
        if (ok) {
          selectedElement.value = null
          selected.value = new Set([noteKey(p.id, n.id)])
        }
      })
    } catch (e) {
      error.value = String(e)
    }
    return
  }
  if (selectedElement.value && !picked.value.length) return
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
    const next = clone(doc.value)
    for (const p of next.parts) {
      if (!p.notes.some((n) => selected.value.has(noteKey(p.id, n.id))))
        continue
      p.staves = staves(p)
      p.notes = p.notes.map((n) =>
        selected.value.has(noteKey(p.id, n.id))
          ? editNote(
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
    const next = clone(doc.value)
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
function noteValue(field: 'beat' | 'velocity' | 'duration', value: number) {
  if (!canEdit.value) return
  const next = clone(doc.value)
  for (const p of next.parts) {
    if (field === 'duration') p.staves = staves(p)
    p.notes = p.notes.map((n) =>
      selected.value.has(noteKey(p.id, n.id))
        ? field === 'beat'
          ? atBeat(n, value)
          : field === 'duration'
            ? withNotation(
                n,
                {
                  ...metadata(n, p),
                  base: value,
                  dots: 0,
                  tuplet_actual: 1,
                  tuplet_normal: 1,
                },
                p,
              )
            : { ...n, [field]: value }
        : n,
    )
  }
  void commit(next)
}
function remove() {
  if (selectedElement.value && canEdit.value) {
    const next = clone(doc.value),
      message = deleteElement(next, selectedElement.value)
    if (message) {
      error.value = message
      return
    }
    void commit(next).then((ok) => {
      if (ok) selectedElement.value = null
    })
    return
  }
  if (props.saving && props.editable && !conflict.value) {
    const targets = new Set(selected.value)
    commandQueue.push(() => {
      selected.value = targets
      remove()
    })
    return
  }
  if (!picked.value.length || !canEdit.value) return
  const next = clone(doc.value)
  for (const p of next.parts) {
    for (const n of p.notes.filter(
      (n) => n.rest && selected.value.has(noteKey(p.id, n.id)),
    ))
      hideRest(next, {
        kind: 'rest',
        part: p.id,
        staff: metadata(n, p).staff,
        rest: n,
      })
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
function undo(redo = false) {
  if (props.saving && props.editable && !conflict.value) {
    commandQueue.push(() => undo(redo))
    return
  }
  const stack = redo ? future : history,
    other = redo ? history : future,
    previous = stack.value.at(-1)
  if (!previous || !canEdit.value) return
  const current = doc.value
  stack.value = stack.value.slice(0, -1)
  other.value = [...other.value, current]
  draft.value = {
    ...current,
    parts: previous.parts,
    score: previous.score ?? null,
    beats_per_bar: previous.beats_per_bar,
    beat_unit: previous.beat_unit,
  }
  selected.value = new Set()
  scheduleSave()
}
function reorder(id: string, delta: number) {
  const next = clone(doc.value),
    index = next.parts.findIndex((p) => p.id === id),
    target = index + delta
  if (target < 0 || target >= next.parts.length) return
  const [p] = next.parts.splice(index, 1)
  next.parts.splice(target, 0, p!)
  void commit(next)
}
function addPart() {
  if (doc.value.parts.length >= 32) return
  const next = clone(doc.value),
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
  const next = clone(doc.value)
  next[key] = value
  void commit(next)
}
function chooseDuration(value: number, isRest: boolean) {
  rest.value = isRest
  placement.value = null
  base.value = value
  if (picked.value.length) {
    const next = clone(doc.value)
    for (const p of next.parts)
      p.notes = p.notes.map((n) =>
        selected.value.has(noteKey(p.id, n.id))
          ? withNotation(
              { ...n, rest: isRest },
              { ...metadata(n, p), base: value },
              p,
            )
          : n,
      )
    void commit(next)
  }
  tool.value = 'write'
}
interface EntryHead {
  step: number
  alter?: number
}
interface EntryOptions {
  rest?: boolean
  base?: number
  dots?: number
  actual?: number
  normal?: number
  voice?: number
  tie?: string | null
}
function enterNotes(
  partId: string,
  staffId: string,
  beat: number,
  heads: EntryHead[],
  options: EntryOptions = {},
): Note[] {
  if (!canEdit.value || !heads.length) return []
  const source = doc.value.parts.find((p) => p.id === partId)
  if (!source) return []
  const p = clone(source)
  p.staves = staves(p)
  const staff = p.staves.find((s) => s.id === staffId)
  if (!staff) return []
  const keyAt = (step: number) =>
    keyAlter(
      step,
      staff.key_signature ??
        doc.value.score?.keys.filter((k) => k.beat <= beat).at(-1)?.key ??
        p.key_signature,
    )
  const asRest = options.rest ?? rest.value,
    baseValue = options.base ?? base.value,
    dotCount = options.dots ?? dots.value,
    tupletActual = options.actual ?? actual.value,
    tupletNormal = options.normal ?? normal.value
  const added: Note[] = []
  for (const head of asRest ? heads.slice(0, 1) : heads) {
    const acc = head.alter ?? alter.value ?? keyAt(head.step)
    const pitch = pitchAt(head.step, acc, staff.transpose)
    if (pitch < 0 || pitch > 127) continue
    if (
      !asRest &&
      p.notes.some(
        (n) =>
          !n.rest &&
          n.pitch === pitch &&
          Math.abs(n.beat - beat) < 1e-9 &&
          metadata(n, p).voice === (options.voice ?? voice.value),
      )
    )
      continue
    const v = {
      staff: staffId,
      step: head.step,
      alter: acc,
      voice: options.voice ?? voice.value,
      base: baseValue,
      dots: dotCount,
      tuplet_actual: tupletActual,
      tuplet_normal: tupletNormal,
      articulation: entryArticulation.value,
    }
    const n: Note = withNotation(
      {
        id: newId(),
        pitch,
        beat,
        duration: baseValue,
        velocity: 90,
        rest: asRest,
        tied: false,
      },
      v,
      p,
    )
    if (options.tie && !asRest) {
      const from = p.notes.find((x) => x.id === options.tie)
      if (from && from.pitch === n.pitch && !from.rest)
        p.notes = p.notes.map((x) =>
          x.id === from.id
            ? withNotation(x, { ...metadata(x, p), tie_to: n.id }, p)
            : x,
        )
    }
    p.notes.push(n)
    added.push(n)
  }
  if (!added.length) return []
  p.notes.sort((a, b) => a.beat - b.beat)
  selected.value = new Set()
  updatePart(p)
  return added
}
function enterNote(
  partId: string,
  staffId: string,
  step: number,
  beat: number,
  exactAlter?: number,
) {
  return enterNotes(partId, staffId, beat, [{ step, alter: exactAlter }])[0]
}
const canQueue = computed(
  () => props.editable && !props.performance && !conflict.value,
)
function placeCaret(
  part: string,
  staff: string,
  beat: number,
  step: number,
  v?: number,
) {
  caret.value = {
    part,
    staff,
    voice: v ?? voice.value,
    beat: Math.max(0, beat),
    step: Math.max(0, Math.min(70, step)),
  }
}
const caretDuration = () =>
  writtenDuration(base.value, dots.value, actual.value, normal.value)
function clefAt(s: Staff, beat: number) {
  return s.clef_changes?.filter((c) => c.beat <= beat).at(-1)?.clef || s.clef
}
function caretTop(s: Staff) {
  const c = caret.value!
  return 118 - (c.step - bottomStep(clefAt(s, c.beat))) * 5
}
const caretStatus = computed(() => {
  const c = caret.value
  if (!c || tool.value !== 'write') return ''
  const bar = measures(doc.value).find(
    (m) => m.start <= c.beat + 1e-9 && m.end > c.beat + 1e-9,
  )
  const position = bar
    ? `bar ${bar.number} · beat ${(
        Math.round((c.beat - bar.start) * 1000) / 1000 +
        1
      ).toString()}`
    : `beat ${Math.round(c.beat * 1000) / 1000 + 1}`
  const index = durationKeys.indexOf(base.value)
  return `Caret ${position} · voice ${c.voice} · ${
    rest.value ? 'rest' : 'note'
  } ${index >= 0 ? durationLabels[index] : base.value}${
    dots.value ? ' ' + '.'.repeat(dots.value) : ''
  }${tieNext.value ? ' · tie pending' : ''}`
})
function scrollToCaret() {
  const c = caret.value,
    v = viewport.value
  if (!c || !v) return
  const x = xAt(c.beat)
  if (x < v.scrollLeft + 40 || x > v.scrollLeft + v.clientWidth - 40)
    v.scrollLeft = Math.max(0, x - v.clientWidth * 0.3)
}
/** Insert at the caret and advance it by the written duration (Finale Speedy Entry). */
function insertAtCaret(heads: EntryHead[], options: { rest?: boolean } = {}) {
  const c = caret.value
  if (!c || !canQueue.value || !heads.length) return
  const at = { ...c },
    duration = caretDuration(),
    tie = tieNext.value,
    asRest = options.rest ?? rest.value,
    entry = {
      base: base.value,
      dots: dots.value,
      actual: actual.value,
      normal: normal.value,
    }
  tieNext.value = null
  lastEntry = { part: at.part, staff: at.staff, voice: at.voice, beat: at.beat, ...entry }
  caret.value = { ...c, beat: c.beat + duration, step: heads[0]!.step }
  selected.value = new Set()
  selectedElement.value = null
  enqueue(() =>
    enterNotes(at.part, at.staff, at.beat, heads, {
      ...entry,
      rest: asRest,
      voice: at.voice,
      tie,
    }),
  )
  nextTick(scrollToCaret)
}
/** Stack another pitch onto the most recent entry without advancing. */
function addToChord(head: EntryHead | number) {
  const at = lastEntry,
    c = caret.value,
    entry: EntryHead = typeof head === 'number' ? { step: head } : head
  if (!at || !c || !canQueue.value) return
  caret.value = { ...c, step: entry.step }
  enqueue(() =>
    enterNotes(at.part, at.staff, at.beat, [entry], {
      base: at.base,
      dots: at.dots,
      actual: at.actual,
      normal: at.normal,
      voice: at.voice,
      rest: false,
    }),
  )
}
function caretStep(delta: number) {
  const c = caret.value
  if (!c) return
  caret.value = { ...c, step: Math.max(0, Math.min(70, c.step + delta)) }
}
function moveCaret(direction: 1 | -1, byBar = false) {
  const c = caret.value,
    p = doc.value.parts.find((p) => p.id === c?.part)
  if (!c || !p) return
  const bars = measures(doc.value)
  const stops = byBar
    ? [0, ...bars.map((m) => m.start), length.value]
    : caretStops(p.notes, p, c.staff, c.voice, bars)
  caret.value = {
    ...c,
    beat: nextCaretStop(
      stops,
      c.beat,
      direction,
      byBar ? barLength.value : caretDuration(),
    ),
  }
  nextTick(scrollToCaret)
}
/** Backspace at the caret removes the entry that ends there, like deleting typed text. */
function deleteBeforeCaret() {
  const c = caret.value
  if (!c || !canQueue.value) return
  // Runs after any in-flight save so the entry just typed is visible here.
  enqueue(() => {
    const p = doc.value.parts.find((p) => p.id === c.part)
    if (!p || !canEdit.value) return
    const victims = p.notes.filter((n) => {
      const v = metadata(n, p)
      return (
        v.staff === c.staff &&
        v.voice === c.voice &&
        !v.grace_to &&
        Math.abs(n.beat + n.duration - c.beat) < 1e-6
      )
    })
    if (!victims.length) {
      moveCaret(-1)
      return
    }
    const ids = new Set(victims.map((n) => n.id)),
      next = clone(doc.value),
      target = next.parts.find((x) => x.id === p.id)!
    target.notes = target.notes.filter((n) => !ids.has(n.id))
    for (const n of target.notes)
      if (n.notation)
        for (const kind of ['tie_to', 'slur_to', 'grace_to'] as const)
          if (n.notation[kind] && ids.has(n.notation[kind]!))
            n.notation[kind] = null
    caret.value = { ...c, beat: Math.min(...victims.map((n) => n.beat)) }
    selected.value = new Set()
    void commit(next)
  })
}
function setVoice(v: number) {
  voice.value = v
  if (caret.value) caret.value = { ...caret.value, voice: v }
}
/** T: tie the selection or the last caret entry to the next same-pitch entry. */
function tieAtCaret() {
  if (!canQueue.value) return
  if (picked.value.length === 2) {
    relation('tie_to')
    return
  }
  if (picked.value.length === 1) {
    const { p, n } = picked.value[0]!,
      v = metadata(n, p),
      next = p.notes.find((x) => {
        const w = metadata(x, p)
        return (
          !x.rest &&
          x.pitch === n.pitch &&
          w.staff === v.staff &&
          w.voice === v.voice &&
          Math.abs(x.beat - (n.beat + n.duration)) < 1e-6
        )
      })
    if (!next) {
      error.value = 'No following note of the same pitch to tie to.'
      return
    }
    patchNotation({ tie_to: v.tie_to === next.id ? null : next.id })
    return
  }
  const c = caret.value
  if (!c) return
  enqueue(() => {
    const p = doc.value.parts.find((p) => p.id === c.part)
    if (!p) return
    const previous = p.notes.find((n) => {
      const v = metadata(n, p)
      return (
        !n.rest &&
        v.staff === c.staff &&
        v.voice === c.voice &&
        Math.abs(n.beat + n.duration - c.beat) < 1e-6
      )
    })
    if (!previous) {
      error.value =
        'Enter a note first, then press T to tie it into the next entry.'
      return
    }
    error.value = ''
    tieNext.value = tieNext.value === previous.id ? null : previous.id
  })
}
function barAt(beat: number) {
  return measures(doc.value).find(
    (m) => m.start <= beat + 1e-9 && m.end > beat + 1e-9,
  )
}
function selectBar(
  partId: string,
  staffId: string,
  beat: number,
  extend = false,
) {
  const m = barAt(beat)
  if (!m) return
  const r = region.value
  region.value =
    extend && r && r.part === partId
      ? {
          part: partId,
          staff: staffId,
          start: Math.min(r.start, m.start),
          end: Math.max(r.end, m.end),
        }
      : { part: partId, staff: staffId, start: m.start, end: m.end }
}
const regionLabel = computed(() => {
  const r = region.value
  if (!r) return ''
  const bars = barsInRange(doc.value, r.start, r.end)
  if (!bars.length) return `beats ${r.start + 1}–${r.end + 1}`
  return bars.length === 1
    ? `bar ${bars[0]!.number}`
    : `bars ${bars[0]!.number}–${bars.at(-1)!.number}`
})
function openMeasureDialog(mode: MeasureMode) {
  if (!region.value) {
    const p = part.value
    if (!p) return
    selectBar(p.id, staves(p)[0]!.id, caret.value?.beat ?? viewportStart.value)
    if (!region.value) return
  }
  measureMode.value = mode
  menu.value = null
  dialog.value = 'measure'
}
function selectRegionNotes() {
  const r = region.value,
    p = doc.value.parts.find((p) => p.id === r?.part)
  if (!r || !p) return
  selected.value = new Set(
    p.notes
      .filter(
        (n) =>
          n.beat >= r.start - 1e-9 &&
          n.beat < r.end - 1e-9 &&
          metadata(n, p).staff === r.staff,
      )
      .map((n) => noteKey(p.id, n.id)),
  )
  selectedElement.value = null
}
function clearRegion() {
  const r = region.value
  if (!r || !canEdit.value) return
  void commit(clearRange(doc.value, r.part, r.staff, r.start, r.end))
}
function pasteAt(partId: string, staffId: string, beat: number) {
  const clip = regionClipboard.value
  if (!clip || !canEdit.value) return
  try {
    const { project: next, ids } = pasteRegion(doc.value, clip, {
      part: partId,
      staff: staffId,
      beat,
    })
    void commit(next).then((ok) => {
      if (!ok) return
      selected.value = new Set(ids.map((id) => noteKey(partId, id)))
      selectedElement.value = null
      if (caret.value && tool.value === 'write')
        caret.value = { ...caret.value, beat: beat + clip.length }
    })
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
function contextMenu(event: MouseEvent) {
  if (event.ctrlKey || event.metaKey) {
    event.preventDefault()
    return
  }
  if (props.performance || !(event.target instanceof Element)) return
  const row = event.target.closest<HTMLElement>('[data-staff-id]')
  if (!row) return
  event.preventDefault()
  const partId = row.dataset.partId!,
    staffId = row.dataset.staffId!,
    beat = Math.max(0, beatAt(point(event).x)),
    node = event.target.closest<HTMLElement>('[data-note-id]')
  focus(partId)
  root.value?.focus({ preventScroll: true })
  if (node) {
    selected.value = new Set([noteKey(partId, node.dataset.noteId!)])
    selectedElement.value = null
  }
  const r = region.value
  if (!(r && r.part === partId && beat >= r.start && beat < r.end))
    selectBar(partId, staffId, beat)
  const current = region.value
  if (!current) return
  const open = (mode: MeasureMode) => () => openMeasureDialog(mode)
  const label = regionLabel.value
  menu.value = {
    x: event.clientX,
    y: event.clientY,
    items: [
      ...(node
        ? ([
            {
              label: 'Edit note…',
              kbd: 'Enter',
              action: () => {
                dialog.value = 'note'
              },
            },
            {
              label: 'Delete note',
              kbd: '⌫',
              danger: true,
              disabled: !canEdit.value,
              action: remove,
            },
            { label: '', separator: true },
          ] as MenuItem[])
        : []),
      { label: `Time signature… (${label})`, action: open('meter') },
      { label: 'Key signature…', action: open('key') },
      { label: 'Clef change…', action: open('clef') },
      { label: 'Tempo change…', action: () => openTempoDialog(current.start) },
      {
        label: 'Text, cue or rehearsal mark…',
        action: () => openMarkDialog(current.part, current.staff, current.start),
      },
      { label: '', separator: true },
      {
        label: `Repeat ${label} ×2`,
        disabled: !canEdit.value,
        action: () =>
          void commit(setRepeat(doc.value, current.start, current.end, 2)),
      },
      { label: 'Repeat passes & endings…', action: open('repeat') },
      { label: 'D.C. / D.S. / Coda…', action: open('navigation') },
      { label: 'Barline style…', action: open('barline') },
      { label: '', separator: true },
      { label: 'Insert, add or delete bars…', action: open('bars') },
      { label: '', separator: true },
      { label: 'Transpose, durations, voice & staff…', action: open('edit') },
      {
        label: `Copy ${label}`,
        kbd: '⌘C',
        action: () => {
          regionClipboard.value = copyRegion(doc.value, current)
        },
      },
      {
        label: `Cut ${label}`,
        kbd: '⌘X',
        disabled: !canEdit.value,
        action: () => {
          regionClipboard.value = copyRegion(doc.value, current)
          clearRegion()
        },
      },
      {
        label: 'Paste here',
        kbd: '⌘V',
        disabled: !canEdit.value || !regionClipboard.value,
        action: () => pasteAt(current.part, current.staff, current.start),
      },
      { label: 'Select notes in bars', action: selectRegionNotes },
      {
        label: `Clear contents of ${label}`,
        kbd: '⌫',
        danger: true,
        disabled: !canEdit.value,
        action: clearRegion,
      },
    ],
  }
}
const gesture = ref<{
  x: number
  y: number
  cx: number
  cy: number
  add: boolean
  extend?: boolean
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
function point(event: MouseEvent) {
  const v = viewport.value!,
    rect = v.querySelector('.ensemble-surface')!.getBoundingClientRect()
  return {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  }
}
async function inspectElement() {
  if (props.performance) return
  const e = selectedElement.value
  if (!e) {
    if (picked.value.length) dialog.value = 'note'
    else if (region.value && tool.value === 'select') openMeasureDialog('meter')
    return
  }
  if (['meter', 'clef', 'barline'].includes(e.kind)) {
    dialog.value = 'structure'
    return
  }
  if (e.kind === 'tempo' && e.beat !== undefined) {
    openTempoDialog(e.beat)
    return
  }
  if (e.kind === 'mark' && e.mark) {
    const p = doc.value.parts.find((p) => p.id === e.part),
      s = p ? staves(p).find((s) => s.id === e.staff) : undefined,
      m = s?.marks?.find((m) => m.id === e.mark)
    if (m) openMarkDialog(e.part, e.staff, m.beat, m.kind, m)
    return
  }
  if (e.kind === 'rest' && canEdit.value) {
    const next = clone(doc.value),
      n = materializeRest(next, e)
    if (await commit(next)) {
      selectedElement.value = null
      selected.value = new Set([noteKey(e.part, n.id)])
      dialog.value = 'note'
    }
    return
  }
  if (e.note) {
    selected.value = new Set(
      (e.notes || [e.note]).map((n) => noteKey(e.part, n)),
    )
    const p = doc.value.parts.find((p) => p.id === e.part)!,
      n = p.notes.find((n) => n.id === e.note)
    if (n) {
      const v = metadata(n, p)
      actual.value = v.tuplet_actual
      normal.value = v.tuplet_normal
      voice.value = v.voice
    }
    dialog.value = e.kind === 'tuplet' ? 'entry' : 'note'
  } else
    dialog.value =
      ['repeat', 'navigation', 'barline'].includes(e.kind) ||
      (['key', 'meter'].includes(e.kind) && e.beat !== undefined)
        ? 'shared'
        : 'part'
}
function pointerDown(event: PointerEvent) {
  if (event.button !== 0 || !(event.target instanceof Element)) return
  if (placement.value && canEdit.value) {
    const row = event.target.closest<HTMLElement>('[data-staff-id]')
    if (!row) return
    const beat = Math.max(
      0,
      Math.round(beatAt(point(event).x) / onsetSnap.value) * onsetSnap.value,
    )
    const partId = row.dataset.partId!,
      staffId = row.dataset.staffId!
    const chosen = placement.value
    try {
      if (chosen.kind === 'clef')
        void commit(
          setClef(doc.value, partId, staffId, beat, chosen.value!),
        )
      else if (chosen.kind === 'meter') {
        const [n, d] = chosen.value!.split('/').map(Number)
        void commit(setMeter(doc.value, beat, n!, d!))
      } else if (chosen.kind === 'accent') {
        const node = event.target.closest<HTMLElement>('[data-note-id]')
        if (!node) return
        selected.value = new Set([noteKey(partId, node.dataset.noteId!)])
        patchNotation({ articulation: chosen.value })
      } else if (chosen.kind === 'tempo') {
        openTempoDialog(beat)
      } else if (chosen.kind === 'mark') {
        openMarkDialog(partId, staffId, beat, chosen.value as MarkKind)
      } else {
        const bar = measures(doc.value).find(
          (m) => m.start <= beat && m.end > beat,
        )
        if (!bar) return
        selectedElement.value = {
          kind: 'barline',
          part: partId,
          staff: staffId,
          beat: bar.start,
        }
        if (chosen.kind === 'bars') dialog.value = 'structure'
        else if (chosen.kind === 'repeat') {
          const next = clone(doc.value)
          next.score = sharedTimeline(next)
          next.score.repeats.push({ start: bar.start, end: bar.end, times: 2 })
          void commit(next)
        } else if (chosen.kind === 'barline') {
          const next = clone(doc.value)
          next.score = sharedTimeline(next)
          next.score.barlines = [
            ...(next.score.barlines || []).filter((b) => b.beat !== bar.end),
            { beat: bar.end, style: chosen.value! },
          ]
          void commit(next)
        } else dialog.value = 'shared'
      }
      placement.value = null
    } catch (e) {
      error.value = String(e)
    }
    event.preventDefault()
    return
  }
  const mark = event.target.closest<HTMLElement>('[data-score-element]')
  menu.value = null
  if (
    !(
      event.target.closest('[data-staff-id]') &&
      !event.target.closest('[data-note-id]') &&
      !mark
    )
  )
    region.value = null
  if (
    mark &&
    !(
      tool.value === 'write' &&
      JSON.parse(mark.dataset.scoreElement!).kind === 'rest'
    )
  ) {
    trackClick(mark.dataset.scoreElement!)
    selectedElement.value = JSON.parse(mark.dataset.scoreElement!)
    selected.value = new Set()
    focus(selectedElement.value!.part)
    root.value?.focus({ preventScroll: true })
    if (canEdit.value) {
      elementDrag = {
        e: selectedElement.value!,
        x: event.clientX,
        y: event.clientY,
        beat: beatAt(point(event).x),
      }
      viewport.value?.setPointerCapture(event.pointerId)
    }
    event.preventDefault()
    return
  }
  selectedElement.value = null
  const row = event.target.closest<HTMLElement>('[data-staff-id]'),
    node = event.target.closest<HTMLElement>('[data-note-id]')
  if (!row) return
  root.value?.focus({ preventScroll: true })
  focus(row.dataset.partId!)
  if (node) {
    trackClick(`${row.dataset.partId}:${node.dataset.noteId}`)
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
    if (!multi) {
      const p = doc.value.parts.find((p) => p.id === row.dataset.partId),
        n = p?.notes.find((n) => n.id === node.dataset.noteId)
      if (p && n) {
        const v = metadata(n, p)
        placeCaret(p.id, v.staff, n.beat + n.duration, v.step, v.voice)
      }
    }
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
  const snap = onsetSnap.value
  const at = Math.max(0, Math.round(beatAt(pos.x) / snap) * snap),
    staff = staves(
      doc.value.parts.find((p) => p.id === row.dataset.partId)!,
    ).find((s) => s.id === row.dataset.staffId)!,
    clef =
      staff.clef_changes?.filter((c) => c.beat <= at).at(-1)?.clef || staff.clef
  gesture.value = {
    ...pos,
    cx: event.clientX,
    cy: event.clientY,
    add: event.ctrlKey || event.metaKey,
    extend: event.shiftKey,
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
  if (elementDrag) return
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
              Math.max(0, Math.round(beatAt(pos.x) / onsetSnap.value)) *
                onsetSnap.value,
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
  if (inspectAfterPointer) {
    inspectAfterPointer = false
    gesture.value = null
    elementDrag = null
    if (viewport.value?.hasPointerCapture(event.pointerId))
      viewport.value.releasePointerCapture(event.pointerId)
    void inspectElement()
    return
  }
  if (elementDrag) {
    const g = elementDrag
    elementDrag = null
    if (
      Math.hypot(event.clientX - g.x, event.clientY - g.y) > 5 &&
      canEdit.value
    ) {
      const next = clone(doc.value),
        el = document
          .elementFromPoint(event.clientX, event.clientY)
          ?.closest<HTMLElement>('[data-note-id]')
      try {
        moveElement(
          next,
          g.e,
          Math.round((beatAt(point(event).x) - g.beat) * 4) / 4,
          Math.round((g.y - event.clientY) / 5),
          el
            ? { part: el.dataset.partId!, note: el.dataset.noteId! }
            : undefined,
        )
        void commit(next).then((ok) => {
          if (ok) selectedElement.value = null
        })
      } catch (e) {
        error.value = e instanceof Error ? e.message : String(e)
      }
    }
    if (viewport.value?.hasPointerCapture(event.pointerId))
      viewport.value.releasePointerCapture(event.pointerId)
    return
  }

  const g = gesture.value
  if (!g) return
  if (g.move && marquee.value && canEdit.value) {
    const delta =
        Math.round((beatAt(g.x + event.clientX - g.cx) - beatAt(g.x)) * 4) / 4,
      steps = -Math.round((event.clientY - g.cy) / 5)
    try {
      const next = clone(doc.value)
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
    if (tool.value === 'select') {
      const snap = onsetSnap.value,
        start = Math.max(0, Math.round(beatAt(m.left) / snap) * snap),
        end = Math.min(
          length.value,
          Math.round(beatAt(m.left + m.width) / snap) * snap,
        )
      region.value =
        end - start >= snap - 1e-9
          ? { part: g.part, staff: g.staff, start, end }
          : null
    }
  } else if (g.move) {
  } else if (tool.value === 'write') {
    region.value = null
    placeCaret(g.part, g.staff, g.beat, g.step)
    insertAtCaret([{ step: g.step }])
  } else if (!g.add) {
    selected.value = new Set()
    placeCaret(g.part, g.staff, g.beat, g.step)
    selectBar(g.part, g.staff, g.beat, g.extend)
  }
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
  if (key === 'Enter') {
    event.preventDefault()
    void inspectElement()
    return
  }
  if (selectedElement.value && (key === 'Delete' || key === 'Backspace')) {
    event.preventDefault()
    remove()
    return
  }
  if (mod && key.toLowerCase() === 'z') {
    event.preventDefault()
    void undo(event.shiftKey)
    return
  }
  if (mod && (key.toLowerCase() === 'c' || key.toLowerCase() === 'x')) {
    if (picked.value.length) {
      clipboard.value = clone(
        picked.value.map(({ p, n }) => ({ part: p.id, note: n })),
      )
      event.preventDefault()
      if (key.toLowerCase() === 'x' && canEdit.value) remove()
    } else if (region.value) {
      regionClipboard.value = copyRegion(doc.value, region.value)
      event.preventDefault()
      if (key.toLowerCase() === 'x') clearRegion()
    }
    return
  }
  if (mod && key.toLowerCase() === 'v' && regionClipboard.value && canEdit.value) {
    const c = caret.value,
      r = region.value
    if (tool.value === 'write' && c) {
      event.preventDefault()
      pasteAt(c.part, c.staff, c.beat)
      return
    }
    if (r) {
      event.preventDefault()
      pasteAt(r.part, r.staff, r.start)
      return
    }
  }
  if (
    mod &&
    key.toLowerCase() === 'v' &&
    clipboard.value.length &&
    canEdit.value
  ) {
    event.preventDefault()
    const next = clone(doc.value),
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
  if (event.altKey && !mod && /^Digit[1-4]$/.test(event.code)) {
    event.preventDefault()
    setVoice(Number(event.code.slice(5)))
    return
  }
  if (mod || event.altKey) return
  if (key === 'Escape') {
    selectedElement.value = null
    gesture.value = null
    marquee.value = null
    tieNext.value = null
    menu.value = null
    if (selected.value.size) selected.value = new Set()
    else if (region.value) region.value = null
    else caret.value = null
    return
  }
  if (
    (key === 'Delete' || key === 'Backspace') &&
    region.value &&
    tool.value === 'select' &&
    !picked.value.length &&
    !selectedElement.value
  ) {
    event.preventDefault()
    if (!event.repeat) clearRegion()
    return
  }
  if (
    (key === 'ArrowUp' || key === 'ArrowDown') &&
    region.value &&
    tool.value === 'select' &&
    !picked.value.length &&
    !selectedElement.value &&
    canEdit.value
  ) {
    event.preventDefault()
    try {
      void commit(
        transposeRegion(doc.value, region.value, {
          steps: (key === 'ArrowUp' ? 1 : -1) * (event.shiftKey ? 7 : 1),
        }),
        !arrowHeld,
      )
      arrowHeld = true
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
    }
    return
  }
  if (key === 'F10' && event.shiftKey && region.value && !props.performance) {
    event.preventDefault()
    const el = viewport.value?.querySelector<HTMLElement>('[data-bar-region]')
    if (el) {
      const b = el.getBoundingClientRect()
      contextMenu(
        new MouseEvent('contextmenu', {
          clientX: b.left + 8,
          clientY: b.top + 8,
          bubbles: true,
        }),
      )
    }
    return
  }
  const digit = /^(?:Digit|Numpad)([0-9])$/.exec(event.code)?.[1]
  const entry =
    view.value === 'notation' &&
    tool.value === 'write' &&
    caret.value &&
    !picked.value.length &&
    !selectedElement.value
  if (entry) {
    const c = caret.value!
    if (key === 'ArrowUp' || key === 'ArrowDown') {
      event.preventDefault()
      caretStep((key === 'ArrowUp' ? 1 : -1) * (event.shiftKey ? 7 : 1))
      return
    }
    if (key === 'ArrowLeft' || key === 'ArrowRight') {
      event.preventDefault()
      moveCaret(key === 'ArrowRight' ? 1 : -1, event.shiftKey)
      return
    }
    if (event.repeat) return
    if (digit && digit !== '9') {
      event.preventDefault()
      if (digit === '0') insertAtCaret([{ step: c.step }], { rest: true })
      else {
        base.value = durationKeys[Number(digit) - 1]!
        const heldHeads =
          midiMode.value === 'hold'
            ? midiHeld.pitches.flatMap((pitch) => {
                const head = midiEntryHead(pitch)
                return head ? [head as EntryHead] : []
              })
            : []
        if (heldHeads.length) insertAtCaret(heldHeads)
        else if (event.shiftKey) addToChord(c.step)
        else insertAtCaret([{ step: c.step }])
      }
      return
    }
    if (/^[a-gA-G]$/.test(key)) {
      event.preventDefault()
      const step = nearestLetterStep(key, c.step)
      if (event.shiftKey) addToChord(step)
      else insertAtCaret([{ step }])
      return
    }
    if (key === 'Backspace' || key === 'Delete') {
      event.preventDefault()
      deleteBeforeCaret()
      return
    }
  }
  if (key.toLowerCase() === 't' && !event.repeat) {
    event.preventDefault()
    tieAtCaret()
    return
  }
  let command: NoteCommand | undefined
  if (view.value === 'notation' && /^[1-8]$/.test(digit ?? key))
    command = {
      kind: 'duration',
      value: durationKeys[Number(digit ?? key) - 1]!,
    }
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
  () => doc.value.parts.map((p) => p.id),
  (ids, old) => {
    visible.value = new Set(
      [...visible.value, ...ids.filter((id) => !old?.includes(id))].filter(
        (id) => ids.includes(id),
      ),
    )
    if (!ids.includes(focused.value)) focus(shown.value[0]?.id || '')
    const c = caret.value
    if (
      c &&
      !doc.value.parts.some(
        (p) => p.id === c.part && staves(p).some((s) => s.id === c.staff),
      )
    )
      caret.value = null
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
    @contextmenu="contextMenu"
  >
    <p v-if="unsupported" class="field-error" role="status">
      {{ unsupported }} note duration(s) need custom rhythm notation. Notes
      marked * show their exact duration in quarter beats.
    </p>
    <p
      v-if="outsideLoop && (!doc.score || doc.mode !== 'structured')"
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
      ><button @click="discardDraft">Discard draft</button
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
            <button @click="visible = new Set(doc.parts.map((p) => p.id))">
              Show all</button
            ><button @click="visible = new Set()">Hide all</button>
          </div>
          <div
            v-for="(p, index) in doc.parts"
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
                  doc.graph.nodes.find((n) => n.id === p.instrument_node)
                    ?.label || 'External / acoustic'
                }}
                · MIDI {{ p.midi_channel || 1 }}</small
              ></button
            ><span class="part-mix"
              ><button
                :disabled="!canEdit"
                :aria-label="`Mute ${p.name}`"
                :aria-pressed="!!p.muted"
                :title="`Mute ${p.name} MIDI`"
                @click="mixPart(p.id, false)"
              >
                M</button
              ><button
                :disabled="!canEdit"
                :aria-label="`Solo ${p.name}`"
                :aria-pressed="!!p.solo"
                :title="`Solo ${p.name}`"
                @click="mixPart(p.id, true)"
              >
                S
              </button></span
            ><span class="part-order"
              ><button
                :disabled="!canEdit || index === 0"
                :aria-label="`Move ${p.name} up`"
                @click="reorder(p.id, -1)"
              >
                ↑</button
              ><button
                :disabled="!canEdit || index === doc.parts.length - 1"
                :aria-label="`Move ${p.name} down`"
                @click="reorder(p.id, 1)"
              >
                ↓
              </button></span
            >
          </div>
          <button
            :disabled="!canEdit || doc.parts.length >= 32"
            aria-label="Add part"
            @click="addPart"
          >
            ＋ Add part
          </button>
        </template>
      </aside>
      <div class="score-main">
        <header
          v-if="!performance"
          ref="palette"
          class="notation-toolbar"
          role="toolbar"
          aria-label="Notation tools"
        >
          <button
            aria-label="Select"
            title="Select and move (Escape)"
            :aria-pressed="tool === 'select'"
            @click="
              () => {
                tool = 'select'
                placement = null
              }
            "
          >
            <MousePointer2 :size="15" /></button
          ><button
            aria-label="Write"
            title="Write notes"
            :aria-pressed="tool === 'write'"
            @click="
              () => {
                tool = 'write'
                placement = null
              }
            "
          >
            <Pencil :size="15" />
          </button>
          <template v-if="view === 'notation'">
            <ScoreToolMenu label="Note values" symbol="♩">
              <button
                v-for="(value, i) in durationKeys"
                :key="value"
                :aria-label="`${durationLabels[i]} note (${i + 1})`"
                :title="`${durationLabels[i]} · ${i + 1}`"
                :aria-pressed="activeBase === value"
                :disabled="!canEdit"
                @click="
                  () => {
                    chooseDuration(value, false)
                  }
                "
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
                    v-for="f in value < 1
                      ? Math.round(Math.log2(1 / value))
                      : 0"
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
            </ScoreToolMenu>
            <ScoreToolMenu label="Rest values" symbol="𝄽"
              ><template #icon><ScoreRestIcon :value="1" /></template
              ><button
                v-for="(value, i) in durationKeys"
                :key="value"
                :title="`${durationLabels[i]} rest`"
                :aria-label="`${durationLabels[i]} rest`"
                :disabled="!canEdit"
                @click="
                  () => {
                    chooseDuration(value, true)
                  }
                "
              >
                <ScoreRestIcon :value="value" /></button
            ></ScoreToolMenu>
            <ScoreToolMenu label="Accidentals and dots" symbol="♯">
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
                title="Add sharp (+)"
                @click="apply({ kind: 'alter', value: 1 })"
              >
                ♯</button
              ><button
                :disabled="!canEdit"
                aria-label="Add flat (-)"
                title="Add flat (-)"
                @click="apply({ kind: 'alter', value: -1 })"
              >
                ♭</button
              ><button
                :disabled="!canEdit"
                aria-label="Natural"
                title="Natural"
                @click="apply({ kind: 'natural' })"
              >
                ♮</button
              ><button
                title="Use key signature"
                aria-label="Use key signature"
                :aria-pressed="alter === null"
                @click="alter = null"
              >
                A♮
              </button>
            </ScoreToolMenu>
            <ScoreToolMenu label="Accents" symbol=">"
              ><button
                v-for="(symbol, a) in {
                  staccato: '·',
                  tenuto: '—',
                  accent: '>',
                  marcato: '^',
                }"
                :key="a"
                :title="a"
                :aria-label="a"
                :disabled="!canEdit"
                @click="
                  () => {
                    entryArticulation = a
                    if (picked.length) patchNotation({ articulation: a })
                    else placeTool('accent', a)
                  }
                "
              >
                {{ symbol }}
              </button></ScoreToolMenu
            >
            <ScoreToolMenu label="Clefs" symbol="𝄞"
              ><button
                v-for="(symbol, c) in {
                  treble: '𝄞',
                  bass: '𝄢',
                  alto: '𝄡',
                  tenor: '𝄡',
                }"
                :key="c"
                :title="`${c} clef · click to place`"
                :aria-label="`${c} clef tool`"
                :disabled="!canEdit"
                @click="placeTool('clef', c)"
              >
                {{ symbol }}<small>{{ c[0] }}</small>
              </button></ScoreToolMenu
            >
            <ScoreToolMenu label="Time signatures" symbol="⁴₄"
              ><button
                v-for="m in [
                  '2/4',
                  '3/4',
                  '4/4',
                  '6/8',
                  '9/8',
                  '12/8',
                  '5/4',
                  '7/8',
                ]"
                :key="m"
                :title="`${m} · click to place`"
                :aria-label="`${m} time signature tool`"
                :disabled="!canEdit"
                @click="placeTool('meter', m)"
              >
                {{ m }}</button
              ><button
                title="Custom time signature"
                aria-label="Custom time signature"
                @click="dialog = 'structure'"
              >
                …
              </button></ScoreToolMenu
            >
            <ScoreToolMenu label="Bar tools and repeats" symbol="𝄆"
              ><button
                v-for="(symbol, style) in {
                  double: '𝄁',
                  final: '𝄂',
                  dashed: '┊',
                }"
                :key="style"
                :title="`${style} barline · click a bar`"
                :aria-label="`${style} barline tool`"
                :disabled="!canEdit"
                @click="placeTool('barline', style)"
              >
                {{ symbol }}</button
              ><button
                title="Repeat clicked bar twice"
                aria-label="Repeat bar tool"
                :disabled="!canEdit"
                @click="placeTool('repeat')"
              >
                𝄆𝄇</button
              ><button
                title="Insert bars · click a bar"
                aria-label="Insert bars tool"
                :disabled="!canEdit"
                @click="placeTool('bars')"
              >
                +𝄀</button
              ><button
                title="Repeats and navigation"
                aria-label="Repeat and navigation settings"
                @click="dialog = 'shared'"
              >
                𝄋
              </button></ScoreToolMenu
            >
            <ScoreToolMenu label="Text and tempo" symbol="T"
              ><button
                title="Tempo mark · click a staff at the beat"
                aria-label="Tempo mark tool"
                :disabled="!canEdit"
                @click="placeTool('tempo')"
              >
                ♩=</button
              ><button
                v-for="k in markKinds"
                :key="k.id"
                :title="`${k.label} · click a staff at the beat`"
                :aria-label="`${k.label} tool`"
                :disabled="!canEdit"
                @click="placeTool('mark', k.id)"
              >
                <small>{{
                  k.id === 'rehearsal'
                    ? 'A'
                    : k.id === 'cue'
                      ? '▶'
                      : k.id === 'expression'
                        ? 'espr.'
                        : k.id === 'tempo'
                          ? 'rit.'
                          : k.id === 'lyric'
                            ? 'la'
                            : 'Tx'
                }}</small>
              </button></ScoreToolMenu
            >
            <small v-if="placement" role="status"
              >Click to place {{ placement.value || placement.kind }}</small
            >
          </template>
          <button
            :disabled="!history.length || !canEdit"
            title="Undo (Ctrl/Cmd Z)"
            aria-label="Undo"
            @click="undo()"
          >
            <Undo2 :size="15" /></button
          ><button
            :disabled="!future.length || !canEdit"
            title="Redo (Ctrl/Cmd Shift Z)"
            aria-label="Redo"
            @click="undo(true)"
          >
            <Redo2 :size="15" />
          </button>
          <button
            v-if="view === 'notation'"
            title="Entry settings: voice, tuplets and phrasing"
            aria-label="Entry settings"
            @click="dialog = 'entry'"
          >
            <SlidersHorizontal :size="15" />
          </button>
          <button
            :disabled="!picked.length && !selectedElement"
            title="Edit selection (Enter)"
            aria-label="Edit selected notes"
            @click="inspectElement"
          >
            <Settings2 :size="15" />
          </button>
          <button
            :disabled="!canEdit || (!picked.length && !selectedElement)"
            title="Delete selected (Backspace)"
            aria-label="Delete selected"
            @click="remove"
          >
            <Trash2 :size="15" />
          </button>
          <template v-if="!performance"
            ><button
              :title="view === 'notation' ? 'Piano roll' : 'Notation'"
              :aria-label="view === 'notation' ? 'Piano roll' : 'Notation'"
              @click="view = view === 'notation' ? 'grid' : 'notation'"
            >
              <Piano v-if="view === 'notation'" :size="16" /><Music2
                v-else
                :size="16"
              />
            </button>
            <button
              title="Shared score · meter, keys, repeats and navigation"
              aria-label="Shared score"
              @click="
                () => {
                  selectedElement = null
                  dialog = 'shared'
                }
              "
            >
              <Music2 :size="16" /> Score
            </button>
            <button
              v-if="part"
              :title="`Part settings · ${part.name}`"
              :aria-label="`Part settings · ${part.name}`"
              @click="dialog = 'part'"
            >
              <Settings2 :size="16" /> Part
            </button></template
          >
          <button
            title="Measure: time signature, key, repeats, D.C./D.S., barlines, clef and bars for the selected bars (right-click a bar)"
            aria-label="Measure"
            @click="openMeasureDialog('meter')"
          >
            <span style="font-size: 17px; line-height: 1">𝄀</span
            ><span style="font-size: 11px">Measure</span></button
          ><button
            title="Time signature, add/insert/delete bars, and clef changes"
            aria-label="Bars & meter"
            @click="dialog = 'structure'"
          >
            <span style="font-size: 11px">Bars & meter</span></button
          ><slot name="tools" />
        </header>

        <div
          ref="viewport"
          class="ensemble-scroll"
          :style="{
            paddingTop: performance ? '0px' : `${paletteHeight + 16}px`,
          }"
          @scroll.passive="updateViewport"
          @dblclick="inspectElement"
          @pointerdown="pointerDown"
          @pointermove="pointerMove"
          @pointerup="pointerUp"
          @pointercancel="
            () => {
              gesture = null
              marquee = null
              elementDrag = null
            }
          "
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
                  <button
                    class="staff-name"
                    :aria-label="`Select staff ${s.name}`"
                    :title="`Edit staff ${s.name} (double-click)`"
                    :data-score-element="
                      elementKey({ kind: 'staff', part: p.id, staff: s.id })
                    "
                    @dblclick.stop="dialog = 'part'"
                  >
                    {{ s.name }}</button
                  ><ScoreStaff
                    :part="p"
                    :staff="s"
                    :length="length"
                    :bar-length="barLength"
                    :beats-per-bar="doc.beats_per_bar"
                    :beat-unit="doc.beat_unit || 4"
                    :scale="scale"
                    :origin="origin"
                    :anchors="anchors"
                    :timeline="doc.score"
                    :view-start="viewportStart"
                    :view-end="viewportEnd"
                    :selected="selected"
                    :selected-element="
                      selectedElement ? elementKey(selectedElement) : null
                    "
                    :beat="beats[p.id] || 0"
                  />
                  <div
                    v-if="
                      !performance &&
                      region &&
                      region.part === p.id &&
                      region.staff === s.id
                    "
                    class="bar-region"
                    data-bar-region
                    :data-start="region.start"
                    :data-end="region.end"
                    :style="{
                      left: `${xAt(region.start) - 12}px`,
                      width: `${Math.max(4, xAt(region.end) - xAt(region.start))}px`,
                    }"
                    aria-hidden="true"
                  />
                  <div
                    v-if="
                      !performance &&
                      tool === 'write' &&
                      caret &&
                      caret.part === p.id &&
                      caret.staff === s.id
                    "
                    class="entry-caret"
                    data-entry-caret
                    :data-beat="caret.beat"
                    :data-step="caret.step"
                    :style="{ left: `${xAt(caret.beat) - 4}px` }"
                    aria-hidden="true"
                  >
                    <span
                      class="entry-caret-head"
                      :style="{ top: `${caretTop(s)}px` }"
                    /><small class="entry-caret-voice">v{{ caret.voice }}</small>
                  </div></div
              ></template>
              <ScorePianoRoll
                v-else
                :part="p"
                :selected="selected"
                :editable="canEdit"
                :tool="tool"
                :anchors="anchors"
                :scale="scale"
                :origin="64"
                :width="width"
                :beat="beats[p.id] || 0"
                @update="updatePart"
                @select="
                  (keys) => {
                    selected = keys
                    selectedElement = null
                  }
                "
                @focus="focus"
                @inspect="dialog = 'note'"
              />
              <ScoreDynamics
                :error="error"
                :part="p"
                :scale="scale"
                :anchors="
                  (performance ? p.view === 'grid' : view === 'grid')
                    ? []
                    : anchors
                "
                :origin="
                  (performance ? p.view === 'grid' : view === 'grid')
                    ? 64
                    : origin
                "
                :width="width"
                :editable="canEdit"
                :performance="performance"
                @update="updatePart"
              />
              <MidiLanes
                :part="p"
                :scale="scale"
                :anchors="
                  (performance ? p.view === 'grid' : view === 'grid')
                    ? []
                    : anchors
                "
                :origin="
                  (performance ? p.view === 'grid' : view === 'grid')
                    ? 64
                    : origin
                "
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
        <footer class="score-footer">
          <label
            v-if="!performance && view === 'notation'"
            :title="
              midiInputs.length
                ? `MIDI inputs: ${midiInputs.join(', ')}`
                : 'Choose how a MIDI keyboard enters notes at the caret'
            "
            >MIDI<select v-model="midiMode" aria-label="MIDI entry">
              <option value="off">Off</option>
              <option value="play">Play to enter</option>
              <option value="hold">Hold + number</option>
            </select
            ><small v-if="midiMode !== 'off'" class="midi-entry-status">{{
              midiError || (midiInputs.length ? midiInputs.join(', ') : 'No inputs')
            }}</small></label
          >
          <label v-if="!performance && view === 'notation'"
            >Snap<select
              v-model.number="onsetSnap"
              aria-label="Note onset grid"
            >
              <option :value="0.25">16th</option>
              <option :value="0.125">32nd</option>
              <option :value="0.0625">64th</option>
              <option :value="1 / 3">Eighth triplet</option>
              <option :value="1 / 6">16th triplet</option>
            </select></label
          >
          <div v-if="performance" class="performance-score-options">
            <label
              ><input v-model="showAll" type="checkbox" /> Show all parts</label
            ><span
              >{{ shown.filter((p) => p.performer === userId).length }} assigned
              part(s)</span
            >
          </div>
          <div v-if="!performance" class="score-status" role="status">
            {{
              selectedElement
                ? `Selected ${selectedElement.kind} · Enter to edit · Backspace to delete`
                : picked.length
                  ? `Editing ${picked.length} selected note(s)`
                  : region && tool === 'select'
                    ? `Selected ${regionLabel} · Shift-click extends · right-click for measure & mass edit · ↑↓ transpose · ⌘C/X/V copy, cut, paste · Backspace clears`
                  : view === 'grid'
                    ? 'Drag to draw duration · drag notes to move · drag right edge to resize'
                    : caretStatus
                      ? `${caretStatus} · 1–8 insert · A–G pitch · 0 rest · ↑↓ pitch · ←→ move · T tie · Alt+1–4 voice`
                      : tool === 'write'
                        ? 'Click a staff to place the caret, then type durations (1–8) and pitches (A–G)'
                        : 'Click or drag to select · double-click to edit'
            }}
          </div>
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
          <slot name="footer" />
        </footer>
      </div>
    </div>
    <ScoreDialog
      v-if="dialog === 'structure'"
      title="Bars, time signature & clef"
      :error="error"
      @close="dialog = null"
      ><ScoreStructureEditor
        :project="doc"
        :editable="canEdit"
        :part-id="focused"
        :staff-id="
          selectedElement?.staff ||
          (picked[0] ? metadata(picked[0].n, picked[0].p).staff : undefined)
        "
        :beat="selectedElement?.beat ?? picked[0]?.n.beat ?? 0"
        @update="commit"
    /></ScoreDialog>
    <ScoreDialog
      :error="error"
      v-if="part && dialog === 'part'"
      :title="`Part settings · ${part.name}`"
      @close="dialog = null"
      ><div class="score-settings">
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
            >Performer<select
              aria-label="Performer"
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
            >Instrument / input<select
              aria-label="Instrument / input"
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
                v-for="n in doc.graph.nodes.filter((n) =>
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
            >Default display<select
              aria-label="Default display"
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
            >Time signature<select
              aria-label="Time signature"
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
                  midi_channel: Number(
                    ($event.target as HTMLInputElement).value,
                  ),
                })
              "
          /></label>
          <label
            >Beats per bar<input
              type="number"
              min="1"
              max="16"
              :value="doc.beats_per_bar"
              :disabled="!canEdit"
              @change="
                meter(
                  'beats_per_bar',
                  Number(($event.target as HTMLInputElement).value),
                )
              "
          /></label>
          <label
            >Beat unit<select
              aria-label="Beat unit"
              :value="doc.beat_unit || 4"
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
                editStaff(s, {
                  name: ($event.target as HTMLInputElement).value,
                })
              " /></label
          ><label
            >Clef<select
              aria-label="Clef"
              :value="s.clef"
              :disabled="!canEdit"
              @change="
                editStaff(s, {
                  clef: ($event.target as HTMLSelectElement).value,
                })
              "
            >
              <option v-for="c in ['treble', 'bass', 'alto', 'tenor']" :key="c">
                {{ c }}
              </option>
            </select></label
          ><label
            >Key signature<select
              aria-label="Key signature"
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
            >Key mode<select
              aria-label="Key mode"
              :value="s.key_mode || 'major'"
              :disabled="!canEdit"
              @change="
                editStaff(s, {
                  key_mode: ($event.target as HTMLSelectElement).value as
                    'major' | 'minor',
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
            >Staff instrument<select
              aria-label="Staff instrument"
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
                v-for="n in doc.graph.nodes.filter((n) =>
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
          ><label
            >Staff MIDI port<input
              :value="s.midi_port || ''"
              :disabled="!canEdit"
              placeholder="Inherit part"
              @change="
                editStaff(s, {
                  midi_port: ($event.target as HTMLInputElement).value || null,
                })
              " /></label
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
                  clef_changes: s.clef_changes!.filter(
                    (x) => x.beat !== c.beat,
                  ),
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
      </div></ScoreDialog
    >
    <ScoreDialog
      v-if="dialog === 'measure' && region"
      :title="`Measure · ${regionLabel} · ${doc.parts.find((p) => p.id === region!.part)?.name || ''}`"
      :error="error"
      @close="dialog = null"
      ><ScoreMeasureDialog
        :project="doc"
        :editable="canEdit"
        :mode="measureMode"
        :region="region"
        @update="(next) => commit(next)"
        @close="dialog = null"
    /></ScoreDialog>
    <ScoreDialog
      v-if="dialog === 'tempo' && tempoDraft"
      title="Tempo change"
      :error="error"
      @close="dialog = null"
      ><div class="mark-dialog">
        <label
          >At quarter beat<input
            v-model.number="tempoDraft.beat"
            type="number"
            min="0"
            step="0.25"
        /></label>
        <label
          >♩ per minute<input
            v-model.number="tempoDraft.bpm"
            type="number"
            min="1"
            max="400"
            step="0.5"
            aria-label="Tempo"
            autofocus
            @keydown.enter.prevent="saveTempo"
        /></label>
        <div class="mark-actions">
          <button type="button" :disabled="!canEdit" @click="saveTempo">
            Apply tempo
          </button>
          <button
            v-if="(doc.score?.tempos || []).some((t) => t.beat === tempoDraft!.beat)"
            type="button"
            class="danger"
            :disabled="!canEdit"
            @click="
              () => {
                void commit(removeTempo(doc, tempoDraft!.beat)).then((ok) => {
                  if (ok) dialog = null
                })
              }
            "
          >
            Remove tempo mark
          </button>
        </div>
        <p>
          The tempo takes effect when playback reaches this position and lasts
          until the next tempo mark. A mark at beat 0 also sets the project tempo.
        </p>
      </div></ScoreDialog
    >
    <ScoreDialog
      v-if="dialog === 'mark' && markDraft"
      :title="markDraft.id ? 'Edit mark' : 'Add mark'"
      :error="error"
      @close="dialog = null"
      ><div class="mark-dialog">
        <label
          >Kind<select v-model="markDraft.kind" aria-label="Mark kind">
            <option v-for="k in markKinds" :key="k.id" :value="k.id">
              {{ k.label }}
            </option>
          </select></label
        >
        <label
          >Text<input
            v-model="markDraft.text"
            maxlength="256"
            aria-label="Mark text"
            autofocus
            placeholder="e.g. start granular, A, espressivo, rit."
            @keydown.enter.prevent="saveMark"
        /></label>
        <label
          >At quarter beat<input
            v-model.number="markDraft.beat"
            type="number"
            min="0"
            step="0.25"
        /></label>
        <div class="mark-actions">
          <button
            type="button"
            :disabled="!canEdit || !markDraft.text.trim()"
            @click="saveMark"
          >
            {{ markDraft.id ? 'Save mark' : 'Add mark' }}
          </button>
          <button
            v-if="markDraft.id"
            type="button"
            class="danger"
            :disabled="!canEdit"
            @click="
              () => {
                void commit(
                  removeMark(doc, markDraft!.part, markDraft!.staff, markDraft!.id!),
                ).then((ok) => {
                  if (ok) {
                    dialog = null
                    selectedElement = null
                  }
                })
              }
            "
          >
            Delete mark
          </button>
        </div>
        <p>
          Marks are attached to this staff and appear on every player’s view of
          the part. Cues are shown bold with an arrow; lyrics sit below the staff.
        </p>
      </div></ScoreDialog
    >
    <ScoreContextMenu
      v-if="menu"
      :x="menu.x"
      :y="menu.y"
      :items="menu.items"
      label="Measure actions"
      @close="menu = null"
    />
    <ScoreDialog
      :error="error"
      v-if="dialog === 'shared'"
      title="Shared score · meter, keys, repeats and navigation"
      @close="dialog = null"
    >
      <ScoreTimelineEditor
        v-if="!performance"
        :project="doc"
        :selection="selectedElement"
        :editable="canEdit"
        @update="(score) => commit({ ...clone(doc), score })"
      />
    </ScoreDialog>
    <ScoreDialog
      :error="error"
      v-if="dialog === 'entry'"
      title="Entry settings"
      @close="dialog = null"
      ><div class="selection-inspector">
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
      </div></ScoreDialog
    >
    <ScoreDialog
      :error="error"
      v-if="dialog === 'note'"
      title="Edit selected notes"
      @close="dialog = null"
    >
      <div v-if="picked.length" class="selection-inspector">
        <label
          >Duration (quarter beats)<input
            type="number"
            min="0.0625"
            step="0.25"
            :value="picked[0]!.n.duration"
            :disabled="!canEdit"
            @change="
              noteValue(
                'duration',
                Number(($event.target as HTMLInputElement).value),
              )
            "
        /></label>
        <label v-if="picked.length === 1"
          >Onset<input
            type="number"
            min="0"
            step="0.25"
            :value="picked[0]!.n.beat"
            :disabled="!canEdit"
            @change="
              noteValue(
                'beat',
                Number(($event.target as HTMLInputElement).value),
              )
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
          >Voice<select
            aria-label="Voice"
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
          >Articulation<select
            aria-label="Articulation"
            :value="picked[0]!.n.notation?.articulation || ''"
            :disabled="!canEdit"
            @change="
              patchNotation({
                articulation:
                  ($event.target as HTMLSelectElement).value || null,
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
          >Octave line<select
            aria-label="Octave line"
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
    </ScoreDialog>
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
  width: 224px;
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
  margin-top: 1px;
}
.part-mix {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.part-entry .part-mix button {
  min-height: 22px;
  height: 22px;
  width: 24px;
  padding: 0;
  text-align: center;
  font-size: 11px;
}
.part-mix button[aria-pressed='true'] {
  background: #c9ead1;
  color: #14532d;
  border-color: #16803c;
}
.part-entry > button {
  padding: 4px;
  min-height: 44px;
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
  background: #ffffff;
}
.staff-name {
  padding: 0;
  border: 0;
  background: transparent;
  text-align: left;
  cursor: pointer;
  min-height: 0;
  position: sticky;
  left: 8px;
  top: 0;
  font-size: 10px;
  color: #000000;
  z-index: 1;
  display: block;
  height: 16px;
  margin-bottom: -16px;
  transform: translateY(8px);
  width: 170px;
}
.score-marquee {
  position: absolute;
  pointer-events: none;
  border: 1px solid #087f8c;
  background: #087f8c20;
  z-index: 5;
}
.entry-ghost {
  position: absolute;
  pointer-events: none;
  color: #087f8c;
  opacity: 0.55;
  transform: translate(-3px, -8px);
}
.mark-dialog {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px;
  font-size: 12px;
}
.mark-dialog label {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.mark-dialog input,
.mark-dialog select {
  max-width: 360px;
}
.mark-actions {
  display: flex;
  gap: 8px;
}
.mark-actions button {
  min-height: 40px;
  padding: 0 16px;
  border: 1px solid #cbd5d7;
  border-radius: 4px;
  background: #fff;
  color: #111;
  cursor: pointer;
}
.mark-actions button.danger {
  color: #9c2d16;
}
.mark-dialog p {
  color: #526267;
  margin: 0;
}
.bar-region {
  position: absolute;
  top: 58px;
  height: 84px;
  background: #087f8c1c;
  border: 1px solid #087f8c66;
  border-radius: 3px;
  pointer-events: none;
  z-index: 1;
}
.entry-caret {
  position: absolute;
  top: 0;
  height: 190px;
  width: 16px;
  pointer-events: none;
  z-index: 3;
}
.entry-caret::before {
  content: '';
  position: absolute;
  top: 56px;
  bottom: 34px;
  border-left: 2px solid #087f8c;
  opacity: 0.85;
}
.entry-caret-head {
  position: absolute;
  left: 1px;
  width: 11px;
  height: 8px;
  border: 2px solid #087f8c;
  border-radius: 50%;
  background: #087f8c33;
  transform: translate(0, -50%) rotate(-20deg);
}
.entry-caret-voice {
  position: absolute;
  top: 42px;
  left: 3px;
  font-size: 9px;
  color: #087f8c;
}
@media (prefers-reduced-motion: no-preference) {
  .entry-caret::before {
    animation: caret-blink 1.1s steps(2, start) infinite;
  }
}
@keyframes caret-blink {
  to {
    opacity: 0.25;
  }
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

.score-main {
  position: relative;
}
.score-view-tools {
  justify-content: flex-end;
  padding: 3px 8px;
  min-height: 30px;
  gap: 10px;
}
.score-view-tools button {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 3px 6px;
  min-height: 26px;
  font-size: 11px;
}
.score-view-tools input[type='range'] {
  width: 75px;
}
.notation-toolbar {
  position: absolute;
  top: 40px;
  left: 10px;
  z-index: 8;
  display: flex;
  gap: 2px;
  padding: 4px;
  max-width: calc(100% - 20px);
  border: 1px solid var(--line);
  border-radius: 9px;
  background: #202b30f5;
  box-shadow: 0 3px 12px #0004;
  flex-wrap: wrap;
}
.notation-toolbar button {
  min-width: 27px;
  min-height: 29px;
  height: 29px;
  padding: 2px 4px;
  font-size: 17px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 1px;
}
.notation-toolbar kbd {
  font-size: 8px;
}
.notation-toolbar .note-symbol {
  width: 18px;
  height: 23px;
}
.notation-toolbar .score-tool-grid .note-symbol,
.notation-toolbar .score-tool-grid svg {
  width: 28px;
  height: 34px;
}
.notation-toolbar .score-tool-grid kbd {
  font-size: 10px;
}
.ensemble-scroll {
  padding-top: 46px;
  background: white;
}
.score-status {
  font-size: 10px;
  color: var(--muted);
  padding: 3px 10px;
}
.score-settings {
  max-height: none;
  padding: 0;
  border: 0;
}
.selection-inspector {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  padding: 8px;
}
.score-layout {
  min-height: 420px;
  flex: 1;
}
.score-workspace {
  min-height: 320px;
}
.ensemble-part-name {
  padding: 3px 12px;
}
.score-parts button {
  font-size: 11px;
}
.score-view-tools label {
  font-size: 10px;
  gap: 3px;
}
@media (pointer: coarse) {
  .notation-toolbar button,
  .score-view-tools button {
    min-height: 44px;
    min-width: 40px;
  }
  .ensemble-scroll {
    padding-top: 82px;
  }
}

.score-workspace {
  --panel: #fff;
  --white: #111;
  --text: #111;
  --muted: #526267;
  --line: #d9e0e1;
  --cyan: #087f8c;
  --amber: #785600;
  color: #111;
  background: white;
}
.notation-toolbar {
  top: 8px;
  left: 50%;
  transform: translateX(-50%);
  width: max-content;
  max-width: calc(100% - 16px);
  justify-content: center;
  background: #fffffff5;
  border-color: #ccd6d8;
  box-shadow: 0 2px 8px #0002;
}
.notation-toolbar button {
  color: #111;
  background: #fff;
}
.notation-toolbar button[aria-pressed='true'] {
  background: #e4f3f2;
  color: #075c65;
}
.notation-toolbar :deep(button),
.notation-toolbar :deep(.score-import) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-height: 29px;
  min-width: 27px;
  padding: 2px 5px;
  border: 1px solid #d9e0e1;
  border-radius: 4px;
  color: #111;
  background: white;
  font-size: 11px;
  cursor: pointer;
}
.ensemble-scroll {
  padding-top: 48px;
}
.ensemble-part-name {
  background: #fff;
  color: #111;
  border-bottom: 1px solid #edf0f0;
  padding: 1px 8px;
  min-height: 20px;
  font-size: 11px;
}
.ensemble-part-name button {
  color: #111;
}
.ensemble-part {
  border-color: #e2e6e6;
}
.score-footer {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 3px 8px;
  border-top: 1px solid var(--line);
  background: #fff;
  color: #526267;
  font-size: 10px;
  flex-shrink: 0;
}
.score-footer label {
  display: flex;
  align-items: center;
  gap: 4px;
}
.score-footer input[type='range'] {
  width: 70px;
  padding: 0;
}
.midi-entry-status {
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #087f8c;
}
.score-status {
  margin-right: auto;
  padding: 0;
  font-size: 10px;
}
.score-footer :deep(button) {
  padding: 2px 6px;
  min-height: 24px;
  color: #111;
  background: #fff;
  font-size: 10px;
}
.score-footer :deep(.mode-pill) {
  padding: 2px 5px;
  color: #526267;
  border-color: #d9e0e1;
}
.performance-score-options {
  padding: 0;
  border: 0;
  font-size: 10px;
}
.score-workspace :deep(input:not([type='range']):not([type='checkbox'])),
.score-workspace :deep(select) {
  background: #fff;
  color: #111;
  border-color: #cbd5d7;
}
.score-workspace :deep(.midi-header) {
  background: #fff;
  color: #111;
}
.score-workspace :deep(.midi-label) {
  color: #111;
}
@media (pointer: coarse) {
  .notation-toolbar :deep(button),
  .notation-toolbar :deep(.score-import) {
    min-height: 44px;
    min-width: 40px;
  }
  .ensemble-scroll {
    padding-top: 96px;
  }
}
.score-workspace {
  --violet: #7054a5;
}
.score-workspace :deep(.midi-lanes) {
  background: #fff;
}
.score-workspace :deep(.midi-label) {
  background: #fff;
}
.score-workspace :deep(.midi-lane) {
  border-color: #d9e0e1;
}
</style>
