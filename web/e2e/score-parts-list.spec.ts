import { test, expect } from '@playwright/test'
test('part list context menu mutes, solos and deletes with confirmation; drag reorders', async ({
  page,
}) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  let project = await (
    await page.request.post('/api/projects', { headers, data: { name: 'Parts list', mode: 'structured' } })
  ).json()
  const base = project.parts[0]
  project.parts = [
    { ...structuredClone(base), id: 'first', name: 'Flute' },
    { ...structuredClone(base), id: 'second', name: 'Cello' },
    { ...structuredClone(base), id: 'third', name: 'Synth' },
  ]
  project = await (
    await page.request.put(`/api/projects/${project.id}`, { headers, data: project })
  ).json()
  const url = `/api/projects/${project.id}`
  const read = async () => (await (await page.request.get(url)).json()).project
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  const rows = page.locator('.part-row')
  await expect(rows).toHaveCount(3)
  // Context menu: mute and solo mirror the row buttons.
  await rows.nth(1).click({ button: 'right' })
  const menu = page.getByRole('menu', { name: 'Measure actions' })
  await menu.getByRole('menuitem', { name: 'Mute Cello', exact: true }).click()
  await expect.poll(async () => (await read()).parts[1].muted).toBe(true)
  await rows.nth(1).click({ button: 'right' })
  await menu.getByRole('menuitem', { name: 'Solo Cello', exact: true }).click()
  await expect.poll(async () => (await read()).parts[1].solo).toBe(true)
  // Drag Synth above Flute; the list previews and commits the new order.
  await rows.nth(2).dragTo(rows.nth(0))
  await expect.poll(async () => (await read()).parts.map((p: any) => p.name)).toEqual([
    'Synth',
    'Flute',
    'Cello',
  ])
  await expect(rows.nth(0)).toContainText('Synth')
  // Delete asks for confirmation; cancelling keeps the part.
  await rows.nth(0).click({ button: 'right' })
  await menu.getByRole('menuitem', { name: 'Delete Synth…', exact: true }).click()
  const confirm = page.getByRole('dialog', { name: 'Delete Synth?', exact: true })
  await expect(confirm).toBeVisible()
  await confirm.getByRole('button', { name: 'Cancel', exact: true }).click()
  expect((await read()).parts.length).toBe(3)
  await rows.nth(0).click({ button: 'right' })
  await menu.getByRole('menuitem', { name: 'Delete Synth…', exact: true }).click()
  await confirm.getByRole('button', { name: 'Delete part', exact: true }).click()
  await expect.poll(async () => (await read()).parts.map((p: any) => p.name)).toEqual([
    'Flute',
    'Cello',
  ])
  await expect(rows).toHaveCount(2)
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await expect.poll(async () => (await read()).parts.length).toBe(3)
  await expect(page.getByRole('alert')).toHaveCount(0)
})
