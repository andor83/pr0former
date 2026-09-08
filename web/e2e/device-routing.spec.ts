import { test, expect } from '@playwright/test'

test('native channel routing persists mixdowns, none, high input channels and undo', async ({page}) => {
  const headers = {'X-Pr0former':'1'}
  const status = await (await page.request.get('/api/status')).json()
  await page.request.post(`/api/${status.bootstrap ? 'register' : 'login'}`, {headers, data:{username:'browser-test',password:'test1234'}})
  let p = await (await page.request.post('/api/projects', {headers,data:{name:'Physical channel routing',mode:'freeform'}})).json()
  p.parts = []
  p.graph = {nodes:[
    {id:'in',kind:'input',label:'Interface input',x:0,y:0,channels:8,parameters:{}},
    {id:'out',kind:'output',label:'Interface output',x:350,y:0,channels:8,parameters:{gain:0}},
  ],edges:[{id:'audio',source:'in',source_port:'out',target:'out',target_port:'in'}]}
  const saved = await page.request.put(`/api/projects/${p.id}`, {headers,data:p})
  expect(saved.ok()).toBeTruthy(); p = await saved.json()
  const load = async () => (await (await page.request.get(`/api/projects/${p.id}`)).json()).project
  // Validate routes using real server endpoints, independent of device discovery fixtures.
  for (const value of [-2, 1.5, 65]) {
    const invalid = structuredClone(p); invalid.graph.nodes[0].parameters.route_1 = value
    expect((await page.request.put(`/api/projects/${p.id}`, {headers,data:invalid})).status()).toBe(400)
  }
  expect((await page.request.put(`/api/projects/${p.id}/parameter`, {headers,data:{node:'out',parameter:'route_1',value:1.5,revision:p.revision}})).status()).toBe(400)
  // Simulated interface inventory; this test never opens physical audio devices.
  await page.route('**/api/devices', route => route.fulfill({json:{
    input_interfaces:[{id:101,name:'64-channel input fixture',channels:64}],
    interfaces:[{id:102,name:'Stereo output fixture',channels:2}],active_inputs:[],
  }}))
  await page.route('**/api/system/audio', route => route.fulfill({json:{sample_rate:48000,block_size:128,
    input_interfaces:[{id:101,name:'64-channel input fixture',enabled:true}],
    interfaces:[{id:102,name:'Stereo output fixture',enabled:true}],
  }}))
  await page.goto('/')
  await page.getByRole('button',{name:'Choose project'}).click()
  await page.getByRole('button',{name:'Physical channel routing',exact:true}).click()
  await page.getByRole('button',{name:'Edit Interface output',exact:true}).click()
  const modal = page.getByRole('dialog')
  await expect(modal.getByText('Stereo output fixture · 2 physical outputs at 48000 Hz')).toBeVisible()
  await expect(modal.locator('tbody tr')).toHaveCount(8)
  await modal.getByRole('button',{name:'Mix to stereo',exact:true}).click()
  await expect.poll(async () => (await load()).graph.nodes[1].parameters.route_8).toBe(2)
  let node = (await load()).graph.nodes[1]
  expect(Array.from({length:8},(_,i)=>node.parameters[`route_${i+1}`])).toEqual([1,2,1,2,1,2,1,2])
  expect(node.parameters.gain).toBeCloseTo(-12.0412, 3)
  await modal.getByLabel('Signal 1 physical destination').selectOption('0')
  await expect.poll(async () => (await load()).graph.nodes[1].parameters.route_1).toBe(0)
  await modal.getByRole('button',{name:'Undo last edit'}).click()
  await expect(modal.getByLabel('Signal 1 physical destination')).toHaveValue('1')
  await modal.getByRole('button',{name:'Disconnect all',exact:true}).click()
  await expect.poll(async () => (await load()).graph.nodes[1].parameters.route_8).toBe(0)
  await modal.getByRole('button',{name:'Close parameters'}).click()
  await page.getByRole('button',{name:'Edit Interface input',exact:true}).click()
  await expect(modal.getByText('64-channel input fixture · 64 physical inputs at 48000 Hz')).toBeVisible()
  await modal.getByLabel('Signal 1 physical source').selectOption('64')
  await expect.poll(async () => (await load()).graph.nodes[0].parameters.route_1).toBe(64)
  await modal.getByLabel('Signal 2 physical source').selectOption('0')
  await expect.poll(async () => (await load()).graph.nodes[0].parameters.route_2).toBe(0)
  await page.reload()
  await page.getByRole('button',{name:'Edit Interface input',exact:true}).click()
  await expect(modal.getByLabel('Signal 1 physical source')).toHaveValue('64')
  await expect(modal.getByLabel('Signal 2 physical source')).toHaveValue('0')
  await page.setViewportSize({width:768,height:1024})
  await expect(modal.getByLabel('Signal 8 physical source')).toBeVisible()
  const bounds = await modal.evaluate(el => ({width:el.clientWidth,scroll:el.scrollWidth}))
  expect(bounds.scroll).toBeLessThanOrEqual(bounds.width)
  await page.screenshot({path:'test-results/device-channel-routing.png'})
})
