<script setup lang="ts">
import { onMounted, ref } from 'vue'
defineProps<{ title: string; detail?: string }>()
const dialog=ref<HTMLDialogElement>()
onMounted(()=>dialog.value?.showModal())
</script>
<template><dialog ref="dialog" class="parameter-modal task-progress" aria-labelledby="task-title" @cancel.prevent><h2 id="task-title">{{ title }}</h2><progress aria-label="Task in progress" /><p role="status">{{ detail || 'Please wait while the server prepares your project.' }}</p></dialog></template>
