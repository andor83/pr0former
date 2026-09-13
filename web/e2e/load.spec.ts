import { test, expect, type BrowserContext } from '@playwright/test'
import { writeFile } from 'node:fs/promises'

const uplink = process.env.PR0_LOAD_INPUT === '1'
const fullUI = process.env.PR0_LOAD_UI === '1'
const seconds = Number(process.env.PR0_LOAD_SECONDS || 15)
if (!Number.isInteger(seconds) || seconds < 5 || seconds > 3600) throw new Error('PR0_LOAD_SECONDS must be an integer from 5 to 3600')
test('32 independent performers receive dedicated WebRTC monitors', async ({ browser }, testInfo) => {
  test.setTimeout((seconds + 180) * 1000)
  const contexts: BrowserContext[] = []
  const headers = { 'X-Pr0former': '1' }
  const baseURL = 'http://127.0.0.1:3101'
  let projectId = ''
  try {
    const owner = await browser.newContext({ baseURL, permissions: uplink ? ['microphone'] : [] }); contexts.push(owner)
    const first = await owner.request.post('/api/register', { headers, data: { username: 'load-owner', password: 'load-test-password-123' } })
    expect(first.ok()).toBeTruthy()
    const users = [(await first.json()).id]
    let project = await (await owner.request.post('/api/projects', { headers, data: { name: '32-player monitor baseline', mode: 'structured' } })).json()
    projectId = project.id
    for (let i = 1; i < 32; i++) {
      const invite = await (await owner.request.post(`/api/projects/${projectId}/invite`, { headers, data: { role: 'performer' } })).json()
      const context = await browser.newContext({ baseURL, permissions: uplink ? ['microphone'] : [] }); contexts.push(context)
      const registration = await context.request.post('/api/register', { headers, data: { username: `load-player-${i}`, password: 'load-test-password-123', invite: invite.token } })
      expect(registration.ok()).toBeTruthy(); users.push((await registration.json()).id)
      expect((await context.request.post('/api/join', { headers, data: { token: invite.token } })).ok()).toBeTruthy()
    }
    const part = project.parts[0]
    const synth = project.graph.nodes.find((node: any) => node.kind === 'synth')
    if (uplink) { synth.kind = 'browser_input'; synth.parameters = {} }
    project.parts = users.map((user, i) => ({ ...part, id: `part-${i}`, name: `Player ${i}`, performer: user, instrument_node: `synth-${i}`, notes: part.notes.map((n: any, j: number) => ({ ...n, id: `note-${i}-${j}`, pitch: 48 + i % 24 })) }))
    project.graph = {
      nodes: users.flatMap((_, i) => [{ ...synth, id: `synth-${i}`, label: `Synth ${i}` }, { id: `monitor-${i}`, kind: 'monitor_output', label: `Monitor ${i}`, x: 500, y: i * 150, channels: 2, parameters: { gain: -12 } }]),
      edges: users.map((_, i) => ({ id: `send-${i}`, source: `synth-${i}`, source_port: 'out', target: `monitor-${i}`, target_port: 'in' })),
    }
    const saved = await owner.request.put(`/api/projects/${projectId}`, { headers, data: project })
    expect(saved.ok()).toBeTruthy(); project = await saved.json()
    expect((await owner.request.post(`/api/projects/${projectId}/transport`, { headers, data: { action: 'activate' } })).ok()).toBeTruthy()
    expect((await owner.request.post(`/api/projects/${projectId}/transport`, { headers, data: { action: 'play' } })).ok()).toBeTruthy()
    const pages = await Promise.all(contexts.map(async (context, index) => {
      const page = await context.newPage()
      page.on('response', async response => { if (response.url().endsWith('/media') && !response.ok()) console.log(`Client ${index} media HTTP ${response.status()}: ${await response.text()}`) })
      if (index === 0) page.on('console', message => console.log('Client 0:', message.text()))
      await page.addInitScript(({ index, uplink }) => {
        const state: any = { snapshots: 0, maxGapMs: 0, lastAt: 0, firstBeat: null, lastBeat: null, hardware: false, messages: {}, errors: [], frames: 0, frameGapMs: 0, playheadChanges: 0 }
        ;(window as any).__load = state
        window.addEventListener('error', event => state.errors.push(event.message))
        window.addEventListener('unhandledrejection', event => state.errors.push(String(event.reason)))
        if (uplink) navigator.mediaDevices.getUserMedia = async () => {
          const Generator = (window as any).MediaStreamTrackGenerator, Data = (window as any).AudioData
          if (!Generator || !Data) throw new Error('Synthetic uplink requires MediaStreamTrackGenerator and AudioData')
          const track = new Generator({ kind: 'audio' }), writer = track.writable.getWriter()
          const stream = new MediaStream([track]); let samples = 0, deadline = performance.now()
          void (async () => {
            try {
              while (track.readyState === 'live') {
                const pcm = new Float32Array(960)
                for (let i = 0; i < pcm.length; i++) pcm[i] = Math.sin((samples + i) * 2 * Math.PI * (220 + index * 7) / 48000) * 0.1
                const frame = new Data({ format: 'f32-planar', sampleRate: 48000, numberOfFrames: pcm.length, numberOfChannels: 1, timestamp: samples / 48000 * 1000000, data: pcm })
                await writer.write(frame); frame.close(); samples += pcm.length; deadline += 20
                await new Promise(resolve => setTimeout(resolve, Math.max(0, deadline - performance.now())))
              }
            } catch (error) { if (track.readyState === 'live') state.errors.push(String(error)) }
          })()
          return stream
        }
        const NativeWS = window.WebSocket
        window.WebSocket = class extends NativeWS {
          constructor(url: string | URL, protocols?: string | string[]) {
            super(url, protocols); state.ws = this
            this.addEventListener('message', event => {
              if (state.ws !== this) return
              const message = JSON.parse(event.data)
              state.messages[message.type] = (state.messages[message.type] || 0) + 1
              if (message.type !== 'telemetry') return
              const now = performance.now()
              if (state.lastAt) state.maxGapMs = Math.max(state.maxGapMs, now - state.lastAt)
              state.lastAt = now; state.snapshots++; state.firstBeat ??= message.beat; state.lastBeat = message.beat; state.hardware = message.hardware_enabled
              state.peak = Math.max(state.peak || 0, message.values[`monitor-${index}`]?._peak || 0); state.synthPeak = Math.max(state.synthPeak || 0, message.values[`synth-${index}`]?._peak || 0); state.running = message.running; state.part = message.parts.find((p: any) => p.id === `part-${index}`)
            })
          }
        }
        const NativeRTC = window.RTCPeerConnection
        window.RTCPeerConnection = class extends NativeRTC { constructor(config?: RTCConfiguration) { super(config); state.peer = this } }
        const frame = (now: number) => {
          if (state.lastFrame) state.frameGapMs = Math.max(state.frameGapMs, now - state.lastFrame)
          state.lastFrame = now; state.frames++
          const position = (document.querySelector('.score-playhead') as HTMLElement | null)?.style.left
          if (position && position !== state.position) { state.playheadChanges++; state.position = position }
          requestAnimationFrame(frame)
        }
        requestAnimationFrame(frame)
      }, { index, uplink })
      if (fullUI) {
        await page.goto('/')
        await page.getByRole('button', { name: 'Performance mode', exact: true }).click()
        await expect(page.getByLabel('Performance part', { exact: true })).toHaveValue(`part-${index}`)
        await expect(page.locator('.notation-surface svg')).toBeVisible()
        await page.getByRole('button', { name: 'Monitor controls', exact: true }).click()
        await page.getByLabel('Monitor feed', { exact: true }).selectOption(`monitor-${index}`)
        if (uplink) {
          await page.getByLabel('Send microphone to the graph', { exact: true }).check()
          await page.getByLabel('Local audio input node', { exact: true }).selectOption(`synth-${index}`)
        }
        await page.getByRole('button', { name: 'Connect monitor', exact: true }).click()
        await expect.poll(async () => {
          const errors = await page.locator('.browser-monitor .field-error').allTextContents()
          if (errors.length) throw new Error(`Client ${index} audio setup: ${errors.join('; ')}`)
          return page.locator('.browser-monitor .mode-pill').textContent()
        }, { timeout: 45000 }).toBe('CONNECTED')
        await page.getByRole('button', { name: 'Resume audio', exact: true }).click()
        await expect.poll(() => page.locator('.browser-monitor audio').evaluate((audio: HTMLAudioElement) => audio.paused)).toBe(false)
        await page.getByRole('button', { name: 'Monitor controls', exact: true }).click()
        return page
      }
      // A real local response establishes Chromium's local-network address space.
      await page.goto('/api/status')
      await page.setContent('<!doctype html><title>Monitor load client</title><button>Start monitor</button>')
      await page.getByRole('button', { name: 'Start monitor' }).click()
      await page.evaluate(async ({ projectId, index, uplink }) => {
        const state = (window as any).__load
        new WebSocket(`ws://${location.host}/api/projects/${projectId}/events`)
        const peer = new RTCPeerConnection({ iceServers: [] }); state.peer = peer
        peer.ontrack = async event => {
          const audio = document.createElement('audio'); audio.autoplay = true
          audio.srcObject = event.streams[0] || new MediaStream([event.track]); document.body.append(audio)
          await audio.play(); state.playing = !audio.paused
        }
        if (uplink) {
          const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
          stream.getTracks().forEach(track => peer.addTrack(track, stream))
        } else peer.addTransceiver('audio', { direction: 'recvonly' })
        await peer.setLocalDescription(await peer.createOffer())
        if (peer.iceGatheringState !== 'complete') await new Promise<void>((resolve, reject) => {
          const timeout = setTimeout(() => reject(new Error('Client ICE timeout')), 15000)
          peer.addEventListener('icegatheringstatechange', () => { if (peer.iceGatheringState === 'complete') { clearTimeout(timeout); resolve() } })
        })
        const response = await fetch(`/api/projects/${projectId}/media`, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Pr0former': '1' }, body: JSON.stringify({ sdp: peer.localDescription?.sdp, monitor_node: `monitor-${index}`, input_node: uplink ? `synth-${index}` : null }) })
        if (!response.ok) throw new Error(`Offer ${index} failed: ${response.status} ${await response.text()}`)
        await peer.setRemoteDescription(await response.json())
      }, { projectId, index, uplink })
      return page
    }))
    await Promise.all(pages.map(page => expect.poll(() => page.evaluate(() => (window as any).__load.peer.connectionState), { timeout: 45000 }).toBe('connected')))
    console.log(`All 32 monitors connected; measuring for ${seconds} seconds.`)
    await Promise.all(pages.map(page => page.evaluate(async () => {
      const s = (window as any).__load
      ;(await s.peer.getStats()).forEach((r: any) => { if (r.type === 'inbound-rtp' && r.kind === 'audio') { s.initialPackets = r.packetsReceived; s.initialEnergy = r.totalAudioEnergy; s.initialConcealed = r.concealedSamples || 0 } if (r.type === 'outbound-rtp' && r.kind === 'audio') s.initialSent = r.packetsSent })
      s.frames = 0; s.lastFrame = 0; s.frameGapMs = 0; s.playheadChanges = 0; s.snapshots = 0; s.maxGapMs = 0; s.lastAt = 0; s.firstBeat = null; s.peak = 0; s.synthPeak = 0
    })))
    const started = Date.now()
    while (Date.now() - started < seconds * 1000) {
      await new Promise(resolve => setTimeout(resolve, Math.min(5000, seconds * 1000 - (Date.now() - started))))
      console.log(`Monitor load elapsed: ${Math.round((Date.now() - started) / 1000)} seconds`)
    }
    const clients = await Promise.all(pages.map((page, index) => page.evaluate(async index => {
      const s = (window as any).__load
      const result: any = { index, audioPaused: (document.querySelector('audio') as HTMLAudioElement | null)?.paused, visibility: document.visibilityState, errors: s.errors, frames: s.frames, frameGapMs: s.frameGapMs, playheadChanges: s.playheadChanges, wsState: s.ws.readyState, wsError: s.wsError, closeCode: s.closeCode, messages: s.messages, playing: s.playing, serverPeak: s.peak, synthPeak: s.synthPeak, running: s.running, part: s.part, state: s.peer.connectionState, snapshots: s.snapshots, maxGapMs: s.maxGapMs, firstBeat: s.firstBeat, lastBeat: s.lastBeat, hardware: s.hardware, sentDelta: 0, packets: 0, lost: 0, energy: 0, jitterMs: 0 }
      ;(await s.peer.getStats()).forEach((r: any) => { if (r.type === 'inbound-rtp' && r.kind === 'audio') Object.assign(result, { packets: r.packetsReceived, bytes: r.bytesReceived, emitted: r.jitterBufferEmittedCount, concealed: r.concealedSamples, concealedDelta: (r.concealedSamples || 0) - (s.initialConcealed || 0), lost: r.packetsLost, energy: r.totalAudioEnergy, packetDelta: r.packetsReceived - (s.initialPackets || 0), energyDelta: r.totalAudioEnergy - (s.initialEnergy || 0), jitterMs: r.jitter * 1000 }); if (r.type === 'outbound-rtp' && r.kind === 'audio') result.sentDelta = r.packetsSent - (s.initialSent || 0) })
      return result
    }, index)))
    const report = { recordedAt: new Date().toISOString(), browser: browser.version(), platform: process.platform, elapsedWallMs: Date.now() - started, seconds, clients: 32, fullUI, uplink, fixture: '32 structured parts with synths or WebCodecs-generated microphone inputs (uplink flag), 32 dedicated stereo monitors; playing HTML audio sinks with Chromium output muted; release server and Chromium on one machine; no physical devices; fullUI indicates whether the Vue stage was rendered', results: clients }
    const json = JSON.stringify(report, null, 2)
    await writeFile(uplink ? '../docs/load-uplink-baseline.json' : fullUI ? '../docs/load-ui-baseline.json' : '../docs/load-baseline.json', json)
    await testInfo.attach('load-baseline', { body: json, contentType: 'application/json' })
    for (const client of clients) {
      const label = `Client ${client.index}`
      expect(client.errors, `${label} browser errors`).toEqual([])
      if (uplink) {
        expect(client.sentDelta, `${label} uplink packets`).toBeGreaterThanOrEqual(seconds * 30)
        expect(client.serverPeak, `${label} routed server audio`).toBeGreaterThan(0)
      }
      if (fullUI) {
        expect(client.playheadChanges, `${label} playhead movement`).toBeGreaterThanOrEqual(seconds * 10)
        expect(client.audioPaused, `${label} monitor playback`).toBe(false)
      }
      expect(client.state, `${label} connection`).toBe('connected')
      expect(client.energyDelta, `${label} return audio energy`).toBeGreaterThan(0)
      expect(client.packetDelta, `${label} return packets`).toBeGreaterThanOrEqual(seconds * 80)
      expect(client.snapshots, `${label} telemetry count`).toBeGreaterThanOrEqual(seconds * 10)
      expect(client.maxGapMs, `${label} telemetry gap`).toBeLessThan(500)
      expect(client.hardware, `${label} hardware remains disabled`).toBe(false)
    }
  } finally {
    if (contexts[0] && projectId) await contexts[0].request.post(`/api/projects/${projectId}/transport`, { headers, data: { action: 'deactivate' } }).catch(() => {})
    await Promise.all(contexts.map(context => context.close()))
  }
})
