import { test, expect } from '@playwright/test'

test('note drags preview notation and routed MIDI before a single undoable save', async ({ page }) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers, data: { username: 'browser-test', password: 'test1234' },
  })
  let project = await (await page.request.post('/api/projects', {
    headers, data: { name: 'Note drag preview', mode: 'structured' },
  })).json()
  project.parts[0].notes = [
    { id: 'drag-c', pitch: 60, beat: 1, duration: 1, velocity: 90, rest: false, tied: false },
    { id: 'stay-e', pitch: 64, beat: 3, duration: 1, velocity: 90, rest: false, tied: false },
  ]
  project.graph.nodes.push({ id: 'preview-midi', kind: 'part_midi', label: 'Preview MIDI',
    x: 0, y: 300, channels: 2, parameters: {}, part_id: project.parts[0].id })
  const url = `/api/projects/${project.id}`
  const saved = await page.request.put(url, { headers, data: project })
  expect(saved.ok()).toBe(true)
  project = await saved.json()
  const read = async () => (await (await page.request.get(url)).json()).project
  let latest: any
  const sounded: number[] = []
  page.on('websocket', socket => socket.on('framereceived', ({ payload }) => {
    const event = JSON.parse(String(payload))
    if (event.type === 'telemetry') {
      latest = event
      if (event.values?.['preview-midi']?.gate === 1) sounded.push(event.values['preview-midi'].pitch)
    }
  }))
  const auditions: any[] = []
  let saves = 0
  page.on('request', request => {
    if (request.url().endsWith(`${url}/audition`)) auditions.push(request.postDataJSON())
    if (request.url().endsWith(url) && request.method() === 'PUT') saves++
  })
  await page.goto('/')
  await page.getByRole('button', { name: 'Enable audio engine', exact: true }).click()
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  const glyph = page.locator('[data-note-id="drag-c"]').first()
  const staff = page.locator('[data-score-part="part-1"] [data-staff-id]').first()
  const head = glyph.locator('.vf-notehead').first()
  const box = await head.boundingBox()
  expect(box).toBeTruthy()
  const x = box!.x + box!.width / 2, y = box!.y + box!.height / 2
  await page.mouse.move(x, y)
  await page.mouse.down()
  await expect.poll(() => sounded.includes(60)).toBe(true)
  await page.mouse.move(x, y - 10, { steps: 4 })
  await expect(glyph).toHaveAttribute('aria-label', 'Note 64 at beat 2')
  await expect(page.locator('.score-marquee')).toHaveCount(0)
  await expect.poll(() => sounded.includes(64)).toBe(true)
  await page.mouse.move(x, y - 20, { steps: 4 })
  await expect(glyph).toHaveAttribute('aria-label', 'Note 67 at beat 2')
  await expect.poll(() => sounded.includes(67)).toBe(true)
  expect((await read()).parts[0].notes[0].pitch).toBe(60)
  expect(saves).toBe(0)
  // Moving within the same staff step does not retrigger.
  const attacks = auditions.length
  await page.mouse.move(x, y - 21)
  expect(auditions.length).toBe(attacks)
  await page.mouse.up()
  await expect.poll(async () => (await read()).parts[0].notes[0].pitch).toBe(67)
  expect(saves).toBe(1)
  await expect.poll(() => latest?.values?.['preview-midi']?.gate).toBe(0)
  await page.keyboard.press('ControlOrMeta+Z')
  await expect.poll(async () => (await read()).parts[0].notes[0].pitch).toBe(60)

  // Playback started by another control cancels an in-flight edit and locks entry.
  const playingBox = await head.boundingBox()
  await page.mouse.move(playingBox!.x + playingBox!.width / 2, playingBox!.y + playingBox!.height / 2)
  await page.mouse.down()
  await page.mouse.move(playingBox!.x + playingBox!.width / 2, playingBox!.y + playingBox!.height / 2 - 10)
  await expect(glyph).toHaveAttribute('aria-label', 'Note 64 at beat 2')
  const beforePlay = saves
  expect((await page.request.post(`${url}/transport`, { headers, data: { action: 'play', count_in_beats: 0 } })).ok()).toBe(true)
  await expect(page.getByRole('button', { name: 'Write', exact: true })).toBeDisabled()
  await expect(glyph).toHaveAttribute('aria-label', 'Note 60 at beat 2')
  await page.mouse.up()
  expect(saves).toBe(beforePlay)
  await page.keyboard.press('ArrowUp')
  expect((await read()).parts[0].notes[0].pitch).toBe(60)
  await page.getByRole('button', { name: 'Stop', exact: true }).click()
  await expect(page.getByRole('button', { name: 'Write', exact: true })).toBeEnabled()

  // Escape and pointer cancellation discard the live position, without saving.
  for (const cancel of ['escape', 'pointercancel', 'blur']) {
    const b = await head.boundingBox()
    await page.mouse.move(b!.x + b!.width / 2, b!.y + b!.height / 2)
    await page.mouse.down()
    await page.mouse.move(b!.x + b!.width / 2 + 96, b!.y + b!.height / 2 - 10, { steps: 4 })
    await expect(glyph).toHaveAttribute('aria-label', /^Note 64 at beat /)
    const before = saves
    if (cancel === 'escape') await page.keyboard.press('Escape')
    else if (cancel === 'blur') await page.evaluate(() => window.dispatchEvent(new Event('blur')))
    else await page.locator('.ensemble-scroll').dispatchEvent('pointercancel', { pointerId: 1 })
    await page.mouse.up()
    await expect(glyph).toHaveAttribute('aria-label', 'Note 60 at beat 2')
    expect(saves).toBe(before)
  }
  // Invalid preview pitches and missing source notes are rejected on the server.
  expect((await page.request.post(`${url}/audition`, { headers,
    data: { part: 'part-1', note: 'drag-c', pitch: 128 } })).status()).toBe(400)
  expect((await page.request.post(`${url}/audition`, { headers,
    data: { part: 'part-1', note: 'missing', pitch: 64 } })).status()).toBe(400)

  // Empty staff drags still select with a rectangle; a disabled engine still permits note moves.
  await page.getByRole('button', { name: 'Disable audio engine', exact: true }).click()
  const row = await staff.boundingBox()
  await page.mouse.move(row!.x + 270, row!.y + 30)
  await page.mouse.down()
  await page.mouse.move(row!.x + 370, row!.y + 60, { steps: 4 })
  await expect(page.locator('.score-marquee')).toBeVisible()
  await page.mouse.up()
  await page.keyboard.press('Escape')
  const b = await head.boundingBox()
  await page.mouse.move(b!.x + b!.width / 2, b!.y + b!.height / 2)
  await page.mouse.down()
  await page.mouse.move(b!.x + b!.width / 2 + 96, b!.y + b!.height / 2 - 10, { steps: 4 })
  await expect(page.locator('.score-marquee')).toHaveCount(0)
  await page.mouse.up()
  await expect.poll(async () => (await read()).parts[0].notes[0].pitch).toBe(64)
  expect((await read()).parts[0].notes[0].beat).toBeGreaterThan(1)
  expect((await read()).parts[0].notes[1].pitch).toBe(64)
  await expect(page.getByRole('alert')).toHaveCount(0)
})
