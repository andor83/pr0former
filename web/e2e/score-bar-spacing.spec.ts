import { test, expect } from '@playwright/test'
test('a final sixteenth keeps readable space before the next barline at every spacing setting', async ({ page }) => {
  const headers = { 'X-Pr0former': '1' }, status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data: { username: 'browser-test', password: 'test1234' } })
  const p = await (await page.request.post('/api/projects', { headers, data: { name: 'Bar ending spacing', mode: 'structured' } })).json()
  p.parts[0].notes = [
    { id: 'a', pitch: 60, beat: 0, duration: 1 },
    { id: 'b', pitch: 62, beat: 1, duration: 1 },
    { id: 'c', pitch: 64, beat: 2, duration: 1 },
    { id: 'd', pitch: 62, beat: 3, duration: 0.75 },
    { id: 'tail', pitch: 60, beat: 3.75, duration: 0.25 },
    { id: 'next', pitch: 60, beat: 4, duration: 1 },
  ].map(n => ({ ...n, velocity: 90, rest: false, tied: false }))
  p.score = { version: 1, length: 8, loop_score: false, meters: [], keys: [], repeats: [] }
  const url = `/api/projects/${p.id}`
  expect((await page.request.put(url, { headers, data: p })).ok()).toBe(true)
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  await page.getByRole('button', { name: 'Select', exact: true }).click()
  const tail = page.locator('[data-note-id="tail"]').first()
  const line = page.locator(`[data-score-element*='"kind":"barline"'][data-score-element*='"beat":4,']`).first().locator('path').first()
  for (const spacing of ['45', '90', '180']) {
    await page.getByLabel('Note spacing', { exact: true }).fill(spacing)
    await expect.poll(async () => {
      const note = await tail.boundingBox(), bar = await line.boundingBox()
      return note && bar ? bar.x - (note.x + note.width) : -1
    }).toBeGreaterThanOrEqual(12)
  }
  await page.getByLabel('Note spacing', { exact: true }).fill('90')
  await tail.locator('.vf-notehead').first().click()
  await expect(tail).toHaveAttribute('data-selected', 'true')
  await page.screenshot({ path: '/tmp/pr0-sixteenth-bar-spacing.png' })
  expect((await (await page.request.get(url)).json()).project.parts[0].notes).toEqual(p.parts[0].notes)
})
