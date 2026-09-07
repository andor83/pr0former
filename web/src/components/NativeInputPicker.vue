<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { api } from '../api'
const props = defineProps<{ value: number; disabled: boolean }>()
const emit = defineEmits<{ change: [value: number] }>()
const devices = ref<any>(), settings = ref<any>(), error = ref('')
let timer: ReturnType<typeof setInterval> | undefined
const available = computed(() => (devices.value?.input_interfaces || []) as {id:number;name:string}[])
const enabled = computed(() => ((settings.value?.input_interfaces || []) as {id:number;name:string;enabled:boolean}[]).filter(i => i.enabled && available.value.some(d => d.id === i.id)))
const selected = computed(() => props.value || enabled.value[0]?.id)
async function refresh() { try { const [s,d] = await Promise.all([api('/system/audio'),api('/devices')]); settings.value=s;devices.value=d;error.value='' } catch(e) { error.value=String(e) } }
onMounted(() => { void refresh();timer=setInterval(refresh,2000) })
onBeforeUnmount(() => clearInterval(timer))
</script>
<template>
  <select aria-label="Input interface" :value="value" :disabled="disabled" @change="emit('change', Number(($event.target as HTMLSelectElement).value))">
    <option :value="0">First enabled input</option>
    <option v-for="i in available" :key="i.id" :value="i.id" :disabled="!enabled.some(d=>d.id===i.id)">{{i.name}}{{enabled.some(d=>d.id===i.id)?'':' · disabled'}}</option>
    <option v-if="value && !available.some(i=>i.id===value)" :value="value" disabled>Selected input unavailable — choose another</option>
  </select>
  <p v-if="!devices && !error" class="feature-note">Checking native inputs…</p>
  <template v-if="devices">
    <p v-if="!available.length" class="field-error">No native audio inputs are available on the server.</p>
    <p v-else-if="!enabled.length" class="field-error">No native inputs are enabled. Enable inputs in System settings and save.</p>
    <p v-else-if="value && !enabled.some(i=>i.id===value)" class="field-error">The selected input is disabled or unavailable.</p>
    <p v-else-if="!devices.active_inputs?.includes(selected)" class="feature-note">Input capture is off. Activate the show and enable server input in System settings.</p>
    <p v-else class="feature-note">Capturing from {{enabled.find(i=>i.id===selected)?.name}}.</p>
    <p v-if="devices.error" class="field-error">{{devices.error}}</p>
  </template>
  <p v-if="error" role="alert" class="field-error">{{error}}</p>
  <button class="button small" @click="refresh">Refresh native inputs</button>
</template>
