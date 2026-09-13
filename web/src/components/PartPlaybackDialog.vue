<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import type { Part, Project, ScoreTimeline } from '../types'
import { metadata, scoreAnchors, scoreBeat, scoreMeasures, scoreX, staves } from '../score'
import ScoreDialog from './ScoreDialog.vue'
import ScoreStaff from './ScoreStaff.vue'
import ScorePianoRoll from './ScorePianoRoll.vue'
const props=defineProps<{project:Project;part?:Part;values?:Record<string,number>;stale:boolean}>()
const emit=defineEmits<{close:[]}>()
const view=ref<'notation'|'grid'>(props.part?.view==='grid'?'grid':'notation')
const viewport=ref<HTMLElement>(),scrollLeft=ref(0),viewportWidth=ref(900)
const scale=64,origin=180,selected=new Set<string>()
const length=computed(()=>Math.min(props.part?.loop_beats??4,props.project.score?.length??Infinity))
const part=computed(()=>props.part?{...props.part,notes:props.part.notes.filter(n=>n.beat<length.value)}:undefined)
const staffs=computed(()=>part.value?staves(part.value):[])
const meters=computed(()=>props.part?.performance_meters?.length?props.part.performance_meters:props.project.score?.meters??[])
const initialMeter=computed(()=>meters.value.find(m=>m.beat===0)??props.project.score?.meters.find(m=>m.beat===0)??{beats:props.project.beats_per_bar,unit:props.project.beat_unit||4})
const timeline=computed<ScoreTimeline>(()=>({...props.project.score,version:1,length:length.value,loop_score:false,meters:meters.value,keys:props.project.score?.keys??[],repeats:props.project.score?.repeats??[]}))
const anchors=computed(()=>scoreAnchors(part.value?[part.value]:[],length.value,scale,origin,
  scoreMeasures(length.value,initialMeter.value.beats,initialMeter.value.unit,meters.value),
  [...meters.value.map(m=>m.beat),...timeline.value.keys.map(k=>k.beat),...staffs.value.flatMap(s=>(s.clef_changes??[]).map(c=>c.beat))]))
const beat=computed(()=>Math.min(length.value,Math.max(0,props.values?._written_position??0)))
const xAt=(beat:number)=>view.value==='grid'?origin+beat*scale:scoreX(beat,anchors.value,scale,origin)
const width=computed(()=>Math.max(viewportWidth.value,xAt(length.value)+100))
const viewStart=computed(()=>scoreBeat(scrollLeft.value,anchors.value,scale,origin))
const viewEnd=computed(()=>scoreBeat(scrollLeft.value+viewportWidth.value,anchors.value,scale,origin))
const status=computed(()=>props.stale?'Playback unavailable':props.values?._pending?'Waiting for beat':props.values?._playing?(props.values?._repeating?'Repeating':'Playing'):'Stopped')
const rolls=computed(()=>staffs.value.map(staff=>({staff,part:{...part.value!,notes:part.value!.notes.filter(n=>!n.rest&&metadata(n,part.value!).staff===staff.id)}})))
function measure(){if(viewport.value){scrollLeft.value=viewport.value.scrollLeft;viewportWidth.value=viewport.value.clientWidth}}
async function follow(){await nextTick();const el=viewport.value;if(!el||props.stale||!props.values?._playing)return;const x=xAt(beat.value);if(x<el.scrollLeft+40||x>el.scrollLeft+el.clientWidth-80){el.scrollLeft=Math.max(0,x-origin);measure()}}
watch([beat,view],follow)
let observer:ResizeObserver|undefined
onMounted(()=>{measure();if(viewport.value){observer=new ResizeObserver(measure);observer.observe(viewport.value)}void follow()})
onUnmounted(()=>observer?.disconnect())
</script>
<template>
  <ScoreDialog :title="`Part playback: ${part?.name || 'Unassigned'}`" wide @close="emit('close')">
    <div class="playback-tools" @keydown.stop>
      <div role="group" aria-label="Part playback view"><button class="button small" :aria-pressed="view==='notation'" @click="view='notation'">Notation</button><button class="button small" :aria-pressed="view==='grid'" @click="view='grid'">Piano roll</button></div>
      <output aria-label="Playback position">{{!stale&&values?._bar?`Bar ${values._bar} · Beat ${values._beat}`:'Bar — · Beat —'}}</output><span role="status">{{status}}</span><span class="small-tag">READ ONLY · ALL STAVES</span>
    </div>
    <p v-if="!part" class="feature-note">Choose a source part in the node’s Options.</p>
    <div v-else ref="viewport" class="part-playback-viewport" :class="{'playback-stale':stale}" @scroll.passive="measure" @keydown.stop>
      <div :style="{width:`${width}px`}" :data-playback-beat="beat">
        <template v-if="view==='notation'"><section v-for="(staff,index) in staffs" :key="staff.id" :aria-label="staff.name" class="playback-staff"><h3>{{staff.name}}</h3><ScoreStaff :part="part" :staff="staff" :anchors="anchors" :length="length" :bar-length="initialMeter.beats*4/initialMeter.unit" :beats-per-bar="initialMeter.beats" :beat-unit="initialMeter.unit" :scale="scale" :origin="origin" :timeline="timeline" :view-start="viewStart" :view-end="viewEnd" :selected="selected" :interactive="false" :beat="beat" :first="index===0" /></section></template>
        <template v-else><section v-for="roll in rolls" :key="roll.staff.id" :aria-label="roll.staff.name" class="playback-staff"><h3>{{roll.staff.name}}</h3><ScorePianoRoll :part="roll.part" :selected="selected" :editable="false" :length="length" fit-notes tool="select" :anchors="[]" :scale="scale" :origin="origin" :width="width" :beat="beat" /></section></template>
      </div>
    </div>
  </ScoreDialog>
</template>
<style scoped>
.playback-tools{display:flex;align-items:center;gap:16px;flex-wrap:wrap;margin-bottom:14px}.playback-tools [role='group']{display:flex;gap:6px}.playback-tools output{color:var(--amber);font-variant-numeric:tabular-nums}.playback-tools [aria-pressed='true']{border-color:var(--amber);color:var(--amber)}
.part-playback-viewport{max-height:60dvh;overflow:auto;border:1px solid var(--line);border-radius:8px}.playback-staff h3{position:sticky;left:0;max-width:75vw;padding:8px 12px;font-size:12px;margin:0;color:var(--white)}
.playback-staff :deep(.piano-roll){height:260px}.playback-staff :deep(button.midi-note){cursor:default;background:#edb469;border:1px solid #9b651c;padding:0;border-radius:3px}.playback-staff :deep(.roll-grid){touch-action:pan-x pan-y}.playback-stale :deep(.score-playhead),.playback-stale :deep(.roll-playhead){visibility:hidden}
</style>
