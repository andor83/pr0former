<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { api } from '../api'
import type { GraphNode, IoConfig } from '../types'
const props=defineProps<{node:GraphNode;disabled:boolean}>()
const emit=defineEmits<{change:[value:IoConfig]}>()
const draft=ref<IoConfig>({port:'',address:'',destination:''})
const devices=ref<{midi_inputs:string[];midi_outputs:string[]}>({midi_inputs:[],midi_outputs:[]})
const error=ref('')
const midi=computed(()=>props.node.kind.startsWith('midi_') && props.node.kind!=='midi_to_osc')
const input=computed(()=>['midi_input','osc_to_midi','osc_input'].includes(props.node.kind))
const ports=computed(()=>input.value?devices.value.midi_inputs:devices.value.midi_outputs)
watch(()=>props.node, node=>{draft.value={port:'',address:'',destination:'',...node.io}},{immediate:true})
async function refresh(){try{devices.value=await api('/devices');error.value=''}catch(e){error.value=String(e)}}
function change(){emit('change',{...draft.value})}
watch(midi,enabled=>{if(enabled)void refresh()},{immediate:true})
</script>
<template>
  <section class="parameter-row node-io-settings">
    <template v-if="midi"><label :for="`midi-port-${node.id}`">MIDI {{input?'input':'output'}} port</label><select :id="`midi-port-${node.id}`" v-model="draft.port" :disabled="disabled" @change="change"><option value="">Unassigned — disconnected</option><option v-if="draft.port && !ports.includes(draft.port)" :value="draft.port">{{draft.port}} (unavailable)</option><option v-for="port in ports" :key="port" :value="port">{{port}}</option></select><button class="text-button" @click="refresh">Refresh MIDI ports</button><p v-if="!ports.length" class="feature-note">No physical MIDI ports found. Connect your keyboard or MIDI interface and refresh.</p></template>
    <template v-else><label :for="`osc-address-${node.id}`">OSC address<HelpNote label="OSC routing"><template v-if="!input">Destination accepts a DNS hostname or IPv4 address followed by :port. Names resolve on the server; the destination must have an IPv4 address.</template><template v-if="node.kind==='osc_input'||node.kind==='osc_output'">Enable OSC {{input?'receiving':'sending'}} in System settings. Messages carry one argument: a number, or text when the node is in text mode.</template><template v-else>Enable OSC {{input?'receiving':'sending'}} in System settings. Note messages contain two integers: pitch and velocity; velocity 0 releases that pitch.</template></HelpNote></label><input aria-label="OSC address" :id="`osc-address-${node.id}`" v-model="draft.address" placeholder="/note" :disabled="disabled" @change="change"><template v-if="!input"><label :for="`osc-destination-${node.id}`">OSC destination</label><input :id="`osc-destination-${node.id}`" v-model="draft.destination" placeholder="synth.local:9000" :disabled="disabled" @change="change"></template></template>
    <p v-if="error" class="field-error">{{error}}</p>
  </section>
</template>
<style scoped>
.node-io-settings>label{display:block;margin-bottom:8px}.node-io-settings>input,.node-io-settings>select{width:100%;margin-bottom:12px}
</style>
