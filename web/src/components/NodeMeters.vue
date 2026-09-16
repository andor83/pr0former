<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{ channels: number; values?: Record<string, number>; stale: boolean }>()
const SCALE = [0, -12, -24, -36, -48, -60]
// Same scale and zones as the monitor tab's LevelMeter: -60..0 dBFS,
// yellow from -12 dB, red at 0 dBFS.
const meters = computed(() => Array.from({ length: Math.max(1, Math.min(8, props.channels)) }, (_, i) => {
  const peak = props.stale ? 0 : Math.max(0, props.values?.[`_level${i + 1}`] ?? 0)
  const db = peak > 0 ? 20 * Math.log10(peak) : -Infinity
  const percent = Math.max(0, Math.min(100, (db + 60) / 60 * 100))
  const zone = db >= 0 ? 'clip' : db >= -12 ? 'warning' : 'normal'
  const readout = props.stale ? '—' : db <= -60 ? '−∞' : db.toFixed(0)
  return { channel: i + 1, db, percent, zone, readout }
}))
</script>
<template>
  <div class="node-meters" :class="{ stale }" role="group" aria-label="Channel levels">
    <div class="node-meter-scale" aria-hidden="true"><span v-for="db in SCALE" :key="db">{{ db }}</span></div>
    <div class="node-meter-strips">
      <div v-for="m in meters" :key="m.channel" class="node-meter" :data-zone="m.zone">
        <div class="node-meter-track" role="meter" :aria-label="`Channel ${m.channel} level`" aria-valuemin="-60" aria-valuemax="0" :aria-valuenow="Math.max(-60, Math.min(0, m.db))" :aria-valuetext="stale ? 'No current data' : `${m.readout} dBFS`">
          <div class="node-meter-fill" :style="{ height: `${m.percent}%` }"></div><div class="node-meter-grid"></div>
        </div>
        <small class="node-meter-readout">{{ m.readout }}</small>
        <small class="node-meter-name">{{ m.channel }}</small>
      </div>
    </div>
  </div>
</template>
<style scoped>
.node-meters{position:absolute;left:58px;right:88px;top:64px;bottom:32px;display:flex;gap:4px;min-height:60px;pointer-events:none}
.node-meter-scale{display:flex;flex-direction:column;justify-content:space-between;padding-bottom:24px;font-size:6px;color:var(--ink-muted);font-family:monospace;line-height:1;text-align:right;width:12px}
.node-meter-strips{flex:1;display:flex;gap:3px;min-width:0}
.node-meter{--level-color:#64d590;flex:1;min-width:0;display:grid;grid-template-rows:minmax(0,1fr) 12px 10px;justify-items:center;gap:2px}
.node-meter[data-zone=warning]{--level-color:#efc45d}.node-meter[data-zone=clip]{--level-color:#f16d69}.node-meter[data-zone=clip] .node-meter-fill{transition:none}
.node-meter-track{position:relative;width:100%;max-width:14px;min-width:6px;height:100%;min-height:0;border:1px solid var(--shade-line);border-radius:3px;background:var(--shade-deep);overflow:hidden;box-shadow:inset 0 0 8px #0008}
.node-meter-fill{position:absolute;bottom:0;left:0;right:0;background:var(--level-color);box-shadow:0 0 8px color-mix(in srgb,var(--level-color),transparent 55%);transition:height 80ms linear,background-color 80ms linear}
.node-meter-grid{position:absolute;inset:0;background:repeating-linear-gradient(to top,transparent 0,transparent 5px,var(--shade-deep) 5px,var(--shade-deep) 6px);pointer-events:none}
.node-meter-readout{font-size:7px;font-family:monospace;font-variant-numeric:tabular-nums;color:var(--level-color);line-height:12px;white-space:nowrap}
.node-meter-name{font-size:7px;color:var(--ink-muted);line-height:10px}
.stale{opacity:.5}.stale .node-meter-readout{color:var(--ink-muted)}
@media(prefers-reduced-motion:reduce){.node-meter-fill{transition:none}}
</style>
