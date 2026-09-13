<script setup lang="ts">
import {computed,inject,ref,type ComputedRef} from 'vue'
import BrowserInputPicker from './BrowserInputPicker.vue'
import {browserInputBusy,remoteAudioInputs,type LocalAudioAccess} from '../browserInputs'
import {api} from '../api'
const props=defineProps<{projectId:string;node:string;active:boolean;saving:boolean}>()
const access=inject<ComputedRef<LocalAudioAccess>>('localAudioAccess')!
const assign=inject<(node:string,user:string)=>Promise<void>>('assignBrowserInput')!
const connect=inject<(node:string)=>Promise<void>>('connectBrowserInput')!
const disconnect=inject<()=>Promise<void>>('disconnectBrowserInput')!
const mine=computed(()=>!!access.value.userId&&access.value.assignments[props.node]===access.value.userId)
const assigned=computed(()=>access.value.members.find(m=>m.id===access.value.assignments[props.node]))
const busy=ref(false),error=ref('')
async function useDevice(){busy.value=true;error.value='';try{if(remoteAudioInputs.value.some(i=>i.node===props.node&&i.project_id===props.projectId))await api(`/projects/${props.projectId}/media`,'DELETE');await connect(props.node)}catch(e){error.value=String(e)}finally{busy.value=false}}
</script>
<template>
<section class="local-audio-settings">
<label>Assigned ensemble user<select aria-label="Assigned ensemble user" :value="access.assignments[node]||''" :disabled="!access.canAssign||saving||busy" @change="assign(node,($event.target as HTMLSelectElement).value)"><option value="">Unassigned</option><option v-for="member in access.members" :key="member.id" :value="member.id">{{member.username}}</option></select></label>
<template v-if="mine"><BrowserInputPicker :input-key="`${projectId}:${node}`" :node-id="node" :active="active" :disabled="busy"/><button v-if="browserInputBusy[`${projectId}:${node}`]" class="button small" :disabled="busy" @click="disconnect">Disconnect this device</button><button v-else class="button small" :disabled="!active||busy||browserInputBusy[`${projectId}:${node}`]" @click="useDevice">Use this device</button></template>
<p v-else class="feature-note">{{assigned?`${assigned.username} chooses the input on their own device.`:'Assign an ensemble member to connect an input.'}}</p>
<p v-if="error" role="alert" class="field-error">{{error}}</p>
</section>
</template>
