import { test, expect } from '@playwright/test'
test('Web MIDI step entry: play to enter with chords, and hold + number', async ({
  page,
}) => {
  // A fake Web MIDI access object; window.__midi(bytes) delivers a message.
  await page.addInitScript(() => {
    const input: any = {
      id: 'fake-in',
      name: 'Fake Keys',
      state: 'connected',
      onmidimessage: null,
    }
    const access: any = {
      inputs: new Map([['fake-in', input]]),
      outputs: new Map(),
      onstatechange: null,
    }
    ;(navigator as any).requestMIDIAccess = async () => access
    ;(window as any).__midi = (bytes: number[]) =>
      input.onmidimessage?.({ data: new Uint8Array(bytes), timeStamp: 0 })
  })
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  let project = await (
    await page.request.post('/api/projects', {
      headers,
      data: { name: 'MIDI entry', mode: 'structured' },
    })
  ).json()
  project.parts[0].notes = []
  project = await (
    await page.request.put(`/api/projects/${project.id}`, {
      headers,
      data: project,
    })
  ).json()
  const url = `/api/projects/${project.id}`
  const notes = async () =>
    ((await (await page.request.get(url)).json()).project.parts[0]
      .notes as any[]).sort((a, b) => a.beat - b.beat || a.pitch - b.pitch)
  const midi = (bytes: number[]) =>
    page.evaluate((b) => (window as any).__midi(b), bytes)
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & parts', exact: true }).click()
  await page.getByLabel('MIDI entry', { exact: true }).selectOption('play')
  await expect(page.getByText('Fake Keys', { exact: true })).toBeVisible()
  const staff = page
    .locator('[data-score-part="part-1"] [data-staff-id]')
    .first()
  await page.getByRole('button', { name: 'Write', exact: true }).click()
  await staff.click({ position: { x: 214, y: 118 } })
  await expect.poll(async () => (await notes()).length).toBe(1)
  const first = (await notes())[0]
  // Play to enter: a note-on inserts at the caret with the current duration.
  await page.keyboard.press('4') // sets eighths and inserts one at the caret
  await expect.poll(async () => (await notes()).length).toBe(2)
  await midi([0x90, 66, 100])
  await midi([0x80, 66, 0])
  await expect.poll(async () => (await notes()).length).toBe(3)
  const played = (await notes()).find((n) => n.beat === first.beat + 1.5)
  expect(played.pitch).toBe(66)
  expect(played.duration).toBe(0.5)
  expect(played.notation.alter).toBe(1) // F sharp in C major
  // Notes held together form a chord and the caret advances once.
  await midi([0x90, 60, 100])
  await midi([0x90, 64, 100])
  await midi([0x90, 67, 100])
  await expect.poll(async () => (await notes()).length).toBe(6)
  const chord = (await notes()).filter((n) => n.beat === first.beat + 2)
  expect(chord.map((n) => n.pitch)).toEqual([60, 64, 67])
  await midi([0x80, 60, 0])
  await midi([0x80, 64, 0])
  await midi([0x80, 67, 0])
  await expect(page.locator('[data-entry-caret]')).toHaveAttribute(
    'data-beat',
    String(first.beat + 2.5),
  )
  // Hold + number: held pitches are entered when a duration key is pressed.
  await page.getByLabel('MIDI entry', { exact: true }).selectOption('hold')
  await midi([0x90, 62, 100])
  await midi([0x90, 65, 100])
  await expect.poll(async () => (await notes()).length).toBe(6)
  await page.keyboard.press('5')
  await expect.poll(async () => (await notes()).length).toBe(8)
  const held = (await notes()).filter((n) => n.beat === first.beat + 2.5)
  expect(held.map((n) => n.pitch)).toEqual([62, 65])
  expect(held.every((n) => n.duration === 1)).toBe(true)
  await midi([0x80, 62, 0])
  await midi([0x80, 65, 0])
  // Without held pitches the number key uses the caret pitch as before.
  await page.keyboard.press('5')
  await expect.poll(async () => (await notes()).length).toBe(9)
  await expect(page.getByRole('alert')).toHaveCount(0)
  await page.reload()
  await page.getByRole('button', { name: 'Score & parts', exact: true }).click()
  await expect(page.getByLabel('MIDI entry', { exact: true })).toHaveValue('hold')
  await page.getByLabel('MIDI entry', { exact: true }).selectOption('off')
})
