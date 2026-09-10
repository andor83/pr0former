<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{device:string;channel:number;direction:string;peak?:number;stale:boolean}>()
const level = computed(() => props.stale || !Number.isFinite(props.peak) ? 0 : Math.max(0,props.peak!))
const db = computed(() => level.value > 0 ? 20 * Math.log10(level.value) : -Infinity)
const percent = computed(() => Math.max(0,Math.min(100,(db.value+60)/60*100)))
const zone = computed(() => db.value >= 0 ? 'clip' : db.value >= -12 ? 'warning' : 'normal')
const readout = computed(() => props.stale ? '—' : db.value <= -60 ? '−∞' : `${db.value > 0 ? '+' : ''}${db.value.toFixed(1)}`)
</script>
<template>
  <article class="vu-strip" :data-channel="channel" :data-zone="zone" :class="{stale}">
    <div class="vu-clip" :class="{lit:zone==='clip' && !stale}">{{zone==='clip' && !stale ? 'CLIP' : stale ? 'NO DATA' : 'PEAK'}}</div>
    <div class="vu-track" role="meter" :aria-label="`${device} ${direction} channel ${channel} level`" aria-valuemin="-60" aria-valuemax="0" :aria-valuenow="Math.max(-60,Math.min(0,db))" :aria-valuetext="stale ? 'No current hardware data' : `${readout} dBFS${zone==='clip'?', clipping':''}`">
      <div class="vu-fill" :style="{height:`${percent}%`}"></div><div class="vu-grid"></div>
    </div>
    <output class="vu-value">{{readout}} <small>dBFS</small></output>
    <div class="vu-name" :title="`Physical channel ${channel}`">Channel {{channel}}</div>
    <div class="vu-detail">{{direction}}</div>
  </article>
</template>
<style scoped>
.vu-strip{--level-color:#64d590;width:84px;min-width:0;display:grid;grid-template-rows:15px minmax(0,1fr) 22px 16px 12px;justify-items:center;gap:6px;padding:0 4px 6px;min-height:0;overflow:hidden}
.vu-strip[data-zone=warning]{--level-color:#efc45d}.vu-strip[data-zone=clip]{--level-color:#f16d69}.vu-strip[data-zone=clip] .vu-fill{transition:none}.vu-clip{height:15px;font-size:8px;letter-spacing:1.1px;color:var(--muted);white-space:nowrap}.vu-clip.lit{color:#ff9490;font-weight:700}.vu-track{position:relative;width:38px;height:100%;min-height:0;border:1px solid #34403f;border-radius:4px;background:#0c1213;overflow:hidden;box-shadow:inset 0 0 14px #0008}.vu-fill{position:absolute;bottom:0;left:0;right:0;background:var(--level-color);box-shadow:0 0 14px color-mix(in srgb,var(--level-color),transparent 55%);transition:height 80ms linear,background-color 80ms linear}.vu-grid{position:absolute;inset:0;background:repeating-linear-gradient(to top,transparent 0,transparent 7px,#101719 7px,#101719 9px);pointer-events:none}.vu-value{display:block;width:100%;text-align:center;line-height:22px;white-space:nowrap;font-size:15px;font-variant-numeric:tabular-nums;color:var(--level-color);margin-top:0}.vu-value small{font-size:9px;color:var(--muted)}.vu-name{font-size:12px;max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--white)}.vu-detail{font-size:9px;color:var(--muted)}.stale{opacity:.5}.stale .vu-value{color:var(--muted)}@media(prefers-reduced-motion:reduce){.vu-fill{transition:none}}
</style>
