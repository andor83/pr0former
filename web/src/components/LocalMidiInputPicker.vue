<script setup lang="ts">
import {computed,inject} from 'vue'
import type {LocalMidi} from '../localMidiInput'
const props=defineProps<{node:string;active:boolean;editable:boolean;values?:Record<string,number>}>()
const midi=inject<LocalMidi>('localMidi')!
const state=computed(()=>midi.state(props.node))
const available=typeof navigator.requestMIDIAccess==='function'
</script>
<template>
  <section class="local-midi-picker">
    <p v-if="!available" role="status">Web MIDI is unavailable in this browser or app session. Use a Web MIDI-capable browser, or the server MIDI Input node.</p>
    <label><span class="field-title">Local MIDI device<HelpNote label="Local MIDI device">Connect this node's MIDI outlet to Knobs or Sliders, then click a control to learn its MIDI assignment. Escape or clicking away cancels learning. Device selection stays in this session; MIDI continues while the modal is closed.</HelpNote></span><select v-model="state.device" aria-label="Local MIDI device" :disabled="!editable||state.connected||state.connecting"><option value="">Select a device…</option><option v-for="device in midi.devices.value" :key="device.id" :value="device.id">{{device.name}}</option></select></label>
    <button class="button" :disabled="!available" @click="midi.refresh">Detect MIDI devices</button>
    <button v-if="state.connected||state.connecting" class="button" @click="midi.disconnect(node)">Disconnect MIDI input</button>
    <button v-else class="button primary" :disabled="!available||!editable||!active||!state.device" @click="midi.connect(node)">Connect MIDI input</button>
    <p role="status">{{state.connecting?'Connecting…':state.connected?'Connected · forwarding MIDI to the server':!active?'Enable the audio engine to connect.':'Disconnected'}}</p>
    <p v-if="state.error||midi.error.value" role="alert" class="field-error">{{state.error||midi.error.value}}</p>
    <p>Messages received by server: {{values?._midi_received??0}}</p>

  </section>
</template>
<style scoped>.local-midi-picker{display:grid;gap:10px}.local-midi-picker p{margin:0}</style>
