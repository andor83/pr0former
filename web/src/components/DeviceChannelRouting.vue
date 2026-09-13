<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { api } from '../api'
import type { GraphNode } from '../types'
const props = defineProps<{node: GraphNode; sampleRate: number; disabled: boolean}>()
const emit = defineEmits<{change: [parameters: Record<string, number>]}>()
type Device = {id: number; name: string; channels: number | null; error?: string}
const devices = ref<{input_interfaces: Device[]; interfaces: Device[]}>()
const settings = ref<{input_interfaces: {id:number;enabled:boolean}[]; interfaces: {id:number;enabled:boolean}[]}>()
const error = ref('')
const input = computed(() => props.node.kind === 'input')
const selected = computed(() => {
  const key = input.value ? 'input_interfaces' : 'interfaces'
  const available = devices.value?.[key] || []
  const id = props.node.parameters.interface || 0
  if (id) return available.filter(d => d.id === id)
  const enabled = (settings.value?.[key] || []).filter(d => d.enabled).flatMap(d => available.filter(a => a.id === d.id))
  return input.value ? enabled.slice(0, 1) : enabled
})
const count = computed(() => Math.max(0, ...selected.value.map(d => d.channels || 0)))
const routes = computed(() => Array.from({length: props.node.channels}, (_, i) => props.node.parameters[`route_${i + 1}`] ?? -1))
// Keep saved unavailable destinations visible so a device change never rewrites the patch.
const options = computed(() => Array.from({length: count.value || 64}, (_, i) => i + 1))
async function refresh() {
  try {
    const [d, s] = await Promise.all([api<NonNullable<typeof devices.value>>('/devices'), api<NonNullable<typeof settings.value>>('/system/audio')])
    devices.value = d; settings.value = s; error.value = ''
  } catch (e) { error.value = String(e) }
}
function preset(mode: 'sequential' | 'none' | 'stereo') {
  const entries = routes.value.map((_, i) => [`route_${i + 1}`, mode === 'none' ? 0 : mode === 'stereo' ? i % 2 + 1 : i + 1])
  const parameters = Object.fromEntries(entries)
  // Average each destination's contributing channels to leave full-scale headroom.
  if (mode === 'stereo') parameters.gain = -20 * Math.log10(Math.ceil(props.node.channels / 2))
  emit('change', parameters)
}
onMounted(refresh)
watch(() => props.sampleRate, refresh)
</script>
<template>
  <section class="parameter-row device-routing" aria-label="Physical channel routing">
    <div class="parameter-heading"><h3 aria-label="Channel routing">Channel routing<HelpNote label="Channel routing">{{input ? 'Each signal channel reads its chosen physical input. Reusing an input duplicates it; None produces silence.' : 'Signal channels sharing a destination are summed. Mix to stereo sends odd channels left and even channels right, and adjusts Output gain for headroom. Manual mappings retain the current gain.'}}<template v-if="selected.length > 1"><br /><br />This mapping applies to every enabled output interface. Destinations beyond an interface’s channel count are ignored.</template></HelpNote></h3><span class="small-tag">{{node.channels}} SIGNAL CHANNELS</span></div>
    <p v-if="!devices && !error" class="feature-note">Checking interface channels…</p>
    <p v-for="device in selected" :key="device.id" class="feature-note">{{device.name}} · {{device.channels ? `${device.channels} physical ${input ? 'inputs' : 'outputs'} at ${sampleRate} Hz` : device.error || 'Channel count unavailable'}}</p>
    <p v-if="devices && !selected.length" class="feature-note">No available interface selected. Configure routes for physical channels 1–64; unavailable channels stay silent.</p>

    <p v-if="error" class="field-error" role="alert">{{error}}</p>
    <table>
      <thead><tr><th scope="col">Signal channel</th><th scope="col">{{input ? 'Physical source' : 'Physical destination'}}</th></tr></thead>
      <tbody><tr v-for="(route, i) in routes" :key="i">
        <th scope="row">{{i + 1}}</th>
        <td><select :aria-label="`Signal ${i + 1} physical ${input ? 'source' : 'destination'}`" :value="route" :disabled="disabled" @change="emit('change', {[`route_${i + 1}`]: Number(($event.target as HTMLSelectElement).value)})">
          <option :value="-1">Automatic · {{input ? 'legacy offset' : `channel ${i + 1}`}}</option>
          <option :value="0">None · {{input ? 'silence' : 'ignore'}}</option>
          <option v-for="ch in options" :key="ch" :value="ch">Channel {{ch}}{{!count ? ' · unverified' : ''}}</option>
          <option v-if="route > count && count" :value="route">Channel {{route}} · unavailable</option>
        </select></td>
      </tr></tbody>
    </table>
    <div class="routing-actions">
      <button class="button small" :disabled="disabled" @click="preset('sequential')">Map 1:1</button>
      <button class="button small" :disabled="disabled" @click="preset('none')">Disconnect all</button>
      <button v-if="!input && node.channels > 1" class="button small" :disabled="disabled || (count > 0 && count < 2)" @click="preset('stereo')">Mix to stereo</button>
      <button class="button small" @click="refresh">Refresh channels</button>
    </div>

  </section>
</template>
<style scoped>
.device-routing h3 { margin: 0; font-size: 14px; }
table { width: 100%; border-collapse: collapse; margin: 12px 0; }
th { text-align: left; font-weight: 500; color: var(--muted, #a5a9b6); }
th, td { padding: 5px 8px; }
td select { width: 100%; min-height: 40px; }
.routing-actions { display: flex; flex-wrap: wrap; gap: 8px; }
</style>
