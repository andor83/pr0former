import { test, expect } from '@playwright/test'
test('score toolbar, multi-staff persistence, selection and player filtering', async ({
  page,
}) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  const me = await (await page.request.get('/api/me')).json()
  let project = await (
    await page.request.post('/api/projects', {
      headers,
      data: { name: 'Score overhaul', mode: 'structured' },
    })
  ).json()
  const p = project.parts[0]
  p.performer = me.id
  p.notes = []
  p.name = 'My piano'
  project.parts = [
    {
      ...structuredClone(p),
      id: 'other',
      name: 'Other player',
      performer: null,
    },
    p,
    { ...structuredClone(p), id: 'mine2', name: 'My second part' },
  ]
  project = await (
    await page.request.put(`/api/projects/${project.id}`, {
      headers,
      data: project,
    })
  ).json()
  const read = async () =>
    (await (await page.request.get(`/api/projects/${project.id}`)).json())
      .project
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  const score = page.getByRole('region', { name: 'Score workspace' })
  await expect(page.locator('[data-score-part]')).toHaveCount(3)
  await page.getByRole('button', { name: 'Write', exact: true }).click()
  await page.getByLabel('Note values', {exact:true}).click()
  await page
    .getByRole('button', { name: 'Quarter note (5)', exact: true })
    .click()
  const staff = page
    .locator(`[data-score-part="${p.id}"] [data-staff-id]`)
    .first()
  await staff.click({ position: { x: 250, y: 118 } })
  await expect
    .poll(
      async () =>
        (await read()).parts.find((x: any) => x.id === p.id).notes.length,
    )
    .toBe(1)
  const note = staff.locator('[data-note-id]').first()
  await expect(note).toBeVisible()
  await note.click()
  await page.keyboard.press('.')
  await expect
    .poll(
      async () =>
        (await read()).parts.find((x: any) => x.id === p.id).notes[0].duration,
    )
    .toBe(1.5)
  await page.keyboard.press('ArrowUp')
  await expect
    .poll(
      async () =>
        (await read()).parts.find((x: any) => x.id === p.id).notes[0].pitch,
    )
    .toBe(65)
  await page.keyboard.press('+')
  await expect
    .poll(
      async () =>
        (await read()).parts.find((x: any) => x.id === p.id).notes[0].pitch,
    )
    .toBe(66)
  await page.keyboard.press('Backspace')
  await expect
    .poll(
      async () =>
        (await read()).parts.find((x: any) => x.id === p.id).notes.length,
    )
    .toBe(0)
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await expect
    .poll(
      async () =>
        (await read()).parts.find((x: any) => x.id === p.id).notes.length,
    )
    .toBe(1)
  await page
    .getByRole('button', { name: 'Part settings · My piano', exact: true })
    .click()
  await page.getByRole('button', { name: '＋ Add staff', exact: true }).click()
  await expect
    .poll(
      async () =>
        (await read()).parts.find((x: any) => x.id === p.id).staves.length,
    )
    .toBe(2)
  await expect(
    page.locator(`[data-score-part="${p.id}"] [data-staff-id]`),
  ).toHaveCount(2)
  await expect(page.getByRole('alert')).toHaveCount(0)
  await page
    .getByRole('button', {
      name: 'Close Part settings · My piano',
      exact: true,
    })
    .click()
  await page.screenshot({ path: '../test-results/score-workspace.png' })
  await page
    .getByRole('button', { name: 'Performance mode', exact: true })
    .click()
  await expect(page.locator('[data-score-part]')).toHaveCount(2)
  await expect(
    page.getByLabel('Show all parts', { exact: true }),
  ).not.toBeChecked()
  await page.getByLabel('Show all parts', { exact: true }).check()
  await expect(page.locator('[data-score-part]')).toHaveCount(3)
  expect(
    await page
      .locator('[data-score-part]')
      .evaluateAll((es) => es.map((e) => e.getAttribute('data-score-part'))),
  ).toEqual([p.id, 'mine2', 'other'])
  await expect(
    page.getByRole('toolbar', { name: 'Notation tools' }),
  ).toHaveCount(0)
  await page.screenshot({ path: '../test-results/score-performance.png' })
})

