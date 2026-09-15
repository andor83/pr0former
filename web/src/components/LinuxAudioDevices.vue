<script setup lang="ts">
import {onMounted,ref} from 'vue'
import {api} from '../api'
const data=ref<any>(),busy=ref(false),error=ref('')
async function refresh(){busy.value=true;error.value='';try{data.value=await api('/system/audio/linux')}catch(e){error.value=String(e)}finally{busy.value=false}}
onMounted(refresh)
</script>
<template>
<section v-if="data?.supported||error" class="linux-audio" aria-label="Linux audio devices">
  <header><h3>Linux audio devices</h3><button class="button small" :disabled="busy" @click="refresh">{{busy?'Refreshing…':'Refresh Linux audio details'}}</button></header>
  <p class="feature-note">Enable a named PulseAudio / PipeWire route in Active interfaces, then choose it in your Audio output node and select its channels. ALSA direct routes bypass the desktop mixer and may require exclusive access. Disable duplicate generic routes to avoid playing twice. Named routes start unchecked.</p>
  <p class="feature-note">Ports and profiles below describe the audio server's current configuration; they are not separate streams. Change the card profile or active port in your Linux sound settings, then refresh. Devices are discovered in the performance server's user session.</p>
  <p v-if="error||data?.error" class="field-error">{{error||data?.error}}</p>
  <p v-if="data?.notice" class="feature-note">{{data.notice}}</p>
  <p v-if="data?.backend">{{data.backend}}</p>
  <p v-if="data?.available&&!data.endpoints?.length">No audio sinks or sources reported.</p>
  <article v-for="endpoint in data?.endpoints" :key="`${endpoint.direction}-${endpoint.name}`" class="interface-card">
    <strong>{{endpoint.description||endpoint.name}}</strong><span> · {{endpoint.direction}}<template v-if="endpoint.default"> · default</template><template v-if="endpoint.device_class==='monitor'"> · loopback monitor (not physical input)</template></span>
    <code>{{endpoint.name}}</code>
    <p>{{endpoint.state}}<template v-if="endpoint.channel_map"> · {{endpoint.channel_map}}</template><template v-if="endpoint.sample_specification"> · {{endpoint.sample_specification}}</template></p>
    <p v-if="endpoint.bus||endpoint.hardware_path">{{endpoint.bus}} {{endpoint.hardware_path}}</p>
    <ul v-if="endpoint.ports?.length"><li v-for="port in endpoint.ports" :key="port.name">{{port.description||port.name}} · {{port.availability||'availability unknown'}}<strong v-if="port.name===endpoint.active_port"> · active</strong></li></ul>
  </article>
  <details v-for="card in data?.cards" :key="card.name"><summary>{{card.description||card.name}} · {{card.active_profile||'no active profile'}}</summary><code>{{card.name}}</code><h4>Ports</h4><ul><li v-for="port in card.ports" :key="port.name">{{port.description||port.name}} · {{port.availability||'availability unknown'}}</li></ul><h4>Profiles</h4><ul><li v-for="profile in card.profiles" :key="profile.name">{{profile.description||profile.name}} · {{profile.availability||'availability unknown'}}<strong v-if="profile.name===card.active_profile"> · active</strong></li></ul></details>
</section>
</template>
<style scoped>
.linux-audio{margin:20px 0;border-top:1px solid var(--line);padding-top:16px;font-size:12px}.linux-audio header{display:flex;align-items:center;justify-content:space-between;gap:12px}.linux-audio p{margin:8px 0}.linux-audio code{display:block;overflow-wrap:anywhere;color:var(--muted);font-size:11px}.linux-audio ul{padding-left:20px;line-height:1.6}.linux-audio details{margin:12px 0}.linux-audio summary{cursor:pointer;min-height:28px;color:var(--amber)}
</style>
