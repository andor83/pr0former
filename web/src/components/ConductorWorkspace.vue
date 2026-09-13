<script setup lang="ts">
import { computed, onBeforeUnmount, onBeforeUpdate, onUpdated, ref, watch } from 'vue'
import { Armchair, CircleStop, Gauge, Play, Plus, Radio, Repeat2, Trash2, X } from '@lucide/vue'
import { newId } from '../id'
import type { ConductedSet, Member, PartPlayback, Project } from '../types'

const props = defineProps<{
  project: Project
  members: Member[]
  playback: PartPlayback[]
  editable: boolean
  performance?: boolean
  active: boolean
  stale: boolean
}>()
const emit = defineEmits<{
  save: [project: Project]
  cue: [request: { action: string; parts?: string[]; repeat?: boolean; count_in_pulses?: number; value?: number | null }]
  midi: [message: Record<string, unknown>]
  enter: []
}>()

const selectedSet = ref('')
type MidiAction = 'play' | 'repeat' | 'stop' | 'dynamic'
const learning = ref<MidiAction | null>(null)
const midiStatus = ref('MIDI off')
let midiAccess: any = null
let dynamicFrame = 0, pendingDynamic = 0
const flipPositions = new Map<string, DOMRect>()
onBeforeUpdate(() => {
  flipPositions.clear()
  for (const element of document.querySelectorAll<HTMLElement>('[data-conductor-flip]')) {
    if (element.dataset.conductorFlip) flipPositions.set(element.dataset.conductorFlip, element.getBoundingClientRect())
  }
})
onUpdated(() => {
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
  requestAnimationFrame(() => {
    for (const element of document.querySelectorAll<HTMLElement>('[data-conductor-flip]')) {
      const previous = flipPositions.get(element.dataset.conductorFlip || '')
      if (!previous) continue
      const current = element.getBoundingClientRect()
      const x = previous.left - current.left, y = previous.top - current.top
      if (Math.abs(x) < 1 && Math.abs(y) < 1) continue
      element.animate(
        [{ transform:`translate(${x}px,${y}px) scale(.97)` }, { transform:'translate(0,0) scale(1)' }],
        { duration:260, easing:'cubic-bezier(.2,.8,.2,1)' },
      )
    }
  })
})

const layout = computed(() => props.project.conducted || { count_in_pulses: 4, pulse_unit: 4, sets: [] })
const sets = computed(() => layout.value.sets || [])
watch(sets, value => {
  if (!value.some(set => set.id === selectedSet.value)) selectedSet.value = value[0]?.id || ''
}, { immediate: true })
const current = computed(() => sets.value.find(set => set.id === selectedSet.value))
const state = (id: string) => props.playback.find(item => item.id === id)
const parts = computed(() => (current.value?.parts || []).map(id => props.project.parts.find(part => part.id === id)).filter(Boolean) as Project['parts'])
const performer = (id: string | null) => props.members.find(member => member.id === id)?.username || 'Electronic / unassigned'
const progress = (id: string) => {
  const play = state(id), part = props.project.parts.find(item => item.id === id)
  return play?.playing && part ? Math.min(1, Math.max(0, play.position / part.loop_beats)) : 0
}
const status = (id: string) => {
  const play = state(id)
  if (play?.count_in_remaining) return `Count-in ${play.count_in_remaining}`
  if (play?.pending) return play.pending[1] ? 'Queued' : 'Stopping'
  if (play?.queue_position) return `Next ${play.queue_position}`
  if (play?.playing) return play.repeating ? 'Playing · repeat' : 'Playing'
  if (play?.armed) return 'Armed'
  return 'Idle'
}
const setCounts = (set: ConductedSet) => {
  const states = set.parts.map(state)
  return { playing: states.filter(item => item?.playing).length, queued: states.filter(item => item?.armed || item?.pending || item?.queue_position).length }
}

