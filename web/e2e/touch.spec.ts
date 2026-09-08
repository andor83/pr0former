import { test, expect } from '@playwright/test'

test.use({ hasTouch: true, viewport: { width: 1024, height: 768 } })
test('touch marquee, tap, two-finger pan and pinch keep the page stationary', async ({ page, context }) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })
  let project = await (await page.request.post('/api/projects', { headers, data: { name: 'Touch gestures', mode: 'freeform' } })).json()
  project.parts = []
  project.graph = { nodes: ['A', 'B'].map((id, i) => ({ id, kind: 'value', label: id, x: i * 300, y: 0, channels: 1, parameters: { value: 1 } })), edges: [] }
  await page.request.put(`/api/projects/${project.id}`, { headers, data: project })
  await page.goto('/')
  const nodes = page.locator('.vue-flow__node')
  await expect(nodes).toHaveCount(2)
  await page.waitForTimeout(1000)
  const cdp = await context.newCDPSession(page)
  const touch = async (type: string, points: { x: number; y: number; id: number }[]) => { await cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points }); await page.waitForTimeout(30) }
  const transform = () => page.locator('.vue-flow__transformationpane').evaluate(el => getComputedStyle(el).transform)
  const boxes = await nodes.evaluateAll(els => els.map(el => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, right: r.right, bottom: r.bottom } }))
  const a = boxes[0]!
  await page.touchscreen.tap(a.x + 35, a.y + 45)
  await expect(page.locator('.vue-flow__node.selected')).toHaveCount(1)
  const start = { x: Math.min(...boxes.map(b => b.x)) - 12, y: a.y - 12, id: 1 }
  const end = { x: Math.max(...boxes.map(b => b.right)) + 12, y: a.bottom + 12, id: 1 }
  const original = await transform()
  await touch('touchStart', [start])
  await touch('touchMove', [end])
  await touch('touchEnd', [])
  await expect(page.locator('.vue-flow__node.selected')).toHaveCount(2)
  expect(await transform()).toBe(original)
  const canvas = (await page.locator('.graph-canvas').boundingBox())!
  const p = { x: canvas.x + 80, y: canvas.y + canvas.height - 120, id: 1 }
  const q = { x: p.x + 100, y: p.y, id: 2 }
  await touch('touchStart', [p])
  await touch('touchStart', [p, q])
  await touch('touchMove', [{ ...p, x: p.x + 40, y: p.y - 20 }, { ...q, x: q.x + 40, y: q.y - 20 }])
  const panned = await transform()
  expect(panned).not.toBe(original)
  await expect(page.locator('.vue-flow__selection')).toHaveCount(0)
  await touch('touchMove', [{ ...p, x: p.x + 10, y: p.y - 20 }, { ...q, x: q.x + 70, y: q.y - 20 }])
  const pinched = await transform()
  expect(pinched).not.toBe(panned)
  expect(Number(pinched.split('(')[1]!.split(',')[0])).toBeGreaterThan(Number(panned.split('(')[1]!.split(',')[0]))
  await touch('touchEnd', [{ ...p, x: p.x + 10, y: p.y - 20 }])
  await touch('touchMove', [{ ...p, x: p.x + 30 }])
  expect(await transform()).toBe(pinched)
  await touch('touchEnd', [])
  expect(await page.evaluate(() => [scrollX, scrollY])).toEqual([0, 0])
  // A gesture may start over a node, and cancellation must release it cleanly.
  const nodeBox = (await nodes.first().boundingBox())!
  const overNode = { x: nodeBox.x + 35, y: nodeBox.y + 45, id: 1 }
  const second = { x: overNode.x + 80, y: overNode.y, id: 2 }
  await touch('touchStart', [overNode])
  await touch('touchStart', [overNode, second])
  await touch('touchMove', [{ ...overNode, y: overNode.y + 20 }, { ...second, y: second.y + 20 }])
  await touch('touchCancel', [])
  const stored = (await (await page.request.get(`/api/projects/${project.id}`)).json()).project
  expect(stored.graph.nodes.map((n: any) => [n.x, n.y])).toEqual([[0, 0], [300, 0]])
  await page.touchscreen.tap(canvas.x + canvas.width - 15, canvas.y + canvas.height - 100)
  await expect(page.locator('.vue-flow__node.selected')).toHaveCount(0)
})

