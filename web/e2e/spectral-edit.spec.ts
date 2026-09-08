import { test, expect } from '@playwright/test'

test('spectral math and drawn curves modify live frames and persist through undo', async ({ page }) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  expect((await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })).ok()).toBeTruthy()
  let project = await (await page.request.post('/api/projects', { headers, data: { name: 'Live spectral editing', mode: 'structured' } })).json()
  project.parts = []
  project.graph = {
    nodes: ['oscillator', 'fft', 'spectral_math', 'spectral_curve', 'spectral_visualizer', 'ifft', 'output'].map((kind, i) => ({ id: kind, kind, label: kind, channels: 2, x: i * 350, y: 0, parameters: kind === 'oscillator' ? { frequency: 750, amplitude: 0.2 } : ['fft', 'ifft', 'spectral_math', 'spectral_curve', 'spectral_visualizer'].includes(kind) ? { size: 256, overlap: 4 } : { gain: -12 } })),
    edges: ['oscillator', 'fft', 'spectral_math', 'spectral_curve', 'spectral_visualizer', 'ifft'].map((source, i) => ({ id: `wire${i}`, source, source_port: 'out', target: ['fft', 'spectral_math', 'spectral_curve', 'spectral_visualizer', 'ifft', 'output'][i], target_port: 'in' })),
  }
  expect((await page.request.put(`/api/projects/${project.id}`, { headers, data: project })).ok()).toBeTruthy()
  const load = async () => (await (await page.request.get(`/api/projects/${project.id}`)).json()).project
  let latest: any
  page.on('websocket', socket => socket.on('framereceived', ({ payload }) => { const m = JSON.parse(String(payload)); if (m.type === 'telemetry') latest = m }))
  await page.goto('/')
  await page.getByLabel('Count in', { exact: true }).selectOption('0')
  await page.getByRole('button', { name: 'Play', exact: true }).click()
  await expect.poll(() => latest?.visualizations?.spectral_visualizer?.channels?.[0]?.magnitude[4]).toBeGreaterThan(10)
  await page.getByRole('button', { name: 'Edit spectral_math', exact: true }).click()
  await page.getByRole('spinbutton', { name: 'Magnitude multiply', exact: true }).fill('0')
  await page.waitForTimeout(150) // Several telemetry refreshes must not replace typed text.
  await page.getByRole('spinbutton', { name: 'Magnitude multiply', exact: true }).press('Tab')
  await expect.poll(() => latest?.visualizations?.spectral_visualizer?.channels?.[0]?.magnitude[4]).toBe(0)
  await page.getByRole('spinbutton', { name: 'Magnitude multiply', exact: true }).fill('1')
  await page.getByRole('spinbutton', { name: 'Magnitude multiply', exact: true }).press('Tab')
  await expect.poll(() => latest?.visualizations?.spectral_visualizer?.channels?.[0]?.magnitude[4]).toBeGreaterThan(10)
  await page.getByRole('button', { name: 'Close parameters' }).click()
  await page.getByRole('button', { name: 'Edit spectral_curve', exact: true }).click()
  const magnitude = page.getByRole('slider', { name: 'Magnitude curve', exact: true })
  await magnitude.scrollIntoViewIfNeeded()
  const rect = (await magnitude.boundingBox())!
  await page.mouse.move(rect.x + 8, rect.y + rect.height - 2)
  await page.mouse.down()
  await page.mouse.move(rect.x + rect.width - 8, rect.y + rect.height - 2, { steps: 12 })
  await page.mouse.up()
  await expect.poll(async () => (await load()).graph.nodes.find((n: any) => n.id === 'spectral_curve').parameters.magnitude_curve_16).toBeLessThan(0.03)
  await expect.poll(() => latest?.visualizations?.spectral_visualizer?.channels?.[0]?.magnitude[4]).toBeLessThan(0.5)
  expect(latest.running).toBe(true)
  await page.getByRole('button', { name: 'Reset magnitude curve', exact: true }).click()
  await expect.poll(() => latest?.visualizations?.spectral_visualizer?.channels?.[0]?.magnitude[4]).toBeGreaterThan(10)
  const phase = page.getByRole('slider', { name: 'Phase curve', exact: true })
  await phase.focus()
  await phase.press('ArrowRight')
  await phase.press('ArrowUp')
  await expect.poll(async () => (await load()).graph.nodes.find((n: any) => n.id === 'spectral_curve').parameters.phase_curve_1).toBeGreaterThan(0)
  await expect(page.getByRole('button', { name: 'Undo last edit', exact: true })).toBeEnabled()
  await page.screenshot({ path: '../docs/spectral-curve-editor.png', fullPage: true })
  await page.getByRole('button', { name: 'Undo last edit', exact: true }).click()
  await expect.poll(async () => (await load()).graph.nodes.find((n: any) => n.id === 'spectral_curve').parameters.phase_curve_1 ?? 0).toBe(0)
  const invalid = await load()
  invalid.graph.nodes.find((n: any) => n.id === 'spectral_curve').parameters.magnitude_curve_0 = 3
  expect((await page.request.put(`/api/projects/${project.id}`, { headers, data: invalid })).status()).toBe(400)
  await page.getByRole('button', { name: 'Close parameters' }).click()
  await page.getByRole('button', { name: 'Deactivate show', exact: true }).click()
})
