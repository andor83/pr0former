<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { Activity, Clock3, Cpu, Headphones } from '@lucide/vue'
import type { GraphNode, Telemetry, HardwareLevels, HardwareDeviceLevels } from '../types'
import { api } from '../api'
import MonitorPanel from './MonitorPanel.vue'
import LevelMeter from './LevelMeter.vue'
import {remoteAudioInputs,type RemoteAudioInput} from '../browserInputs'
const props = defineProps<{projectId:string;autoInput?:string;nodes:GraphNode[];active:boolean;stale:boolean;telemetry:Telemetry|null;hardware:HardwareLevels|null;hardwareStale:boolean;age:number;sampleRate:number;blockSize:number;visible:boolean;stage:boolean}>()
const monitorPanel = ref<InstanceType<typeof MonitorPanel>>()
const monitorState = computed(() => monitorPanel.value?.state ?? 'disconnected')
const monitorError = computed(() => monitorPanel.value?.error ?? '')
const monitorLevel = computed(() => monitorPanel.value?.meter ?? 0)
defineExpose({ toggleMonitor: () => monitorPanel.value?.toggle(), connectInput: (node:string) => monitorPanel.value?.connectInput(node), monitorState, monitorError, monitorLevel })
const inventory = ref<{input_interfaces:{id:number;name:string;channels:number|null}[]}>()
const deviceError = ref('')
const inputs = computed(() => {
  const live = props.hardware?.inputs || []
  const available = inventory.value?.input_interfaces || []
  return [...available.map(device => live.find(d=>d.id===device.id) || {
    id:device.id,name:device.name,channels:device.channels||0,
    levels:Array.from({length:device.channels||0},(_,i)=>({channel:i+1,peak:undefined})),
  }), ...live.filter(d=>!available.some(a=>a.id===d.id))] as HardwareDeviceLevels[]
})
const outputs = computed(() => props.hardware?.project_id ? props.hardware.outputs : [])
const resources = ref<{cpu_percent:number|null;resident_bytes:number|null;sampled_at_ms:number}>()
const resourceError = ref('')
let timer: ReturnType<typeof setInterval> | undefined
let generation=0
async function refresh(discover = false) {
  const current=generation
  const [stats,devices] = await Promise.allSettled([
    api<NonNullable<typeof resources.value>>('/system/stats'),
    discover ? api<NonNullable<typeof inventory.value>>('/devices') : Promise.resolve(undefined),
  ])
  if(current!==generation)return
  if(stats.status==='fulfilled'){resources.value=stats.value;resourceError.value=''}
  else {resources.value=undefined;resourceError.value='Resource stats unavailable'}
  if(discover){
    if(devices.status==='fulfilled'){inventory.value=devices.value;deviceError.value=''}
    else {deviceError.value='Device discovery unavailable'}
  }
}
watch(()=>props.visible, visible=>{
  generation++;clearInterval(timer)
  if(visible){void refresh(true);timer=setInterval(()=>{void refresh()},2000)}
},{immediate:true})
onBeforeUnmount(()=>{generation++;clearInterval(timer)})
let inputGeneration=0
async function refreshInputs(){const generation=inputGeneration;try{const inputs=await api<RemoteAudioInput[]>(`/projects/${props.projectId}/media`);if(generation===inputGeneration)remoteAudioInputs.value=inputs}catch{if(generation===inputGeneration)remoteAudioInputs.value=[]}}
void refreshInputs()
const inputTimer=setInterval(refreshInputs,1500)
onBeforeUnmount(()=>{inputGeneration++;clearInterval(inputTimer);remoteAudioInputs.value=[]})
const localInputs=computed(()=>remoteAudioInputs.value.filter(i=>i.project_id===props.projectId))
const cpu = computed(()=>resources.value?.cpu_percent == null ? '—' : `${resources.value.cpu_percent.toFixed(1)}%`)
const memory = computed(()=>resources.value?.resident_bytes == null ? '—' : `${(resources.value.resident_bytes/1024/1024).toFixed(0)} MB`)
</script>
<template>
  <section class="monitor-workspace" :class="{'stage-monitor':stage}" aria-label="Performance monitor">
    <aside class="monitor-sidebar" aria-label="Monitor controls and statistics">
      <div v-show="!stage" class="monitor-title"><div class="eyebrow">PERFORMANCE</div><h2>Monitor</h2><span class="monitor-live"><i class="status-dot" :class="{live:active&&!stale}"></i>{{active ? stale ? 'Timing lost' : 'Live engine' : 'Engine inactive'}}</span></div>
      <section v-show="!stage" class="monitor-stat-card"><h3><Clock3 :size="14" />Clock sync</h3><strong class="clock-readout">{{active&&!stale ? `${Math.max(0,Math.round(age))} ms` : '—'}}</strong><span class="stat-caption">{{active ? stale ? 'Waiting for engine timing' : 'Latest timing snapshot age' : 'Enable the audio engine for timing'}}</span><div class="mini-stats"><span>Rate<strong>{{sampleRate/1000}} kHz</strong></span><span>Block<strong>{{blockSize}} frames</strong></span></div></section>
      <section v-if="active && !stale && telemetry?.worker_max_work_us !== undefined" v-show="!stage" class="monitor-stat-card"><h3>Worker observations</h3><div class="mini-stats"><span>Peak work<strong>{{(telemetry.worker_max_work_us / 1000).toFixed(2)}} ms</strong></span><span>Peak block gap<strong>{{((telemetry.worker_max_block_gap_us || 0) / 1000).toFixed(2)}} ms</strong></span></div><span class="stat-caption">Since graph load; block gaps include normal pacing.</span></section>
      <section v-show="!stage" class="monitor-stat-card"><h3><Cpu :size="14" />Server process</h3><div class="resource-readouts"><div><span>CPU</span><strong>{{cpu}}</strong></div><div><span>Memory</span><strong>{{memory}}</strong></div></div><p class="stat-caption">{{resourceError || 'CPU: 100% = one core · Memory: resident use'}}</p></section>
      <MonitorPanel :auto-input="autoInput" ref="monitorPanel" :project-id="projectId" :active="active" :nodes="nodes" compact />
      <section v-show="!stage" class="monitor-stat-card"><h3><Activity :size="14" />Audio status</h3><dl><div><dt>Hardware output</dt><dd>{{active&&!stale ? telemetry?.hardware_enabled ? 'Enabled' : 'Muted' : '—'}}</dd></div><div><dt>Underrun frames</dt><dd>{{active&&!stale ? telemetry?.underruns ?? '—' : '—'}}</dd></div></dl><p v-if="active && telemetry?.error" class="field-error">{{telemetry.error}}</p></section>
    </aside>
    <div v-show="!stage" class="meter-banks">
      <section v-if="localInputs.length" class="meter-bank local-input-bank" aria-label="Connected local audio inputs">
        <header><h2>Connected local inputs <span>{{localInputs.length}}</span></h2></header>
        <div class="device-groups"><section v-for="input in localInputs" :key="input.node" class="device-group" style="--channels:1">
          <div class="device-heading"><h3>{{input.user_name}} ({{input.machine_name}})</h3></div>
          <span class="stat-caption">{{input.state}} · {{telemetry?.values[input.node]?.mute ? 'Muted' : 'Unmuted'}}</span>
          <div class="vu-strips"><LevelMeter v-for="channel in nodes.find(n=>n.id===input.node)?.channels||2" :key="channel" :device="`${input.user_name} (${input.machine_name})`" direction="local input" :channel="channel" :peak="telemetry?.values[input.node]?.[`_level${channel}`]" :stale="stale||!active" /></div>
        </section></div>
      </section>
      <section v-for="bank in [{title:'Inputs',direction:'input',devices:inputs},{title:'Outputs',direction:'output',devices:outputs}]" :key="bank.title" class="meter-bank" :aria-label="`${bank.title} VU meters`">
        <header><h2>{{bank.title}}<span>{{bank.devices.length}} devices</span></h2><span class="bank-caption">{{bank.direction==='input' ? 'ALL HARDWARE INPUT CHANNELS' : 'ACTIVE GRAPH → HARDWARE'}} · dBFS</span></header>
        <div v-if="bank.devices.length" class="device-groups">
          <section v-for="device in bank.devices" :key="device.id" class="device-group" :data-device="device.id" :style="{'--channels':Math.max(1,device.levels.length)}" :aria-label="`${device.name} ${bank.direction} channels`">
            <div class="device-heading"><h3>{{device.name}}</h3><span>{{bank.direction==='input' ? `${device.channels} channels` : `${device.levels.length} of ${device.channels} channels routed`}}</span></div>
            <div v-if="device.levels.length" class="meter-bank-body">
              <div class="vu-scale" aria-hidden="true"><span v-for="db in [0,-12,-24,-36,-48,-60]" :key="db">{{db}}</span></div>
              <div class="vu-strips"><LevelMeter v-for="level in device.levels" :key="level.channel" :device="device.name" :direction="bank.direction" :channel="level.channel" :peak="level.peak" :stale="hardwareStale || level.peak === undefined" /></div>
            </div>
            <p v-else class="feature-note">Channel count unavailable at this sample rate.</p>
          </section>
        </div>
        <div v-else class="meters-empty"><Headphones :size="25" /><p>{{bank.direction==='input' ? 'No hardware inputs discovered' : 'No active graph routes to hardware outputs'}}</p><span>{{bank.direction==='input' ? deviceError || 'Enable input devices in System settings to capture their channels, even without input nodes.' : 'Connect Audio output nodes to enabled physical devices. Browser monitor feeds are separate.'}}</span></div>
      </section>
    </div>
  </section>
