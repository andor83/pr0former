<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { Headphones, Mic, Radio, Square } from '@lucide/vue'
import BrowserInputPicker from './BrowserInputPicker.vue'
import { localMachineName, browserInputChoices, browserInputBusy, browserInputState, captureError, refreshBrowserInputs } from '../browserInputs'
import { api } from '../api'
import type { GraphNode } from '../types'
const props = defineProps<{ compact?: boolean; autoInput?:string; projectId: string; active: boolean; nodes: GraphNode[] }>()
const error = ref(''), state = ref('disconnected'), microphone = ref(false), inputNode = ref(''), monitorNode = ref(''), volume = ref(0.5)
const monitor = ref<HTMLAudioElement>(), settings = ref(''), stats = ref({ lost: 0, jitter: 0, buffer: 0, rtt: 0 })
type Attempt = { inputKey?: string; peer: RTCPeerConnection; abort: AbortController; stream?: MediaStream; offer?: Promise<RTCSessionDescriptionInit>; timer?: ReturnType<typeof setInterval> }
let current: Attempt | null = null
// Peak level of the received monitor stream, 0–1 on a 60 dB scale, measured
// with an analyser so the footer button can double as a VU meter.
const meter = ref(0)
let context: AudioContext | undefined, source: MediaStreamAudioSourceNode | undefined, analyser: AnalyserNode | undefined, meterFrame = 0
let samples = new Float32Array(0)
function tickMeter() {
  if (!analyser) return
  analyser.getFloatTimeDomainData(samples)
  let peak = 0
  for (const value of samples) peak = Math.max(peak, Math.abs(value))
  const target = Math.max(0, Math.min(1, (20 * Math.log10(Math.max(peak, 1e-5)) + 60) / 60))
  meter.value = target > meter.value ? target : meter.value * 0.9 + target * 0.1
  meterFrame = requestAnimationFrame(tickMeter)
}
function startMeter(stream: MediaStream) {
  stopMeter()
  try {
    context ??= new AudioContext()
    source = context.createMediaStreamSource(stream)
    analyser = context.createAnalyser(); analyser.fftSize = 1024; analyser.smoothingTimeConstant = 0
    samples = new Float32Array(analyser.fftSize)
    source.connect(analyser)
    void context.resume()
    tickMeter()
  } catch { meter.value = 0 }
}
function stopMeter() {
  cancelAnimationFrame(meterFrame)
  source?.disconnect(); analyser?.disconnect(); source = undefined; analyser = undefined; meter.value = 0
}
async function disconnect() {
  const attempt = current
  if (!attempt) return
  current = null
  stopMeter()
  if (attempt.inputKey) delete browserInputBusy[attempt.inputKey]
  attempt.abort.abort(); clearInterval(attempt.timer)
  attempt.stream?.getTracks().forEach(track => track.stop())
  attempt.peer.ontrack = null; attempt.peer.onconnectionstatechange = null; attempt.peer.close()
  if (monitor.value) { monitor.value.pause(); monitor.value.srcObject = null }
  // An offer already submitted can create a server peer after cancellation.
  // Finish that request before deleting it, and do not permit a replacement yet.
  state.value = attempt.offer ? 'disconnecting' : 'disconnected'
  if (attempt.offer) {
    try { await attempt.offer } catch { /* The server may have rejected the offer. */ }
    try { await api(`/projects/${props.projectId}/media`, 'DELETE') } catch { /* Disconnected server or expired session. */ }
    state.value = 'disconnected'
  }
}
function gather(attempt: Attempt): Promise<void> {
  const pc = attempt.peer, signal = attempt.abort.signal
  if (signal.aborted) return Promise.reject(new Error('Connection cancelled'))
  if (pc.iceGatheringState === 'complete') return Promise.resolve()
  return new Promise((resolve, reject) => {
    const finish = (error?: Error) => {
      clearTimeout(timeout); pc.removeEventListener('icegatheringstatechange', changed); signal.removeEventListener('abort', cancelled)
      if (error) reject(error); else resolve()
    }
    const changed = () => { if (pc.iceGatheringState === 'complete') finish() }
    const cancelled = () => finish(new Error('Connection cancelled'))
    const timeout = setTimeout(() => finish(new Error('ICE gathering timed out')), 10000)
    pc.addEventListener('icegatheringstatechange', changed); signal.addEventListener('abort', cancelled, { once: true })
    changed()
  })
}
async function resume() {
  void context?.resume()
  try { await monitor.value?.play(); error.value = '' }
  catch { error.value = 'Browser playback is blocked. Tap Resume audio to try again.' }
}
async function connect() {
  if (!props.active || state.value !== 'disconnected') return
  error.value = ''; settings.value = ''; stats.value = { lost: 0, jitter: 0, buffer: 0, rtt: 0 }; state.value = 'connecting'
  let attempt: Attempt | null = null
  try {
    if (!window.isSecureContext) throw new Error('Open this server over trusted HTTPS to use browser audio.')
    if (microphone.value && !inputNode.value) throw new Error('Select your assigned local audio input node first.')
    const pc = new RTCPeerConnection({ iceServers: [] })
    attempt = { peer: pc, abort: new AbortController() }; current = attempt
    const owned = attempt
    pc.onconnectionstatechange = () => {
      if (current !== owned) return
      state.value = pc.connectionState === 'disconnected' ? 'interrupted' : pc.connectionState
    }
    pc.ontrack = async event => {
      if (current !== owned || !monitor.value) return
      const stream = event.streams[0] || new MediaStream([event.track])
      monitor.value.srcObject = stream; monitor.value.volume = volume.value
      startMeter(stream)
      try { await monitor.value.play() } catch { if (current === owned) error.value = 'Tap Resume audio to permit monitor playback.' }
    }
    if (microphone.value) {
      const key = `${props.projectId}:${inputNode.value}`
      owned.inputKey = key; browserInputBusy[key] = true
      const deviceId = browserInputChoices[key]
      const captured = await navigator.mediaDevices.getUserMedia({ audio: { ...(deviceId ? { deviceId: { exact: deviceId } } : {}), channelCount: 2, echoCancellation: false, noiseSuppression: false, autoGainControl: false }, video: false })
      if (current !== owned) { captured.getTracks().forEach(track => track.stop()); return }
      void refreshBrowserInputs()
      owned.stream = captured
      settings.value = JSON.stringify(captured.getAudioTracks()[0].getSettings(), null, 2)
      captured.getAudioTracks().forEach(track => pc.addTrack(track, captured))
    } else pc.addTransceiver('audio', { direction: 'recvonly' })
    const offer = await pc.createOffer()
    if (current !== owned) return
    await pc.setLocalDescription(offer)
    if (current !== owned) return
    await gather(owned)
    if (current !== owned) return
    owned.offer = api<RTCSessionDescriptionInit>(`/projects/${props.projectId}/media`, 'POST', { sdp: pc.localDescription?.sdp, machine_name:localMachineName.value, input_node: microphone.value ? inputNode.value : null, monitor_node: monitorNode.value || null })
    const answer = await owned.offer
    if (current !== owned) return
    await pc.setRemoteDescription(answer)
    if (current !== owned) return
    owned.timer = setInterval(async () => {
      try {
        const reports = await pc.getStats()
        if (current !== owned) return
        reports.forEach(report => {
          if (report.type === 'inbound-rtp' && report.kind === 'audio') stats.value = { ...stats.value, lost: report.packetsLost || 0, jitter: (report.jitter || 0) * 1000, buffer: report.jitterBufferEmittedCount ? report.jitterBufferDelay / report.jitterBufferEmittedCount * 1000 : 0 }
          if (report.type === 'candidate-pair' && report.state === 'succeeded') stats.value.rtt = (report.currentRoundTripTime || 0) * 1000
        })
      } catch { /* Stats can race with a closed connection. */ }
    }, 1000)
  } catch (e) {
    if (attempt && current !== attempt) return
    error.value = captureError(e)
    if (attempt) await disconnect(); else state.value = 'disconnected'
  }
}
async function toggle() {
  if (state.value === 'disconnecting') return
  if (current) await disconnect()
  else await connect()
}
async function connectInput(node:string) {
  if(current?.inputKey===`${props.projectId}:${node}` && !['failed','interrupted','closed'].includes(state.value)) return
  if(state.value!=='disconnected')await disconnect()
  microphone.value=true;inputNode.value=node
  await connect()
}
defineExpose({ toggle, state, error, meter, connectInput, disconnect })
function level() { if (monitor.value) monitor.value.volume = volume.value }
watch(state, value => { if(inputNode.value) browserInputState[`${props.projectId}:${inputNode.value}`]=value }, {flush:'sync'})
let reconciliation=Promise.resolve(), disposed=false
watch(() => [props.active,props.autoInput,props.nodes.map(n=>n.id).join('|')] as const,()=>{
  reconciliation=reconciliation.then(async()=>{
    if(disposed)return
    if(current&&(!props.active||(current.inputKey&&!props.nodes.some(n=>n.id===inputNode.value))))await disconnect()
    if(inputNode.value&&!props.nodes.some(n=>n.id===inputNode.value)){inputNode.value='';microphone.value=false}
    const node=props.autoInput
    if(disposed||!props.active||!node||current||state.value!=='disconnected')return
    const inputs=await api<import('../browserInputs').RemoteAudioInput[]>(`/projects/${props.projectId}/media`).catch(()=>[])
    if(!disposed&&props.active&&props.autoInput===node&&!current&&!inputs.some(i=>i.node===node))await connectInput(node)
  }).catch(e=>{error.value=captureError(e)})
},{immediate:true,flush:'post'})
onBeforeUnmount(() => { disposed=true;void disconnect() })
</script>
<template>
  <section class="browser-monitor" :class="{compact}">
    <div class="section-heading"><div><div class="eyebrow">WEBRTC / OPUS · STEREO</div><h2>Browser audio</h2></div><span class="mode-pill">{{ state.toUpperCase() }}</span></div>
    <audio ref="monitor" autoplay playsinline></audio>
    <div class="monitor-form"><label><span class="field-title">Monitor feed<HelpNote label="Monitor feed">Choose the master mix or a dedicated Monitor output from the patch. Disconnect to change feeds. Use headphones when sending a microphone. Per-part mix-minus sends and physical end-to-end latency calibration are still pending; these network statistics do not measure total listening latency.</HelpNote></span><select v-model="monitorNode" aria-label="Monitor feed" :disabled="state !== 'disconnected'"><option value="">Master mix</option><option v-for="node in nodes.filter(n => n.kind === 'monitor_output')" :key="node.id" :value="node.id">{{ node.label }}</option></select></label><label><input v-model="microphone" type="checkbox" :disabled="state !== 'disconnected'"> Send microphone to the graph</label><select v-if="microphone" v-model="inputNode" :disabled="state !== 'disconnected'" aria-label="Local audio input node"><option value="">Select assigned local audio input…</option><option v-for="node in nodes.filter(n => n.kind === 'browser_input')" :key="node.id" :value="node.id">{{ node.label }} · {{ node.id.slice(0, 8) }}</option></select><BrowserInputPicker v-if="microphone && inputNode && nodes.some(n=>n.id===inputNode&&n.kind==='browser_input')" :input-key="`${projectId}:${inputNode}`" :disabled="state !== 'disconnected'" /><p v-if="microphone && !nodes.some(n => n.kind === 'browser_input')" class="field-error">No Local audio input nodes exist. Add one to the graph first.</p><div class="monitor-buttons"><button v-if="state === 'disconnected'" class="button primary" :disabled="!active" @click="connect"><Headphones :size="16" /> Connect monitor</button><button v-else class="button" :disabled="state === 'disconnecting'" @click="disconnect"><Square :size="14" /> Disconnect</button><button v-if="state === 'connected'" class="button" @click="resume">Resume audio</button></div><label>Monitor level<input v-model.number="volume" type="range" min="0" max="1" step="0.01" aria-label="Monitor level" @input="level"></label></div>
    <p v-if="error" class="field-error" role="alert">{{ error }}</p>
    <div class="media-stats"><span>RTT <strong>{{ stats.rtt.toFixed(1) }} ms</strong></span><span>Jitter <strong>{{ stats.jitter.toFixed(1) }} ms</strong></span><span>Mean jitter buffer <strong>{{ stats.buffer.toFixed(1) }} ms</strong></span><span>Packets lost <strong>{{ stats.lost }}</strong></span></div>

    <details v-if="settings"><summary>Applied microphone settings</summary><pre>{{ settings }}</pre></details>
  </section>
