import { test, expect } from '@playwright/test'
test('caret keyboard entry, letters, ties, voices and Finale-style measure editing', async ({
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
      data: { name: 'Speedy entry', mode: 'structured' },
    })
  ).json()
  project.parts[0].notes = []
  project.score = {
    version: 1,
    length: 16,
    loop_score: false,
    meters: [],
    keys: [],
    repeats: [],
  }
  project = await (
    await page.request.put(`/api/projects/${project.id}`, {
      headers,
      data: project,
    })
  ).json()
  const url = `/api/projects/${project.id}`
  const read = async () => (await (await page.request.get(url)).json()).project
  const notes = async () =>
    ((await read()).parts[0].notes as any[]).sort(
      (a, b) => a.beat - b.beat || a.pitch - b.pitch,
    )
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  const staff = page
    .locator('[data-score-part="part-1"] [data-staff-id]')
    .first()
  // Write mode: a click enters a quarter note and places the caret after it.
  await page.getByRole('button', { name: 'Write', exact: true }).click()
  await staff.click({ position: { x: 214, y: 118 } })
  await expect.poll(async () => (await notes()).length).toBe(1)
  const first = (await notes())[0]
  expect(first.duration).toBe(1)
  const caret = page.locator('[data-entry-caret]')
  await expect(caret).toBeVisible()
  await expect(caret).toHaveAttribute('data-beat', String(first.beat + 1))
  // 4 = eighth at the caret pitch, caret advances by the written duration.
  await page.keyboard.press('4')
  await expect.poll(async () => (await notes()).length).toBe(2)
  expect((await notes())[1]).toMatchObject({
    beat: first.beat + 1,
    duration: 0.5,
    pitch: first.pitch,
  })
  await expect(caret).toHaveAttribute('data-beat', String(first.beat + 1.5))
  // Letter entry picks the nearest A and keeps the eighth duration.
  await page.keyboard.press('a')
  await expect.poll(async () => (await notes()).length).toBe(3)
  expect((await notes())[2].pitch % 12).toBe(9)
  expect((await notes())[2].duration).toBe(0.5)
  // Shift+letter stacks a chord tone on the last entry without advancing.
  await page.keyboard.press('Shift+C')
  await expect.poll(async () => (await notes()).length).toBe(4)
  const chord = (await notes()).filter((n) => n.beat === first.beat + 1.5)
  expect(chord.length).toBe(2)
  expect(chord.map((n) => n.pitch % 12).sort()).toEqual([0, 9])
  await expect(caret).toHaveAttribute('data-beat', String(first.beat + 2))
  // Arrow keys move the caret pitch; 5 inserts a quarter; T then 5 ties a repeated pitch.
  await page.keyboard.press('ArrowUp')
  await page.keyboard.press('ArrowUp')
  await page.keyboard.press('5')
  await expect.poll(async () => (await notes()).length).toBe(5)
  const fifth = (await notes()).find((n) => n.beat === first.beat + 2)
  expect(fifth.duration).toBe(1)
  await page.keyboard.press('t')
  await expect(page.getByText('tie pending', { exact: false })).toBeVisible()
  await page.keyboard.press('5')
  await expect.poll(async () => (await notes()).length).toBe(6)
  const tied = (await notes()).find((n) => n.beat === first.beat + 2),
    target = (await notes()).find((n) => n.beat === first.beat + 3)
  expect(tied.notation.tie_to).toBe(target.id)
  expect(target.pitch).toBe(tied.pitch)
  // 0 inserts a rest; Backspace removes the entry before the caret.
  await page.keyboard.press('0')
  await expect.poll(async () => (await notes()).length).toBe(7)
  expect((await notes()).at(-1).rest).toBe(true)
  await page.keyboard.press('Backspace')
  await expect.poll(async () => (await notes()).length).toBe(6)
  await expect(caret).toHaveAttribute('data-beat', String(first.beat + 4))
  // Alt+2 switches to voice 2 for the next entry.
  await page.keyboard.press('Alt+2')
  await page.keyboard.press('ArrowLeft')
  await page.keyboard.press('ArrowDown')
  await page.keyboard.press('ArrowDown')
  await page.keyboard.press('4')
  await expect.poll(async () => (await notes()).length).toBe(7)
  expect(
    (await notes()).find((n) => n.notation.voice === 2),
  ).toBeTruthy()
  await expect(page.getByRole('alert')).toHaveCount(0)
  await page.screenshot({ path: '../test-results/score-speedy-entry.png' })
  // Escape twice: clear selection state, then the caret.
  await page.keyboard.press('Escape')
  await page.keyboard.press('Escape')
  await expect(caret).toHaveCount(0)

  // Select mode: click a bar, Shift-click extends, right-click opens measure actions.
  await page.getByRole('button', { name: 'Select', exact: true }).click()
  await staff.click({ position: { x: 300, y: 60 } })
  const region = page.locator('[data-bar-region]')
  await expect(region).toBeVisible()
  await expect(region).toHaveAttribute('data-start', '0')
  await expect(region).toHaveAttribute('data-end', '4')
  await staff.click({ position: { x: 700, y: 60 }, modifiers: ['Shift'] })
  await expect(region).toHaveAttribute('data-end', '8')
  await staff.click({ position: { x: 700, y: 60 }, button: 'right' })
  const menu = page.getByRole('menu', { name: 'Measure actions' })
  await expect(menu).toBeVisible()
  await menu.getByRole('menuitem', { name: /^Time signature/ }).click()
  const dialog = page.getByRole('dialog', { name: /^Measure · bars 1–2/ })
  await expect(dialog).toBeVisible()
  const measure = page.getByRole('dialog', { name: /^Measure/ })
  await measure.getByRole('button', { name: '3/4', exact: true }).click()
  await measure.getByRole('button', { name: 'Apply', exact: true }).click()
  await expect
    .poll(async () => (await read()).score?.meters)
    .toEqual([
      { beat: 0, beats: 3, unit: 4 },
      { beat: 8, beats: 4, unit: 4 },
    ])
  await measure.getByRole('button', { name: 'Key signature', exact: true }).click()
  await measure.getByRole('radio', { name: 'G major', exact: true }).click()
  await measure.getByRole('button', { name: 'Apply', exact: true }).click()
  await expect
    .poll(async () => (await read()).score?.keys?.map((k: any) => [k.beat, k.key]))
    .toEqual([
      [0, 'G'],
      [8, 'C'],
    ])
  await measure.getByRole('button', { name: 'Repeat', exact: true }).click()
  await measure.getByRole('button', { name: 'More passes', exact: true }).click()
  await measure.getByRole('button', { name: 'Apply', exact: true }).click()
  await expect
    .poll(async () => (await read()).score?.repeats)
    .toEqual([{ start: 0, end: 8, times: 3, first_ending: null }])
  await page.screenshot({ path: '../test-results/score-measure-dialog.png' })
  await measure.getByRole('button', { name: 'Close', exact: true }).click()
  await expect(page.getByRole('dialog')).toHaveCount(0)
  // Double-clicking the selected bars reopens the dialog; Escape closes it.
  await staff.dblclick({ position: { x: 300, y: 60 } })
  await expect(page.getByRole('dialog', { name: /^Measure/ })).toBeVisible()
  await page.keyboard.press('Escape')
  // Backspace on a bar selection clears its contents, keeping notes elsewhere.
  await staff.click({ position: { x: 300, y: 60 } })
  await expect(region).toBeVisible()
  const before = (await notes()).length
  await page.keyboard.press('Backspace')
  await expect.poll(async () => (await notes()).length).toBeLessThan(before)
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await expect.poll(async () => (await notes()).length).toBe(before)
  await expect(page.getByRole('alert')).toHaveCount(0)
})