</template>
<style scoped>
.meter-banks:has(.local-input-bank){grid-template-rows:repeat(3,minmax(0,1fr))}.local-input-bank .device-group{flex-basis:230px}.local-input-bank .device-heading{height:auto}.local-input-bank .vu-strips{flex:1;margin-top:8px}.local-input-bank .device-heading h3{white-space:normal}
.monitor-workspace{display:grid;grid-template-columns:280px minmax(0,1fr);flex:1;min-height:0;overflow:hidden;background:#111718}.monitor-sidebar{padding:20px 16px;overflow:auto;min-width:0;border-right:1px solid var(--line);background:#171e1f;display:flex;flex-direction:column;gap:14px}.monitor-title{padding:0 4px 4px}.monitor-title h2{font-size:23px;margin:2px 0 9px}.monitor-live{font-size:10px;color:var(--muted);display:flex;align-items:center;gap:7px}.monitor-stat-card{border:1px solid var(--line);border-radius:7px;padding:14px;background:#1b2324}.monitor-stat-card h3{display:flex;align-items:center;gap:8px;font-size:11px;color:#c1cdca;margin-bottom:12px}.clock-readout{display:block;font-size:25px;font-weight:500;color:var(--cyan);font-variant-numeric:tabular-nums}.stat-caption{display:block;font-size:9px;line-height:1.6;color:var(--muted);margin-top:4px}.mini-stats{display:flex;gap:25px;margin-top:14px}.mini-stats span,.resource-readouts span{font-size:9px;color:var(--muted)}.mini-stats strong{display:block;font-size:11px;font-weight:500;color:#c9d3d0;margin-top:4px}.resource-readouts{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}.resource-readouts strong{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-size:21px;font-weight:500;font-variant-numeric:tabular-nums;margin:4px 0 7px}dl{margin:0;font-size:10px}dl div{display:flex;justify-content:space-between;gap:12px;margin-top:9px}dt{color:var(--muted)}dd{margin:0}.meter-banks{min-width:0;min-height:0;display:grid;grid-template-rows:repeat(2,minmax(0,1fr));gap:16px;padding:20px}.meter-bank{min-height:0;min-width:0;background:#192122;border:1px solid var(--line);border-radius:8px;display:flex;flex-direction:column;padding:16px 18px 10px}.meter-bank header{flex-shrink:0;display:flex;align-items:center;justify-content:space-between;gap:12px;margin-bottom:15px}.meter-bank h2{font-size:17px;display:flex;align-items:center;gap:10px;letter-spacing:-.3px}.meter-bank h2 span{font-size:10px;color:var(--muted);font-family:'DM Sans',sans-serif;background:#273031;padding:3px 7px;border-radius:4px}.bank-caption{font-size:8px;letter-spacing:1px;color:var(--muted)}.meter-bank-body{display:flex;min-height:0;flex:1;gap:10px}.vu-strips{display:grid;grid-auto-flow:column;grid-auto-columns:84px;grid-template-rows:minmax(0,1fr);gap:8px;min-height:0;min-width:0}.vu-scale{width:23px;flex-shrink:0;min-height:0;margin:21px 0 74px;display:flex;flex-direction:column;justify-content:space-between;font-size:9px;color:#70827c;font-variant-numeric:tabular-nums}.meters-empty{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:9px;color:var(--muted);font-size:12px;text-align:center}.meters-empty span{font-size:10px;max-width:280px;line-height:1.6}.stage-monitor{display:block;flex:0 0 auto;max-height:38dvh;overflow:auto}.stage-monitor .monitor-sidebar{border:0;padding:12px 18px}.stage-monitor .monitor-sidebar>:not(.browser-monitor){display:none!important}@media(max-width:1000px){.monitor-workspace:not(.stage-monitor){grid-template-columns:240px minmax(0,1fr)}.monitor-sidebar{padding:12px;gap:10px}.meter-banks{padding:12px;gap:12px}.meter-bank{padding:12px 10px 8px}.bank-caption{display:none}}@media(max-width:600px){.monitor-workspace:not(.stage-monitor){grid-template-columns:190px minmax(0,1fr)}.monitor-sidebar{padding:8px}.monitor-stat-card{padding:10px}.meter-banks{padding:8px;gap:8px}.resource-readouts strong{font-size:17px}}@media(max-height:650px){.meter-banks{grid-template-rows:minmax(0,1fr);grid-template-columns:repeat(2,minmax(0,1fr))}.meter-bank header{margin-bottom:8px}}
.device-groups{display:flex;gap:16px;min-height:0;flex:1;overflow-x:auto;overflow-y:hidden;padding-bottom:4px}.device-group{display:flex;flex-direction:column;flex:0 0 calc(43px + var(--channels)*92px);min-width:0;min-height:0;padding:0 10px;border-left:1px solid #33403e}.device-group:first-child{border-left:0;padding-left:0}.device-heading{height:18px;flex-shrink:0;min-width:0;display:flex;align-items:baseline;gap:12px;justify-content:space-between;margin-bottom:10px}.device-heading h3{font-size:12px;color:#cbd8d2;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.device-heading span{max-width:55%;overflow:hidden;text-overflow:ellipsis;font-size:9px;color:var(--muted);flex-shrink:0;white-space:nowrap}
@media (max-width:600px), (max-height:500px) and (pointer:coarse){.monitor-workspace:not(.stage-monitor){grid-template-columns:144px minmax(0,1fr)}.monitor-sidebar{padding:6px;gap:6px}.monitor-stat-card{padding:7px}.monitor-title h2{font-size:18px}.resource-readouts{gap:6px}.resource-readouts strong{font-size:14px}.meter-banks{padding:6px;gap:6px}.meter-bank{padding:8px 6px}.meter-bank h2{font-size:13px}.device-group{padding:0 6px}}
</style>
