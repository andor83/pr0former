<script setup lang="ts">
import { defineAsyncComponent } from 'vue'
import type { Part, Project } from '../types'
const ScoreEditor = defineAsyncComponent(() => import('./ScoreEditor.vue'))
defineProps<{ project: Project; part?: Part; beat: number; meterBeat: number; bpm: number; active: boolean; stale: boolean; running: boolean; status: string; canLaunch: boolean; monitorOpen: boolean }>()
defineEmits<{ select: [id: string]; launch: [playing: boolean]; exit: []; fullscreen: []; monitor: [] }>()
</script>
<template>
  <main class="stage-view" aria-label="Performance stage">
    <header class="stage-header"><div><div class="eyebrow">{{ project.mode }} PERFORMANCE</div><h1>{{ project.name }}</h1></div><div class="stage-buttons"><button class="button" :aria-expanded="monitorOpen" @click="$emit('monitor')">Monitor controls</button><button class="button" @click="$emit('fullscreen')">Fullscreen</button><button class="button" @click="$emit('exit')">Exit performance mode</button></div></header>
    <div class="stage-clock" :class="{ unavailable: active && stale }"><div><span>SHARED BAR · BEAT</span><output aria-label="Stage position">{{ Math.floor(meterBeat / project.beats_per_bar) + 1 }} : {{ Math.floor(meterBeat % project.beats_per_bar) + 1 }}</output></div><div><span>♩ BPM</span><strong>{{ bpm }}</strong></div><p role="status">{{ !active ? 'Waiting for show activation' : stale ? 'Timing unavailable — score held' : running ? 'Transport playing' : 'Transport stopped / paused' }}</p></div>
    <div class="stage-part"><label>Viewing part<select aria-label="Performance part" :value="part?.id || ''" @change="$emit('select', ($event.target as HTMLSelectElement).value)"><option v-for="p in project.parts" :key="p.id" :value="p.id">{{ p.name }}</option></select></label><output aria-label="Stage part status" aria-live="polite">{{ status }}</output><div v-if="project.mode !== 'structured' && canLaunch" class="stage-buttons"><button class="button primary" :disabled="!active || stale || !part" @click="$emit('launch', true)">Launch part</button><button class="button" :disabled="!active || stale || !part" @click="$emit('launch', false)">Stop part</button></div></div>
    <ScoreEditor v-if="part" :key="part.id" :part="part" :beat="beat" :editable="false" :performance="true" :beats-per-bar="project.beats_per_bar" :beat-unit="project.beat_unit || 4" />
    <p v-else>No score parts have been added to this project.</p>
  </main>
</template>
<style scoped>
.stage-view{flex:1;min-height:0;overflow:auto;padding:24px 32px}.stage-header,.stage-part,.stage-buttons,.stage-clock{display:flex;align-items:center;gap:16px;flex-wrap:wrap}.stage-header{justify-content:space-between;margin-bottom:24px}.stage-header h1{font-size:24px;margin-top:8px}.stage-buttons .button{min-height:44px}.stage-clock{padding:20px 24px;background:var(--panel);border:1px solid var(--line);border-left:4px solid var(--cyan);border-radius:8px;gap:40px;margin-bottom:24px}.stage-clock span{display:block;font-size:11px;color:var(--muted);letter-spacing:1px}.stage-clock output{display:block;font:54px 'Space Grotesk',sans-serif;font-variant-numeric:tabular-nums;color:var(--cyan)}.stage-clock strong{font:32px 'Space Grotesk',sans-serif}.stage-clock p{color:var(--cyan)}.stage-clock.unavailable{border-color:var(--amber)}.stage-clock.unavailable p{color:var(--amber)}.stage-part{margin-bottom:24px}.stage-part label{display:flex;align-items:center;gap:12px;font-size:13px}.stage-part select{max-width:300px;min-height:44px}.stage-part output{color:var(--amber)}
@media(max-width:740px){.stage-view{padding:16px}.stage-header h1{font-size:20px}.stage-buttons{gap:8px}.stage-clock{gap:20px;padding:16px}.stage-clock output{font-size:40px}.stage-part label{width:100%}.stage-part select{flex:1}}
</style>
