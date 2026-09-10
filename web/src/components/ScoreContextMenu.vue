<script setup lang="ts">
import { computed, nextTick, onMounted, onBeforeUnmount, ref } from 'vue'
export interface MenuItem {
  label: string
  kbd?: string
  action?: () => void
  disabled?: boolean
  danger?: boolean
  separator?: boolean
}
const props = defineProps<{
  x: number
  y: number
  items: MenuItem[]
  label: string
}>()
const emit = defineEmits<{ close: [] }>()
const root = ref<HTMLElement>()
const position = computed(() => ({
  left: `${Math.max(4, Math.min(props.x, window.innerWidth - 260))}px`,
  top: `${Math.max(4, Math.min(props.y, window.innerHeight - 38 * props.items.length - 24))}px`,
}))
function outside(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) emit('close')
}
function keydown(event: KeyboardEvent) {
  if (event.key === 'Escape' || event.key === 'Tab') {
    event.preventDefault()
    emit('close')
    return
  }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    const buttons = [
        ...(root.value?.querySelectorAll<HTMLButtonElement>(
          'button:not(:disabled)',
        ) || []),
      ],
      current = buttons.indexOf(document.activeElement as HTMLButtonElement)
    buttons[
      (current + (event.key === 'ArrowDown' ? 1 : buttons.length - 1)) %
        buttons.length
    ]?.focus()
  }
}
onMounted(() => {
  document.addEventListener('pointerdown', outside, true)
  nextTick(() => root.value?.querySelector('button')?.focus())
})
onBeforeUnmount(() =>
  document.removeEventListener('pointerdown', outside, true),
)
function run(item: MenuItem) {
  if (item.disabled) return
  emit('close')
  item.action?.()
}
</script>
<template>
  <Teleport to="body">
    <div
      ref="root"
      class="score-context-menu"
      role="menu"
      :aria-label="label"
      :style="position"
      @keydown="keydown"
      @contextmenu.prevent
    >
      <template v-for="(item, i) in props.items" :key="i">
        <hr v-if="item.separator" />
        <button
          v-else
          type="button"
          role="menuitem"
          :class="{ danger: item.danger }"
          :disabled="item.disabled"
          @click="run(item)"
        >
          <span>{{ item.label }}</span><kbd v-if="item.kbd">{{ item.kbd }}</kbd>
        </button>
      </template>
    </div>
  </Teleport>
</template>
<style scoped>
.score-context-menu {
  position: fixed;
  z-index: 120;
  min-width: 240px;
  max-height: calc(100vh - 16px);
  overflow: auto;
  padding: 5px;
  border: 1px solid #bcc7ca;
  border-radius: 8px;
  background: #fff;
  color: #111;
  box-shadow: 0 12px 30px #0004;
  font-size: 13px;
}
.score-context-menu button {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  width: 100%;
  min-height: 36px;
  padding: 6px 12px;
  text-align: left;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: #111;
  cursor: pointer;
}
.score-context-menu button:hover,
.score-context-menu button:focus-visible {
  background: #e4f3f2;
  outline: 1px solid #087f8c;
}
.score-context-menu button:disabled {
  opacity: 0.4;
  cursor: default;
}
.score-context-menu button.danger {
  color: #9c2d16;
}
.score-context-menu kbd {
  font-size: 10px;
  color: #526267;
}
.score-context-menu hr {
  border: 0;
  border-top: 1px solid #e2e6e6;
  margin: 4px 0;
}
@media (pointer: coarse) {
  .score-context-menu button {
    min-height: 44px;
  }
}
</style>