</template>
<style scoped>
.browser-monitor{margin-top:30px;padding:25px;border:1px solid var(--line);border-radius:8px;background:var(--panel)}.monitor-form{display:flex;flex-wrap:wrap;align-items:center;gap:20px}.monitor-form label{display:flex;align-items:center;gap:10px;font-size:12px}.monitor-form select{max-width:300px;font-size:12px}.monitor-buttons{display:flex;gap:10px}.monitor-form input[type=range],input[type=checkbox]{accent-color:var(--cyan)}.media-stats{display:flex;flex-wrap:wrap;gap:30px;margin-top:25px;font-size:10px;color:var(--muted)}.media-stats strong{display:block;font-family:monospace;font-size:16px;color:var(--white);margin-top:5px}details{margin-top:20px;color:var(--muted);font-size:11px}pre{white-space:pre-wrap}
.compact.browser-monitor{margin:0;padding:14px;border-radius:7px;min-width:0}.compact .section-heading{display:block;margin-bottom:14px}.compact .section-heading h2{font-size:16px;letter-spacing:-.3px;margin:3px 0 9px}.compact .section-heading .eyebrow{font-size:7px;letter-spacing:1px}.compact .mode-pill{font-size:8px}.compact .monitor-form{display:flex;flex-direction:column;align-items:stretch;gap:13px}.compact .monitor-form label{flex-direction:column;align-items:stretch;gap:7px;font-size:10px}.compact .monitor-form label:has(input[type=checkbox]){flex-direction:row;align-items:center}.compact .monitor-form select{width:100%;max-width:100%;font-size:10px;padding:9px 7px}.compact .monitor-buttons{flex-wrap:wrap;gap:7px}.compact .monitor-buttons .button{flex:1;font-size:10px;padding:9px 7px;min-height:38px}.compact .media-stats{display:grid;grid-template-columns:1fr 1fr;gap:12px;margin-top:18px;font-size:8px}.compact .media-stats strong{font-size:13px}.compact .feature-note{font-size:9px;line-height:1.6;margin-top:12px}.compact :deep(input){max-width:100%}
</style>
