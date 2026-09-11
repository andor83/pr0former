import { test, expect } from '@playwright/test'

test('conducted sets arm and launch atomically with performer count-in state', async ({ page }) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {
    headers,
    data: { username: 'browser-test', password: 'test1234' },
  })
  const me = await (await page.request.get('/api/me')).json()
  let project = await (await page.request.post('/api/projects', {
    headers,
    data: { name: 'Conducted interface', mode: 'conducted' },
  })).json()
  project.bpm = 240
  project.conductor = me.id
  project.conducted.count_in_pulses = 2
  project.parts[0].name = 'Flute fragment'
  project.parts[0].loop_beats = 4
  project.parts[0].performer = 'shared-performer'
  const second = structuredClone(project.parts[0])
  second.id = 'second-part'
  second.name = 'Cello fragment'
  project.parts.push(second)
  project.conducted.sets = [{ id: 'opening', name: 'Opening', parts: [project.parts[0].id, second.id] }, { id:'other', name:'Other', parts:[] }]
  project = await (await page.request.put(`/api/projects/${project.id}`, { headers, data: project })).json()

  let latest: any
  page.on('websocket', socket => socket.on('framereceived', ({ payload }) => {
    const message = JSON.parse(String(payload))
    if (message.type === 'telemetry') latest = message
  }))
  await page.goto('/')
  await page.getByRole('button', { name: 'Enable audio engine', exact: true }).click()
  await page.getByRole('button', { name: 'Play', exact: true }).click()
  await expect.poll(() => latest?.running).toBe(true)
  await page.getByRole('button', { name: 'Conductor', exact: true }).click()
  await expect(page.getByText('Opening', { exact: true }).first()).toBeVisible()
  await page.getByRole('button', { name: 'Perform', exact: true }).click()
  await page.getByRole('button', { name: 'ARM', exact: true }).first().click()
  await expect(page.locator('.part-tile.armed')).toHaveCount(1)
  await page.getByRole('button', { name: 'PLAY ARMED', exact: true }).click()
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === project.parts[0].id)?.count_in_remaining).toBeGreaterThan(0)
  await expect(page.locator('.part-tile').first()).toContainText('Count-in')
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === project.parts[0].id)?.playing).toBe(true)
  await expect(page.locator('.part-tile.playing .part-progress')).toHaveCount(1)
  await page.getByRole('button', { name:'Other', exact:true }).click()
  await expect(page.getByRole('button', { name:'Opening', exact:true }).locator('.set-live')).toBeVisible()
  await page.getByRole('button', { name:'Opening', exact:true }).click()
  await page.locator('.part-tile').first().locator('input[type="range"]').fill('111')
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === project.parts[0].id)?.dynamic_override).toBe(111)
  await page.locator('.part-tile').first().getByTitle('Return to score dynamics').click()
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === project.parts[0].id)?.dynamic_override).toBeNull()
  await page.locator('.part-tile').nth(1).click()
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === second.id)?.queue_position).toBe(1)
  await page.locator('.part-tile').first().click()
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === project.parts[0].id)?.playing).toBe(false)
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === second.id)?.playing, {timeout:4000}).toBe(true)
  await page.locator('.part-tile').nth(1).click()
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === second.id)?.playing).toBe(false)
  await page.getByRole('button', {name:'ARM', exact:true}).first().click()
  await page.getByRole('button', {name:'PLAY + REPEAT', exact:true}).click()
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === project.parts[0].id)?.repeating, {timeout:4000}).toBe(true)
  await page.locator('.part-tile').first().click()
  await expect.poll(() => latest?.parts?.find((part: any) => part.id === project.parts[0].id)?.playing).toBe(false)
  await page.getByRole('button', { name: 'Exit performance', exact: true }).click()
})

