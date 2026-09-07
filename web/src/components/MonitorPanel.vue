<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { Headphones, Mic, Radio, Square } from 'lucide-vue-next'
import { api } from '../api'
import type { GraphNode } from '../types'
const props = defineProps<{ projectId: string; active: boolean; nodes: GraphNode[] }>()
const error = ref(''), state = ref('disconnected'), microphone = ref(false), inputNode = ref(''), monitorNode = ref(''), volume = ref(0.5)
const monitor = ref<HTMLAudioElement>(), settings = ref(''), stats = ref({ lost: 0, jitter: 0, buffer: 0, rtt: 0 })
type Attempt = { peer: RTCPeerConnection; abort: AbortController; stream?: MediaStream; offer?: Promise<RTCSessionDescriptionInit>; timer?: ReturnType<typeof setInterval> }
let current: Attempt | null = null
async function disconnect() {
  const attempt = current
  if (!attempt) return
  current = null
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
  try { await monitor.value?.play(); error.value = '' }
  catch { error.value = 'Browser playback is blocked. Tap Resume audio to try again.' }
}
async function connect() {
  if (!props.active || state.value !== 'disconnected') return
  error.value = ''; settings.value = ''; stats.value = { lost: 0, jitter: 0, buffer: 0, rtt: 0 }; state.value = 'connecting'
  let attempt: Attempt | null = null
  try {
    if (!window.isSecureContext) throw new Error('Open this server over trusted HTTPS to use browser audio.')
    if (microphone.value && !inputNode.value) throw new Error('Select your assigned browser input node first.')
    const pc = new RTCPeerConnection({ iceServers: [] })
    attempt = { peer: pc, abort: new AbortController() }; current = attempt
    const owned = attempt
    pc.onconnectionstatechange = () => {
      if (current !== owned) return
      state.value = pc.connectionState === 'disconnected' ? 'interrupted' : pc.connectionState
    }
    pc.ontrack = async event => {
      if (current !== owned || !monitor.value) return
      monitor.value.srcObject = event.streams[0] || new MediaStream([event.track]); monitor.value.volume = volume.value
      try { await monitor.value.play() } catch { if (current === owned) error.value = 'Tap Resume audio to permit monitor playback.' }
    }
    if (microphone.value) {
      const captured = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 2, echoCancellation: false, noiseSuppression: false, autoGainControl: false }, video: false })
      if (current !== owned) { captured.getTracks().forEach(track => track.stop()); return }
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
    owned.offer = api<RTCSessionDescriptionInit>(`/projects/${props.projectId}/media`, 'POST', { sdp: pc.localDescription?.sdp, input_node: microphone.value ? inputNode.value : null, monitor_node: monitorNode.value || null })
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
    error.value = e instanceof Error ? e.message : String(e)
    if (attempt) await disconnect(); else state.value = 'disconnected'
  }
}
function level() { if (monitor.value) monitor.value.volume = volume.value }
watch(() => props.active, active => { if (!active && current) void disconnect() })
onBeforeUnmount(() => { void disconnect() })
</script>
<template>
  <section class="browser-monitor">
    <div class="section-heading"><div><div class="eyebrow">WEBRTC / OPUS · STEREO</div><h2>Browser audio</h2></div><span class="mode-pill">{{ state.toUpperCase() }}</span></div>
    <audio ref="monitor" autoplay playsinline></audio>
    <div class="monitor-form"><label>Monitor feed<select v-model="monitorNode" aria-label="Monitor feed" :disabled="state !== 'disconnected'"><option value="">Master mix</option><option v-for="node in nodes.filter(n => n.kind === 'monitor_output')" :key="node.id" :value="node.id">{{ node.label }}</option></select></label><label><input v-model="microphone" type="checkbox" :disabled="state !== 'disconnected'"> Send microphone to the graph</label><select v-if="microphone" v-model="inputNode" :disabled="state !== 'disconnected'" aria-label="Browser input node"><option value="">Select assigned browser input…</option><option v-for="node in nodes.filter(n => n.kind === 'browser_input')" :key="node.id" :value="node.id">{{ node.label }} · {{ node.id.slice(0, 8) }}</option></select><div class="monitor-buttons"><button v-if="state === 'disconnected'" class="button primary" :disabled="!active" @click="connect"><Headphones :size="16" /> Connect monitor</button><button v-else class="button" :disabled="state === 'disconnecting'" @click="disconnect"><Square :size="14" /> Disconnect</button><button v-if="state === 'connected'" class="button" @click="resume">Resume audio</button></div><label>Monitor level<input v-model.number="volume" type="range" min="0" max="1" step="0.01" aria-label="Monitor level" @input="level"></label></div>
    <p v-if="error" class="field-error" role="alert">{{ error }}</p>
    <div class="media-stats"><span>RTT <strong>{{ stats.rtt.toFixed(1) }} ms</strong></span><span>Jitter <strong>{{ stats.jitter.toFixed(1) }} ms</strong></span><span>Mean jitter buffer <strong>{{ stats.buffer.toFixed(1) }} ms</strong></span><span>Packets lost <strong>{{ stats.lost }}</strong></span></div>
    <p class="feature-note">Choose the master mix or a dedicated Monitor output from the patch. Disconnect to change feeds. Use headphones when sending a microphone. Per-part mix-minus sends and physical end-to-end latency calibration are still pending; these network statistics do not measure total listening latency.</p>
    <details v-if="settings"><summary>Applied microphone settings</summary><pre>{{ settings }}</pre></details>
  </section>
</template>
<style scoped>
.browser-monitor{margin-top:30px;padding:25px;border:1px solid var(--line);border-radius:8px;background:var(--panel)}.monitor-form{display:flex;flex-wrap:wrap;align-items:center;gap:20px}.monitor-form label{display:flex;align-items:center;gap:10px;font-size:12px}.monitor-form select{max-width:300px;font-size:12px}.monitor-buttons{display:flex;gap:10px}.monitor-form input[type=range],input[type=checkbox]{accent-color:var(--cyan)}.media-stats{display:flex;flex-wrap:wrap;gap:30px;margin-top:25px;font-size:10px;color:var(--muted)}.media-stats strong{display:block;font-family:monospace;font-size:16px;color:var(--white);margin-top:5px}details{margin-top:20px;color:var(--muted);font-size:11px}pre{white-space:pre-wrap}
</style>