function copyProject() { const next=JSON.parse(JSON.stringify(props.project)) as Project;next.conducted ||= {count_in_pulses:4,pulse_unit:4,sets:[]};return next }
function save(mutator: (project: Project) => void) { const next = copyProject(); mutator(next); emit('save', next) }
function addSet() {
  const name = window.prompt('Set name', `Set ${sets.value.length + 1}`)?.trim()
  if (!name) return
  save(project => { project.conducted!.sets.push({ id: newId(), name, parts: [] }) })
}
function setting(key: 'count_in_pulses'|'pulse_unit', value: number) { save(project => { project.conducted![key] = value }) }
function renameSet(set: ConductedSet) {
  const name = window.prompt('Set name', set.name)?.trim()
  if (name) save(project => { project.conducted!.sets.find(item => item.id === set.id)!.name = name })
}
function removeSet(set: ConductedSet) {
  if (!window.confirm(`Remove ${set.name}? Parts and notation are not deleted.`)) return
  save(project => { project.conducted!.sets = project.conducted!.sets.filter(item => item.id !== set.id) })
}
function toggleMembership(part: Project['parts'][number]) {
  if (!current.value) return
  save(project => {
    const set = project.conducted!.sets.find(item => item.id === current.value!.id)!
    set.parts = set.parts.includes(part.id) ? set.parts.filter(id => id !== part.id) : [...set.parts, part.id]
  })
}
function move(kind: 'set' | 'part', from: string, to: string) {
  if (from === to) return
  save(project => {
    const list = kind === 'set' ? project.conducted!.sets : project.conducted!.sets.find(set => set.id === current.value?.id)!.parts
    const fromIndex = list.findIndex((item: any) => (typeof item === 'string' ? item : item.id) === from)
    const toIndex = list.findIndex((item: any) => (typeof item === 'string' ? item : item.id) === to)
    if (fromIndex < 0 || toIndex < 0) return
    const [item] = list.splice(fromIndex, 1)
    list.splice(toIndex, 0, item as never)
  })
}
let drag: { kind: 'set' | 'part'; id: string; x: number; y: number; moved: boolean } | null = null
function dragStart(event: PointerEvent, kind: 'set' | 'part', id: string) {
  if (!props.editable || props.performance) return
  drag = { kind, id, x: event.clientX, y: event.clientY, moved: false }
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
}
function dragMove(event: PointerEvent) {
  if (!drag) return
  drag.moved ||= Math.hypot(event.clientX - drag.x, event.clientY - drag.y) > 8
  if (drag.moved) (event.currentTarget as HTMLElement).classList.add('dragging')
}
function dragEnd(event: PointerEvent) {
  const active = drag; drag = null
  ;(event.currentTarget as HTMLElement).classList.remove('dragging')
  if (!active?.moved) return
  const target = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>(`[data-drag-kind="${active.kind}"]`)
  if (target?.dataset.dragId) move(active.kind, active.id, target.dataset.dragId)
}
function tile(partId: string) {
  if (!props.performance || !props.active || props.stale) return
  const play = state(partId)
  emit('cue', { action: play?.playing || play?.pending?.[1] ? 'stop' : 'start', parts: [partId], count_in_pulses: layout.value.count_in_pulses })
}
const tileTimers = new Map<string,number>()
function tileGesture(event:MouseEvent, partId:string) {
  const pending = tileTimers.get(partId)
  if (pending) clearTimeout(pending)
  if (event.detail >= 2) { tileTimers.delete(partId);arm(partId);return }
  tileTimers.set(partId,window.setTimeout(()=>{tileTimers.delete(partId);tile(partId)},230))
}
function arm(partId: string) {
  const play = state(partId)
  emit('cue', { action: play?.armed ? 'unarm' : 'arm', parts: [partId] })
}
function dynamic(partId: string, event: Event) {
  emit('cue', { action: 'dynamic', parts: [partId], value: Number((event.target as HTMLInputElement).value) })
}
function returnToScore(partId: string) { emit('cue', { action: 'dynamic', parts: [partId], value: null }) }

