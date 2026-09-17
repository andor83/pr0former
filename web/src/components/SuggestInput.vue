<script setup lang="ts">
import { computed, ref, useId } from 'vue'
// A text field with its own suggestion list. Chrome on iOS locks up when the native
// <datalist> popup opens inside a modal dialog, so the list is ordinary markup here:
// a combobox that filters as you type, with arrow keys, Enter and click to pick.
const props = defineProps<{ value: string; suggestions: { value: string; detail?: string }[]; label: string; placeholder?: string; disabled?: boolean; maxlength?: number; limit?: number }>()
const emit = defineEmits<{ input: [text: string]; change: [text: string]; enter: [] }>()
const uid = useId()
const input = ref<HTMLInputElement>()
const open = ref(false)
const highlighted = ref(-1)
// What the user has typed since focusing; null shows the parent's value.
const typed = ref<string | null>(null)
const matches = computed(() => {
  const q = (typed.value ?? props.value).trim().toLowerCase()
  const list = q ? props.suggestions.filter(s => s.value.toLowerCase().includes(q) || s.detail?.toLowerCase().includes(q)) : props.suggestions
  return list.slice(0, props.limit ?? 12)
})
const listId = `suggest-${uid}`
function onInput(event: Event) {
  typed.value = (event.target as HTMLInputElement).value
  open.value = true; highlighted.value = -1
  emit('input', typed.value)
}
function onChange(event: Event) { emit('change', (event.target as HTMLInputElement).value) }
function pick(value: string) {
  typed.value = null
  if (input.value) input.value.value = value
  open.value = false; highlighted.value = -1
  emit('input', value); emit('change', value)
}
function onKey(event: KeyboardEvent) {
  const count = matches.value.length
  if (event.key === 'ArrowDown' && count) { open.value = true; highlighted.value = (highlighted.value + 1) % count; event.preventDefault() }
  else if (event.key === 'ArrowUp' && count) { open.value = true; highlighted.value = (highlighted.value - 1 + count) % count; event.preventDefault() }
  else if (event.key === 'Enter') {
    if (open.value && highlighted.value >= 0) pick(matches.value[highlighted.value]!.value)
    event.preventDefault(); emit('enter')
  } else if (event.key === 'Escape' && open.value) {
    // Close the list without closing the surrounding dialog.
    open.value = false; event.preventDefault(); event.stopPropagation()
  }
}
function onBlur() { setTimeout(() => { open.value = false; highlighted.value = -1; typed.value = null }, 120) }
</script>
<template>
  <div class="suggest-input">
    <input ref="input" type="text" autocomplete="off" autocapitalize="off" spellcheck="false" role="combobox" :aria-label="label" :placeholder="placeholder" :disabled="disabled" :maxlength="maxlength" :aria-expanded="open && matches.length > 0" :aria-controls="listId" :aria-activedescendant="highlighted >= 0 ? `${listId}-${highlighted}` : undefined" :value="typed ?? value" @input="onInput" @change="onChange" @focus="open = true" @blur="onBlur" @keydown="onKey">
    <ul v-if="open && matches.length && !disabled" :id="listId" role="listbox" class="suggest-list" :aria-label="`${label} suggestions`">
      <li v-for="(m, i) in matches" :key="m.value" :id="`${listId}-${i}`" role="option" :aria-selected="i === highlighted" :class="{ highlighted: i === highlighted }" @mousedown.prevent @click="pick(m.value)"><span>{{ m.value }}</span><small v-if="m.detail">{{ m.detail }}</small></li>
    </ul>
  </div>
</template>
<style scoped>
.suggest-input{position:relative;display:block;width:100%}
.suggest-input input{width:100%}
.suggest-list{position:absolute;left:0;right:0;top:calc(100% + 4px);z-index:5;margin:0;padding:4px;list-style:none;max-height:240px;overflow:auto;background:var(--panel-raised);border:1px solid var(--line);border-radius:6px;box-shadow:0 12px 30px #0008}
.suggest-list li{display:flex;justify-content:space-between;gap:12px;padding:8px 10px;border-radius:4px;font-size:12px;cursor:pointer}
.suggest-list li small{color:var(--muted);white-space:nowrap}
.suggest-list li:hover,.suggest-list li.highlighted{background:#2f3a3b}
@media(pointer:coarse){.suggest-list li{min-height:44px;align-items:center}}
</style>