test('touch library placement, scrolling and fullscreen swipe containment', async ({ page, context }) => {
  await page.addInitScript(() => Object.defineProperty(crypto, 'randomUUID', { configurable: true, value: undefined }))
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })
  let project = await (await page.request.post('/api/projects', { headers, data: { name: 'Touch library', mode: 'freeform' } })).json()
  project.parts = []
  project.graph = { nodes: [
    { id: 'Group', kind: 'subgraph', label: 'Pack', x: 0, y: 0, channels: 2, parameters: {} },
    { id: 'Leaf', kind: 'value', label: 'Leaf', parent: 'Group', x: 0, y: 0, channels: 2, parameters: { value: 1 } },
  ], edges: [] }
  project = await (await page.request.put(`/api/projects/${project.id}`, { headers, data: project })).json()
  const pack = await (await page.request.post(`/api/projects/${project.id}/subgraphs`, { headers, data: { node: 'Group', revision: project.revision, name: 'Touch pack', public: false } })).json()
  const load = async () => (await (await page.request.get(`/api/projects/${project.id}`)).json()).project
  await page.goto('/')
  await expect(page.locator('.vue-flow__node')).toHaveCount(1)
  await page.waitForTimeout(1000)
  const cdp = await context.newCDPSession(page)
  const touch = async (type: string, points: { x: number; y: number; id: number }[]) => {
    await cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points })
    await page.waitForTimeout(40)
  }
  const canvas = (await page.locator('.graph-canvas').boundingBox())!
  const target = { x: canvas.x + canvas.width * .6, y: canvas.y + canvas.height * .65, id: 1 }
  await page.getByLabel('Search nodes').fill('Value')
  const entry = page.locator('.node-library-content .library-node').filter({ hasText: 'Value' }).first()
  const drag = async (locator: typeof entry, destination: typeof target, cancel = false) => {
    await locator.scrollIntoViewIfNeeded()
    await page.waitForTimeout(300) // Accordion layout must settle before raw touch coordinates.
    const box = (await locator.boundingBox())!
    const origin = { x: box.x + box.width / 2, y: box.y + box.height / 2, id: 1 }
    await touch('touchStart', [origin])
    await touch('touchMove', [{ ...origin, x: origin.x + 40 }])
    await expect(page.locator('.library-touch-preview')).toBeVisible()
    await touch('touchMove', [destination])
    await touch(cancel ? 'touchCancel' : 'touchEnd', [])
    await expect(page.locator('.library-touch-preview')).toHaveCount(0)
  }
  await page.locator('.vue-flow__controls-zoomout').click()
  await page.waitForTimeout(300)
  await drag(entry, target)
  await expect.poll(async () => (await load()).graph.nodes.length).toBe(3)
  const placed = (await load()).graph.nodes.find((n: any) => n.kind === 'value' && !n.parent)
  const placedBox = (await page.locator(`.vue-flow__node[data-id="${placed.id}"]`).boundingBox())!
  expect(placedBox.x).toBeCloseTo(target.x, 0); expect(placedBox.y).toBeCloseTo(target.y, 0)
  await page.waitForTimeout(350)
  expect((await load()).graph.nodes.length).toBe(3) // No compatibility-click duplicate.
  await drag(entry, target, true)
  await drag(entry, { ...target, x: 25, y: 100 })
  expect((await load()).graph.nodes.length).toBe(3)
  await entry.tap()
  await expect.poll(async () => (await load()).graph.nodes.length).toBe(4)
  // Saved subgraphs use the same touch path and keep their version reference.
  await page.getByRole('button', { name: 'Subgraph library', exact: true }).click()
  await drag(page.getByRole('button', { name: 'Insert Touch pack', exact: true }), target)
  await expect.poll(async () => (await load()).graph.nodes.length).toBe(6)
  expect((await load()).graph.nodes.some((n: any) => n.library?.id === pack.id && n.library.version === 1)).toBe(true)
  await page.getByRole('button', { name: 'Node library', exact: true }).click()
  await page.getByLabel('Search nodes').fill('')
  const list = page.locator('.node-library-content .library-list')
  await expect.poll(() => list.evaluate(el => el.scrollHeight - el.clientHeight)).toBeGreaterThan(100)
  await list.locator('.library-node').first().scrollIntoViewIfNeeded()
  await page.waitForTimeout(300)
  const first = (await list.locator('.library-node').first().boundingBox())!
  const scrollOrigin = { x: first.x + 70, y: first.y + 20, id: 1 }
  await touch('touchStart', [scrollOrigin])
  await touch('touchMove', [{ ...scrollOrigin, y: scrollOrigin.y - 90 }])
  await touch('touchEnd', [])
  await expect.poll(() => list.evaluate(el => el.scrollTop)).toBeGreaterThan(50)
  expect((await load()).graph.nodes.length).toBe(6)
  await page.getByRole('button', { name: 'Fullscreen', exact: true }).click()
  await expect.poll(() => page.evaluate(() => !!document.fullscreenElement)).toBe(true)
  await page.evaluate(() => {
    (window as any).__preventedSwipes = 0
    document.addEventListener('touchmove', event => { if (event.defaultPrevented) (window as any).__preventedSwipes++ })
  })
  const heading = (await page.locator('.workspace-header').boundingBox())!
  const pull = { x: heading.x + heading.width / 2, y: heading.y + 20, id: 1 }
  await touch('touchStart', [pull])
  await touch('touchMove', [{ ...pull, y: pull.y + 100 }])
  await touch('touchEnd', [])
  expect(await page.evaluate(() => (window as any).__preventedSwipes)).toBeGreaterThan(0)
  expect(await page.evaluate(() => [!!document.fullscreenElement, scrollX, scrollY])).toEqual([true, 0, 0])
  await list.evaluate(el => { el.scrollTop = 0 })
  const row = (await list.locator('.library-node').first().boundingBox())!
  const edge = { x: row.x + 70, y: row.y + 20, id: 1 }
  await touch('touchStart', [edge]); await touch('touchMove', [{ ...edge, y: edge.y + 100 }]); await touch('touchEnd', [])
  expect(await list.evaluate(el => el.scrollTop)).toBe(0)
  expect(await page.evaluate(() => !!document.fullscreenElement)).toBe(true)
  await page.evaluate(() => document.exitFullscreen())
})
