import { test, expect, type BrowserContext, type APIRequestContext } from '@playwright/test'
import { readFileSync, readdirSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { connect as tcpConnect } from 'node:net'
const headers = { 'X-Pr0former': '1' }
async function login(request: APIRequestContext) {
  const status = await (await request.get('/api/status')).json()
  expect((await request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })).ok()).toBe(true)
}
async function create(request: APIRequestContext, name: string) {
  return (await request.post('/api/projects', { headers, data: { name, mode: 'freeform' } })).json()
}
async function watch(context: BrowserContext, id: string) {
  const page = await context.newPage()
  await page.goto('/api/status') // no Vue reconnect behavior; control each socket explicitly
  await page.evaluate(id => new Promise<void>((resolve, reject) => {
    const socket = new WebSocket(`${location.origin.replace('http', 'ws')}/api/projects/${id}/events`)
    ;(window as any).presenceSocket = socket
    socket.onerror = () => reject(new Error('Socket failed'))
    socket.onmessage = event => { if (JSON.parse(event.data).type === 'project') resolve() }
  }), id)
  return page
}
async function enabled(request: APIRequestContext, id: string) {
  const r = await request.post(`/api/projects/${id}/engine`, { headers, data: { enabled: true } })
  expect(r.ok(), await r.text()).toBe(true)
}
async function status(request: APIRequestContext) { return (await request.get('/api/status')).json() }
function takes(root: string): any[] {
  if (!existsSync(root)) return []
  return readdirSync(root, { withFileTypes: true }).flatMap(e => e.isDirectory() ? takes(join(root, e.name)) : e.name.endsWith('.json') ? [JSON.parse(readFileSync(join(root, e.name), 'utf8'))] : [])
}

test('all users/tabs count, reconnect cancels shutdown, last departure stops show and finalizes archive', async ({ context, browser }) => {
  const request = context.request
  test.setTimeout(45000)
  await login(request)
  const p = await create(request, 'Presence archive')
  p.parts = []
  p.graph = { nodes: [
    { id: 'Tone', kind: 'oscillator', label: 'Tone', x: 0, y: 0, channels: 2, parameters: { frequency: 440, amplitude: .2 } },
    { id: 'Archive', kind: 'record', label: 'Archive', x: 200, y: 0, channels: 1, parameters: {} },
    { id: 'Start', kind: 'value', label: 'Start', x: 0, y: 200, channels: 1, parameters: { value: 1 } },
  ], edges: [
    { id: 'audio', source: 'Tone', source_port: 'out', target: 'Archive', target_port: 'in' },
    { id: 'start', source: 'Start', source_port: 'out', target: 'Archive', target_port: 'start' },
  ] }
  expect((await request.put(`/api/projects/${p.id}`, { headers, data: p })).ok()).toBe(true)
  const invitation = await (await request.post(`/api/projects/${p.id}/invite`, { headers, data: { role: 'performer' } })).json()
  const guest = await browser.newContext({ baseURL: 'http://127.0.0.1:3101' })
  try {
    expect((await guest.request.post('/api/register', { headers, data: { username: `viewer-${p.id}`, password: 'test1234', invite: invitation.token } })).ok()).toBe(true)
    expect((await guest.request.post('/api/join', { headers, data: { token: invitation.token } })).ok()).toBe(true)
    const a = await watch(context, p.id), b = await watch(context, p.id), c = await watch(guest, p.id)
    await enabled(request, p.id)
    for (const action of ['activate', 'play']) expect((await request.post(`/api/projects/${p.id}/transport`, { headers, data: { action } })).ok()).toBe(true)
    await a.close(); await b.close()
    await new Promise(resolve => setTimeout(resolve, 6000))
    expect((await status(request)).active_project).toBe(p.id) // performer alone keeps performance alive
    await c.close()
    await new Promise(resolve => setTimeout(resolve, 1000))
    const reconnected = await watch(context, p.id)
    await new Promise(resolve => setTimeout(resolve, 5500))
    expect((await status(request)).graph_project).toBe(p.id)
    await reconnected.close()
    await expect.poll(async () => (await status(request)).graph_project, { timeout: 12000 }).toBeNull()
    expect((await status(request)).active_project).toBeNull()
    const archive = takes(join(process.env.PR0_TEST_DATA!, 'recordings')).find(t => t.project_id === p.id)
    expect(archive?.complete).toBe(true); expect(archive.channels).toBe(2)
    const returning = await watch(context, p.id)
    expect((await status(request)).graph_project).toBeNull() // never silently restart on return
    await returning.close()
  } finally { await guest.close() }
})

test('old project disconnect never disables the replacement development graph', async ({ context }) => {
  const request = context.request
  await login(request)
  const old = await create(request, 'Old graph'), next = await create(request, 'Next graph')
  const a = await watch(context, old.id)
  await enabled(request, old.id)
  await a.close()
  const b = await watch(context, next.id)
  await enabled(request, next.id)
  await new Promise(resolve => setTimeout(resolve, 6000))
  expect((await status(request)).graph_project).toBe(next.id)
  await b.close()
  await expect.poll(async () => (await status(request)).graph_project, { timeout: 12000 }).toBeNull()
})

test('a silent broken client is expired by server heartbeat', async ({ request }) => {
  test.setTimeout(55000)
  await login(request)
  const p = await create(request, 'Lost network')
  const cookies = (await request.storageState()).cookies.map(c => `${c.name}=${c.value}`).join('; ')
  const socket = tcpConnect(3101, '127.0.0.1')
  try {
    await new Promise<void>((resolve, reject) => {
      socket.once('error', reject)
      socket.once('connect', () => socket.write(`GET /api/projects/${p.id}/events HTTP/1.1\r\nHost: 127.0.0.1:3101\r\nOrigin: http://127.0.0.1:3101\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nCookie: ${cookies}\r\n\r\n`))
      socket.once('data', data => { if (data.toString().includes('101 Switching Protocols')) resolve(); else reject(new Error('Upgrade failed')) })
    })
    socket.on('data', () => {}) // deliberately never return protocol Pong frames
    await enabled(request, p.id)
    await expect.poll(async () => (await status(request)).graph_project, { timeout: 47000, intervals: [500] }).toBeNull()
  } finally { socket.destroy() }
})
