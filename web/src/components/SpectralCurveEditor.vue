<script setup lang="ts">
import { computed, ref, watch } from 'vue'
const props = defineProps<{ label: string; values: number[]; min: number; max: number; neutral: number; disabled: boolean; sampleRate: number }>()
const emit = defineEmits<{ change: [values: number[]] }>()
const draft = ref([...props.values]), selected = ref(0), drawing = ref(false)
let previous = 0
watch(() => props.values.join(','), () => { if (!drawing.value) draft.value = [...props.values] })
const points = computed(() => draft.value.map((value, i) => `${i * 640 / 32},${160 - (value - props.min) / (props.max - props.min) * 160}`).join(' '))
function paint(event: PointerEvent) {
  const rect = (event.currentTarget as SVGElement).getBoundingClientRect()
  const index = Math.max(0, Math.min(32, Math.round((event.clientX - rect.left) / rect.width * 32)))
  const value = props.min + Math.max(0, Math.min(1, 1 - (event.clientY - rect.top) / rect.height)) * (props.max - props.min)
  if (drawing.value && index !== previous) {
    const start = draft.value[previous]!
    for (let i = Math.min(previous, index); i <= Math.max(previous, index); i++) draft.value[i] = start + (value - start) * (i - previous) / (index - previous)
  }
  draft.value[index] = value; selected.value = index; previous = index
}
function down(event: PointerEvent) {
  if (props.disabled || event.button !== 0) return
  event.preventDefault(); (event.currentTarget as SVGElement).focus(); (event.currentTarget as SVGElement).setPointerCapture(event.pointerId)
  paint(event); drawing.value = true
}
function up(event: PointerEvent) {
  if (!drawing.value) return
  paint(event); drawing.value = false; emit('change', [...draft.value])
}
function key(event: KeyboardEvent) {
  if (props.disabled || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return
  event.preventDefault(); event.stopPropagation()
  if (event.key === 'ArrowLeft') selected.value = Math.max(0, selected.value - 1)
  else if (event.key === 'ArrowRight') selected.value = Math.min(32, selected.value + 1)
  else {
    const value = event.key === 'Home' ? props.min : event.key === 'End' ? props.max : draft.value[selected.value]! + (event.key === 'ArrowUp' ? 1 : -1) * (props.max - props.min) / 100
    draft.value[selected.value] = Math.max(props.min, Math.min(props.max, value)); emit('change', [...draft.value])
  }
}
function reset() { draft.value = Array(33).fill(props.neutral); emit('change', [...draft.value]) }
</script>
<template>
  <section class="spectral-curve-editor">
    <div class="parameter-heading"><strong>{{ label }}</strong><button class="button small" :disabled="disabled" @click="reset">Reset {{ label.toLowerCase() }}</button></div>
    <svg viewBox="0 0 640 160" preserveAspectRatio="none" tabindex="0" role="slider" :aria-label="label" :aria-valuemin="min" :aria-valuemax="max" :aria-valuenow="draft[selected]" :aria-valuetext="`Point ${selected + 1}, ${Math.round(selected * sampleRate / 64)} Hz: ${draft[selected]?.toFixed(3)}`" :aria-disabled="disabled" @pointerdown="down" @pointermove="drawing && paint($event)" @pointerup="up" @pointercancel="drawing = false; draft = [...values]" @keydown="key">
      <path d="M0 40H640 M0 80H640 M0 120H640 M160 0V160 M320 0V160 M480 0V160" stroke="#39484d" fill="none" />
      <polyline :points="points" fill="none" stroke="currentColor" stroke-width="3" />
      <circle :cx="selected * 20" :cy="160 - (draft[selected]! - min) / (max - min) * 160" r="5" fill="currentColor" />
    </svg>
    <div class="parameter-range"><span>0 Hz</span><span>{{ (sampleRate / 2000).toFixed(1) }} kHz</span></div>
    <small>Draw to edit; release to apply. ←/→ selects a point; ↑/↓ adjusts it. Point {{ selected + 1 }}: {{ draft[selected]?.toFixed(3) }}</small>
  </section>
</template>