type MidiBinding = { status: number; data1: number }
const bindingKey = computed(() => `pr0former.conductor-midi.${props.project.id}`)
function bindings(): Partial<Record<MidiAction, MidiBinding>> {
  try { return JSON.parse(localStorage.getItem(bindingKey.value) || '{}') } catch { return {} }
}
function midiMessage(event: any) {
  const [raw = 0, data1 = 0, data2 = 0] = [...event.data]
  const status = raw & 0xf0
  if ((status === 0x90 && data2 === 0) || status === 0x80) return
  if (learning.value) {
    const all = bindings(); all[learning.value] = { status, data1 }; localStorage.setItem(bindingKey.value, JSON.stringify(all))
    midiStatus.value = `${learning.value} mapped`; learning.value = null; return
  }
  const action = Object.entries(bindings()).find(([, binding]) => binding?.status === status && binding.data1 === data1)?.[0]
  if (status === 0xb0 && data2 < 64 && action !== 'dynamic') return
  if (action === 'play') emit('cue', { action: 'start', parts: [], repeat: false })
  if (action === 'repeat') emit('cue', { action: 'start', parts: [], repeat: true })
  if (action === 'stop') emit('cue', { action: 'stop', parts: props.project.parts.map(part => part.id) })
  if (action === 'dynamic') { pendingDynamic=data2;if(!dynamicFrame)dynamicFrame=requestAnimationFrame(()=>{dynamicFrame=0;emit('cue',{action:'dynamic',parts:[],value:pendingDynamic})}) }
}
async function enableMidi() {
  try {
    const request = (navigator as any).requestMIDIAccess
    if (!request) throw new Error('Web MIDI unavailable')
    midiAccess = await request.call(navigator)
    const connect = () => {
      const devices = [...midiAccess.inputs.values()].map((input:any) => input.name || input.id).slice(0,32)
      emit('midi', { type:'midi_devices', devices })
      for (const input of midiAccess.inputs.values()) input.onmidimessage = midiMessage
    }
    connect(); midiAccess.onstatechange = connect; midiStatus.value = 'MIDI ready'
  } catch (error) { midiStatus.value = error instanceof Error ? error.message : String(error) }
}
onBeforeUnmount(() => { cancelAnimationFrame(dynamicFrame);for(const timer of tileTimers.values())clearTimeout(timer);if (midiAccess) for (const input of midiAccess.inputs.values()) input.onmidimessage = null })
</script>

<template>
  <main class="conductor" :class="{ performance }">
    <aside class="set-rail">
      <header><div><small>CONDUCTED SETS</small><strong>{{ project.name }}</strong></div><button v-if="editable&&!performance" class="icon-button" aria-label="Add set" @click="addSet"><Plus :size="18" /></button></header>
      <button v-for="set in sets" :key="set.id" class="set-button" :class="{ selected:set.id===selectedSet }" :data-conductor-flip="`set:${set.id}`" :data-drag-kind="editable&&!performance?'set':undefined" :data-drag-id="set.id" @pointerdown="dragStart($event,'set',set.id)" @pointermove="dragMove" @pointerup="dragEnd" @click="selectedSet=set.id">
        <span><i v-if="setCounts(set).playing" class="set-live"></i><i v-if="setCounts(set).queued" class="set-queued"></i></span><strong>{{set.name}}</strong><em v-if="setCounts(set).playing">{{setCounts(set).playing}}</em>
      </button>
      <div v-if="current&&editable&&!performance" class="set-edit"><button @click="renameSet(current)">Rename</button><button @click="removeSet(current)"><Trash2 :size="14"/> Remove</button></div>
      <section v-if="editable&&!performance" class="pulse-settings"><label>Count-in pulses<input type="number" min="0" max="32" :value="layout.count_in_pulses" @change="setting('count_in_pulses',Number(($event.target as HTMLInputElement).value))"></label><label>Global pulse<select :value="layout.pulse_unit" @change="setting('pulse_unit',Number(($event.target as HTMLSelectElement).value))"><option v-for="unit in [1,2,4,8,16,32]" :key="unit" :value="unit">1 / {{unit}}</option></select></label></section>
    </aside>

    <section class="part-board">
      <header><div><small>{{ performance?'LIVE SET':'LAYOUT EDITOR' }}</small><h2>{{current?.name||'Add a set to begin'}}</h2></div><button v-if="!performance" class="button primary" :disabled="!active" @click="emit('enter')"><Radio :size="16"/> Perform</button></header>
      <div v-if="current" class="part-grid">
        <article v-for="part in parts" :key="part.id" class="part-tile" :class="{playing:state(part.id)?.playing,armed:state(part.id)?.armed,queued:state(part.id)?.pending||state(part.id)?.queue_position,editable:editable&&!performance}" :style="{'--progress-left':`${(1-progress(part.id))*100}%`}" :data-conductor-flip="`part:${part.id}`" :data-drag-kind="editable&&!performance?'part':undefined" :data-drag-id="part.id" @pointerdown="dragStart($event,'part',part.id)" @pointermove="dragMove" @pointerup="dragEnd" @click="tileGesture($event,part.id)">
          <i v-if="state(part.id)?.playing" class="part-progress" aria-hidden="true"></i>
          <div class="part-state"><span>{{status(part.id)}}</span><Repeat2 v-if="state(part.id)?.repeating" :size="15"/></div>
          <h3>{{part.name}}</h3><p>{{performer(part.performer)}}</p>
          <div v-if="performance" class="tile-actions" @click.stop><button class="arm-button" :aria-pressed="!!state(part.id)?.armed" @click="arm(part.id)"><Armchair :size="17"/> {{state(part.id)?.armed?'UNARM':'ARM'}}</button></div>
          <div v-if="performance" class="dynamic" @click.stop><Gauge :size="15"/><input type="range" min="1" max="127" :value="state(part.id)?.dynamic_override??96" :aria-label="`${part.name} live dynamic`" @input="dynamic(part.id,$event)"><button v-if="state(part.id)?.dynamic_override!=null" title="Return to score dynamics" @click="returnToScore(part.id)"><X :size="14"/></button></div>
          <button v-if="editable&&!performance" class="remove-tile" aria-label="Remove from set" @click.stop="toggleMembership(part)"><X :size="16"/></button>
        </article>
      </div>
      <section v-if="current&&editable&&!performance" class="part-picker"><h3>Parts in this project</h3><button v-for="part in project.parts" :key="part.id" :class="{included:current.parts.includes(part.id)}" @click="toggleMembership(part)">{{part.name}}<span>{{current.parts.includes(part.id)?'Included':'Add'}}</span></button></section>
    </section>

    <aside v-if="performance" class="cue-deck">
      <div><small>GLOBAL PULSE</small><strong>♩ {{project.bpm}}</strong><span>{{layout.count_in_pulses}} pulse count-in</span></div>
      <button class="cue-play" :disabled="!active||stale" @click="emit('cue',{action:'start',parts:[],repeat:false})"><Play :size="32" fill="currentColor"/><span>PLAY ARMED</span></button>
      <button class="cue-repeat" :disabled="!active||stale" @click="emit('cue',{action:'start',parts:[],repeat:true})"><Repeat2 :size="28"/><span>PLAY + REPEAT</span></button>
      <label class="group-dynamic"><span>ARMED / PLAYING DYNAMIC</span><input type="range" min="1" max="127" value="96" @input="emit('cue',{action:'dynamic',parts:[],value:Number(($event.target as HTMLInputElement).value)})"></label>
      <button @click="emit('cue',{action:'unarm',parts:playback.filter(item=>item.armed).map(item=>item.id)})">Clear armed</button>
      <button class="cue-stop" :disabled="!active" @click="emit('cue',{action:'stop',parts:project.parts.map(part=>part.id)})"><CircleStop :size="20"/> Stop all next pulse</button>
      <section class="midi-panel"><small>MIDI CONTROL</small><button @click="enableMidi">{{midiStatus}}</button><div><button v-for="action in ['play','repeat','stop','dynamic'] as const" :key="action" :class="{learning:learning===action}" @click="learning=action">Learn {{action}}</button></div></section>
    </aside>
  </main>
