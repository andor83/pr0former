import { test, expect } from '@playwright/test'
test('metronome toggle clicks through monitors while playing and reports its state', async ({
  page,
}) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  const project = await (
    await page.request.post('/api/projects', {
      headers,
      data: { name: 'Metronome', mode: 'structured' },
    })
  ).json()
  // Silence all score instruments so received energy can only be the metronome.
  for (const part of project.parts) part.notes = []
  project.graph.edges = []
  await page.request.put(`/api/projects/${project.id}`, { headers, data: project })
  await page.addInitScript(() => { const Native = window.RTCPeerConnection; window.RTCPeerConnection = class extends Native { constructor(config?: RTCConfiguration) { super(config); (window as any).__metroPeer = this } } })
  let latest: any
  page.on('websocket', (socket) =>
    socket.on('framereceived', ({ payload }) => {
      const m = JSON.parse(String(payload))
      if (m.type === 'telemetry' && m.project_id === project.id) latest = m
    }),
  )
  await page.goto('/')
  const metronome = page.locator('.transport-bar').getByRole('button', { name: 'Metronome', exact: true })
  await expect(metronome).toBeDisabled()
  await page.getByRole('button', { name: 'Enable audio engine', exact: true }).click()
  await expect(metronome).toBeEnabled()
  await page.getByRole('button', {name:'Monitor',exact:true}).click()
  await page.getByRole('button', {name:'Connect monitor',exact:true}).click()
  await expect(page.locator('.browser-monitor .mode-pill')).toHaveText('CONNECTED', {timeout:20000})
  const energy = () => page.evaluate(async () => { let energy=0; (await (window as any).__metroPeer.getStats()).forEach((r:any)=>{if(r.type==='inbound-rtp'&&r.kind==='audio')energy=r.totalAudioEnergy||0});return energy })
  const initialEnergy = await energy()
  await metronome.click()
  await expect(metronome).toHaveAttribute('aria-pressed', 'true')
  await expect.poll(() => latest?.metronome).toBe(true)
  // The transport action is conductor-only and validated like the others.
  expect(
    (
      await page.request.post(`/api/projects/${project.id}/transport`, {
        headers,
        data: { action: 'metronome', enabled: false },
      })
    ).ok(),
  ).toBeTruthy()
  await expect(metronome).toHaveAttribute('aria-pressed', 'false')
  await metronome.click()
  await expect.poll(() => latest?.metronome).toBe(true)
  await page.getByLabel('Count in', { exact: true }).selectOption('0')
  await page.getByRole('button', { name: 'Play', exact: true }).click()
  await expect.poll(() => latest?.running).toBe(true)
  await expect.poll(energy).toBeGreaterThan(initialEnergy)
  await page.getByRole('button', { name: 'Stop', exact: true }).click()
  await page.getByRole('button', { name: 'Disable audio engine', exact: true }).click()
  await expect(metronome).toBeDisabled()
})
