<script setup lang="ts">
import { computed } from 'vue'
import type { EdgeProps } from '@vue-flow/core'
import { BaseEdge, EdgeLabelRenderer, getBezierPath } from '@vue-flow/core'
const props = defineProps<EdgeProps<{ signal: string; channels: number; active: boolean }>>()
const path = computed(() => getBezierPath(props))
</script>
<template>
  <BaseEdge :id="id" :path="path[0]" :class="['signal-edge', data?.signal, { flowing: data?.active, selected }]" :interaction-width="24" />
  <EdgeLabelRenderer v-if="data?.signal === 'audio' && data.channels > 1"><span class="channel-badge" :style="{ transform: `translate(-50%, -50%) translate(${path[1]}px, ${path[2]}px)` }">{{ data.channels }} ch</span></EdgeLabelRenderer>
</template>

