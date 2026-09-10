import { test, expect } from '@playwright/test'
import { createSocket } from 'node:dgram'

async function freePort() {
  const socket = createSocket('udp4')
  await new Promise<void>(resolve => socket.bind(0, '127.0.0.1', resolve))
  const address = socket.address()
  await new Promise<void>(resolve => socket.close(resolve))
  return address.port
}

test('part note-on/off streams round-trip through OSC and play a polyphonic sampler', async ({ page }) => {
  const headers = { 'X-Pr0former': '1' }, status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })
  let p = await (await page.request.post('/api/projects', { headers, data: { name: 'MIDI note paths', mode: 'structured' } })).json()
  const port = await freePort()
  expect((await page.request.put(`/api/projects/${p.id}/system/osc`, { headers, data: { receive_enabled: true, bind_addresses: ['127.0.0.1'], receive_port: port, send_enabled: true, send_address: '127.0.0.1', send_port: 0 } })).ok()).toBe(true)
  const wav = Buffer.alloc(44 + 48000 * 4)
  wav.write('RIFF'); wav.writeUInt32LE(wav.length - 8, 4); wav.write('WAVEfmt ', 8); wav.writeUInt32LE(16, 16); wav.writeUInt16LE(1, 20); wav.writeUInt16LE(2, 22); wav.writeUInt32LE(48000, 24); wav.writeUInt32LE(192000, 28); wav.writeUInt16LE(4, 32); wav.writeUInt16LE(16, 34); wav.write('data', 36); wav.writeUInt32LE(wav.length - 44, 40)
  for (let i = 0; i < 48000; i++) for (let ch = 0; ch < 2; ch++) wav.writeInt16LE(Math.round(Math.sin(i * 2 * Math.PI * 261.626 / 48000) * 10000), 44 + i * 4 + ch * 2)
  const upload = await (await page.request.post(`/api/projects/${p.id}/samples`, { headers, multipart: { sample: { name: 'root-c.wav', mimeType: 'audio/wav', buffer: wav } } })).json()
  const note = (id: string, pitch: number) => ({ id, pitch, beat: 0, duration: 1, velocity: 90, rest: false, tied: false })
  const part = { ...p.parts[0], id: 'part-a', name: 'Part A', instrument_node: null, loop_beats: 16, notes: [note('a', 60), note('b', 64)] }
  p.parts = [part, { ...part, id: 'part-b', name: 'Part B', notes: [note('other', 90)] }]
  const n = (id: string, kind: string, x: number, y: number, extra = {}) => ({ id, kind, label: id, x, y, channels: 2, parameters: {}, ...extra })
  p.graph = { nodes: [
    n('Part notes', 'part_midi', 0, 0),
    n('Send notes', 'midi_to_osc', 300, 0, { io: { port: '', address: '/notes', destination: `127.0.0.1:${port}` } }),
    n('Receive notes', 'osc_to_midi', 600, 0, { io: { port: '', address: '/notes', destination: '' } }),
    n('Sampler', 'poly_sampler', 900, 0, { parameters: { asset: upload.asset, root_note: 60, loop: 1, release: 5, amplitude: .5 } }),
    n('Headphones', 'monitor_output', 1200, 0),
    n('On count', 'counter', 0, 420), n('Off count', 'counter', 300, 420),
    n('Received on', 'counter', 600, 420), n('Received off', 'counter', 900, 420),
    n('Keyboard', 'midi_input', 1200, 420), n('MIDI destination', 'midi_output', 1500, 420),
  ], edges: [] }
  const wire = (source: string, source_port: string, target: string, target_port: string) => p.graph.edges.push({ id: `${source}-${source_port}-${target}`, source, source_port, target, target_port })
  for (const name of ['pitch', 'velocity', 'gate', 'trigger', 'note_off']) { wire('Part notes', name, 'Send notes', name); wire('Receive notes', name, 'Sampler', name) }
  wire('Part notes', 'trigger', 'On count', 'trigger'); wire('Part notes', 'note_off', 'Off count', 'trigger')
  wire('Receive notes', 'trigger', 'Received on', 'trigger'); wire('Receive notes', 'note_off', 'Received off', 'trigger')
  wire('Sampler', 'out', 'Headphones', 'in')
  const saved = await page.request.put(`/api/projects/${p.id}`, { headers, data: p }); expect(saved.ok()).toBe(true); p = await saved.json()
  const load = async () => (await (await page.request.get(`/api/projects/${p.id}`)).json()).project
  const invalid = structuredClone(p); invalid.graph.nodes[0].part_id = 'missing-part'
  expect((await page.request.put(`/api/projects/${p.id}`, { headers, data: invalid })).status()).toBe(400)
  await page.addInitScript(() => {
    const Native = RTCPeerConnection
    window.RTCPeerConnection = class extends Native { constructor(config?: RTCConfiguration) { super(config); (window as any).__notesPeer = this } }
  })
  let latest: any
  page.on('websocket', socket => socket.on('framereceived', ({ payload }) => { const event = JSON.parse(String(payload)); if (event.type === 'telemetry') latest = event }))
  await page.goto('/')
  await page.getByRole('button', { name: 'Edit Part notes', exact: true }).click()
  await expect(page.getByLabel('Source part')).toHaveValue('')
  await page.getByLabel('Source part').selectOption('part-a')
  await expect.poll(async () => (await load()).graph.nodes.find((n: any) => n.id === 'Part notes').part_id).toBe('part-a')
  await page.getByRole('button', { name: 'Close parameters' }).click()
  await page.getByRole('button', { name: 'Edit Keyboard', exact: true }).click()
  await expect(page.getByLabel('MIDI input port')).toBeVisible()
  await page.getByRole('button', { name: 'Close parameters' }).click()
  await page.getByRole('button', { name: 'Edit MIDI destination', exact: true }).click()
  await expect(page.getByLabel('MIDI output port')).toBeVisible()
  await page.getByRole('button', { name: 'Close parameters' }).click()
  await page.getByRole('button', { name: 'Edit Sampler', exact: true }).click()
  await expect(page.getByRole('spinbutton', { name: 'Root MIDI note', exact: true })).toBeVisible()
  await page.getByRole('button', { name: 'Close parameters' }).click()
  await page.getByLabel('Count in', { exact: true }).selectOption('0')
  await page.getByRole('button', { name: 'Enable audio engine', exact: true }).click()
  await page.getByRole('button', { name: 'Monitor', exact: true }).click()
  await page.getByLabel('Monitor feed').selectOption('Headphones')
  await page.getByRole('button', { name: 'Connect monitor', exact: true }).click()
  await expect(page.locator('.browser-monitor .mode-pill')).toHaveText('CONNECTED', { timeout: 20000 })
  await page.getByRole('button', { name: 'Play', exact: true }).click()
  for (const id of ['On count', 'Off count', 'Received on', 'Received off']) await expect.poll(() => latest?.values?.[id]?._out).toBe(2)
  await expect.poll(() => latest?.values?.['Receive notes']?.gate).toBe(0)
  expect([60, 64]).toContain(latest.values['Receive notes'].pitch)
  await expect.poll(() => latest?.values?.Sampler?._peak).toBe(0)
  await expect.poll(async () => page.evaluate(async () => {
    let energy = 0
    ;(await (window as any).__notesPeer.getStats()).forEach((r: any) => { if (r.type === 'inbound-rtp' && r.kind === 'audio') energy = r.totalAudioEnergy || 0 })
    return energy
  })).toBeGreaterThan(0)
  await page.getByRole('button', { name: 'Stop', exact: true }).click()
  await expect.poll(() => latest?.running).toBe(false)
  await page.getByRole('button', { name: 'Signal Graph', exact: true }).click()
  await page.getByRole('button', { name: 'Edit Part notes', exact: true }).click()
  await page.getByLabel('Source part').selectOption('part-b')
  await expect.poll(async () => (await load()).graph.nodes.find((n: any) => n.id === 'Part notes').part_id).toBe('part-b')
  await page.getByRole('button', { name: 'Close parameters' }).click()
  const before = latest.values['Received on']._out
  await page.getByRole('button', { name: 'Play', exact: true }).click()
  await expect.poll(() => latest?.values?.['Received on']?._out).toBe(before + 1)
  await expect.poll(() => latest?.values?.['Receive notes']?.pitch).toBe(90)
  await page.getByRole('button', { name: 'Disable audio engine', exact: true }).click()
  await page.request.put(`/api/projects/${p.id}/system/osc`, { headers, data: { receive_enabled: false, bind_addresses: ['127.0.0.1'], receive_port: port, send_enabled: true, send_address: '0.0.0.0', send_port: 0 } })
})
