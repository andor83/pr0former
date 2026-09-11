<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, ref, watch } from 'vue'
import type { Part, PartPlayback, Project } from '../types'
import { staves } from '../score'
import { automationValue, neutralLevel, staffDynamicsEvents } from '../scoreRamps'
import { buildConductedLane, type ConductedLaneHistory } from '../conductedLane'

const focusedId = ref('')
const ScoreWorkspace = defineAsyncComponent(() => import('./ScoreWorkspace.vue'))
const props = defineProps<{ project: Project; position:{bar:number;beat:number;beats:number}; userId:string; beats:Record<string,number>; part?: Part; playback?:PartPlayback; playbackAll?:PartPlayback[]; globalBeat?:number; beat: number; meterBeat: number; bpm: number; active: boolean; stale: boolean; running: boolean; status: string; canLaunch: boolean; monitorOpen: boolean }>()
const emit = defineEmits<{ select: [id: string]; launch: [playing: boolean]; exit: []; fullscreen: []; monitor: []; midi: [message:Record<string,unknown>] }>()

const writtenDynamic = computed(() => {
  const part = props.part
  if (!part) return neutralLevel
  const staff = staves(part)[0]
  return staff ? automationValue(staffDynamicsEvents(part, staff), props.beat, neutralLevel) : neutralLevel
})
const effectiveDynamic = computed(() => props.playback?.dynamic_override ?? writtenDynamic.value)
const laneHistory = ref<ConductedLaneHistory[]>([])
watch(() => props.playbackAll, states => {
  if (props.project.mode !== 'conducted') return
  for (const state of states || []) {
    if (!state.playing || laneHistory.value.some(item => item.part === state.id && item.start === state.start)) continue
    laneHistory.value.push({ part: state.id, start: state.start })
  }
  if (laneHistory.value.length > 128) laneHistory.value.splice(0, laneHistory.value.length - 128)
}, { deep: true, immediate: true })
watch(() => props.project.id, () => { laneHistory.value = [] })
const liveLane = computed(() => props.project.mode === 'conducted'
  ? buildConductedLane(props.project, props.userId, props.playbackAll || [], props.globalBeat || 0, laneHistory.value)
  : null)
const displayProject = computed(() => liveLane.value?.project || props.project)
const displayBeats = computed(() => liveLane.value
  ? { [liveLane.value.partId]: liveLane.value.beat }
  : props.beats)
const arriving = ref(false)
let arrivalTimer = 0
watch(() => liveLane.value?.segmentKeys.join('|'), (next, previous) => {
  if (!previous || next === previous) return
  arriving.value = true
  clearTimeout(arrivalTimer)
  arrivalTimer = window.setTimeout(() => { arriving.value = false }, 650)
})
const midiStatus = ref('MIDI off')
let midiAccess: any = null
function announceMidi() {
  const devices = midiAccess ? [...midiAccess.inputs.values()].map((input:any) => input.name || input.id).slice(0,32) : []
  emit('midi', { type:'midi_devices', devices })
  for (const input of midiAccess?.inputs.values() || []) input.onmidimessage = (event:any) => {
    if (!props.active || !props.part || props.project.mode === 'structured') return
    const data = [...event.data].slice(0,3)
    if (data.length === 3) emit('midi', { type:'performance_midi', part:props.part.id, data })
  }
}
async function enableMidi() {
  try {
    const request = (navigator as any).requestMIDIAccess
    if (!request) throw new Error('Web MIDI unavailable')
    midiAccess = await request.call(navigator)
    announceMidi();midiAccess.onstatechange = announceMidi;midiStatus.value = 'MIDI streaming'
  } catch (error) { midiStatus.value = error instanceof Error ? error.message : String(error) }
}
onBeforeUnmount(() => {
  clearTimeout(arrivalTimer)
  if (midiAccess) for (const input of midiAccess.inputs.values()) input.onmidimessage = null
  emit('midi', { type:'performance_midi_panic' })
})
</script>
<template>
  <main class="stage-view" aria-label="Performance stage">
    <header class="stage-header"><div><div class="eyebrow">{{ project.mode }} PERFORMANCE</div><h1>{{ project.name }}</h1></div><div class="stage-buttons"><button v-if="project.mode!=='structured'" class="button" @click="enableMidi">{{midiStatus}}</button><button class="button" :aria-expanded="monitorOpen" @click="$emit('monitor')">Monitor controls</button><button class="button" @click="$emit('fullscreen')">Fullscreen</button><button class="button" @click="$emit('exit')">Exit performance mode</button></div></header>
    <div class="stage-clock" :class="{ unavailable: active && stale }"><div><span>SHARED BAR · BEAT</span><output aria-label="Stage position">{{ position.bar }} : {{ position.beat }}</output></div><div><span>♩ BPM</span><strong>{{ bpm }}</strong></div><p role="status">{{ !active ? 'Waiting for show activation' : stale ? 'Timing unavailable — score held' : running ? 'Transport playing' : 'Transport stopped / paused' }}</p></div>
    <div class="stage-part"><output aria-label="Stage part status" aria-live="polite">{{ status }}</output><div v-if="project.mode !== 'structured' && canLaunch && part && focusedId===part.id" class="stage-buttons"><button class="button primary" aria-label="Launch part" :disabled="!active || stale" @click="$emit('launch', true)">Launch {{part.name}}</button><button class="button" aria-label="Stop part" :disabled="!active || stale" @click="$emit('launch', false)">Stop {{part.name}}</button></div></div>
    <div class="performer-score" :class="{ 'notes-arriving': arriving }">
      <aside class="dynamic-meter" :class="{ override: playback?.dynamic_override != null }" aria-label="Effective dynamic">
        <span>fff</span><span>ff</span><span>f</span><span>mf</span><span>mp</span><span>p</span><span>pp</span><span>ppp</span>
        <i :style="{ height: `${(effectiveDynamic / 127) * 100}%` }"></i>
      </aside>
      <ScoreWorkspace :key="displayProject.id" :project="displayProject" :user-id="userId" :beats="displayBeats" :editable="false" performance @focus="focusedId=$event;$emit('select',$event)" />
      <Transition name="incoming">
        <div v-if="liveLane?.upcoming" :key="liveLane.upcoming" class="incoming-part">NEXT · {{ liveLane.upcoming }}</div>
      </Transition>
      <Transition name="count-in" mode="out-in">
        <div v-if="playback?.count_in_remaining" :key="playback.count_in_remaining" class="count-in-overlay" role="status" aria-live="assertive">
          <strong>{{playback.count_in_remaining}}</strong><span>{{part?.name}} · GET READY</span>
        </div>
      </Transition>
    </div>
  </main>
