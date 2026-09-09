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
  await page.getByRole('button', { name: 'Score & parts', exact: true }).click()
  const score = page.getByRole('region', { name: 'Score workspace' })
  await expect(page.locator('[data-score-part]')).toHaveCount(3)
  await page.getByRole('button', { name: 'Write', exact: true }).click()
  await page
    .getByRole('button', { name: 'Quarter note (4)', exact: true })
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
  await page.getByText('Part settings · My piano', { exact: true }).click()
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
  await page.getByRole('button', { name: 'Score & parts', exact: true }).click()
  await expect(page.getByText('Expression', { exact: true })).toBeVisible()
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
    .getByRole('button', { name: 'Deactivate show', exact: true })
    .click()
  const downloadPromise = page.waitForEvent('download')
  await page.getByRole('button', { name: 'MusicXML', exact: true }).click()
  const download = await downloadPromise
  const { readFile } = await import('node:fs/promises')
  const xml = await readFile((await download.path())!, 'utf8')
  expect(xml).toContain('<repeat direction="backward" times="2"/>')
  expect(xml).toContain('<beats>2</beats><beat-type>4</beat-type>')
  await page
    .locator('input[accept=".xml,.musicxml"]')
    .setInputFiles({
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
  await page.getByRole('button', { name: 'Score & parts', exact: true }).click()
  await expect(page.locator('[data-staff-id]')).toHaveCount(2)
  await expect(page.getByRole('alert')).toHaveCount(0)
  const downloadPromise = page.waitForEvent('download')
  await page.getByRole('button', { name: 'MusicXML', exact: true }).click()
  const { readFile } = await import('node:fs/promises')
  const xml = await readFile((await (await downloadPromise).path())!, 'utf8')
  expect(xml).toContain('<staves>2</staves>')
  expect(xml).toContain('<mode>minor</mode>')
  await page
    .locator('input[accept=".xml,.musicxml"]')
    .setInputFiles({
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
  expect(rp.staves[0].clef_changes).toEqual([{ beat: 2, clef: 'alto' }])
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

test('multiple-note commands, marquee deletion and visual MIDI curve editing persist',async({page})=>{
 const headers={'X-Pr0former':'1'}
 const status=await(await page.request.get('/api/status')).json()
 await page.request.post(`/api/${status.bootstrap?'register':'login'}`,{headers,data:{username:'browser-test',password:'test1234'}})
 let p=await(await page.request.post('/api/projects',{headers,data:{name:'Selection and curves',mode:'structured'}})).json()
 p.parts[0].notes=[{id:'one',pitch:60,beat:0,duration:1,velocity:90,rest:false,tied:false},{id:'two',pitch:64,beat:2,duration:1,velocity:90,rest:false,tied:false}]
 p=await(await page.request.put(`/api/projects/${p.id}`,{headers,data:p})).json()
 const read=async()=> (await(await page.request.get(`/api/projects/${p.id}`)).json()).project
 await page.goto('/');await page.getByRole('button',{name:'Score & parts',exact:true}).click()
 const one=page.locator('[data-note-id="one"]'),two=page.locator('[data-note-id="two"]')
 await one.click();await two.click({modifiers:['ControlOrMeta']})
 await expect(page.getByText('Editing 2 selected note(s)',{exact:false})).toBeVisible()
 await page.keyboard.press('.')
 await expect.poll(async()=> (await read()).parts[0].notes.map((n:any)=>n.duration)).toEqual([1.5,1.5])
 await page.keyboard.press('Escape')
 let boxes:{x:number;y:number;width:number;height:number}[]=[]
 await expect.poll(async()=>{
  boxes=await page.locator('[data-note-id="one"], [data-note-id="two"]').evaluateAll(elements=>elements.map(el=>{const r=el.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}}).filter(r=>r.width>0&&r.height>0))
  return boxes.length
 }).toBe(2)
 const [a,b]=boxes as [typeof boxes[number],typeof boxes[number]]
 await page.mouse.move(Math.min(a.x,b.x)-15,Math.min(a.y,b.y)-15);await page.mouse.down();await page.mouse.move(Math.max(a.x+a.width,b.x+b.width)+15,Math.max(a.y+a.height,b.y+b.height)+15,{steps:8});await page.mouse.up()
 await expect(page.getByText('Editing 2 selected note(s)',{exact:false})).toBeVisible()
 await page.keyboard.press('Backspace')
 await expect.poll(async()=> (await read()).parts[0].notes.length).toBe(0)
 await page.getByRole('button',{name:'Undo',exact:true}).click()
 await expect.poll(async()=> (await read()).parts[0].notes.length).toBe(2)
 await page.getByRole('button',{name:'＋ MIDI lane',exact:true}).click()
 await expect.poll(async()=> (await read()).parts[0].automation.length).toBe(1)
 await page.getByRole('combobox',{name:'Message',exact:true}).selectOption('bend')
 await expect.poll(async()=> (await read()).parts[0].automation[0].message).toBe('bend')
 await page.getByLabel('MIDI drawing tool',{exact:true}).selectOption('ramp')
 await page.getByLabel('New MIDI curve',{exact:true}).selectOption('s_curve')
 const lane=page.locator('svg[aria-label$="MIDI lane"]')
 await lane.click({position:{x:230,y:65}})
 await expect.poll(async()=> (await read()).parts[0].automation[0].events.length).toBe(1)
 const event=(await read()).parts[0].automation[0].events[0]
 expect(event.curve).toBe('s_curve');expect(event.duration).toBe(1)
 await expect(lane.locator('[data-midi-event] path')).toBeVisible()
 await expect(page.getByRole('alert')).toHaveCount(0)
 await page.screenshot({path:'../test-results/score-midi-curves.png'})
})