test('conducted designation and cue API enforce membership, performer exclusion, and authority', async ({ request, playwright }) => {
  const headers = { 'X-Pr0former': '1' }
  const status = await (await request.get('/api/status')).json()
  await request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, { headers, data:{username:'browser-test',password:'test1234'} })
  const owner = await (await request.get('/api/me')).json()
  let project = await (await request.post('/api/projects', { headers, data:{name:'Conducted authorization',mode:'conducted'} })).json()

  project.conductor = owner.id
  project.parts[0].performer = owner.id
  expect((await request.put(`/api/projects/${project.id}`, {headers,data:project})).status()).toBe(400)
  project.parts[0].performer = null
  project.conductor = 'not-a-member'
  expect((await request.put(`/api/projects/${project.id}`, {headers,data:project})).status()).toBe(400)

  const invitation = await (await request.post(`/api/projects/${project.id}/invite`, {headers,data:{role:'performer'}})).json()
  const performer = await playwright.request.newContext({baseURL:'http://127.0.0.1:3101'})
  await performer.post('/api/register', {headers,data:{username:'conducted-api-player',password:'test5678',invite:invitation.token}})
  await performer.post('/api/join', {headers,data:{token:invitation.token}})
  const player = await (await performer.get('/api/me')).json()
  project.conductor = owner.id
  project.parts[0].performer = player.id
  project.conducted.sets = [{id:'all',name:'All',parts:[project.parts[0].id]}]
  project = await (await request.put(`/api/projects/${project.id}`, {headers,data:project})).json()

  await request.post(`/api/projects/${project.id}/transport`, {headers,data:{action:'activate'}})
  expect((await performer.post(`/api/projects/${project.id}/cue`, {headers,data:{action:'arm',parts:[project.parts[0].id]}})).status()).toBe(403)
  expect((await request.post(`/api/projects/${project.id}/cue`, {headers,data:{action:'arm',parts:['missing']}})).status()).toBe(400)
  expect((await request.post(`/api/projects/${project.id}/cue`, {headers,data:{action:'arm',parts:[project.parts[0].id]}})).ok()).toBeTruthy()
  expect((await request.post(`/api/projects/${project.id}/cue`, {headers,data:{action:'start',parts:[],request_id:'atomic-cue'}})).ok()).toBeTruthy()
  expect((await request.post(`/api/projects/${project.id}/cue`, {headers,data:{action:'start',parts:[],request_id:'atomic-cue'}})).ok()).toBeTruthy()
  await request.post(`/api/projects/${project.id}/transport`, {headers,data:{action:'deactivate'}})
  await performer.dispose()
})

test('reduced motion keeps touch reorder persistent without FLIP animation', async ({page}) => {
  await page.emulateMedia({reducedMotion:'reduce'})
  const headers = {'X-Pr0former':'1'}
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap?'register':'login'}`, {headers,data:{username:'browser-test',password:'test1234'}})
  let project = await (await page.request.post('/api/projects', {headers,data:{name:'Reduced conductor',mode:'conducted'}})).json()
  project.conducted.sets = [{id:'one',name:'One',parts:[]},{id:'two',name:'Two',parts:[]}]
  project = await (await page.request.put(`/api/projects/${project.id}`, {headers,data:project})).json()
  await page.goto('/')
  await page.getByRole('button',{name:'Conductor',exact:true}).click()
  const one = page.getByRole('button',{name:'One',exact:true}), two = page.getByRole('button',{name:'Two',exact:true})
  const a = await one.boundingBox(), b = await two.boundingBox()
  await page.mouse.move(a!.x+a!.width/2,a!.y+a!.height/2)
  await page.mouse.down();await page.mouse.move(b!.x+b!.width/2,b!.y+b!.height/2,{steps:5});await page.mouse.up()
  await expect.poll(async() => ((await (await page.request.get(`/api/projects/${project.id}`)).json()).project.conducted.sets[0].id)).toBe('two')
  expect(await page.locator('[data-conductor-flip]').evaluateAll(elements => elements.flatMap(element => element.getAnimations()).length)).toBe(0)
})
