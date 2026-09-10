<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
defineProps<{ label: string; symbol: string }>()
const root = ref<HTMLDetailsElement>()
function outside(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node))
    root.value?.removeAttribute('open')
}
onMounted(() => document.addEventListener('pointerdown', outside))
onBeforeUnmount(() => document.removeEventListener('pointerdown', outside))
</script>
<template>
  <details
    ref="root"
    class="score-tool-menu"
    @keydown.esc.stop="
      ($event.currentTarget as HTMLElement).removeAttribute('open')
    "
  >
    <summary :title="label" :aria-label="label">
      <slot name="icon">{{ symbol }}</slot
      ><small>▾</small>
    </summary>
    <div
      class="score-tool-grid"
      @click="
        ($event.currentTarget as HTMLElement).parentElement?.removeAttribute(
          'open',
        )
      "
    >
      <slot />
    </div>
  </details>
</template>
<style scoped>
.score-tool-menu {
  position: relative;
  color: #111;
}
.score-tool-menu summary {
  list-style: none;
  cursor: pointer;
  min-width: 30px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 21px;
  border: 1px solid #d9e0e1;
  border-radius: 4px;
  background: white;
}
.score-tool-menu summary::-webkit-details-marker {
  display: none;
}
small {
  font-size: 9px;
}
.score-tool-grid {
  position: absolute;
  top: 35px;
  left: 50%;
  transform: translateX(-50%);
  display: grid;
  grid-template-columns: repeat(4, 48px);
  gap: 6px;
  padding: 10px;
  background: white;
  border: 1px solid #bcc7ca;
  border-radius: 6px;
  box-shadow: 0 4px 16px #0003;
  z-index: 30;
}
.score-tool-grid :deep(button) {
  min-width: 46px !important;
  height: 46px !important;
  padding: 2px !important;
  color: #111;
  background: white;
  font-size: 27px;
  line-height: 1;
}
.score-tool-grid :deep(button small) {
  font-size: 11px;
}
@media (pointer: coarse) {
  .score-tool-grid {
    grid-template-columns: repeat(4, 52px);
  }
  .score-tool-grid :deep(button) {
    min-width: 50px !important;
    height: 50px !important;
  }
}
</style>
