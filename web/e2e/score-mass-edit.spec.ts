import { test, expect } from '@playwright/test'
test('bar selection mass edit: transpose, copy/cut/paste, durations and voices', async ({
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
      data: { name: 'Mass edit', mode: 'structured' },
    })
  ).json()
  project.parts[0].notes = [
    { id: 'c', pitch: 60, beat: 0, duration: 1, velocity: 90, rest: false, tied: false },
    { id: 'e', pitch: 64, beat: 1, duration: 1, velocity: 90, rest: false, tied: false },
  ]
  project.score = { version: 1, length: 16, loop_score: false, meters: [], keys: [], repeats: [] }
  project = await (
    await page.request.put(`/api/projects/${project.id}`, { headers, data: project })
  ).json()
  const url = `/api/projects/${project.id}`
  const notes = async () =>
    ((await (await page.request.get(url)).json()).project.parts[0].notes as any[]).sort(
      (a, b) => a.beat - b.beat || a.pitch - b.pitch,
    )
  const at = async (beat: number) => (await notes()).filter((n) => n.beat === beat)
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & parts', exact: true }).click()
  const staff = page.locator('[data-score-part="part-1"] [data-staff-id]').first()
  const region = page.locator('[data-bar-region]')
  // Select bar 1 and transpose with the arrow keys.
  await staff.click({ position: { x: 300, y: 60 } })
  await expect(region).toHaveAttribute('data-end', '4')
  await page.keyboard.press('ArrowUp')
  await expect.poll(async () => (await notes()).map((n) => n.pitch)).toEqual([62, 65])
  await page.keyboard.press('Shift+ArrowDown')
  await expect.poll(async () => (await notes()).map((n) => n.pitch)).toEqual([50, 53])
  // Copy bar 1, paste onto bar 3.
  await page.keyboard.press('ControlOrMeta+C')
  await staff.click({ position: { x: 1000, y: 60 } })
  await expect(region).toHaveAttribute('data-start', '8')
  await page.keyboard.press('ControlOrMeta+V')
  await expect.poll(async () => (await at(8)).map((n) => n.pitch)).toEqual([50])
  expect((await at(9)).map((n) => n.pitch)).toEqual([53])
  await expect(page.getByText('Editing 2 selected note(s)', { exact: false })).toBeVisible()
  await page.keyboard.press('Escape')
  // Mass edit dialog: double durations and move to voice 2 in bar 1.
  await staff.click({ position: { x: 300, y: 60 }, button: 'right' })
  await page
    .getByRole('menu', { name: 'Measure actions' })
    .getByRole('menuitem', { name: /^Transpose, durations/ })
    .click()
  const dialog = page.getByRole('dialog', { name: /^Measure/ })
  await dialog.getByRole('button', { name: 'Double (×2)', exact: true }).click()
  await expect
    .poll(async () => (await notes()).slice(0, 2).map((n) => [n.beat, n.duration]))
    .toEqual([
      [0, 2],
      [2, 2],
    ])
  await dialog.getByLabel('Move to voice', { exact: true }).selectOption('2')
  await dialog.getByRole('button', { name: 'Move to voice', exact: true }).click()
  await expect
    .poll(async () => (await notes()).slice(0, 2).map((n) => n.notation.voice))
    .toEqual([2, 2])
  await page.keyboard.press('Escape')
  await expect(page.getByRole('dialog')).toHaveCount(0)
  // Cut bar 3, then paste at the caret in Write mode after a clicked note in bar 2.
  await staff.click({ position: { x: 1000, y: 60 } })
  await expect(region).toHaveAttribute('data-start', '8')
  await page.keyboard.press('ControlOrMeta+X')
  await expect.poll(async () => (await at(8)).length).toBe(0)
  await page.getByRole('button', { name: 'Write', exact: true }).click()
  await staff.click({ position: { x: 574, y: 118 } })
  await expect.poll(async () => (await at(4)).length).toBe(1)
  await expect(page.locator('[data-entry-caret]')).toHaveAttribute('data-beat', '5')
  await page.keyboard.press('ControlOrMeta+V')
  await expect.poll(async () => (await at(5)).map((n) => n.pitch)).toEqual([50])
  expect((await at(6)).map((n) => n.pitch)).toEqual([53])
  await expect(page.locator('[data-entry-caret]')).toHaveAttribute('data-beat', '9')
  await expect(page.getByRole('alert')).toHaveCount(0)
  await page.screenshot({ path: '../test-results/score-mass-edit.png' })
})