test('shared score meter/repeats and MIDI automation reach the graph and round-trip MusicXML', async ({
  page,
}) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  let p = await (
    await page.request.post('/api/projects', {
      headers,
      data: { name: 'Score traversal and MIDI', mode: 'structured' },
    })
  ).json()
  p.bpm = 240
  p.parts[0].notes = []
  p.score = {
    version: 1,
    length: 4,
    loop_score: false,
    meters: [{ beat: 2, beats: 2, unit: 4 }],
    keys: [{ beat: 2, key: 'G' }],
    repeats: [{ start: 0, end: 2, times: 2 }],
    navigation: null,
  }
  p.parts[0].automation = [
    {
      id: 'lane',
      name: 'Expression',
      message: 'cc',
      channel: 1,
      number: 11,
      initial: 0,
      events: [
        {
          id: 'ramp',
          beat: 0,
          duration: 2,
          start: 0,
          end: 127,
          curve: 'linear',
        },
      ],
    },
  ]
  p.graph = {
    nodes: [
      {
        id: 'source',
        kind: 'part_midi',
        part_id: p.parts[0].id,
        label: 'Part MIDI',
        x: 0,
        y: 0,
        channels: 1,
        parameters: {},
      },
      {
        id: 'decode',
        kind: 'midi_to_control',
        label: 'Decode',
        x: 200,
        y: 0,
        channels: 1,
        parameters: { message_type: 11, number_filter: 11 },
      },
      {
        id: 'value',
        kind: 'control_input',
        label: 'Value',
        x: 400,
        y: 0,
        channels: 1,
        control_value: 0,
        parameters: { mode: 2, min: 0, max: 127 },
      },
    ],
    edges: [
      {
        id: 'events',
        source: 'source',
        source_port: 'events',
        target: 'decode',
        target_port: 'events',
      },
      {
        id: 'value',
        source: 'decode',
        source_port: 'value',
        target: 'value',
        target_port: 'in',
      },
    ],
  }
  const saved = await page.request.put(`/api/projects/${p.id}`, {
    headers,
    data: p,
  })
  expect(saved.ok(), await saved.text()).toBe(true)
  p = await saved.json()
  const packets: any[] = []
  page.on('websocket', (socket) =>
    socket.on('framereceived', ({ payload }) => {
      const value = JSON.parse(String(payload))
      if (value.type === 'telemetry') packets.push(value)
    }),
  )
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  await page.getByRole('button', { name: /^▸ Dynamics & ramps/ }).click()
  await expect(
    page.getByRole('button', { name: 'Expression · CC11', exact: true }),
  ).toBeVisible()
  await page.getByLabel('Count in', { exact: true }).selectOption('0')
  await page.getByRole('button', { name: 'Play', exact: true }).click()
  await expect
    .poll(() =>
      packets.some(
        (v) => v.values.value?._out > 10 && v.values.value?._out < 120,
      ),
    )
    .toBe(true)
  await expect
    .poll(() =>
      packets.some(
        (v) => v.beat > 2.1 && v.beat < 3.9 && v.parts[0].position < 2,
      ),
    )
    .toBe(true)
  await expect.poll(() => packets.at(-1)?.running).toBe(false)
  expect(packets.at(-1)?.beat).toBeGreaterThanOrEqual(6)
  await page
    .getByRole('button', { name: 'Disable audio engine', exact: true })
    .click()
  const downloadPromise = page.waitForEvent('download')
  await page.getByRole('button', { name: 'MusicXML', exact: true }).click()
  const download = await downloadPromise
  const { readFile } = await import('node:fs/promises')
  const xml = await readFile((await download.path())!, 'utf8')
  expect(xml).toContain('<repeat direction="backward" times="2"/>')
  expect(xml).toContain('<beats>2</beats><beat-type>4</beat-type>')
  await page.locator('input[accept=".xml,.musicxml"]').setInputFiles({
    name: 'roundtrip.musicxml',
    mimeType: 'application/xml',
    buffer: Buffer.from(xml),
  })
  await expect
    .poll(async () => {
      const current = (
        await (await page.request.get(`/api/projects/${p.id}`)).json()
      ).project
      return (
        current.revision > p.revision && current.score?.repeats?.length === 1
      )
    })
    .toBe(true)
})

