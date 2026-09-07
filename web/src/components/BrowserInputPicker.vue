<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { browserInputs, browserInputMessage, browserInputChoices, browserInputBusy, captureError, refreshBrowserInputs } from '../browserInputs'
const props = defineProps<{ inputKey: string; disabled?: boolean }>()
const asking = ref(false), error = ref('')
const choice = computed({ get: () => browserInputChoices[props.inputKey] || '', set: value => { browserInputChoices[props.inputKey] = value } })
async function permit() {
  asking.value = true; error.value = ''
  try {
    if (!navigator.mediaDevices?.getUserMedia) throw new Error('Browser microphone access requires trusted HTTPS and a compatible browser.')
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false })
    stream.getTracks().forEach(track => track.stop())
    await refreshBrowserInputs()
  } catch (e) { error.value = captureError(e) }
  finally { asking.value = false }
}
onMounted(() => { void refreshBrowserInputs(); navigator.mediaDevices?.addEventListener('devicechange', refreshBrowserInputs) })
onBeforeUnmount(() => navigator.mediaDevices?.removeEventListener('devicechange', refreshBrowserInputs))
</script>
<template>
  <section class="browser-input-picker">
    <label>Browser microphone device<select v-model="choice" aria-label="Browser microphone device" :disabled="disabled || asking || browserInputBusy[inputKey]">
      <option value="">Browser default input</option>
      <option v-for="(device, index) in browserInputs" :key="device.deviceId || index" :value="device.deviceId" :disabled="!device.deviceId">{{device.label || `Audio input ${index + 1} · permission needed`}}</option>
      <option v-if="choice && !browserInputs.some(d => d.deviceId === choice)" :value="choice" disabled>Selected input unavailable — refresh and choose another</option>
    </select></label>
    <p role="status" class="feature-note">{{browserInputMessage}}</p>
    <p v-if="browserInputBusy[inputKey]" class="feature-note">This input is in use. Disconnect browser audio to change devices.</p>
    <button class="button small" :disabled="asking" @click="refreshBrowserInputs">Refresh browser inputs</button>
    <button class="button small" :disabled="asking || browserInputBusy[inputKey]" @click="permit">{{asking ? 'Waiting for permission…' : 'Grant microphone access'}}</button>
    <p v-if="error" role="alert" class="field-error">{{error}}</p>
    <p class="feature-note">Device selection applies only to this browser session. Connect browser audio and assign this node to send its input.</p>
  </section>
</template>
