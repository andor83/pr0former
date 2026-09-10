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
  // The end handle lifts only its own end.
  const endHandle = page.getByRole('button', { name: 'End handle of slur', exact: true })
  const endBox = (await endHandle.boundingBox())!
  await drag(
    { x: endBox.x + endBox.width / 2, y: endBox.y + endBox.height / 2 },
    { x: endBox.x + endBox.width / 2, y: endBox.y + endBox.height / 2 - 18 },
  )
  await expect.poll(async () => (await curves())[0]?.end_lift).toBeLessThan(-10)
  expect((await curves())[0].lift).toBe(0)
  expect((await curves())[0].end_note).toBe('c')
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
  // Crescendo: a wedge on the staff plus a one-level rise on the velocity ramp.
  await page.getByLabel('Dynamics', { exact: true }).click()
  await page.getByRole('button', { name: 'Crescendo tool', exact: true }).click()
  await drag({ x: staffBox.x + 640, y: staffBox.y + 150 }, { x: staffBox.x + 900, y: staffBox.y + 150 })
  await expect.poll(async () => (await curves()).some((c: any) => c.kind === 'crescendo')).toBe(true)
  const hairpin = (await curves()).find((c: any) => c.kind === 'crescendo')
  const ramp = async () => (await read()).parts[0].staves[0].dynamics?.events ?? []
  await expect.poll(async () => (await ramp()).length).toBeGreaterThan(0)
  const rampStart = (await ramp()).find((e: any) => Math.abs(e.beat - hairpin.start_beat) < 1e-9)
  expect(rampStart.end - rampStart.start).toBe(16)
  await page.keyboard.press('Escape')
  // A multi-frame endpoint drag is one undo entry and carries the owned ramp.
  const wedge = page.locator(`[data-score-element*="${hairpin.id}"]`).first()
  const wedgePoint = await wedge.evaluate(el => {
    const path = el.querySelector<SVGPathElement>('path:not([data-score-hit])')!
    const pt = path.getPointAtLength(path.getTotalLength()/4), m=path.getScreenCTM()!
    return {x:pt.x*m.a+pt.y*m.c+m.e,y:pt.x*m.b+pt.y*m.d+m.f}
  })
  await page.mouse.click(wedgePoint.x,wedgePoint.y)
  const hairpinEnd = page.getByRole('button',{name:'End handle of crescendo',exact:true})
  const hb=(await hairpinEnd.boundingBox())!
  await drag({x:hb.x+hb.width/2,y:hb.y+hb.height/2},{x:hb.x+hb.width/2+70,y:hb.y+hb.height/2})
  await expect.poll(async()=> (await curves()).find((c:any)=>c.id===hairpin.id)?.end_beat).toBeGreaterThan(hairpin.end_beat)
  const moved=(await curves()).find((c:any)=>c.id===hairpin.id)
  const owned=(await ramp()).find((e:any)=>e.id===`hairpin:${hairpin.id}`)
  expect(owned.beat+owned.duration).toBe(moved.end_beat)
  await page.getByRole('button',{name:'Undo',exact:true}).click()
  await expect.poll(async()=> (await curves()).find((c:any)=>c.id===hairpin.id)?.end_beat).toBe(hairpin.end_beat)
  expect((await ramp()).find((e:any)=>e.id===`hairpin:${hairpin.id}`).duration).toBe(hairpin.end_beat-hairpin.start_beat)
  await page.keyboard.press('Escape')
  // Dynamic marks show on the staff as text and can be deleted there.
  await page.getByLabel('Dynamics', { exact: true }).click()
  await page.getByRole('button', { name: 'p dynamic tool', exact: true }).click()
  await staff.click({ position: { x: 300, y: 118 } })
  const dynamicText = page.locator('[data-score-element*="dynamic"]').filter({ hasText: /^p$/ })
  await expect(dynamicText).toBeVisible()
  await page.keyboard.press('Escape')
  await dynamicText.locator('text').first().click()
  await expect(page.getByText('Selected dynamic', { exact: false })).toBeVisible()
  await page.keyboard.press('Backspace')
  await expect.poll(async () => (await ramp()).some((e: any) => e.start === 48)).toBe(false)
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