test('multi-staff MusicXML preserves voices, ties, grace, tuplets and spelling; invalid edits are rejected', async ({
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
      data: { name: 'Piano interchange', mode: 'structured' },
    })
  ).json()
  const p = project.parts[0]
  p.notes = []
  p.staves = [
    {
      id: 'upper',
      name: 'Upper',
      clef: 'treble',
      transpose: 0,
      clef_changes: [{ beat: 2, clef: 'alto' }],
    },
    { id: 'lower', name: 'Lower', clef: 'bass', transpose: 0 },
  ]
  const add = (
    id: string,
    beat: number,
    pitch: number,
    step: number,
    staff = 'upper',
    extra: any = {},
  ) =>
    p.notes.push({
      id,
      beat,
      pitch,
      duration: 1,
      velocity: 90,
      rest: false,
      tied: false,
      notation: {
        staff,
        step,
        alter: 0,
        voice: 1,
        base: 1,
        dots: 0,
        tuplet_actual: 1,
        tuplet_normal: 1,
        ...extra,
      },
    })
  add('a', 0, 60, 28, 'upper', { tie_to: 'b' })
  add('b', 1, 60, 28)
  add('chord', 0, 64, 30)
  add('bass', 0, 36, 14, 'lower', { voice: 2 })
  add('grace', 2, 62, 29, 'upper', { grace_to: 'principal', base: 0.125 })
  p.notes.at(-1).duration = 0.125
  add('principal', 2, 64, 30, 'upper', { articulation: 'staccato', octave: 1 })
  p.notes.at(-1).pitch = 76
  for (let i = 0; i < 3; i++) {
    add('triplet' + i, 3 + i / 3, 60 + i * 2, 28 + i, 'upper', {
      base: 0.5,
      tuplet_actual: 3,
      tuplet_normal: 2,
    })
    p.notes.at(-1).duration = 1 / 3
  }
  project.score = {
    version: 1,
    length: 4,
    loop_score: false,
    meters: [],
    keys: [
      { beat: 0, key: 'C', mode: 'minor' },
      { beat: 2, key: 'G', mode: 'minor' },
    ],
    repeats: [],
  }
  const saved = await page.request.put(`/api/projects/${project.id}`, {
    headers,
    data: project,
  })
  expect(saved.ok(), await saved.text()).toBe(true)
  project = await saved.json()
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  await expect(page.locator('[data-staff-id]')).toHaveCount(2)
  // Multi-staff parts get a system bracket and barlines joining the staves.
  await expect(page.locator('.system-lines .system-bracket')).toBeVisible()
  expect(await page.locator('.system-lines line').count()).toBeGreaterThanOrEqual(2)
  await expect(page.getByRole('alert')).toHaveCount(0)
  const downloadPromise = page.waitForEvent('download')
  await page.getByRole('button', { name: 'MusicXML', exact: true }).click()
  const { readFile } = await import('node:fs/promises')
  const xml = await readFile((await (await downloadPromise).path())!, 'utf8')
  expect(xml).toContain('<staves>2</staves>')
  expect(xml).toContain('<mode>minor</mode>')
  await page.locator('input[accept=".xml,.musicxml"]').setInputFiles({
    name: 'piano.musicxml',
    mimeType: 'application/xml',
    buffer: Buffer.from(xml),
  })
  const read = async () =>
    (await (await page.request.get(`/api/projects/${project.id}`)).json())
      .project
  await expect
    .poll(async () => (await read()).revision)
    .toBeGreaterThan(project.revision)
  const restored = await read(),
    rp = restored.parts[0]
  expect(rp.staves).toHaveLength(2)
  expect(rp.staves?.[0]?.clef_changes).toEqual([{ beat: 2, clef: 'alto' }])
  expect(rp.notes).toHaveLength(p.notes.length)
  expect(rp.notes.filter((n: any) => n.notation.tie_to)).toHaveLength(1)
  expect(rp.notes.filter((n: any) => n.notation.grace_to)).toHaveLength(1)
  expect(rp.notes.find((n: any) => n.notation.octave === 1)?.pitch).toBe(76)
  expect(
    rp.notes.filter((n: any) => n.notation.tuplet_actual === 3),
  ).toHaveLength(3)
  expect(restored.score.keys).toEqual([
    { beat: 0, key: 'C', mode: 'minor' },
    { beat: 2, key: 'G', mode: 'minor' },
  ])
  rp.notes[0].notation.onset = { numerator: 1, denominator: 0 }
  expect(
    (
      await page.request.put(`/api/projects/${project.id}`, {
        headers,
        data: restored,
      })
    ).status(),
  ).toBe(400)
  await page
    .getByRole('button', { name: 'Performance mode', exact: true })
    .click()
  await expect(
    page.getByText('No parts assigned to you.', { exact: false }),
  ).toBeVisible()
  await page.getByLabel('Show all parts', { exact: true }).check()
  await expect(page.locator('[data-score-part]')).toHaveCount(1)
  await page
    .getByRole('button', { name: 'Exit performance mode', exact: true })
    .click()
  await page
    .getByRole('button', { name: 'Performance mode', exact: true })
    .click()
  await expect(
    page.getByLabel('Show all parts', { exact: true }),
  ).not.toBeChecked()
})

