<script setup lang="ts">
import { computed, inject, onMounted, onBeforeUnmount, onBeforeUpdate, onUpdated, ref, watch } from 'vue'
import { Armchair, CircleStop, Gauge, Play, Plus, Radio, Repeat2, Trash2, X } from '@lucide/vue'
import { conductorMidiKey } from '../conductorMidi'
import { newId } from '../id'
import type { ConductedSet, Member, PartPlayback, Project } from '../types'

const props = defineProps<{
  project: Project
  members: Member[]
  playback: PartPlayback[]
  editable: boolean
  canCue: boolean
  canBind: boolean
  localOwner: boolean
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
const midi = inject(conductorMidiKey)!
const bindMode=ref(false), learning=ref(''), token=ref(''), source=ref('any'), device=ref('')
const root=ref<HTMLElement>()
const midiStatus=computed(()=>learning.value ? `Waiting for MIDI · ${targetName(learning.value)}` : midi.status.value)
const bindings=computed(()=>layout.value.midi_bindings||[])
const devices=computed(()=>source.value==='local'?midi.local.value.map(d=>({id:d.id,name:d.name})):source.value==='server'?midi.server.value.map(id=>({id,name:id})):[])
watch(source,()=>{device.value='';cancelLearn()})
watch(device,()=>cancelLearn())
watch(()=>midi.selectedSet.value,id=>{if(id)selectedSet.value=id},{immediate:true})
watch(()=>midi.lastEvent.value,event=>{
  if(event.type==='conductor_midi_learned'&&event.token===token.value){learning.value='';token.value=''}
  if(event.type==='conductor_midi_error')endBind()
})
function targetName(key:string){const [action,...rest]=key.split(':');const id=rest.join(':');return action==='part'||action==='arm'?`${action==='arm'?'Arm ':''}${props.project.parts.find(p=>p.id===id)?.name||id}`:action==='set'?`Set: ${sets.value.find(s=>s.id===id)?.name||id}`:({select_set:'Set selector',next_set:'Next set',play:'Play armed',repeat:'Play + repeat',stop:'Stop all',dynamic:'Group dynamic'} as Record<string,string>)[action!]||key}
function cancelLearn(){if(token.value)midi.send({type:'conductor_midi_cancel'});learning.value='';token.value=''}
function endBind(){cancelLearn();if(bindMode.value)midi.send({type:'conductor_midi_mode',enabled:false});bindMode.value=false}
function toggleBind(){if(bindMode.value)endBind();else {bindMode.value=midi.send({type:'conductor_midi_mode',enabled:true});midi.send({type:'conductor_midi_inventory'})}}
function capture(event:Event){
  if(!bindMode.value)return
  const target=(event.target as HTMLElement).closest<HTMLElement>('[data-midi]')
  if(!target)return
  event.preventDefault();event.stopPropagation();event.stopImmediatePropagation()
  if(event.type!=='click')return
  cancelLearn()
  const key=target.dataset.midi!;const [action,...rest]=key.split(':')
  const nextToken=newId()
  if(midi.send({type:'conductor_midi_learn',token:nextToken,binding:{action,target:rest.join(':'),source:source.value,device:device.value,status:176,control:0}})){learning.value=key;token.value=nextToken}
}
function outside(event:PointerEvent){if(bindMode.value&&!(event.target as HTMLElement).closest('[data-midi],.midi-panel'))endBind()}
function keydown(event:KeyboardEvent){if(event.key==='Escape'&&bindMode.value){event.preventDefault();event.stopImmediatePropagation();endBind()}}
function chooseSet(id:string){selectedSet.value=id;if(props.canBind)midi.send({type:'conductor_midi_set',selected_set:id})}
function nextSet(){if(sets.value.length)chooseSet(sets.value[(sets.value.findIndex(s=>s.id===selectedSet.value)+1)%sets.value.length]!.id)}
function bindingDevice(event:Event,action:string,target:string){const choice=JSON.parse((event.target as HTMLSelectElement).value);midi.send({type:'conductor_midi_device',action,target,...choice})}
onMounted(()=>{window.addEventListener('pointerdown',outside,true);window.addEventListener('keydown',keydown,true);midi.send({type:'conductor_midi_inventory'})})
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
let suppressClickUntil=0
let drag: { kind: 'set' | 'part'; id: string; x: number; y: number; moved: boolean } | null = null
function dragStart(event: PointerEvent, kind: 'set' | 'part', id: string) {
  if (!props.editable || props.performance || bindMode.value || (event.target as HTMLElement).closest('button,input') && kind==='part') return
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
  if(active?.moved) suppressClickUntil=performance.now()+300
  ;(event.currentTarget as HTMLElement).classList.remove('dragging')
  if (!active?.moved) return
  const target = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>(`[data-drag-kind="${active.kind}"]`)
  if (target?.dataset.dragId) move(active.kind, active.id, target.dataset.dragId)
}
function tile(partId: string) {
  if (!props.canCue || !props.active || props.stale) return
  const play = state(partId)
  emit('cue', { action: play?.playing || play?.pending?.[1] ? 'stop' : 'start', parts: [partId], count_in_pulses: layout.value.count_in_pulses })
}
const tileTimers = new Map<string,number>()
function tileGesture(event:MouseEvent, partId:string) {
  if(performance.now()<suppressClickUntil||bindMode.value)return
  const pending = tileTimers.get(partId)
  if (pending) clearTimeout(pending)
  if (event.detail >= 2) { tileTimers.delete(partId);arm(partId);return }
  tileTimers.set(partId,window.setTimeout(()=>{tileTimers.delete(partId);tile(partId)},230))
}
function arm(partId: string) {
  if(!props.canCue||!props.active||props.stale)return
  const play = state(partId)
  emit('cue', { action: play?.armed ? 'unarm' : 'arm', parts: [partId] })
}
function dynamic(partId: string, event: Event) {
  emit('cue', { action: 'dynamic', parts: [partId], value: Number((event.target as HTMLInputElement).value) })
}
function returnToScore(partId: string) { emit('cue', { action: 'dynamic', parts: [partId], value: null }) }

onBeforeUnmount(() => { endBind();for(const timer of tileTimers.values())clearTimeout(timer);window.removeEventListener('pointerdown',outside,true);window.removeEventListener('keydown',keydown,true) })
</script>

<template>
  <main ref="root" class="conductor" :class="{ performance, 'bind-mode':bindMode }" @click.capture="capture" @pointerdown.capture="capture">
    <aside class="set-rail">
      <header><div><small>CONDUCTED SETS</small><strong>{{ project.name }}</strong></div><button v-if="editable&&!performance" class="icon-button" aria-label="Add set" @click="addSet"><Plus :size="18" /></button></header>
      <button v-for="set in sets" :key="set.id" class="set-button" :class="{ selected:set.id===selectedSet,learning:learning===`set:${set.id}` }" :data-midi="`set:${set.id}`" :data-conductor-flip="`set:${set.id}`" :data-drag-kind="editable&&!performance?'set':undefined" :data-drag-id="set.id" @pointerdown="dragStart($event,'set',set.id)" @pointermove="dragMove" @pointerup="dragEnd" @click="chooseSet(set.id)">
        <span><i v-if="setCounts(set).playing" class="set-live"></i><i v-if="setCounts(set).queued" class="set-queued"></i></span><strong>{{set.name}}</strong><em v-if="setCounts(set).playing">{{setCounts(set).playing}}</em>
      </button>
      <button class="next-set" data-midi="next_set" :class="{learning:learning==='next_set'}" @click="nextSet">Next set</button>
      <button v-if="bindMode" class="next-set" data-midi="select_set" :class="{learning:learning==='select_set'}">Bind set slider / knob</button>
      <div v-if="current&&editable&&!performance" class="set-edit"><button @click="renameSet(current)">Rename</button><button @click="removeSet(current)"><Trash2 :size="14"/> Remove</button></div>
      <section v-if="editable&&!performance" class="pulse-settings"><label>Count-in pulses<input type="number" min="0" max="32" :value="layout.count_in_pulses" @change="setting('count_in_pulses',Number(($event.target as HTMLInputElement).value))"></label><label>Global pulse<select :value="layout.pulse_unit" @change="setting('pulse_unit',Number(($event.target as HTMLSelectElement).value))"><option v-for="unit in [1,2,4,8,16,32]" :key="unit" :value="unit">1 / {{unit}}</option></select></label></section>
    </aside>

    <section class="part-board">
      <header><div><small>{{ performance?'LIVE SET':'LAYOUT EDITOR' }}</small><h2>{{current?.name||'Add a set to begin'}}</h2></div><button v-if="!performance" class="button primary" :disabled="!active" @click="emit('enter')"><Radio :size="16"/> Perform</button></header>
      <div v-if="current" class="part-grid">
        <article v-for="part in parts" :key="part.id" class="part-tile" :class="{playing:state(part.id)?.playing,armed:state(part.id)?.armed,queued:state(part.id)?.pending||state(part.id)?.queue_position,editable:editable&&!performance,learning:learning===`part:${part.id}`}" :style="{'--progress-left':`${progress(part.id)*100}%`}" :data-midi="`part:${part.id}`" tabindex="0" role="button" :aria-label="`${part.name}: ${status(part.id)}`" @keydown.enter.prevent="($event.currentTarget as HTMLElement).click()" @keydown.space.prevent="($event.currentTarget as HTMLElement).click()" :data-conductor-flip="`part:${part.id}`" :data-drag-kind="editable&&!performance?'part':undefined" :data-drag-id="part.id" @pointerdown="dragStart($event,'part',part.id)" @pointermove="dragMove" @pointerup="dragEnd" @click="tileGesture($event,part.id)">
          <i v-if="state(part.id)?.playing" class="part-progress" aria-hidden="true"></i>
          <div class="part-state"><span>{{status(part.id)}}</span><Repeat2 v-if="state(part.id)?.repeating" :size="15"/></div>
          <h3>{{part.name}}</h3><p>{{performer(part.performer)}}</p>
          <div class="tile-actions"><button class="arm-button" :data-midi="`arm:${part.id}`" :class="{learning:learning===`arm:${part.id}`}" :disabled="(!active||stale||!canCue)&&!bindMode" :aria-pressed="!!state(part.id)?.armed" @click.stop="arm(part.id)"><Armchair :size="17"/> {{state(part.id)?.armed?'UNARM':'ARM'}}</button></div>
          <div class="dynamic" @pointerdown.stop @click.stop><Gauge :size="15"/><input type="range" :disabled="!active||stale||!canCue||bindMode" min="1" max="127" :value="state(part.id)?.dynamic_override??96" :aria-label="`${part.name} live dynamic`" @input="dynamic(part.id,$event)"><button v-if="state(part.id)?.dynamic_override!=null" title="Return to score dynamics" @click="returnToScore(part.id)"><X :size="14"/></button></div>
          <button v-if="editable&&!performance" class="remove-tile" aria-label="Remove from set" @click.stop="toggleMembership(part)"><X :size="16"/></button>
        </article>
      </div>
      <section v-if="current&&editable&&!performance" class="part-picker"><h3>Parts in this project</h3><button v-for="part in project.parts" :key="part.id" :class="{included:current.parts.includes(part.id)}" @click="toggleMembership(part)">{{part.name}}<span>{{current.parts.includes(part.id)?'Included':'Add'}}</span></button></section>
    </section>

    <aside class="cue-deck">
      <p v-if="!active" class="cue-hint">Start the project with Play to test cues here.</p>
      <div><small>GLOBAL PULSE</small><strong>♩ {{project.bpm}}</strong><span>{{layout.count_in_pulses}} pulse count-in</span></div>
      <button data-midi="play" :class="{learning:learning==='play'}" class="cue-play" :disabled="(!active||stale||!canCue)&&!bindMode" @click="emit('cue',{action:'start',parts:[],repeat:false})"><Play :size="32" fill="currentColor"/><span>PLAY ARMED</span></button>
      <button data-midi="repeat" :class="{learning:learning==='repeat'}" class="cue-repeat" :disabled="(!active||stale||!canCue)&&!bindMode" @click="emit('cue',{action:'start',parts:[],repeat:true})"><Repeat2 :size="28"/><span>PLAY + REPEAT</span></button>
      <label data-midi="dynamic" :class="{learning:learning==='dynamic'}" class="group-dynamic"><span>ARMED / PLAYING DYNAMIC</span><input type="range" min="1" max="127" :disabled="(!active||stale||!canCue)&&!bindMode" value="96" @input="emit('cue',{action:'dynamic',parts:[],value:Number(($event.target as HTMLInputElement).value)})"></label>
      <button :disabled="!active||stale||!canCue||bindMode" @click="emit('cue',{action:'unarm',parts:playback.filter(item=>item.armed).map(item=>item.id)})">Clear armed</button>
      <button data-midi="stop" :class="{learning:learning==='stop'}" class="cue-stop" :disabled="(!active||stale||!canCue)&&!bindMode" @click="emit('cue',{action:'stop',parts:project.parts.map(part=>part.id)})"><CircleStop :size="20"/> Stop all next pulse</button>
      <section v-if="canBind" class="midi-panel">
        <small>MIDI CONTROL</small>
        <button class="bind-toggle" :aria-pressed="bindMode" @click="toggleBind">{{bindMode?'End bind mode':'Bind MIDI'}}</button>
        <button v-if="localOwner" :disabled="midi.connecting.value" @click="midi.ready.value?midi.disable():midi.enable()">{{midi.connecting.value?'Connecting local MIDI…':midi.ready.value?'Disconnect local MIDI':'Connect local MIDI'}}</button>
        <p v-else>Local MIDI comes from the designated conductor.</p>
        <template v-if="bindMode">
          <p>Select a highlighted control, then move or press the MIDI control. Escape or click outside to finish.</p>
          <label>Source<select v-model="source" aria-label="MIDI binding source"><option value="any">First local or server input</option><option value="local">Conductor’s local MIDI</option><option value="server">Server MIDI</option></select></label>
          <label v-if="source!=='any'">Device<select v-model="device" aria-label="MIDI binding device"><option value="">First device received</option><option v-for="d in devices" :key="d.id" :value="d.id">{{d.name}}</option></select></label>
          <button v-if="learning" @click="cancelLearn">Cancel pending binding</button>
        </template>
        <p role="status">{{midiStatus}}</p>
        <details v-if="bindings.length"><summary>Bindings ({{bindings.length}})</summary><div v-for="b in bindings" :key="`${b.action}:${b.target}`" class="binding-row">
          <strong>{{targetName(`${b.action}:${b.target}`)}}</strong><span>Channel {{(b.status & 15)+1}} · {{b.status >> 4===11?'CC':b.status >> 4===9?'Note':'Control'}} {{b.control}}</span>
          <select :aria-label="`${targetName(`${b.action}:${b.target}`)} MIDI device`" :value="JSON.stringify({source:b.source,device:b.device})" @change="bindingDevice($event,b.action,b.target)">
            <option :value="JSON.stringify({source:b.source,device:b.device})">{{b.source==='local'?'Local':'Server'}} · {{midi.local.value.find(d=>d.id===b.device)?.name||b.device}}</option>
            <optgroup label="Conductor’s local MIDI"><option v-for="d in midi.local.value.filter(d=>b.source!=='local'||d.id!==b.device)" :key="d.id" :value="JSON.stringify({source:'local',device:d.id})">{{d.name}}</option></optgroup>
            <optgroup label="Server MIDI"><option v-for="d in midi.server.value.filter(d=>b.source!=='server'||d!==b.device)" :key="d" :value="JSON.stringify({source:'server',device:d})">{{d}}</option></optgroup>
          </select><button @click="midi.send({type:'conductor_midi_remove',action:b.action,target:b.target})">Remove binding</button>
        </div></details>
      </section>
    </aside>
  </main>
</template>

<style scoped>
.group-dynamic{display:grid;gap:7px;padding:10px;color:var(--muted);font-size:8px;letter-spacing:1px}.group-dynamic input{width:100%;accent-color:var(--violet)}
.pulse-settings{display:grid;gap:10px;margin-top:18px;padding:14px 8px;border-top:1px solid var(--line)}.pulse-settings label{color:var(--muted);font-size:9px;letter-spacing:.5px}.pulse-settings input,.pulse-settings select{display:block;width:100%;margin-top:5px}
.conductor{display:grid;grid-template-columns:190px minmax(0,1fr) 220px;height:100%;min-height:0;background:#111819;color:var(--white)}.conductor.performance{grid-template-columns:190px minmax(0,1fr) 220px}.set-rail{padding:18px 12px;border-right:1px solid var(--line);background:#151d1f;overflow:auto}.set-rail header,.part-board>header{display:flex;align-items:center;justify-content:space-between;gap:12px}.set-rail header{padding:5px 8px 18px}.set-rail small,.part-board small,.cue-deck small{display:block;color:var(--cyan);font-size:9px;letter-spacing:1.4px}.set-rail strong{display:block;margin-top:4px}.set-button{display:grid;grid-template-columns:22px 1fr auto;align-items:center;width:100%;min-height:52px;margin:4px 0;padding:10px;border:1px solid transparent;border-radius:8px;text-align:left;transition:transform .16s,background .16s,border-color .16s}.set-button.selected{background:#233033;border-color:#43585b}.set-button.dragging,.part-tile.dragging{transform:scale(1.035);opacity:.72}.set-button i{display:inline-block;width:8px;height:8px;border-radius:50%}.set-live{background:#5de09a;box-shadow:0 0 8px #5de09a}.set-queued{margin-left:-3px;background:var(--amber)}.set-button em{display:grid;place-items:center;width:22px;height:22px;border-radius:50%;background:#244a3d;color:#9af3bd;font-size:10px;font-style:normal}.set-edit{display:flex;gap:5px;padding:8px}.set-edit button{display:flex;gap:4px;align-items:center;color:var(--muted);font-size:10px}.part-board{padding:24px;overflow:auto}.part-board>header{margin-bottom:20px}.part-board h2{margin-top:4px;font-size:24px}.part-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:14px}.part-tile{position:relative;isolation:isolate;min-height:218px;display:flex;flex-direction:column;gap:9px;overflow:hidden;padding:16px;border:1px solid #344447;border-radius:13px;background:#1a2325;box-shadow:0 8px 22px #0003;transition:transform .18s,border-color .18s,background .18s}.part-tile.editable{touch-action:none}.part-tile.armed{border:2px solid var(--amber);background:#29271f}.part-tile.queued{border-color:var(--cyan)}.part-tile.playing{border-color:#61dfaa;background:#183029;box-shadow:0 0 24px #37d99718}.part-progress{position:absolute;z-index:-1;top:0;bottom:0;left:0!important;width:var(--progress-left);background:#52df8c26;border-right:2px solid #91ffd0;transition:none}.part-state{min-height:22px;padding-right:25px;display:flex;justify-content:space-between;color:var(--amber);font-size:10px;letter-spacing:1px;text-transform:uppercase}.part-tile.playing .part-state{color:#76efb3}.part-tile h3{margin:0;font-size:20px;line-height:1.25;overflow-wrap:anywhere}.part-tile p{margin:0;color:var(--muted);font-size:11px;overflow-wrap:anywhere}.tile-actions{display:flex;justify-content:flex-end;margin-top:auto;padding-top:8px}.arm-button{display:flex;align-items:center;gap:5px;min-height:38px;padding:0 10px;border:1px solid #596064;border-radius:7px;background:#11191b;color:#bfcacc;font-size:10px}.part-tile.armed .arm-button{border-color:var(--amber);color:var(--amber)}.dynamic{min-height:28px;display:flex;align-items:center;gap:6px}.dynamic input{min-width:0;flex:1;accent-color:var(--violet)}.remove-tile{position:absolute;top:8px;right:8px;width:34px;height:34px}.part-picker{margin-top:28px;padding-top:20px;border-top:1px solid var(--line)}.part-picker h3{margin-bottom:10px}.part-picker button{margin:4px;padding:8px 11px;border:1px solid var(--line);border-radius:6px}.part-picker button span{margin-left:8px;color:var(--muted);font-size:9px}.part-picker button.included{border-color:var(--cyan);color:var(--cyan)}.cue-deck{display:flex;min-height:0;flex-direction:column;gap:10px;padding:20px 12px;border-left:1px solid var(--line);background:#151d1f;overflow-y:auto}.cue-deck>div{padding:12px}.cue-deck strong,.cue-deck span{display:block}.cue-deck strong{margin:6px 0;font-size:25px}.cue-deck>button{display:flex;align-items:center;justify-content:center;gap:9px;min-height:52px;flex-shrink:0;border:1px solid #405154;border-radius:9px;background:#202a2c}.cue-deck .cue-play{min-height:104px;background:var(--cyan);color:#092023}.cue-deck .cue-repeat{min-height:70px;border-color:var(--violet);color:#d1bfff}.cue-deck .cue-stop{margin-top:auto;border-color:#804d49;color:#ff9c92}.midi-panel{padding-top:14px;border-top:1px solid var(--line);flex-shrink:0}.midi-panel>button{margin:9px 0;color:var(--cyan)}.midi-panel div{display:flex;flex-wrap:wrap;gap:4px}.midi-panel div button{padding:5px;border:1px solid var(--line);border-radius:4px;font-size:9px}.midi-panel button.learning{border-color:var(--amber);color:var(--amber)}
.next-set{width:100%;min-height:40px;margin:6px 0;border:1px solid var(--line);border-radius:7px}.midi-panel p,.cue-hint{font-size:11px;line-height:1.5;color:var(--muted)}.midi-panel label{display:grid;gap:5px;margin:10px 0;font-size:11px}.midi-panel select{width:100%;min-width:0}.midi-panel .bind-toggle{display:block;min-height:40px;padding:8px 12px;border:1px solid var(--amber);border-radius:8px;color:var(--amber)}.bind-mode [data-midi]{outline:1px dashed var(--amber);outline-offset:-3px;cursor:crosshair}.bind-mode [data-midi].learning{outline:3px solid var(--amber);box-shadow:0 0 18px #f1b65040}.midi-panel .binding-row{display:grid;gap:6px;margin-top:12px;padding-bottom:12px;border-bottom:1px solid var(--line);font-size:11px}.binding-row strong{font-size:12px}.binding-row span{color:var(--muted)}
@media(max-width:850px){.conductor,.conductor.performance{grid-template-columns:150px minmax(0,1fr)}.cue-deck{position:fixed;right:10px;bottom:76px;z-index:20;width:190px;max-height:65vh;box-shadow:0 8px 30px #0008}.part-board{padding:14px}.part-grid{grid-template-columns:repeat(2,minmax(140px,1fr))}}@media(prefers-reduced-motion:reduce){.set-button,.part-tile,.part-progress{transition:none}}
</style>
