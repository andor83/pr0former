import { test, expect } from '@playwright/test'
test('syncopation, point tools, mixing and preparation playback', async ({
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
      data: { name: 'Entry tools', mode: 'structured' },
    })
  ).json()
  project.parts[0].notes = []
  project.parts.push({
    ...structuredClone(project.parts[0]),
    id: 'second',
    name: 'Second',
  })
  project = await (
    await page.request.put(`/api/projects/${project.id}`, {
      headers,
      data: project,
    })
  ).json()
  const url = `/api/projects/${project.id}`
  const read = async () => (await (await page.request.get(url)).json()).project
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  const staff = page
    .locator('[data-score-part="part-1"] [data-staff-id]')
    .first()
  await page.getByLabel('Note values', { exact: true }).click()
  await page
    .getByRole('button', { name: 'Quarter note (5)', exact: true })
    .click()
  await staff.click({ position: { x: 255, y: 118 } })
  await expect.poll(async () => (await read()).parts[0].notes.length).toBe(1)
  const note = (await read()).parts[0].notes[0]
  expect(note.duration).toBe(1)
  expect(note.beat % 1).not.toBe(0)
  const glyph = staff.locator('[data-note-id]').first()
  await glyph.hover()
  await expect(glyph.locator('path').first()).toHaveCSS(
    'fill',
    'rgb(232, 120, 22)',
  )
  await glyph.click()
  await expect(glyph.locator('path').first()).toHaveCSS(
    'fill',
    'rgb(22, 128, 60)',
  )
  await page.keyboard.press('Escape')
  await page.getByLabel('Clefs', { exact: true }).click()
  await page
    .getByRole('button', { name: 'bass clef tool', exact: true })
    .click()
  await staff.click({ position: { x: 430, y: 95 } })
  await expect
    .poll(async () => (await read()).parts[0].staves?.[0].clef_changes?.length)
    .toBe(1)
  await page.getByLabel('Time signatures', { exact: true }).click()
  await page
    .getByRole('button', { name: '3/4 time signature tool', exact: true })
    .click()
  await staff.click({ position: { x: 520, y: 95 } })
  await expect
    .poll(async () => (await read()).score?.meters?.[0]?.beats)
    .toBe(3)
  await page.getByRole('button', { name: 'Mute Second', exact: true }).click()
  await expect.poll(async () => (await read()).parts[1].muted).toBe(true)
  await page.getByRole('button', { name: 'Solo Second', exact: true }).click()
  await expect.poll(async () => (await read()).parts[1].solo).toBe(true)
  expect((await read()).parts[1].muted).toBe(false)
  await page.getByLabel('Bar tools and repeats', { exact: true }).click()
  await page
    .getByRole('button', { name: 'Insert bars tool', exact: true })
    .click()
  await staff.click({ position: { x: 280, y: 95 } })
  const modal = page.getByRole('dialog')
  await expect(modal).toBeVisible()
  const beforeLength = (await read()).score.length
  await modal.getByLabel('Number of bars', { exact: true }).fill('2')
  await modal
    .getByRole('button', { name: 'Insert after bar', exact: true })
    .click()
  await expect
    .poll(async () => (await read()).score.length)
    .toBeGreaterThan(beforeLength)
  await page.keyboard.press('Escape')
  await page.getByLabel('Bar tools and repeats', { exact: true }).click()
  await page
    .getByRole('button', { name: 'double barline tool', exact: true })
    .click()
  await staff.click({ position: { x: 280, y: 95 } })
  await expect
    .poll(async () => (await read()).score.barlines?.[0]?.style)
    .toBe('double')
  const overfull = await read()
  overfull.parts[1].notes = [
    {
      id: 'overflow',
      pitch: 60,
      beat: 2.5,
      duration: 4,
      velocity: 90,
      rest: false,
      tied: false,
    },
  ]
  expect(
    (await page.request.put(url, { headers, data: overfull })).ok(),
  ).toBeTruthy()
  await expect(page.getByRole('img', { name: /^Overfull bar / })).toBeVisible()
  await page
    .getByRole('button', { name: 'Enable audio engine', exact: true })
    .click()
  await page.getByLabel('Count in', { exact: true }).selectOption('0')
  await page.getByRole('button', { name: 'Play', exact: true }).click()
  await page.getByRole('button', { name: 'Mute Second', exact: true }).click()
  await expect.poll(async () => (await read()).parts[1].muted).toBe(true)
  // Audition is authorized against an existing saved note and uses the graph.
  expect(
    (
      await page.request.post(`${url}/audition`, {
        headers,
        data: { part: 'part-1', note: note.id },
      })
    ).ok(),
  ).toBeTruthy()
  expect(
    (
      await page.request.post(`${url}/audition`, {
        headers,
        data: { part: 'part-1', note: 'missing' },
      })
    ).status(),
  ).toBe(400)
  await page
    .getByRole('button', { name: 'Performance mode', exact: true })
    .click()
  // Performance mode is a read-only score view: notation must not acquire a
  // selection or highlight when it is clicked.
  const performanceGlyph = page
    .locator('[data-score-part="part-1"] [data-note-id]')
    .first()
  await expect(performanceGlyph).toHaveAttribute('data-selected', 'false')
  await performanceGlyph.dispatchEvent('pointerdown', { button: 0 })
  await performanceGlyph.dispatchEvent('pointerup', { button: 0 })
  await expect(
    page.locator('[data-score-element][data-selected="true"]'),
  ).toHaveCount(0)
  const locked = await read()
  locked.parts[0].name = 'Forbidden'
  expect(
    (await page.request.put(url, { headers, data: locked })).status(),
  ).toBe(400)
  await page
    .getByRole('button', { name: 'Exit performance mode', exact: true })
    .click()
  await page
    .getByRole('button', { name: 'Disable audio engine', exact: true })
    .click()
  await expect(
    page.getByRole('button', { name: 'Enable audio engine', exact: true }),
  ).toBeVisible()
  await expect(
    page.getByRole('button', { name: 'Mute Second', exact: true }),
  ).toBeEnabled()
  await expect(page.getByRole('alert')).toHaveCount(0)
  await page.screenshot({ path: '../test-results/score-entry-tools.png' })
})