test('multiple-note commands, marquee deletion and visual MIDI curve editing persist', async ({
  page,
}) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  let p = await (
    await page.request.post('/api/projects', {
      headers,
      data: { name: 'Selection and curves', mode: 'structured' },
    })
  ).json()
  p.parts[0].notes = [
    {
      id: 'one',
      pitch: 60,
      beat: 0,
      duration: 1,
      velocity: 90,
      rest: false,
      tied: false,
    },
    {
      id: 'two',
      pitch: 64,
      beat: 2,
      duration: 1,
      velocity: 90,
      rest: false,
      tied: false,
    },
  ]
  p = await (
    await page.request.put(`/api/projects/${p.id}`, { headers, data: p })
  ).json()
  const read = async () =>
    (await (await page.request.get(`/api/projects/${p.id}`)).json()).project
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  const one = page.locator('[data-note-id="one"]'),
    two = page.locator('[data-note-id="two"]')
  await one.click()
  await two.click({ modifiers: ['ControlOrMeta'] })
  await expect(
    page.getByText('Editing 2 selected note(s)', { exact: false }),
  ).toBeVisible()
  await page.keyboard.press('.')
  await expect
    .poll(async () => (await read()).parts[0].notes.map((n: any) => n.duration))
    .toEqual([1.5, 1.5])
  await page.keyboard.press('Escape')
  let boxes: { x: number; y: number; width: number; height: number }[] = []
  await expect
    .poll(async () => {
      boxes = await page
        .locator('[data-note-id="one"], [data-note-id="two"]')
        .evaluateAll((elements) =>
          elements
            .map((el) => {
              const r = el.getBoundingClientRect()
              return { x: r.x, y: r.y, width: r.width, height: r.height }
            })
            .filter((r) => r.width > 0 && r.height > 0),
        )
      return boxes.length
    })
    .toBe(2)
  const [a, b] = boxes as [(typeof boxes)[number], (typeof boxes)[number]]
  await page.mouse.move(Math.min(a.x, b.x) - 15, Math.min(a.y, b.y) - 15)
  await page.mouse.down()
  await page.mouse.move(
    Math.max(a.x + a.width, b.x + b.width) + 15,
    Math.max(a.y + a.height, b.y + b.height) + 15,
    { steps: 8 },
  )
  await page.mouse.up()
  await expect(
    page.getByText('Editing 2 selected note(s)', { exact: false }),
  ).toBeVisible()
  await page.keyboard.press('Backspace')
  await expect.poll(async () => (await read()).parts[0].notes.length).toBe(0)
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await expect.poll(async () => (await read()).parts[0].notes.length).toBe(2)
  // Continuous MIDI lanes are managed inside the Dynamics & ramps graph.
  await page.getByRole('button', { name: /^▸ Dynamics & ramps/ }).click()
  await page.getByRole('button', { name: 'Add MIDI lane', exact: true }).click()
  await page.getByLabel('Lane name', { exact: true }).fill('Bend')
  await page.getByLabel('Message', { exact: true }).selectOption('bend')
  await page.getByRole('button', { name: 'Add lane', exact: true }).click()
  await expect
    .poll(async () => (await read()).parts[0].automation?.length)
    .toBe(1)
  expect((await read()).parts[0].automation[0].message).toBe('bend')
  // The new lane becomes the active line; clicking the graph adds its first point.
  const graph = page.getByRole('application', { name: 'Dynamics and ramp graph' })
  const graphBox = (await graph.boundingBox())!
  await graph.click({ position: { x: 230, y: graphBox.height * 0.3 } })
  await expect
    .poll(async () => (await read()).parts[0].automation?.[0]?.events.length)
    .toBe(1)
  const event = (await read()).parts[0].automation?.[0]?.events[0]
  expect(event.duration).toBe(0)
  expect(event.start).toBeGreaterThan(8000)
  await expect(graph.getByRole('button', { name: /^Bend · Bend \d+ at beat/ })).toBeVisible()
  // Lane settings can rename and delete the lane.
  await page.getByRole('button', { name: 'Lane settings: Bend', exact: true }).click()
  await page.getByRole('button', { name: 'Delete lane', exact: true }).click()
  await expect
    .poll(async () => (await read()).parts[0].automation?.length ?? 0)
    .toBe(0)
  await expect(page.getByRole('alert')).toHaveCount(0)
  await page.screenshot({ path: '../test-results/score-midi-curves.png' })
})

