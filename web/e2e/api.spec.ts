import { test, expect } from '@playwright/test'

test('authorization, invitations, revision conflicts, and sample preparation', async ({ request, playwright }) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await request.get('/api/status')).json()
  const credentials = { username: 'browser-test', password: 'test1234' }
  const shortPassword = await request.post('/api/register', { headers, data: { username: 'short-password', password: 'test123' } })
  expect(shortPassword.status()).toBe(400)
  expect((await shortPassword.json()).error).toContain('8–256 character password')
  expect((await request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: credentials })).ok()).toBeTruthy()
  const response = await request.post('/api/projects', { headers, data: { name: 'API integration', mode: 'structured' } })
  expect(response.ok()).toBeTruthy()
  let project = await response.json()
  const id = project.id
  const stranger = await playwright.request.newContext({ baseURL: 'http://127.0.0.1:3101' })
  expect((await stranger.get(`/api/projects/${id}`)).status()).toBe(401)
  expect((await request.put(`/api/projects/${id}/parameter`, { headers, data: { node: 'mod', parameter: 'a', value: 5, revision: 0 } })).status()).toBe(400)
  expect((await request.put(`/api/projects/${id}/parameter`, { headers, data: { node: 'mod', parameter: 'b', value: 6, revision: 999 } })).status()).toBe(409)
  const invitation = await (await request.post(`/api/projects/${id}/invite`, { headers, data: { role: 'performer' } })).json()
  expect((await stranger.post('/api/register', { headers, data: { username: 'guest-performer', password: 'test5678', invite: invitation.token } })).ok()).toBeTruthy()
  expect((await stranger.post('/api/join', { headers, data: { token: invitation.token } })).ok()).toBeTruthy()
  expect((await stranger.get(`/api/projects/${id}`)).ok()).toBeTruthy()
  expect((await stranger.put(`/api/projects/${id}/parameter`, { headers, data: { node: 'mod', parameter: 'b', value: 6, revision: 0 } })).status()).toBe(403)
  expect((await stranger.post('/api/join', { headers, data: { token: invitation.token } })).status()).toBe(400)
  const catalog = await (await request.get('/api/catalog')).json()
  const descriptor = catalog.find((d: any) => d.kind === 'phase_vocoder')
  const pcm = Buffer.alloc(44 + 4800 * 4)
  pcm.write('RIFF', 0); pcm.writeUInt32LE(pcm.length - 8, 4); pcm.write('WAVEfmt ', 8); pcm.writeUInt32LE(16, 16); pcm.writeUInt16LE(1, 20); pcm.writeUInt16LE(2, 22); pcm.writeUInt32LE(48000, 24); pcm.writeUInt32LE(192000, 28); pcm.writeUInt16LE(4, 32); pcm.writeUInt16LE(16, 34); pcm.write('data', 36); pcm.writeUInt32LE(pcm.length - 44, 40)
  for (let i = 0; i < 4800; i++) for (let ch = 0; ch < 2; ch++) pcm.writeInt16LE(Math.round(Math.sin(i * Math.PI * 2 * 440 / 48000) * 12000), 44 + i * 4 + ch * 2)
  const uploaded = await request.post(`/api/projects/${id}/samples`, { headers, multipart: { sample: { name: 'tone.wav', mimeType: 'audio/wav', buffer: pcm } } })
  expect(uploaded.ok()).toBeTruthy()
  const sample = await uploaded.json()
  project.graph.nodes.find((n: any) => n.id === 'tone').kind = 'phase_vocoder'
  project.graph.nodes.find((n: any) => n.id === 'tone').parameters = { ...Object.fromEntries(descriptor.parameters.map((p: any) => [p.id, p.default])), asset: sample.asset, speed: 2 }
  const saved = await request.put(`/api/projects/${id}`, { headers, data: project })
  expect(saved.ok()).toBeTruthy(); project = await saved.json()
  expect((await request.post(`/api/projects/${id}/transport`, { headers, data: { action: 'activate' } })).ok()).toBeTruthy()
  expect((await request.put(`/api/projects/${id}`, { headers, data: project })).status()).toBe(409)
  expect((await request.post(`/api/projects/${id}/transport`, { headers, data: { action: 'play' } })).ok()).toBeTruthy()
  expect((await request.post(`/api/projects/${id}/transport`, { headers, data: { action: 'deactivate' } })).ok()).toBeTruthy()
  await stranger.dispose()
})
