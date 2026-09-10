import { test, expect } from '@playwright/test'
test('tempo map and staff marks: entry, editing, playback tempo and MusicXML round trip', async ({
  page,
}) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  let project = await (
    await page.request.post('/api/projects', {
      headers,
      data: { name: 'Tempo marks', mode: 'structured' },
    })
  ).json()
  project.parts[0].notes = [
    { id: 'c', pitch: 60, beat: 0, duration: 1, velocity: 90, rest: false, tied: false },
    { id: 'e', pitch: 64, beat: 4, duration: 1, velocity: 90, rest: false, tied: false },
  ]
  project.score = { version: 1, length: 8, loop_score: false, meters: [], keys: [], repeats: [] }
  project = await (
    await page.request.put(`/api/projects/${project.id}`, { headers, data: project })
  ).json()
  const url = `/api/projects/${project.id}`
  const read = async () => (await (await page.request.get(url)).json()).project
  const packets: any[] = []
  page.on('websocket', (socket) =>
    socket.on('framereceived', ({ payload }) => {
      const value = JSON.parse(String(payload))
      if (value.type === 'telemetry') packets.push(value)
    }),
  )
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  const staff = page.locator('[data-score-part="part-1"] [data-staff-id]').first()
  // Tempo change at bar 2 via the measure menu.
  await staff.click({ position: { x: 700, y: 60 }, button: 'right' })
  await page
    .getByRole('menu', { name: 'Measure actions' })
    .getByRole('menuitem', { name: 'Tempo change…', exact: true })
    .click()
  const tempoDialog = page.getByRole('dialog', { name: 'Tempo change', exact: true })
  await tempoDialog.getByLabel('Tempo', { exact: true }).fill('60')
  await tempoDialog.getByRole('button', { name: 'Apply tempo', exact: true }).click()
  await expect.poll(async () => (await read()).score?.tempos).toEqual([{ beat: 4, bpm: 60 }])
  const tempoMark = page.locator('[data-score-element*="tempo"]').first()
  await expect(tempoMark).toBeVisible()
  await expect(tempoMark).toContainText('♩ = 60')
  // A tempo mark at bar 1 through the Measure dialog also sets the project tempo.
  await staff.click({ position: { x: 300, y: 60 } })
  await page.getByRole('button', { name: 'Measure', exact: true }).click()
  const measure = page.getByRole('dialog', { name: /^Measure/ })
  await measure.getByRole('button', { name: 'Tempo', exact: true }).click()
  await measure.getByLabel('Quarter notes per minute', { exact: true }).fill('240')
  await measure.getByRole('button', { name: 'Apply', exact: true }).click()
  await expect.poll(async () => (await read()).bpm).toBe(240)
  expect((await read()).score.tempos).toEqual([
    { beat: 0, bpm: 240 },
    { beat: 4, bpm: 60 },
  ])
  await measure.getByRole('button', { name: 'Close', exact: true }).click()
  // Cue text via the palette tool and the mark dialog.
  await page.getByLabel('Text and tempo', { exact: true }).click()
  await page.getByRole('button', { name: 'Cue tool', exact: true }).click()
  await staff.click({ position: { x: 300, y: 118 } })
  const markDialog = page.getByRole('dialog', { name: 'Add mark', exact: true })
  await markDialog.getByLabel('Mark text', { exact: true }).fill('start granular')
  await markDialog.getByLabel('Mark text', { exact: true }).press('Enter')
  await expect
    .poll(async () => (await read()).parts[0].staves?.[0]?.marks)
    .toMatchObject([{ kind: 'cue', text: 'start granular' }])
  const cue = page.locator('[data-score-element*="mark"]').first()
  await expect(cue).toContainText('start granular')
  // Double-click edits; Backspace deletes; Undo restores.
  await cue.locator('text').first().dblclick()
  const edit = page.getByRole('dialog', { name: 'Edit mark', exact: true })
  await edit.getByLabel('Mark text', { exact: true }).fill('start granular now')
  await edit.getByRole('button', { name: 'Save mark', exact: true }).click()
  await expect
    .poll(async () => (await read()).parts[0].staves[0].marks[0].text)
    .toBe('start granular now')
  await cue.locator('text').first().click()
  await expect(page.getByText('Selected mark', { exact: false })).toBeVisible()
  await page.keyboard.press('Backspace')
  await expect.poll(async () => (await read()).parts[0].staves?.[0]?.marks?.length).toBe(0)
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await expect.poll(async () => (await read()).parts[0].staves?.[0]?.marks?.length).toBe(1)
  // Playback follows the tempo map: 240 at the start, 60 once bar 2 is reached.
  await page.getByLabel('Count in', { exact: true }).selectOption('0')
  await page.getByRole('button', { name: 'Play', exact: true }).click()
  await expect.poll(() => packets.some((v) => v.running && v.beat < 4 && v.bpm === 240)).toBe(true)
  await expect
    .poll(() => packets.some((v) => v.beat >= 4 && v.bpm === 60), { timeout: 15000 })
    .toBe(true)
  await page.getByRole('button', { name: 'Stop', exact: true }).click()
  await page.getByRole('button', { name: 'Disable audio engine', exact: true }).click()
  // MusicXML export carries the metronome marks and the cue text; import restores them.
  const downloadPromise = page.waitForEvent('download')
  await page.getByRole('button', { name: 'MusicXML', exact: true }).click()
  const { readFile } = await import('node:fs/promises')
  const xml = await readFile((await (await downloadPromise).path())!, 'utf8')
  expect(xml).toContain('<per-minute>240</per-minute>')
  expect(xml).toContain('<per-minute>60</per-minute>')
  expect(xml).toContain('<words font-weight="bold">start granular now</words>')
  const before = (await read()).revision
  await page.locator('input[accept=".xml,.musicxml"]').setInputFiles({
    name: 'tempo.musicxml',
    mimeType: 'application/xml',
    buffer: Buffer.from(xml),
  })
  await expect
    .poll(async () => {
      const current = await read()
      return current.revision > before ? current : null
    })
    .toMatchObject({
      score: { tempos: [{ beat: 0, bpm: 240 }, { beat: 4, bpm: 60 }] },
    })
  const imported = await read()
  expect(imported.parts[0].staves[0].marks).toMatchObject([
    { kind: 'cue', text: 'start granular now' },
  ])
  await expect(page.getByRole('alert')).toHaveCount(0)
  await page.screenshot({ path: '../test-results/score-tempo-marks.png' })
})