test('compact score dialogs, editable marks and drag-based piano roll', async ({
  page,
}) => {
  test.setTimeout(60000)
  const headers = { 'X-Pr0former': '1' },
    status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  let p = await (
    await page.request.post('/api/projects', {
      headers,
      data: { name: 'Direct score editing', mode: 'structured' },
    })
  ).json()
  p.parts = p.parts.slice(0, 1)
  const part = p.parts[0]
  part.dynamics = {
    mode: 'velocity',
    controller: 11,
    events: [
      { id: 'dyn', beat: 1, duration: 1, start: 64, end: 96, curve: 'linear' },
    ],
  }
  part.staves = [{ id: 'upper', name: 'Upper', clef: 'treble', transpose: 0 }]
  part.notes = [
    {
      id: 'accent-note',
      pitch: 60,
      beat: 0,
      duration: 1,
      velocity: 90,
      rest: false,
      tied: false,
      notation: {
        staff: 'upper',
        step: 28,
        alter: 0,
        voice: 1,
        base: 1,
        dots: 0,
        tuplet_actual: 1,
        tuplet_normal: 1,
        articulation: 'accent',
      },
    },
    {
      id: 'rest-note',
      pitch: 64,
      beat: 2,
      duration: 1,
      velocity: 0,
      rest: true,
      tied: false,
      notation: {
        staff: 'upper',
        step: 30,
        alter: 0,
        voice: 1,
        base: 1,
        dots: 0,
        tuplet_actual: 1,
        tuplet_normal: 1,
      },
    },
  ]
  p.score = {
    version: 1,
    length: 16,
    loop_score: false,
    meters: [{ beat: 4, beats: 3, unit: 4 }],
    keys: [{ beat: 4, key: 'G' }],
    repeats: [{ start: 0, end: 7, times: 2 }],
  }
  const saved = await page.request.put(`/api/projects/${p.id}`, {
    headers,
    data: p,
  })
  expect(saved.ok()).toBeTruthy()
  const read = async () =>
    (await (await page.request.get(`/api/projects/${p.id}`)).json()).project
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  const viewer = page.locator('.score-main'),
    toolbar = page.getByRole('toolbar', { name: 'Notation tools' })
  await expect(viewer.locator('.notation-toolbar')).toBeVisible()
  expect(await toolbar.evaluate((el) => getComputedStyle(el).position)).toBe(
    'absolute',
  )
  await expect(page.getByLabel('Articulation', { exact: true })).toHaveCount(0)
  await page.getByRole('button', { name: 'Shared score', exact: true }).click()
  await expect(page.getByRole('dialog', { name: /Shared score/ })).toBeVisible()
  await expect(page.getByLabel('Score length', { exact: true })).toHaveValue(
    '16',
  )
  await page.keyboard.press('Escape')
  await expect(page.getByRole('dialog')).toHaveCount(0)
  const accent = page.locator('[data-score-element*="articulation"]')
  await accent.click()
  await expect(
    page.getByText('Selected articulation', { exact: false }),
  ).toBeVisible()
  await page.keyboard.press('Backspace')
  await expect
    .poll(async () => (await read()).parts[0].notes[0]?.notation?.articulation)
    .toBeNull()
  expect((await read()).parts[0].notes.length).toBe(2)
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await expect(accent).toBeVisible()
  await accent.dblclick()
  await expect(
    page.getByRole('dialog', { name: 'Edit selected notes', exact: true }),
  ).toBeVisible()
  await page.getByLabel('Articulation', { exact: true }).selectOption('tenuto')
  await expect
    .poll(async () => (await read()).parts[0].notes[0]?.notation?.articulation)
    .toBe('tenuto')
  await page.keyboard.press('Escape')
  const auto = page.locator('[data-score-element*="rest"]').first()
  await auto.click()
  await page.keyboard.press('Backspace')
  await expect
    .poll(async () => (await read()).parts[0].staves?.[0]?.hidden_rests?.length)
    .toBe(1)
  await page.reload()
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  expect((await read()).parts[0].staves?.[0]?.hidden_rests?.length).toBe(1)
  const authored = page.locator('[data-note-id="rest-note"]')
  await authored.click()
  await page.keyboard.press('Backspace')
  await expect.poll(async () => (await read()).parts[0].notes.length).toBe(1)
  await page.locator('[data-score-element*="clef"]').first().dblclick()
  await expect(
    page.getByRole('dialog', {
      name: 'Bars, time signature & clef',
      exact: true,
    }),
  ).toBeVisible()
  await page.keyboard.press('Escape')
  await page.locator('[data-score-element*="meter"]').last().click()
  await page.keyboard.press('Backspace')
  await expect.poll(async () => (await read()).score?.meters?.length).toBe(0)
  await page.locator('[data-score-element*="repeat"] text').first().dblclick()
  await expect(page.getByRole('dialog', { name: /Shared score/ })).toBeVisible()
  await page.getByLabel('Passes', { exact: true }).fill('3')
  await page.getByRole('button', { name: 'Update repeat', exact: true }).click()
  await expect.poll(async () => (await read()).score?.repeats?.[0]?.times).toBe(3)
  await page.keyboard.press('Escape')
  // Dynamics are a breakpoint graph below the staff: the fixture ramp 64→96 is two points.
  await page.getByRole('button', { name: /^▸ Dynamics & ramps/ }).click()
  const graph = page.getByRole('application', { name: 'Dynamics and ramp graph' })
  await expect(graph).toBeVisible()
  const point = graph.getByRole('button', { name: 'Velocity 96 f at beat 3', exact: true })
  await expect(point).toBeVisible()
  await point.click()
  await page.keyboard.press('Backspace')
  // Editing writes the first staff's own dynamics and retires the legacy part-level copy.
  await expect
    .poll(async () => (await read()).parts[0].staves[0].dynamics?.events)
    .toEqual([{ id: 'dyn', beat: 1, duration: 0, start: 64, end: 64, curve: 'linear' }])
  expect((await read()).parts[0].dynamics.events).toEqual([{ id: 'dyn', beat: 1, duration: 1, start: 64, end: 96, curve: 'linear' }])
  // Clicking the graph adds a point on the active (velocity) line.
  const graphBox = (await graph.boundingBox())!
  await graph.click({ position: { x: 700, y: graphBox.height / 2 } })
  await expect.poll(async () => (await read()).parts[0].staves[0].dynamics?.events.length).toBe(2)
  // A written dynamic from the toolbar adds a rapid ramp into its level.
  await page.getByLabel('Dynamics', { exact: true }).click()
  await page.getByRole('button', { name: 'ff dynamic tool', exact: true }).click()
  await page.locator('[data-score-part] [data-staff-id]').first().click({ position: { x: 1000, y: 118 } })
  await expect
    .poll(async () => (await read()).parts[0].staves[0].dynamics?.events.some((e: any) => e.end === 112 && e.duration > 0 && e.duration <= 0.125))
    .toBe(true)
  await expect(graph.getByRole('button', { name: /^Velocity 112 ff at beat/ })).toBeVisible()
  await page.getByRole('button', { name: 'Piano roll', exact: true }).click()
  await expect(
    toolbar.getByRole('button', { name: 'Quarter note (5)', exact: true }),
  ).toHaveCount(0)
  await page.getByRole('button', { name: 'Write', exact: true }).click()
  const row = page.locator('[data-roll-pitch="67"]')
  await row.scrollIntoViewIfNeeded()
  const box = (await row.boundingBox())!
  await page.mouse.move(box.x + 300, box.y + 10)
  await page.mouse.down()
  await page.mouse.move(box.x + 390, box.y + 10, { steps: 8 })
  await page.mouse.up()
  await expect.poll(async () => (await read()).parts[0].notes.length).toBe(2)
  let n = (await read()).parts[0].notes.find((n: any) => n.id !== 'accent-note')
  expect(n.pitch).toBe(67)
  expect(n.duration).toBe(1)
  const note = page.locator(`[data-roll-note="${n.id}"]`),
    r = (await note.boundingBox())!
  await page.mouse.move(r.x + 12, r.y + 8)
  await page.mouse.down()
  await page.mouse.move(r.x + 102, r.y - 12, { steps: 8 })
  await page.mouse.up()
  await expect
    .poll(
      async () =>
        (await read()).parts[0].notes.find((x: any) => x.id === n.id).pitch,
    )
    .toBe(68)
  await expect
    .poll(
      async () =>
        (await read()).parts[0].notes.find((x: any) => x.id === n.id).beat,
    )
    .toBe(n.beat + 1)
  await expect(
    page.getByRole('button', { name: 'Undo', exact: true }),
  ).toBeEnabled()
  await expect(
    page
      .locator('[data-roll-pitch="68"]')
      .locator(`[data-roll-note="${n.id}"]`),
  ).toBeVisible()
  const resize = (await note.locator('[data-resize]').boundingBox())!
  await page.mouse.move(resize.x + 3, resize.y + 8)
  await page.mouse.down()
  await page.mouse.move(resize.x + 48, resize.y + 8, { steps: 8 })
  await page.mouse.up()
  await expect
    .poll(
      async () =>
        (await read()).parts[0].notes.find((x: any) => x.id === n.id).duration,
    )
    .toBe(1.5)
  await note.click()
  await page
    .locator('[data-roll-note="accent-note"]')
    .click({ modifiers: ['ControlOrMeta'] })
  await page.keyboard.press('Delete')
  await expect.poll(async () => (await read()).parts[0].notes.length).toBe(0)
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await expect.poll(async () => (await read()).parts[0].notes.length).toBe(2)
  await expect(page.getByRole('alert')).toHaveCount(0)
  await expect(
    page.getByRole('button', { name: 'Undo', exact: true }),
  ).toBeEnabled()
  await expect(page.locator('[data-roll-note]')).toHaveCount(2)
  await page.getByRole('button', { name: 'Select', exact: true }).click()
  const rects = await page.locator('[data-roll-note]').evaluateAll((es) =>
    es.map((e) => {
      const r = e.getBoundingClientRect()
      return { x: r.x, y: r.y, width: r.width, height: r.height }
    }),
  )
  const left = Math.min(...rects.map((r) => r.x)) + 1,
    top = Math.min(...rects.map((r) => r.y)) - 12,
    right = Math.max(...rects.map((r) => r.x + r.width)) + 8,
    bottom = Math.max(...rects.map((r) => r.y + r.height)) + 12
  await page.mouse.move(left, top)
  await page.mouse.down()
  await page.mouse.move(right, bottom, { steps: 8 })
  await page.mouse.up()
  await expect(
    page.getByText('Editing 2 selected note(s)', { exact: false }),
  ).toBeVisible()
  await page.keyboard.press('Delete')
  await expect.poll(async () => (await read()).parts[0].notes.length).toBe(0)
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await expect(page.locator('[data-roll-note]')).toHaveCount(2)
  await expect(
    page.getByRole('button', { name: 'Undo', exact: true }),
  ).toBeEnabled()
  const keyLabel = page.locator('[data-roll-pitch="60"] .roll-key')
  await expect(keyLabel).toHaveText('C4')
  await page.screenshot({ path: '../test-results/score-piano-roll.png' })
})

