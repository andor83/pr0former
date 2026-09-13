<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { localMachineName, saveMachineName, browserInputs, browserInputMessage, browserInputChoices, browserInputBusy, captureError, refreshBrowserInputs } from '../browserInputs'
const props = defineProps<{ inputKey: string; disabled?: boolean; nodeId?:string; active?:boolean }>()
const asking = ref(false), error = ref('')
const choice = computed({ get: () => browserInputChoices[props.inputKey] || '', set: value => { browserInputChoices[props.inputKey] = value } })
async function permit() {
  asking.value = true; error.value = ''
  try {
    if (!navigator.mediaDevices?.getUserMedia) throw new Error('Audio capture requires trusted HTTPS or localhost and a compatible browser or app session.')
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
    <label>Machine name<input aria-label="Machine name" :value="localMachineName" maxlength="80" :disabled="browserInputBusy[inputKey]" @change="saveMachineName(($event.target as HTMLInputElement).value)"></label>
    <label>Local audio input device<select v-model="choice" aria-label="Local audio input device" :disabled="disabled || asking || browserInputBusy[inputKey]">
      <option value="">System default input</option>
      <option v-for="(device, index) in browserInputs" :key="device.deviceId || index" :value="device.deviceId" :disabled="!device.deviceId">{{device.label || `Audio input ${index + 1} · permission needed`}}</option>
      <option v-if="choice && !browserInputs.some(d => d.deviceId === choice)" :value="choice" disabled>Selected input unavailable — refresh and choose another</option>
    </select></label>
    <p role="status" class="feature-note">{{browserInputMessage}}</p>
    <p v-if="browserInputBusy[inputKey]" class="feature-note">This input is in use. Disconnect audio to change devices.</p>
    <button class="button small" :disabled="asking" @click="refreshBrowserInputs">Refresh local audio inputs</button>
    <button class="button small" :disabled="asking || browserInputBusy[inputKey]" @click="permit">{{asking ? 'Waiting for permission…' : 'Grant microphone access'}}</button>
    <p v-if="error" role="alert" class="field-error">{{error}}</p>
    <p v-if="nodeId&&!active" class="feature-note">Enable the audio engine to connect this input.</p>
    <HelpNote>Device selection applies to this browser or app session. The node’s microphone button mutes or unmutes its graph output. Capture connects automatically when the engine is enabled for your assigned input (or the owner’s first unassigned input). Monitor shows the connection and lets you stop capture. Muting keeps the connection open.</HelpNote>
  </section>
</template>
