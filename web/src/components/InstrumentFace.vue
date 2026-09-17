<script setup lang="ts">
import { computed, inject, type Ref } from 'vue'
import type { Descriptor, GraphNode } from '../types'
import type { SampleEntry } from '../samples'
import { readEnvelope } from '../envelopeGeometry'
import { decodeMidiDebug } from '../midiDebug'
import EnvelopeGraph from './EnvelopeGraph.vue'
// The face of a polyphonic instrument: which sample it plays, its per-voice
// envelope with the loudest live voice traced over it, and the last five
// note events the engine dispatched to its voices.
const props = defineProps<{ node: GraphNode; descriptor: Descriptor; values?: Record<string, number>; connected: string[]; active: boolean; stale: boolean }>()
const samples = inject<Ref<SampleEntry[]>>('projectSamples')
const live = computed(() => props.active && !props.stale)
const sampleBacked = computed(() => ['poly_sampler', 'granular_synth', 'granular_cloud'].includes(props.node.kind))
const sampleName = computed(() => {
  if (props.connected.includes('sample_id')) return live.value && props.values?.sample_id ? nameFor(props.values.sample_id) : 'Sample from input'
  return nameFor(props.node.parameters.asset ?? 0)
})
function nameFor(asset: number) { return asset ? samples?.value.find(s => s.asset === asset)?.name ?? `Sample ${asset}` : 'No sample loaded' }
const envelope = computed(() => readEnvelope(props.node.parameters, props.descriptor.parameters, live.value ? props.values : undefined, props.connected))
const voices = computed(() => live.value ? props.values?._voices ?? 0 : 0)
const recent = computed(() => {
  if (!live.value || !props.values) return []
  const list = []
  for (let i = 0; i < 5; i++) {
    const status = props.values[`_recent_${i}_status`]
    if (!status) break
    const message = decodeMidiDebug(status, props.values[`_recent_${i}_data1`] ?? 0, props.values[`_recent_${i}_data2`] ?? 0)
    if (message) list.push({ key: `${props.values._note_count ?? 0}-${i}`, ...message })
  }
  return list
})
</script>

<template>
  <div class="instrument-face nodrag nopan">
    <div v-if="sampleBacked" class="face-sample" :title="sampleName"><span class="face-label">Sample</span><strong>{{ sampleName }}</strong></div>
    <EnvelopeGraph v-if="envelope" compact class="face-envelope" :envelope="envelope" :level="live ? values?._envelope : undefined" :gate="live && (values?._held ?? 0) > 0" />
    <span v-else class="face-note">Envelope waiting for live values</span>
    <div class="face-midi">
      <header><span class="face-label">Recent notes</span><small v-if="live">{{ voices }} {{ voices === 1 ? 'voice' : 'voices' }}</small><small v-else>{{ active ? 'Stale' : 'Engine off' }}</small></header>
      <ol v-if="recent.length" aria-label="Recent note events"><li v-for="item in recent" :key="item.key"><span>{{ item.type }}</span><span>{{ item.detail }}</span><span>{{ item.value }}</span></li></ol>
      <p v-else>Waiting for notes…</p>
    </div>
  </div>
</template>

<style scoped>
.instrument-face{position:absolute;left:104px;right:56px;top:64px;display:flex;flex-direction:column;gap:8px;font-size:9px;color:var(--ink-muted)}
.face-label{text-transform:uppercase;letter-spacing:1.2px;font-size:7px;color:var(--well-ink-muted)}
.face-sample{display:flex;flex-direction:column;gap:2px;padding:6px 9px;border-radius:6px;background:var(--control-well);border:1px solid color-mix(in srgb,var(--amber) 45%,transparent);color:var(--well-ink-muted);min-width:0}
.face-sample strong{font-weight:500;font-size:10px;color:var(--well-control-ink);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.face-envelope{width:100%;aspect-ratio:2/1}
.face-note{padding:10px 0;font-size:9px}
.face-midi{padding:7px 9px 8px;border-radius:6px;background:var(--shade-deep);border:1px solid color-mix(in srgb,var(--amber) 45%,transparent);color:var(--well-control-ink);font-variant-numeric:tabular-nums}
.face-midi header{display:flex;justify-content:space-between;align-items:center;gap:8px;margin-bottom:5px}.face-midi header small{font-size:8px;color:var(--well-ink-muted)}
.face-midi ol{list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:3px}
.face-midi li{display:grid;grid-template-columns:52px 1fr 28px;gap:6px;font-size:9px;line-height:1.2}.face-midi li span:last-child{text-align:right}.face-midi li span:nth-child(2){overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.face-midi p{margin:0;font-size:9px;color:var(--well-ink-muted)}
</style>
