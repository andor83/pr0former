<script setup lang="ts">
import {ref} from 'vue'
import {api} from '../api'
const busy=ref(false),error=ref('')
const props=defineProps<{projectId?:string;nodeId:string;editable:boolean;values?:Record<string,number>;active:boolean;stale:boolean}>()
function value(track:number,key:string){return props.values?.[`_track_${track}_${key}`]||0}
function state(track:number){if(!props.active)return 'Engine off';if(props.stale)return 'Stale';if(value(track,'pending_record'))return 'Record next bar';if(value(track,'pending_play'))return 'Play next bar';if(value(track,'recording'))return 'Recording';if(value(track,'playing'))return 'Playing';return value(track,'seconds')?'Ready':'Empty'}
async function clear(track:number){if(!props.projectId)return;busy.value=true;error.value='';try{await api(`/projects/${props.projectId}/loops/clear`,'PUT',{node:props.nodeId,track})}catch(e){error.value=String(e)}finally{busy.value=false}}
</script>
<template>
<section class="parameter-row looper-status" aria-label="Loop tracks">
  <h3>Loop tracks</h3>
  <table><thead><tr><th>Track</th><th>Status</th><th>Length</th><th>Clear</th></tr></thead><tbody><tr v-for="track in 8" :key="track" :aria-label="`Loop track ${track}`"><th>{{track}}</th><td>{{state(track)}}<small v-if="active&&!stale&&value(track,'full')">Capacity reached</small></td><td>{{active&&!stale?`${value(track,'seconds').toFixed(2)} s`:'—'}}</td><td><button :aria-label="`Clear loop track ${track}`" :disabled="!editable||busy||!projectId" @click="clear(track)">Clear</button></td></tr></tbody></table>
  <p v-if="error" role="alert">{{error}}</p>
  <p class="feature-note">Send track numbers 1–8; return the command input to 0 before repeating that track. Freeform starts immediately. Beat mode waits for the next bar of the running graph clock. Stops are immediate.</p>
  <p class="feature-note">Stopping recording does not automatically play it. Start playback to loop the captured audio. Recording again replaces that track. Completed recordings are saved on this server per project and node. Disabling the engine finishes and saves ongoing recordings. Clear stops the track and deletes its saved audio. Saved tracks load ready to play when the engine starts.</p>
</section>
</template>
<style scoped>
.looper-status h3{font-size:14px;margin:0 0 12px}.looper-status table{width:100%;border-collapse:collapse;font-size:12px}.looper-status th,.looper-status td{text-align:left;padding:8px;border-bottom:1px solid var(--line)}.looper-status tbody th{color:var(--cyan);width:48px}.looper-status td:last-child{text-align:right;font-variant-numeric:tabular-nums}.looper-status small{display:block;color:var(--amber);font-size:10px}.looper-status .feature-note{line-height:1.5}
</style>