test('centered paper toolbar, clean beaming and structural bar edits persist across all parts', async ({
  page,
}) => {
  const headers = { 'X-Pr0former': '1' },
    status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  let p = await (
    await page.request.post('/api/projects', {
      headers,
      data: { name: 'Bars and engraving', mode: 'structured' },
    })
  ).json()
  const part = p.parts[0]
  part.staves = [{ id: 's', name: 'Upper', clef: 'treble', transpose: 0 }]
  part.notes = [0, 0.25, 0.5, 0.75, 2].map((beat, i) => ({
    id: `n${i}`,
    beat,
    duration: 0.25,
    pitch: 60 + i,
    velocity: 90,
    rest: false,
    tied: false,
  }))
  part.loop_beats = 12
  part.automation = [
    {
      id: 'a',
      name: 'CC11',
      channel: 1,
      message: 'cc',
      number: 11,
      events: [
        { id: 'e', beat: 8, duration: 1, start: 0, end: 100, curve: 'linear' },
      ],
    },
  ]
  p.parts = [
    part,
    {
      ...structuredClone(part),
      id: 'q',
      name: 'Lower part',
      notes: [
        {
          id: 'late',
          beat: 8,
          duration: 1,
          pitch: 60,
          velocity: 90,
          rest: false,
          tied: false,
        },
      ],
    },
  ]
  p.score = null
  expect(
    (
      await page.request.put(`/api/projects/${p.id}`, { headers, data: p })
    ).ok(),
  ).toBeTruthy()
  const read = async () =>
    (await (await page.request.get(`/api/projects/${p.id}`)).json()).project
  await page.goto('/')
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  await expect(page.locator('.score-actions')).toHaveCount(0)
  await expect(page.locator('.score-view-tools')).toHaveCount(0)
  const toolbar = page.getByRole('toolbar', { name: 'Notation tools' }),
    viewer = page.locator('.score-main')
  await expect(
    toolbar.getByRole('button', { name: 'MusicXML', exact: true }),
  ).toBeVisible()
  await expect(toolbar.locator('input[accept=".xml,.musicxml"]')).toHaveCount(1)
  const a = (await toolbar.boundingBox())!,
    b = (await viewer.boundingBox())!
  expect(Math.abs(a.x + a.width / 2 - b.x - b.width / 2)).toBeLessThan(2)
  await expect(
    page.locator('.score-footer').getByLabel('Follow playback'),
  ).toBeVisible()
  for (const id of ['n0', 'n1', 'n2', 'n3'])
    await expect(page.locator(`[data-note-id="${id}"] .vf-flag`)).toHaveCount(0)
  await expect(page.locator('[data-note-id="n4"] .vf-flag')).toHaveCount(1)
  await expect(
    page.locator('[data-score-part="part-1"] [data-score-element*="rhythm"]'),
  ).not.toHaveCount(0)
  expect(
    await page
      .locator('[data-score-hit]')
      .evaluateAll((es) =>
        es.every((e) => getComputedStyle(e).opacity === '0'),
      ),
  ).toBe(true)
  expect(
    await page
      .locator('.ensemble-part-name')
      .first()
      .evaluate((e) => getComputedStyle(e).backgroundColor),
  ).toBe('rgb(255, 255, 255)')
  await page.screenshot({ path: '../test-results/score-paper-toolbar.png' })
  await toolbar
    .getByRole('button', { name: 'Bars & meter', exact: true })
    .click()
  const dialog = page.getByRole('dialog', {
    name: 'Bars, time signature & clef',
    exact: true,
  })
  await dialog.getByLabel('Bar', { exact: true }).fill('2')
  await dialog.getByLabel('Bar', { exact: true }).press('Tab')
  await dialog.getByLabel('Beats per bar', { exact: true }).fill('3')
  await dialog
    .getByRole('button', { name: 'Set time signature', exact: true })
    .click()
  await expect
    .poll(async () => (await read()).score?.meters)
    .toEqual([{ beat: 4, beats: 3, unit: 4 }])
  await dialog.getByRole('button', { name: 'Add / delete bars', exact: true }).click()
  await expect(
    dialog.getByRole('button', { name: 'Insert before bar 2', exact: true }),
  ).toBeEnabled()
  await dialog
    .getByRole('button', { name: 'Insert before bar 2', exact: true })
    .click()
  await expect.poll(async () => (await read()).score.length).toBe(15)
  await expect.poll(async () => (await read()).parts[1].notes[0].beat).toBe(11)
  expect((await read()).parts[0].automation?.[0]?.events[0].beat).toBe(11)
  await expect(
    dialog.getByRole('button', { name: 'Delete from bar 2', exact: true }),
  ).toBeEnabled()
  await dialog
    .getByRole('button', { name: 'Delete from bar 2', exact: true })
    .click()
  await expect.poll(async () => (await read()).score.length).toBe(12)
  await expect.poll(async () => (await read()).parts[1].notes[0].beat).toBe(8)
  await expect(
    dialog.getByRole('button', { name: 'Add bars at end', exact: true }),
  ).toBeEnabled()
  await dialog
    .getByRole('button', { name: 'Add bars at end', exact: true })
    .click()
  await expect.poll(async () => (await read()).score.length).toBe(16)
  await dialog.getByRole('button', { name: 'Clef change', exact: true }).click()
  await expect(
    dialog.getByRole('button', { name: 'Set clef at beat', exact: true }),
  ).toBeEnabled()
  await dialog.getByLabel('At quarter beat', { exact: true }).fill('1.5')
  await dialog.getByLabel('Clef', { exact: true }).selectOption('bass')
  await dialog
    .getByRole('button', { name: 'Set clef at beat', exact: true })
    .click()
  await expect
    .poll(async () => (await read()).parts[0].staves?.[0]?.clef_changes)
    .toEqual([{ beat: 1.5, clef: 'bass' }])
  await expect(dialog.getByRole('alert')).toHaveCount(0)
  await page.keyboard.press('Escape')
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await expect
    .poll(async () => (await read()).parts[0].staves?.[0]?.clef_changes || [])
    .toEqual([])
  await page.reload()
  await page.getByRole('button', { name: 'Score & Parts', exact: true }).click()
  expect((await read()).score.length).toBe(16)
  await expect(page.getByRole('alert')).toHaveCount(0)
})
