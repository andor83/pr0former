<script setup lang="ts">
import { computed } from 'vue'
import type { GraphNode } from '../types'
import { controlPoint, fieldSources, fieldValue, sourcePoint, sourceWeights } from '../granularField'
import GranularFieldPad from './GranularFieldPad.vue'
// The face of a Granular Field node: the field itself, with the control point
// draggable when its axes are free, and a summary of every source's share.
const props = defineProps<{ node: GraphNode; values?: Record<string, number>; connected: string[]; active: boolean; stale: boolean; editable: boolean; parameter?: (node: string, key: string, value: number) => void }>()
const live = computed(() => props.active && !props.stale)
const telemetry = computed(() => live.value ? props.values : undefined)
const sources = computed(() => fieldSources(props.node, props.connected))
const control = computed(() => controlPoint(props.node, telemetry.value, props.connected))
const focus = computed(() => fieldValue(props.node, 'focus', telemetry.value))
const padSources = computed(() => {
  const points = sources.value.map(s => sourcePoint(props.node, s, telemetry.value))
  const preview = control.value ? sourceWeights(control.value, points, focus.value) : points.map(() => 0)
  return sources.value.map((s, i) => ({ key: s.port, label: s.label, kind: s.kind, point: points[i]!, weight: live.value ? props.values?.[s.weightKey] ?? 0 : preview[i]!, missing: live.value && !!props.values?.[s.missingKey ?? ''], keys: { x: s.keys.x, y: s.keys.y } }))
})
const grains = computed(() => live.value ? props.values?._grains ?? 0 : null)
</script>
<template>
  <div class="field-face nodrag nopan">
    <GranularFieldPad compact :sources="padSources" :control="control" :focus="focus" :locked="connected" :editable="editable && !connected.includes('x') && !connected.includes('y')" :live="live" @change="(key, value) => parameter?.(node.id, key, value)" />
    <ol v-if="padSources.length" class="face-sources" aria-label="Field sources">
      <li v-for="s in padSources" :key="s.key" :class="s.kind"><span class="badge">{{ s.kind === 'live' ? 'LIVE' : 'SMP' }}</span><span class="name">{{ s.label }}</span><span class="bar" aria-hidden="true"><i :style="{ width: `${Math.round(Math.min(1, s.weight) * 100)}%` }" /></span><span v-if="s.missing" class="missing">missing</span></li>
    </ol>
    <p v-else class="face-note">Add samples in Options or cable audio into a live input.</p>
    <small v-if="grains !== null" class="face-grains">{{ grains }} grains sounding</small>
  </div>
</template>
<style scoped>
.field-face{position:absolute;left:104px;right:56px;top:64px;display:flex;flex-direction:column;gap:8px;font-size:9px;color:var(--ink-muted)}
.face-sources{list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:3px}
.face-sources li{display:grid;grid-template-columns:26px minmax(0,1fr) 60px auto;gap:6px;align-items:center;line-height:1.2}
.badge{font-size:6px;letter-spacing:1px;padding:1px 3px;border-radius:3px;border:1px solid color-mix(in srgb,var(--amber) 50%,transparent);color:var(--control-ink);text-align:center}
.live .badge{border-color:color-mix(in srgb,var(--cyan) 60%,transparent);color:var(--cyan)}
.name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--ink)}
.bar{display:block;height:5px;border-radius:3px;background:var(--shade);overflow:hidden}.bar i{display:block;height:100%;background:var(--amber);transition:width 120ms linear}.live .bar i{background:var(--cyan)}
.missing{color:var(--red);font-size:8px}
.face-note{margin:0;font-size:9px}.face-grains{font-size:8px;letter-spacing:.6px;text-transform:uppercase}
</style>
