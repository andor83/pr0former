<script setup lang="ts">
defineProps<{ value: number }>()
</script>
<template>
  <svg width="24" height="28" viewBox="0 0 24 28" aria-hidden="true">
    <template v-if="value >= 2">
      <path d="M3 12H21" stroke="currentColor" />
      <path
        :d="value >= 4 ? 'M7 12h10v5H7z' : 'M7 7h10v5H7z'"
        fill="currentColor"
      />
      <path v-if="value === 8" d="M7 7h10v5H7z" fill="currentColor" />
    </template>
    <path
      v-else-if="value === 1"
      d="M9 2L16 9L11 13L15 18Q6 15 10 25Q2 20 8 16L5 12L11 7Z"
      fill="currentColor"
    />
    <template v-else>
      <path :d="`M17 5L10 25`" stroke="currentColor" stroke-width="2" />
      <path
        v-for="f in Math.max(1, Math.round(Math.log2(1 / value)))"
        :key="f"
        :d="`M${17 - (f - 1) * 1.5} ${5 + (f - 1) * 5}q-5 6-9 1q-2-5 2-4q3 0 2 3`"
        fill="currentColor"
      />
    </template>
  </svg>
</template>