</template>
<style scoped>
.performer-score{position:relative;display:flex;flex:1;min-height:350px}.performer-score>.score-workspace{flex:1;min-width:0}.dynamic-meter{position:relative;z-index:2;display:flex;width:42px;flex-direction:column;justify-content:space-between;padding:12px 8px;border:1px solid var(--line);border-radius:7px 0 0 7px;background:#172022;color:var(--muted);font-size:8px;text-align:center}.dynamic-meter i{position:absolute;right:3px;bottom:3px;width:3px;max-height:calc(100% - 6px);border-radius:2px;background:var(--cyan);transition:height .12s}.dynamic-meter.override{box-shadow:inset 0 0 14px #9b7cff55}.dynamic-meter.override i{background:var(--violet);box-shadow:0 0 8px var(--violet)}.incoming-part{position:absolute;z-index:4;top:12px;right:18px;padding:9px 14px;border:1px solid var(--cyan);border-radius:20px;background:#112426e8;color:var(--cyan);font-size:11px;letter-spacing:1px}.incoming-enter-active,.incoming-leave-active{transition:transform .4s,opacity .4s}.incoming-enter-from,.incoming-leave-to{transform:translateX(35px);opacity:0}.notes-arriving :deep(.editable-score-mark){animation:notation-arrival .6s ease-out}.count-in-overlay{position:absolute;z-index:5;inset:0;display:grid;place-content:center;pointer-events:none;background:#10191a66;text-align:center}.count-in-overlay strong{font:clamp(100px,24vw,260px) 'Space Grotesk',sans-serif;color:var(--cyan);opacity:.82;animation:count-pulse .45s ease-out}.count-in-overlay span{color:white;font-size:15px;letter-spacing:2px}@keyframes notation-arrival{from{opacity:0;transform:translateX(16px)}to{opacity:1;transform:translateX(0)}}@keyframes count-pulse{from{transform:scale(.7);opacity:.3}to{transform:scale(1);opacity:.82}}@media(prefers-reduced-motion:reduce){.count-in-overlay strong,.notes-arriving :deep(.editable-score-mark){animation:none}.dynamic-meter i,.incoming-enter-active,.incoming-leave-active{transition:none}}
.stage-view{flex:1;min-height:0;overflow:auto;padding:16px 24px;display:flex;flex-direction:column}.stage-view>.score-workspace{flex:1;min-height:350px}.stage-header,.stage-clock,.stage-part{flex-shrink:0}.stage-header,.stage-part,.stage-buttons,.stage-clock{display:flex;align-items:center;gap:16px;flex-wrap:wrap}.stage-header{justify-content:space-between;margin-bottom:24px}.stage-header h1{font-size:24px;margin-top:8px}.stage-buttons .button{min-height:44px}.stage-clock{padding:20px 24px;background:var(--panel);border:1px solid var(--line);border-left:4px solid var(--cyan);border-radius:8px;gap:40px;margin-bottom:24px}.stage-clock span{display:block;font-size:11px;color:var(--muted);letter-spacing:1px}.stage-clock output{display:block;font:54px 'Space Grotesk',sans-serif;font-variant-numeric:tabular-nums;color:var(--cyan)}.stage-clock strong{font:32px 'Space Grotesk',sans-serif}.stage-clock p{color:var(--cyan)}.stage-clock.unavailable{border-color:var(--amber)}.stage-clock.unavailable p{color:var(--amber)}.stage-part{margin-bottom:24px}.stage-part label{display:flex;align-items:center;gap:12px;font-size:13px}.stage-part select{max-width:300px;min-height:44px}.stage-part output{color:var(--amber)}
@media(max-width:740px){.stage-view{padding:16px}.stage-header h1{font-size:20px}.stage-buttons{gap:8px}.stage-clock{gap:20px;padding:16px}.stage-clock output{font-size:40px}.stage-part label{width:100%}.stage-part select{flex:1}}
</style>
