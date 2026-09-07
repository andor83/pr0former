import { test, expect } from '@playwright/test'

test('live graph editing, FM conversion, context menus and keyboard transport in every mode', async ({ page }) => {
  test.setTimeout(120000)
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  expect((await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })).ok()).toBeTruthy()
  let latest: any
  const commands: string[] = [], errors: string[] = []
  page.on('pageerror', error => errors.push(error.message))
  page.on('request', request => { if (request.url().endsWith('/transport') && request.method() === 'POST') commands.push(request.postDataJSON().action) })
  page.on('websocket', socket => socket.on('framereceived', ({ payload }) => { const m = JSON.parse(String(payload)); if (m.type === 'telemetry') latest = m }))
  for (const mode of ['structured', 'conducted', 'freeform']) {
    let project = await (await page.request.post('/api/projects', { headers, data: { name: `Live ${mode}`, mode } })).json()
    project.bpm = 1 // A bar takes four minutes: live edits must not wait for it.
    // Keep the starter score's route to the already removed "tone" node,
    // as can occur in older projects. Unrelated live graph edits must work.
    const node = (id: string, kind: string, x: number, y: number, parameters = {}) => ({ id, kind, label: id, x, y, channels: 2, parameters })
    project.graph = { nodes: [node('Modulator', 'oscillator', 0, 0, { frequency: 100, amplitude: 1 }), node('Carrier', 'oscillator', 0, 450, { frequency: 440, amplitude: 0.1 }), node('Output', 'output', 800, 450, { gain: -12 })], edges: [{ id: 'sound', source: 'Carrier', source_port: 'out', target: 'Output', target_port: 'in' }] }
    expect((await page.request.put(`/api/projects/${project.id}`, { headers, data: project })).ok()).toBeTruthy()
    const load = async () => (await (await page.request.get(`/api/projects/${project.id}`)).json()).project
    await page.goto('/')
    await expect(page.getByRole('button', { name: `Live ${mode}`, exact: true })).toBeVisible()
    const graphNode = (id: string) => page.locator(`.vue-flow__node[data-id="${id}"]`)
    const focusGraph = async () => { await graphNode('Carrier').locator('.patch-node').focus() }
    await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeEnabled()
    const before = commands.length
    await focusGraph()
    await page.keyboard.down('Space')
    await page.keyboard.down('Space')
    await page.keyboard.up('Space')
    await expect.poll(() => commands.slice(before)).toEqual(['activate', 'play'])
    await expect.poll(() => latest?.project_id === project.id && latest?.running).toBe(true)
    expect(commands.slice(before)).toEqual(['activate', 'play'])
    const sample = latest.sample
    await page.getByLabel('Search nodes').fill('Audio to control')
    await page.getByLabel('Search nodes').press('Space')
    expect(commands.length).toBe(before + 2)
    await page.locator('.library-node').filter({ hasText: 'Audio to control' }).click()
    await expect.poll(async () => (await load()).graph.nodes.length).toBe(4)
    const converter = (await load()).graph.nodes.find((n: any) => n.kind === 'audio_to_control').id
    await expect.poll(() => latest?.values?.[converter]).toBeTruthy()
    expect(latest.running).toBe(true)
    expect(latest.sample).toBeGreaterThan(sample)
    await page.locator('.vue-flow__controls-fitview').click()
    await graphNode(converter).locator('.patch-node').click({ button: 'right', position: { x: 25, y: 45 } })
    await expect(page.getByRole('menu', { name: 'Node actions' })).toBeVisible()
    await page.getByRole('menuitem', { name: 'Edit node', exact: true }).click()
    await page.getByRole('spinbutton', { name: 'Scale', exact: true }).fill('100')
    await page.getByRole('spinbutton', { name: 'Scale', exact: true }).press('Tab')
    await expect.poll(async () => (await load()).graph.nodes.find((n: any) => n.id === converter).parameters.scale).toBe(100)
    await page.getByRole('spinbutton', { name: 'Offset', exact: true }).fill('440')
    await page.getByRole('spinbutton', { name: 'Offset', exact: true }).press('Tab')
    await expect.poll(async () => (await load()).graph.nodes.find((n: any) => n.id === converter).parameters.offset).toBe(440)
    await page.getByRole('button', { name: 'Close parameters' }).click()
    const connect = async (source: string, target: string, port: string) => {
      await graphNode(source).locator('.vue-flow__handle.source[data-handleid="out"]').click()
      await graphNode(target).locator(`.vue-flow__handle.target[data-handleid="${port}"]`).click()
    }
    await connect('Modulator', converter, 'in')
    await expect.poll(async () => (await load()).graph.edges.length).toBe(2)
    await connect(converter, 'Carrier', 'frequency')
    await expect.poll(async () => (await load()).graph.edges.length).toBe(3)
    await expect.poll(() => latest?.values?.Carrier?.frequency).toBeGreaterThan(339)
    expect(latest.values.Carrier.frequency).toBeLessThan(541)
    await page.getByRole('button', { name: 'Edit Carrier', exact: true }).click()
    await expect(page.getByRole('spinbutton', { name: 'Frequency', exact: true })).toHaveCount(0)
    await page.getByRole('button', { name: 'Disconnect', exact: true }).click()
    await expect.poll(async () => (await load()).graph.edges.length).toBe(2)
    await page.getByRole('button', { name: 'Close parameters' }).click()
    await focusGraph()
    await expect(page.getByRole('button', { name: 'Undo', exact: true })).toBeEnabled()
    await page.keyboard.press('ControlOrMeta+z')
    await expect.poll(async () => (await load()).graph.edges.length).toBe(3)
    // Selecting a node, then Backspace, removes it and all attached wires live.
    await graphNode(converter).locator('.patch-node').click({ position: { x: 25, y: 45 } })
    await page.keyboard.press('Backspace')
    await expect.poll(async () => (await load()).graph.nodes.length).toBe(3)
    expect((await load()).graph.edges.length).toBe(1)
    await focusGraph()
    await expect(page.getByRole('button', { name: 'Undo', exact: true })).toBeEnabled()
    await page.keyboard.press('ControlOrMeta+z')
    await expect.poll(async () => (await load()).graph.nodes.length).toBe(4)
    await graphNode(converter).locator('.patch-node').click({ button: 'right', position: { x: 25, y: 45 } })
    await page.getByRole('menuitem', { name: 'Delete node', exact: true }).click()
    await expect.poll(async () => (await load()).graph.nodes.length).toBe(3)
    await focusGraph()
    await page.keyboard.press('Space')
    await expect.poll(() => latest?.running).toBe(false)
    await page.keyboard.press('Space')
    await expect.poll(() => latest?.running).toBe(true)
    // Server validation continues to reject signal mismatches during a show.
    const invalid = await load()
    invalid.graph.edges.push({ id: 'bad', source: 'Modulator', source_port: 'out', target: 'Carrier', target_port: 'frequency' })
    expect((await page.request.put(`/api/projects/${project.id}`, { headers, data: invalid })).status()).toBe(400)
    await page.getByRole('button', { name: 'Deactivate show', exact: true }).click()
  }
  expect(errors).toEqual([])
})
