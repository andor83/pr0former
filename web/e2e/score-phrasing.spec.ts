import { test, expect } from '@playwright/test'
test('slur, tie and bracket drag tools, curve handles, and inline text entry', async ({
  page,
}) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  let project = await (
    await page.request.post('/api/projects', { headers, data: { name: 'Phrasing', mode: 'structured' } })
  ).json()
  const note = (id: string, pitch: number, beat: number) => ({
    id, pitch, beat, duration: 1, velocity: 90, rest: false, tied: false,
  })
  project.parts[0].notes = [note('a', 60, 0), note('b', 60, 1), note('c', 64, 2), note('d', 67, 3)]
  project.score = { version: 1, length: 8, loop_score: false, meters: [], keys: [], repeats: [] }
  project = await (
    await page.request.put(`/api/projects/${project.id}`, { headers, data: project })
  ).json()
  const url = `/api/projects/${project.id}`
  const read = async () => (await (await page.request.get(url)).json()).project
  const curves = async () => (await read()).parts[0].staves?.[0]?.curves ?? []
  const center = async (id: string) => {
    const box = (await page.locator(`[data-note-id="${id}"]`).boundingBox())!
    return { x: box.x + box.width / 2, y: box.y + box.height / 2 }
  }
  const drag = async (from: { x: number; y: number }, to: { x: number; y: number }) => {
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move((from.x + to.x) / 2, (from.y + to.y) / 2, { steps: 4 })
    await page.mouse.move(to.x, to.y, { steps: 4 })
    await page.mouse.up()
  }
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  await expect(page.locator('[data-note-id="a"]')).toBeVisible()
  // Slur: drag from the first note to the third.
  await page.getByLabel('Phrasing', { exact: true }).click()
  await page.getByRole('button', { name: 'Slur tool', exact: true }).click()
  await drag(await center('a'), await center('c'))
  await expect.poll(async () => (await curves()).length).toBe(1)
  expect((await curves())[0]).toMatchObject({ kind: 'slur', start_note: 'a', end_note: 'c', height: -26 })
  // Select it and pull the shape handle down; the height follows.
  await page.keyboard.press('Escape')
  const slur = page.locator('[data-score-element*="curve"]').first()
  await expect(slur).toBeVisible()
  // Click the visible middle of the curve itself (a bezier's box centre is off the line).
  const onCurve = async () =>
    slur.evaluate((el) => {
      const path = el.querySelector<SVGPathElement>('path:not([data-score-hit])')!
      const pt = path.getPointAtLength(path.getTotalLength() / 2),
        m = path.getScreenCTM()!
      return { x: pt.x * m.a + pt.y * m.c + m.e, y: pt.x * m.b + pt.y * m.d + m.f }
    })
  let hit = await onCurve()
  await page.mouse.click(hit.x, hit.y)
  await expect(page.getByText('Selected curve', { exact: false })).toBeVisible()
  const shape = page.getByRole('button', { name: 'Shape handle of slur', exact: true })
  await expect(shape).toBeVisible()
  const shapeBox = (await shape.boundingBox())!
  await drag(
    { x: shapeBox.x + shapeBox.width / 2, y: shapeBox.y + shapeBox.height / 2 },
    { x: shapeBox.x + shapeBox.width / 2, y: shapeBox.y + shapeBox.height / 2 + 20 },
  )
  await expect.poll(async () => (await curves())[0]?.height).toBeGreaterThan(-15)
  // Backspace deletes the selected curve.
  hit = await onCurve()
  await page.mouse.click(hit.x, hit.y)
  await expect(page.getByText('Selected curve', { exact: false })).toBeVisible()
  await page.keyboard.press('Backspace')
  await expect.poll(async () => (await curves()).length).toBe(0)
  // Bracket: drag across empty space above the staff.
  const staff = page.locator('[data-score-part] [data-staff-id]').first()
  const staffBox = (await staff.boundingBox())!
  await page.getByLabel('Phrasing', { exact: true }).click()
  await page.getByRole('button', { name: 'Bracket tool', exact: true }).click()
  await drag({ x: staffBox.x + 300, y: staffBox.y + 60 }, { x: staffBox.x + 700, y: staffBox.y + 60 })
  await expect.poll(async () => (await curves()).length).toBe(1)
  expect((await curves())[0]).toMatchObject({ kind: 'bracket', start_note: null, end_note: null })
  expect((await curves())[0].end_beat).toBeGreaterThan((await curves())[0].start_beat)
  await page.keyboard.press('Escape')
  // Tie: same pitch works, a different pitch is refused with a message.
  await page.getByLabel('Phrasing', { exact: true }).click()
  await page.getByRole('button', { name: 'Tie tool', exact: true }).click()
  await drag(await center('a'), await center('b'))
  await expect
    .poll(async () => (await read()).parts[0].notes.find((n: any) => n.id === 'a').notation?.tie_to)
    .toBe('b')
  await drag(await center('c'), await center('d'))
  await expect(page.getByRole('alert')).toContainText('same pitch')
  await page.keyboard.press('Escape')
  // Text tool: inline entry at the clicked beat.
  await page.getByLabel('Text and tempo', { exact: true }).click()
  await page.getByRole('button', { name: 'Text tool', exact: true }).click()
  await staff.click({ position: { x: 600, y: 118 } })
  const input = page.getByLabel('Score text', { exact: true })
  await expect(input).toBeFocused()
  await input.fill('con sord.')
  await input.press('Enter')
  await expect
    .poll(async () => (await read()).parts[0].staves?.[0]?.marks?.map((m: any) => [m.kind, m.text]))
    .toEqual([['text', 'con sord.']])
  await expect(page.locator('[data-score-element*="mark"]').first()).toContainText('con sord.')
  await page.screenshot({ path: '../test-results/score-phrasing.png' })
})