</template>

<style scoped>
.group-dynamic{display:grid;gap:7px;padding:10px;color:var(--muted);font-size:8px;letter-spacing:1px}.group-dynamic input{width:100%;accent-color:var(--violet)}
.part-progress{left:var(--progress-left)!important}
.pulse-settings{display:grid;gap:10px;margin-top:18px;padding:14px 8px;border-top:1px solid var(--line)}.pulse-settings label{color:var(--muted);font-size:9px;letter-spacing:.5px}.pulse-settings input,.pulse-settings select{display:block;width:100%;margin-top:5px}
.conductor{display:grid;grid-template-columns:220px minmax(0,1fr);height:100%;min-height:0;background:#111819;color:var(--white)}.conductor.performance{grid-template-columns:190px minmax(0,1fr) 220px}.set-rail{padding:18px 12px;border-right:1px solid var(--line);background:#151d1f;overflow:auto}.set-rail header,.part-board>header{display:flex;align-items:center;justify-content:space-between;gap:12px}.set-rail header{padding:5px 8px 18px}.set-rail small,.part-board small,.cue-deck small{display:block;color:var(--cyan);font-size:9px;letter-spacing:1.4px}.set-rail strong{display:block;margin-top:4px}.set-button{display:grid;grid-template-columns:22px 1fr auto;align-items:center;width:100%;min-height:52px;margin:4px 0;padding:10px;border:1px solid transparent;border-radius:8px;text-align:left;transition:transform .16s,background .16s,border-color .16s}.set-button.selected{background:#233033;border-color:#43585b}.set-button.dragging,.part-tile.dragging{transform:scale(1.035);opacity:.72}.set-button i{display:inline-block;width:8px;height:8px;border-radius:50%}.set-live{background:#5de09a;box-shadow:0 0 8px #5de09a}.set-queued{margin-left:-3px;background:var(--amber)}.set-button em{display:grid;place-items:center;width:22px;height:22px;border-radius:50%;background:#244a3d;color:#9af3bd;font-size:10px;font-style:normal}.set-edit{display:flex;gap:5px;padding:8px}.set-edit button{display:flex;gap:4px;align-items:center;color:var(--muted);font-size:10px}.part-board{padding:24px;overflow:auto}.part-board>header{margin-bottom:20px}.part-board h2{margin-top:4px;font-size:24px}.part-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(180px,1fr));gap:14px}.part-tile{position:relative;isolation:isolate;min-height:180px;overflow:hidden;padding:18px;border:1px solid #344447;border-radius:13px;background:#1a2325;box-shadow:0 8px 22px #0003;transition:transform .18s,border-color .18s,background .18s}.part-tile.editable{touch-action:none}.part-tile.armed{border:2px solid var(--amber);background:#29271f}.part-tile.queued{border-color:var(--cyan)}.part-tile.playing{border-color:#61dfaa;background:#183029;box-shadow:0 0 24px #37d99718}.part-progress{position:absolute;z-index:-1;top:0;bottom:0;left:calc((1 - var(--progress)) * 100%);width:2px;background:#91ffd0;box-shadow:0 0 14px #7dffca;transition:left .05s linear}.part-state{display:flex;justify-content:space-between;color:var(--amber);font-size:10px;letter-spacing:1px;text-transform:uppercase}.part-tile.playing .part-state{color:#76efb3}.part-tile h3{margin-top:32px;font-size:20px}.part-tile p{margin-top:6px;color:var(--muted);font-size:11px}.tile-actions{position:absolute;right:12px;bottom:44px}.arm-button{display:flex;align-items:center;gap:5px;min-height:38px;padding:0 10px;border:1px solid #596064;border-radius:7px;background:#11191b;color:#bfcacc;font-size:10px}.part-tile.armed .arm-button{border-color:var(--amber);color:var(--amber)}.dynamic{position:absolute;right:12px;bottom:10px;left:12px;display:flex;align-items:center;gap:6px}.dynamic input{min-width:0;flex:1;accent-color:var(--violet)}.remove-tile{position:absolute;top:8px;right:8px;width:34px;height:34px}.part-picker{margin-top:28px;padding-top:20px;border-top:1px solid var(--line)}.part-picker h3{margin-bottom:10px}.part-picker button{margin:4px;padding:8px 11px;border:1px solid var(--line);border-radius:6px}.part-picker button span{margin-left:8px;color:var(--muted);font-size:9px}.part-picker button.included{border-color:var(--cyan);color:var(--cyan)}.cue-deck{display:flex;min-height:0;flex-direction:column;gap:10px;padding:20px 12px;border-left:1px solid var(--line);background:#151d1f;overflow-y:auto}.cue-deck>div{padding:12px}.cue-deck strong,.cue-deck span{display:block}.cue-deck strong{margin:6px 0;font-size:25px}.cue-deck>button{display:flex;align-items:center;justify-content:center;gap:9px;min-height:52px;flex-shrink:0;border:1px solid #405154;border-radius:9px;background:#202a2c}.cue-deck .cue-play{min-height:104px;background:var(--cyan);color:#092023}.cue-deck .cue-repeat{min-height:70px;border-color:var(--violet);color:#d1bfff}.cue-deck .cue-stop{margin-top:auto;border-color:#804d49;color:#ff9c92}.midi-panel{padding-top:14px;border-top:1px solid var(--line);flex-shrink:0}.midi-panel>button{margin:9px 0;color:var(--cyan)}.midi-panel div{display:flex;flex-wrap:wrap;gap:4px}.midi-panel div button{padding:5px;border:1px solid var(--line);border-radius:4px;font-size:9px}.midi-panel button.learning{border-color:var(--amber);color:var(--amber)}
@media(max-width:850px){.conductor,.conductor.performance{grid-template-columns:150px minmax(0,1fr)}.cue-deck{position:fixed;right:10px;bottom:76px;z-index:20;width:190px;max-height:65vh;box-shadow:0 8px 30px #0008}.part-board{padding:14px}.part-grid{grid-template-columns:repeat(2,minmax(140px,1fr))}}@media(prefers-reduced-motion:reduce){.set-button,.part-tile,.part-progress{transition:none}}
</style>
