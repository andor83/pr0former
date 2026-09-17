<script setup lang="ts">
const props=defineProps<{slots:number;values?:Record<string,number>;stale:boolean}>()
function pitch(slot:number){const value=props.values?.[`pitch${slot}`];return !props.stale&&value!==undefined&&value>=0?value:null}
function note(value:number){return ['C','C♯','D','D♯','E','F','F♯','G','G♯','A','A♯','B'][value%12]+String(Math.floor(value/12)-1)}
</script>
<template>
  <div class="tracker-pitches" aria-label="Detected pitches, strongest first">
    <div v-for="slot in slots" :key="slot" class="tracker-slot">
      <small>Pitch {{slot}}</small><output :aria-label="`Pitch ${slot} MIDI note`">{{pitch(slot)??'—'}}</output><span>{{pitch(slot)===null?'No pitch':note(pitch(slot)!)}}</span>
    </div>
  </div>
</template>
<style scoped>
.tracker-pitches{position:absolute;bottom:0;left:12px;right:12px;display:flex;height:76px;gap:8px}.tracker-slot{position:relative;flex:1;text-align:center;border-top:1px solid color-mix(in srgb,var(--ink-muted) 35%,transparent);padding-top:7px}.tracker-slot small{font-size:9px;color:var(--ink-muted);display:block}.tracker-slot output{display:block;font-size:21px;color:var(--control-ink);font-variant-numeric:tabular-nums}.tracker-slot span{font-size:9px;color:var(--ink-muted)}
</style>
