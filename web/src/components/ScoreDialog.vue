<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick } from 'vue'
import { X } from '@lucide/vue'
defineProps<{ title: string; error?: string }>()
const emit = defineEmits<{ close: [] }>()
const dialog = ref<HTMLDialogElement>()
let previous: HTMLElement | null = null
onMounted(() => {
  previous = document.activeElement as HTMLElement
  dialog.value?.showModal()
})
onUnmounted(() =>
  nextTick(() => {
    const target = previous?.isConnected
      ? previous
      : document.querySelector<HTMLElement>('.score-workspace')
    target?.focus({ preventScroll: true })
  }),
)
</script>
<template>
  <Teleport to="body"
    ><dialog
      ref="dialog"
      class="score-dialog"
      :aria-label="title"
      @cancel.prevent="emit('close')"
      @close="emit('close')"
    >
      <header>
        <h2>{{ title }}</h2>
        <button
          type="button"
          :aria-label="`Close ${title}`"
          title="Close (Escape)"
          @click="emit('close')"
        >
          <X :size="18" />
        </button>
      </header>
      <div class="score-dialog-body">
        <p v-if="error" class="field-error" role="alert">{{ error }}</p>
        <slot />
      </div></dialog
  ></Teleport>
</template>
<style scoped>
.score-dialog {
  position: fixed;
  inset: 0;
  margin: auto;
  width: min(820px, calc(100vw - 32px));
  max-height: 85dvh;
  padding: 0;
  color: var(--white);
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 10px;
  box-shadow: 0 20px 80px #0008;
}
.score-dialog::backdrop {
  background: #0008;
}
.score-dialog > header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 14px 18px;
  border-bottom: 1px solid var(--line);
}
h2 {
  font-size: 18px;
}
.score-dialog header button {
  display: grid;
  place-items: center;
  min-width: 32px;
  min-height: 32px;
}
.score-dialog-body {
  padding: 16px;
  overflow: auto;
  max-height: calc(85dvh - 64px);
}
.score-dialog {
  --panel: #fff;
  --white: #111;
  --muted: #526267;
  --line: #d9e0e1;
  --cyan: #087f8c;
  --amber: #785600;
  color: #111;
  background: #fff;
}
.score-dialog :deep(input),
.score-dialog :deep(select) {
  background: #fff;
  color: #111;
  border-color: #cbd5d7;
}
.score-dialog :deep(button) {
  color: #111;
  background: #fff;
  border: 1px solid #cbd5d7;
  border-radius: 4px;
  padding: 5px 8px;
}
</style>
