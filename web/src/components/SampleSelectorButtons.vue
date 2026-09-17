<script setup lang="ts">
import type { SampleChoice } from '../types'
defineProps<{ choices: SampleChoice[]; index: number; disabled: boolean; connected: boolean }>()
const emit = defineEmits<{ select: [index: number] }>()
</script>
<template>
  <div class="sample-selector-buttons nodrag nopan nowheel" @pointerdown.stop @dblclick.stop @keydown.stop>
    <button v-for="(sample,i) in choices" :key="i" :disabled="disabled" :aria-label="`Select sample ${i}: ${sample.nickname || sample.name}`" :aria-pressed="index>=0 && Math.floor(index)===i" :title="`${sample.name} · Sample ID ${sample.asset}`" @click.stop="emit('select',i)"><span>{{i}}</span>{{sample.nickname || sample.name}}</button>
    <small v-if="!choices.length">Add samples in options</small>
    <small v-else-if="connected">Index controlled by connection</small>
  </div>
</template>
<style scoped>
.sample-selector-buttons{position:relative;margin:40px 14px 0;display:flex;flex-direction:column;gap:6px;max-height:308px;overflow:auto}
button{display:flex;align-items:center;gap:9px;flex-shrink:0;min-height:32px;text-align:left;padding:6px 9px;border:1px solid #b86c32;border-radius:7px;background:color-mix(in srgb,var(--amber) 30%,var(--shade));color:var(--well-ink);font-size:11px;overflow-wrap:anywhere}
button span{font-variant-numeric:tabular-nums;color:var(--well-control-ink);min-width:16px}
button:hover:enabled{background:color-mix(in srgb,var(--amber) 42%,var(--shade));border-color:#f0a15f}button[aria-pressed=true]{background:color-mix(in srgb,var(--amber) 58%,var(--shade));border-color:#ffc186}button:disabled{opacity:.65}small{font-size:10px;color:var(--ink-muted)}
@media(pointer:coarse){button{min-height:44px}}
</style>
