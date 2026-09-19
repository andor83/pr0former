<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import type { StateBank } from '../states'
const props=defineProps<{bank?:StateBank|null;editable:boolean;canRecall:boolean;active:boolean;busy:boolean;status:string;error:string}>()
const emit=defineEmits<{close:[];rename:[slot:number,name:string];remove:[slot:number];load:[slot:number]}>()
const dialog=ref<HTMLDialogElement>()
onMounted(()=>dialog.value?.showModal())
onBeforeUnmount(()=>dialog.value?.close())
</script>
<template>
  <dialog ref="dialog" class="parameter-modal state-manager" aria-labelledby="state-manager-title" @cancel.prevent="emit('close')">
    <header class="modal-header"><h2 id="state-manager-title">Manage states</h2><button class="icon-button" aria-label="Close states" @click="emit('close')">×</button></header>
    <div class="parameter-list">
      <p v-if="!active">Enable the engine to load a state.</p>
      <p role="status" aria-live="polite">{{status}}</p><p v-if="error" role="alert" class="field-error">{{error}}</p>
      <table><caption class="sr-only">Saved subgraph states</caption><thead><tr><th scope="col">Slot</th><th scope="col">Name</th><th scope="col">Load</th><th scope="col">Delete</th></tr></thead>
        <tbody><tr v-for="slot in props.bank?.slots??[]" :key="slot.slot"><th scope="row">{{slot.slot}}</th><td><input :aria-label="`State ${slot.slot} name`" :value="slot.name" :disabled="!editable||busy" @change="emit('rename',slot.slot,($event.target as HTMLInputElement).value)"></td><td><button class="button" :aria-label="`Load state ${slot.slot}`" :disabled="!active||!canRecall||busy" @click="emit('load',slot.slot)">Load</button></td><td><button class="button" :aria-label="`Delete state ${slot.slot}`" :disabled="!editable||busy" @click="emit('remove',slot.slot)">Delete</button></td></tr></tbody>
      </table><p v-if="!bank?.slots.length">No saved states yet.</p>
    </div>
  </dialog>
</template>
<style scoped>
table{width:100%;border-collapse:collapse}th,td{padding:8px;text-align:left}input{width:100%;min-width:100px}.state-manager button,.state-manager input{min-height:44px}.modal-header{display:flex;align-items:center;justify-content:space-between}.state-manager{max-width:min(680px,95vw)}
</style>
